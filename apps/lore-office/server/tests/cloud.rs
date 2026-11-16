#![allow(clippy::unwrap_used, clippy::assert_is_empty)]
use lore_office_server::{Modules, Site, SiteConfig};
use office_cloud::{LocalStore, new_id};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::prelude::{ContainerBuilder, Endpoint, Request};
use std::sync::Arc;

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

struct Rig {
    site: Site,
    container: Arc<quench_http::prelude::Container>,
    dir: std::path::PathBuf,
}

async fn rig(base: &str, quota: u64) -> Rig {
    let dir = std::env::temp_dir().join(format!("lore-site-test-{}", new_id(6)));
    let fake = vec![0, 97, 115, 109];
    let site = Site::new(SiteConfig {
        modules: Modules {
            launcher: fake.clone(),
            docs: fake.clone(),
            sheets: fake.clone(),
            notes: fake.clone(),
            pdf: fake,
        },
        store: Arc::new(LocalStore::new(&dir, quota)),
        jwt: JwtConfig::for_tests(),
        sso: None,
        base: base.to_string(),
        max_upload: 1 << 20,
        dev: false,
    });
    Rig {
        site,
        container: Arc::new(ContainerBuilder::new().build().await.unwrap()),
        dir,
    }
}

impl Rig {
    async fn call(&self, method: &str, uri: &str, headers: &[(&str, &str)], body: &[u8]) -> Answer {
        let mut map = quench_http::prelude::http::HeaderMap::default();
        for (k, v) in headers {
            map.insert(*k, v.parse().unwrap());
        }
        let req = Request::new(
            method.parse().unwrap(),
            uri.parse().unwrap(),
            map,
            InboundBody::from_bytes(body.to_vec()),
            self.container.clone(),
        );
        let response = self.site.call(req).await;
        let status = response.status().as_u16();
        let inner = response.into_inner();
        let headers = inner
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();
        let body = inner.body.collect_limited(1 << 24).await.unwrap().to_vec();
        Answer {
            status,
            headers,
            body,
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn token_of(json: &str) -> String {
    let v = quench_serde::json::parse(json).unwrap();
    v.get("token").unwrap().as_str().unwrap().to_string()
}

#[quench_test::test]
fn the_launcher_the_four_apps_the_assets_and_health_are_served() {
    run(async {
        let r = rig("", 1 << 20).await;
        assert_eq!(r.call("GET", "/health/ready", &[], b"").await.status, 200);
        let home = r.call("GET", "/", &[], b"").await;
        assert_eq!(home.status, 200);
        assert!(home.text().contains("lore-office"));
        let mut css = None;
        for app in ["docs", "sheets", "notes", "pdf"] {
            let page = r.call("GET", &format!("/{app}"), &[], b"").await;
            assert_eq!(page.status, 200, "{app}");
            let html = page.text();
            assert!(html.contains(&format!("lore-{app}")), "{app}");
            // The page tells the browser where the API is.
            assert!(html.contains("\"api\":\"/api\""), "{app}: {html}");
            if let Some(i) = html.find("/_quench/") {
                let end = html[i..].find(['"', '\'', ' ']).unwrap();
                css = Some(html[i..i + end].to_string());
            }
        }
        // An asset of any app answers through the one address.
        let asset = r
            .call("GET", &css.expect("a page links its assets"), &[], b"")
            .await;
        assert_eq!(asset.status, 200);
        assert_eq!(r.call("GET", "/nothing", &[], b"").await.status, 404);
    });
}

#[quench_test::test]
fn documents_are_kept_listed_read_changed_with_etags_and_removed() {
    run(async {
        let r = rig("", 1 << 20).await;
        assert_eq!(r.call("GET", "/api/status", &[], b"").await.status, 200);
        let put = r
            .call(
                "PUT",
                "/api/docs?name=Plan.docx&kind=docs",
                &[],
                b"version one",
            )
            .await;
        assert_eq!(put.status, 200, "{}", put.text());
        let v = quench_serde::json::parse(&put.text()).unwrap();
        let (id, etag) = (
            v.get("id").unwrap().as_str().unwrap().to_string(),
            v.get("etag").unwrap().as_str().unwrap().to_string(),
        );
        let list = r.call("GET", "/api/docs?kind=docs", &[], b"").await;
        assert!(list.text().contains("Plan.docx"));
        assert!(
            !r.call("GET", "/api/docs?kind=sheets", &[], b"")
                .await
                .text()
                .contains("Plan.docx"),
            "lists are of one kind"
        );
        let got = r.call("GET", &format!("/api/docs/{id}"), &[], b"").await;
        assert_eq!(
            (got.body.as_slice(), got.header("x-etag")),
            (b"version one".as_slice(), Some(etag.as_str()))
        );
        // A save says which version it began from: the right one goes through, a stale one is turned away.
        let next = r
            .call(
                "PUT",
                &format!("/api/docs/{id}?name=Plan.docx&kind=docs"),
                &[("if-match", &etag)],
                b"version two",
            )
            .await;
        assert_eq!(next.status, 200);
        let stale = r
            .call(
                "PUT",
                &format!("/api/docs/{id}?name=Plan.docx&kind=docs"),
                &[("if-match", &etag)],
                b"version three",
            )
            .await;
        assert_eq!(stale.status, 409);
        assert_eq!(
            r.call("GET", &format!("/api/docs/{id}"), &[], b"")
                .await
                .body,
            b"version two"
        );
        let usage = r.call("GET", "/api/usage", &[], b"").await.text();
        assert!(usage.contains("\"used\":11"), "{usage}");
        assert_eq!(
            r.call("DELETE", &format!("/api/docs/{id}"), &[], b"")
                .await
                .status,
            204
        );
        assert_eq!(
            r.call("GET", &format!("/api/docs/{id}"), &[], b"")
                .await
                .status,
            404
        );
    });
}

#[quench_test::test]
fn people_see_only_their_own_documents_and_the_quota_holds() {
    run(async {
        let r = rig("", 100).await;
        let a = [("x-lore-user", "alice")];
        let b = [("x-lore-user", "bob")];
        let put = r
            .call("PUT", "/api/docs?name=a.docx&kind=docs", &a, b"secret")
            .await;
        let id = quench_serde::json::parse(&put.text())
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            r.call("GET", &format!("/api/docs/{id}"), &b, b"")
                .await
                .status,
            404
        );
        assert_eq!(
            r.call("DELETE", &format!("/api/docs/{id}"), &b, b"")
                .await
                .status,
            404
        );
        assert!(
            !r.call("GET", "/api/docs", &b, b"")
                .await
                .text()
                .contains("a.docx")
        );
        let big = vec![0_u8; 200];
        assert_eq!(
            r.call("PUT", "/api/docs?name=big.docx&kind=docs", &a, &big)
                .await
                .status,
            507,
            "no room"
        );
        assert_eq!(
            r.call(
                "POST",
                &format!("/api/docs/{id}/share?rights=view"),
                &b,
                b""
            )
            .await
            .status,
            404,
            "only the owner shares"
        );
    });
}

#[quench_test::test]
fn share_links_open_without_signing_in_and_edit_only_with_edit_rights() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        let alice = [
            ("x-lore-user", "alice"),
            ("host", "office.example"),
            ("x-forwarded-proto", "https"),
        ];
        let put = r
            .call(
                "PUT",
                "/lore-office/api/docs?name=Shared.docx&kind=docs",
                &alice,
                b"hello",
            )
            .await;
        assert_eq!(put.status, 200, "{}", put.text());
        let id = quench_serde::json::parse(&put.text())
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let view = r
            .call(
                "POST",
                &format!("/lore-office/api/docs/{id}/share?rights=view"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(view.status, 200, "{}", view.text());
        let (vt, link) = (
            token_of(&view.text()),
            view.header("x-link").unwrap().to_string(),
        );
        assert_eq!(
            link,
            format!("https://office.example/lore-office/docs?share={vt}")
        );
        let edit = r
            .call(
                "POST",
                &format!("/lore-office/api/docs/{id}/share?rights=edit&days=7"),
                &alice,
                b"",
            )
            .await;
        let et = token_of(&edit.text());
        assert_ne!(vt, et);
        // Nobody signed in, no header: the link alone opens it.
        let open = r
            .call("GET", &format!("/lore-office/api/s/{vt}"), &[], b"")
            .await;
        assert_eq!(
            (open.status, open.body.as_slice(), open.header("x-rights")),
            (200, b"hello".as_slice(), Some("view"))
        );
        let etag = open.header("x-etag").unwrap().to_string();
        assert_eq!(
            r.call(
                "PUT",
                &format!("/lore-office/api/s/{vt}"),
                &[("if-match", &etag)],
                b"vandal"
            )
            .await
            .status,
            403,
            "a viewer cannot change it"
        );
        let saved = r
            .call(
                "PUT",
                &format!("/lore-office/api/s/{et}"),
                &[("if-match", &etag)],
                b"edited",
            )
            .await;
        assert_eq!(saved.status, 200, "{}", saved.text());
        assert_eq!(
            r.call("GET", &format!("/lore-office/api/docs/{id}"), &alice, b"")
                .await
                .body,
            b"edited",
            "the owner sees the change"
        );
        // The page opened by the link knows what it may do; a wrong app or a made-up token gets nothing.
        let page = r
            .call("GET", &format!("/lore-office/docs?share={et}"), &[], b"")
            .await
            .text();
        assert!(
            page.contains(&format!("\"token\":\"{et}\",\"rights\":\"edit\"")),
            "{page}"
        );
        assert!(
            !r.call("GET", &format!("/lore-office/sheets?share={et}"), &[], b"")
                .await
                .text()
                .contains(&et)
        );
        assert!(
            !r.call("GET", "/lore-office/docs?share=nonsense", &[], b"")
                .await
                .text()
                .contains("\"share\"")
        );
        // The owner lists the links and withdraws one: it stops working at once.
        assert!(
            r.call("GET", "/lore-office/api/shares", &alice, b"")
                .await
                .text()
                .contains(&vt)
        );
        assert_eq!(
            r.call(
                "DELETE",
                &format!("/lore-office/api/shares/{vt}"),
                &alice,
                b""
            )
            .await
            .status,
            204
        );
        assert_eq!(
            r.call("GET", &format!("/lore-office/api/s/{vt}"), &[], b"")
                .await
                .status,
            404
        );
        assert_eq!(
            r.call("GET", "/lore-office/docs", &[], b"").await.status,
            200
        );
        assert_eq!(
            r.call("GET", "/docs", &[], b"").await.status,
            200,
            "the base path is optional (the ingress strips it)"
        );
    });
}

quench_test::main!();
