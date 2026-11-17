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

#[quench_test::test]
fn after_signing_in_a_person_goes_back_to_the_page_they_came_from() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        // No note: the launcher.
        let plain = r.call("GET", "/lore-office/ui/home", &[], b"").await;
        assert_eq!(plain.status, 302);
        assert_eq!(plain.header("location"), Some("/lore-office/"));
        // A note of the page they were on: back to it, and the note is cleared.
        let cookie = lore_office_server::site::return_cookie("/lore-office/docs");
        let pair = cookie.split(';').next().unwrap().to_string();
        let back = r
            .call("GET", "/lore-office/ui/home", &[("cookie", &pair)], b"")
            .await;
        assert_eq!(back.status, 302);
        assert_eq!(back.header("location"), Some("/lore-office/docs"));
        assert!(
            back.headers
                .iter()
                .any(|(k, v)| k == "set-cookie" && v.contains("Max-Age=0"))
        );
        // Only pages of this service count: no way out.
        for bad in [
            "https://evil.example/",
            "//evil.example/",
            "/elsewhere",
            "/lore-office//evil.example",
        ] {
            let pair = lore_office_server::site::return_cookie(bad);
            let pair = pair.split(';').next().unwrap().to_string();
            let answer = r
                .call("GET", "/lore-office/ui/home", &[("cookie", &pair)], b"")
                .await;
            assert_eq!(answer.header("location"), Some("/lore-office/"), "{bad}");
        }
        assert!(lore_office_server::site::valid_return(
            "/lore-office",
            "/lore-office/sheets?share=ab12"
        ));
    });
}

#[quench_test::test]
fn the_meta_calls_say_whether_a_document_changed_without_sending_it() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        let alice = [("x-lore-user", "alice"), ("host", "office.example")];
        let field = |text: &str, key: &str| {
            quench_serde::json::parse(text)
                .unwrap()
                .get(key)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        };
        let put = r
            .call(
                "PUT",
                "/lore-office/api/docs?name=A.docx&kind=docs",
                &alice,
                b"one",
            )
            .await;
        let (id, first) = (field(&put.text(), "id"), field(&put.text(), "etag"));
        let meta = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/meta"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(meta.status, 200);
        assert_eq!(field(&meta.text(), "etag"), first);
        assert!(!meta.text().contains("one"), "no bytes");
        // Someone else's document is not found; nobody signed in is refused.
        let bob = [("x-lore-user", "bob")];
        assert_eq!(
            r.call(
                "GET",
                &format!("/lore-office/api/docs/{id}/meta"),
                &bob,
                b""
            )
            .await
            .status,
            404
        );
        // A link: its meta is open to whoever holds it, and follows the document.
        let share = r
            .call(
                "POST",
                &format!("/lore-office/api/docs/{id}/share?rights=edit"),
                &alice,
                b"",
            )
            .await;
        let token = field(&share.text(), "token");
        let linked = r
            .call("GET", &format!("/lore-office/api/s/{token}/meta"), &[], b"")
            .await;
        assert_eq!(linked.status, 200);
        assert_eq!(linked.header("x-rights"), Some("edit"));
        assert_eq!(field(&linked.text(), "etag"), first);
        let saved = r
            .call(
                "PUT",
                &format!("/lore-office/api/s/{token}"),
                &[("if-match", &first)],
                b"two",
            )
            .await;
        let second = field(&saved.text(), "etag");
        assert_ne!(first, second);
        let again = r
            .call("GET", &format!("/lore-office/api/s/{token}/meta"), &[], b"")
            .await;
        assert_eq!(field(&again.text(), "etag"), second);
        assert_eq!(
            r.call(
                "GET",
                &format!("/lore-office/api/s/{}/meta", "0".repeat(64)),
                &[],
                b""
            )
            .await
            .status,
            404
        );
    });
}

#[quench_test::test]
fn folders_move_documents_and_a_document_can_be_shared_with_a_named_person() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        let (alice, bob) = ([("x-lore-user", "alice")], [("x-lore-user", "bob")]);
        let field = |text: &str, key: &str| {
            quench_serde::json::parse(text)
                .unwrap()
                .get(key)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        };
        // A document made into a folder; the folders list; a move; a folder that is not empty is kept.
        let made = r
            .call(
                "PUT",
                "/lore-office/api/docs?name=Plan.docx&kind=docs&folder=Work%2F2026",
                &alice,
                b"one",
            )
            .await;
        assert_eq!(made.status, 200, "{}", made.text());
        assert_eq!(field(&made.text(), "folder"), "Work/2026");
        let id = field(&made.text(), "id");
        let folders = r.call("GET", "/lore-office/api/folders", &alice, b"").await;
        assert_eq!(folders.text(), r#"["Work","Work/2026"]"#);
        let made_folder = r
            .call("POST", "/lore-office/api/folders?path=Later", &alice, b"")
            .await;
        assert!(made_folder.text().contains("Later"));
        let moved = r
            .call(
                "PUT",
                &format!("/lore-office/api/docs/{id}/move?folder=Later"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(field(&moved.text(), "folder"), "Later");
        assert_eq!(
            r.call("DELETE", "/lore-office/api/folders?path=Later", &alice, b"")
                .await
                .status,
            400,
            "not empty"
        );
        assert_eq!(
            r.call("PUT", "/lore-office/api/docs/zz/move?folder=a", &alice, b"")
                .await
                .status,
            400
        );
        // Shared with bob: he sees it, opens it, and with edit rights saves it into alice's cloud.
        assert_eq!(
            r.call("GET", "/lore-office/api/grants", &bob, b"")
                .await
                .text(),
            "[]"
        );
        let shared = r
            .call(
                "POST",
                &format!("/lore-office/api/docs/{id}/grant?user=bob&rights=view"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(shared.status, 200, "{}", shared.text());
        assert_eq!(
            r.call(
                "POST",
                &format!("/lore-office/api/docs/{id}/grant?user=alice"),
                &alice,
                b""
            )
            .await
            .status,
            400,
            "not with oneself"
        );
        let listed = r.call("GET", "/lore-office/api/grants", &bob, b"").await;
        assert!(
            listed.text().contains("Plan.docx") && listed.text().contains("alice"),
            "{}",
            listed.text()
        );
        let opened = r
            .call(
                "GET",
                &format!("/lore-office/api/shared/alice/{id}"),
                &bob,
                b"",
            )
            .await;
        assert_eq!(
            (
                opened.status,
                opened.body.as_slice(),
                opened.header("x-rights")
            ),
            (200, &b"one"[..], Some("view"))
        );
        let etag = opened.header("x-etag").unwrap().to_string();
        let refused = r
            .call(
                "PUT",
                &format!("/lore-office/api/shared/alice/{id}"),
                &[("x-lore-user", "bob"), ("if-match", &etag)],
                b"two",
            )
            .await;
        assert_eq!(refused.status, 403);
        r.call(
            "POST",
            &format!("/lore-office/api/docs/{id}/grant?user=bob&rights=edit"),
            &alice,
            b"",
        )
        .await;
        let saved = r
            .call(
                "PUT",
                &format!("/lore-office/api/shared/alice/{id}"),
                &[("x-lore-user", "bob"), ("if-match", &etag)],
                b"two",
            )
            .await;
        assert_eq!(saved.status, 200, "{}", saved.text());
        assert_eq!(
            r.call("GET", &format!("/lore-office/api/docs/{id}"), &alice, b"")
                .await
                .body,
            b"two"
        );
        let meta = r
            .call(
                "GET",
                &format!("/lore-office/api/shared/alice/{id}/meta"),
                &bob,
                b"",
            )
            .await;
        assert_eq!(field(&meta.text(), "etag"), field(&saved.text(), "etag"));
        // Others have nothing; alice sees who it is shared with and takes it back.
        let carol = [("x-lore-user", "carol")];
        assert_eq!(
            r.call(
                "GET",
                &format!("/lore-office/api/shared/alice/{id}"),
                &carol,
                b""
            )
            .await
            .status,
            404
        );
        let grants = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/grants"),
                &alice,
                b"",
            )
            .await;
        assert!(grants.text().contains("bob"));
        assert_eq!(
            r.call(
                "DELETE",
                &format!("/lore-office/api/docs/{id}/grant?user=bob"),
                &alice,
                b""
            )
            .await
            .status,
            204
        );
        assert_eq!(
            r.call(
                "GET",
                &format!("/lore-office/api/shared/alice/{id}"),
                &bob,
                b""
            )
            .await
            .status,
            404
        );
    });
}

#[quench_test::test]
fn changes_are_numbered_by_the_service_and_folded_into_a_snapshot_by_whoever_is_up_to_date() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        let (alice, bob) = ([("x-lore-user", "alice")], [("x-lore-user", "bob")]);
        let text = |a: &Answer| a.text();
        let field = |t: &str, key: &str| {
            quench_serde::json::parse(t)
                .unwrap()
                .get(key)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        };
        let made = r
            .call(
                "PUT",
                "/lore-office/api/docs?name=Book.xlsx&kind=sheets",
                &alice,
                b"v0",
            )
            .await;
        let id = field(&made.text(), "id");
        let at = |h: u64| format!("/lore-office/api/docs/{id}/ops?base={h}");
        // The file carries the version of its log; there is no change yet.
        let opened = r
            .call("GET", &format!("/lore-office/api/docs/{id}"), &alice, b"")
            .await;
        assert_eq!(
            (opened.header("x-version"), opened.header("x-head")),
            (Some("0"), Some("0"))
        );
        // Changes are numbered in order; one that began from an old version is told the latest.
        let first = r
            .call("POST", &at(0), &alice, br#"{"cells":[{"r":0}]}"#)
            .await;
        assert_eq!((first.status, text(&first).as_str()), (200, r#"{"n":1}"#));
        let second = r
            .call("POST", &at(1), &alice, br#"{"cells":[{"r":1}]}"#)
            .await;
        assert_eq!(text(&second), r#"{"n":2}"#);
        let late = r
            .call("POST", &at(1), &alice, br#"{"cells":[{"r":2}]}"#)
            .await;
        assert_eq!((late.status, text(&late).as_str()), (409, r#"{"head":2}"#));
        assert_eq!(
            r.call("POST", &at(2), &alice, b"not json").await.status,
            400
        );
        assert_eq!(
            r.call(
                "POST",
                &format!("/lore-office/api/docs/{id}/ops"),
                &alice,
                b"{}"
            )
            .await
            .status,
            400,
            "no base"
        );
        let since = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/ops?after=1"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(
            text(&since),
            r#"{"head":2,"version":0,"ops":[{"n":2,"op":{"cells":[{"r":1}]}}]}"#
        );
        // Someone with no right to the document has no log of it.
        assert_eq!(
            r.call(
                "GET",
                &format!("/lore-office/api/docs/{id}/ops?after=0"),
                &bob,
                b""
            )
            .await
            .status,
            404
        );
        // Through a link with edit rights, and through a share with a person.
        let share = r
            .call(
                "POST",
                &format!("/lore-office/api/docs/{id}/share?rights=edit"),
                &alice,
                b"",
            )
            .await;
        let token = field(&share.text(), "token");
        let linked = r
            .call(
                "POST",
                &format!("/lore-office/api/s/{token}/ops?base=2"),
                &[],
                br#"{"cells":[{"r":3}]}"#,
            )
            .await;
        assert_eq!(text(&linked), r#"{"n":3}"#);
        r.call(
            "POST",
            &format!("/lore-office/api/docs/{id}/grant?user=bob&rights=view"),
            &alice,
            b"",
        )
        .await;
        let view_post = r
            .call(
                "POST",
                &format!("/lore-office/api/shared/alice/{id}/ops?base=3"),
                &bob,
                b"{}",
            )
            .await;
        assert_eq!(view_post.status, 403, "a view share cannot add changes");
        let read = r
            .call(
                "GET",
                &format!("/lore-office/api/shared/alice/{id}/ops?after=2"),
                &bob,
                b"",
            )
            .await;
        assert!(text(&read).contains(r#""n":3"#), "{}", text(&read));
        r.call(
            "POST",
            &format!("/lore-office/api/docs/{id}/grant?user=bob&rights=edit"),
            &alice,
            b"",
        )
        .await;
        let shared = r
            .call(
                "POST",
                &format!("/lore-office/api/shared/alice/{id}/ops?base=3"),
                &bob,
                br#"{"cells":[{"r":4}]}"#,
            )
            .await;
        assert_eq!(text(&shared), r#"{"n":4}"#);
        // A snapshot at an old version is refused; at the latest it folds the changes in, and followers are told.
        let old = r
            .call(
                "PUT",
                &format!("/lore-office/api/docs/{id}?name=Book.xlsx&kind=sheets&at=3"),
                &alice,
                b"v3",
            )
            .await;
        assert_eq!((old.status, text(&old).as_str()), (409, r#"{"head":4}"#));
        let folded = r
            .call(
                "PUT",
                &format!("/lore-office/api/docs/{id}?name=Book.xlsx&kind=sheets&at=4"),
                &alice,
                b"v4",
            )
            .await;
        assert_eq!(
            (folded.status, folded.header("x-version")),
            (200, Some("4"))
        );
        let gone = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/ops?after=2"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(
            (gone.status, text(&gone).as_str()),
            (410, r#"{"head":4,"version":4}"#)
        );
        let current = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/ops?after=4"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(text(&current), r#"{"head":4,"version":4,"ops":[]}"#);
        // The meta carries both numbers.
        let meta = r
            .call(
                "GET",
                &format!("/lore-office/api/docs/{id}/meta"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(
            (meta.header("x-version"), meta.header("x-head")),
            (Some("4"), Some("4"))
        );
    });
}

#[quench_test::test]
fn pages_tell_where_they_are_and_are_told_at_once_when_anything_about_the_document_changes() {
    run(async {
        let r = rig("/lore-office", 1 << 20).await;
        let (alice, bob) = ([("x-lore-user", "alice")], [("x-lore-user", "bob")]);
        let made = r
            .call(
                "PUT",
                "/lore-office/api/docs?name=Book.xlsx&kind=sheets",
                &alice,
                b"v0",
            )
            .await;
        let id = quench_serde::json::parse(&made.text())
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let room = format!("/lore-office/api/docs/{id}/presence");
        let here = |client: &str, pos: &str| {
            format!(r##"{{"client":"{client}","name":"Otter","color":"#3366cc","pos":"{pos}"}}"##)
        };
        // Someone who is not the owner and was given nothing cannot be in the room.
        assert_eq!(
            r.call("POST", &room, &bob, here("b1", "0,0").as_bytes())
                .await
                .status,
            404
        );
        assert_eq!(
            r.call("POST", &room, &alice, here("a1", "0,0").as_bytes())
                .await
                .status,
            200
        );
        // Nothing to say about others yet: the answer waits for the limit, with the same number.
        let alone = r
            .call(
                "GET",
                &format!("{room}?client=a1&seen=0&wait=0"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(alone.status, 200);
        let rev = |t: &str| {
            quench_serde::json::parse(t)
                .unwrap()
                .get("rev")
                .unwrap()
                .as_u64()
                .unwrap()
        };
        let seen = rev(&alone.text());
        assert!(alone.text().contains(r#""peers":[]"#), "{}", alone.text());
        let held = r
            .call(
                "GET",
                &format!("{room}?client=a1&seen={seen}&wait=150"),
                &alice,
                b"",
            )
            .await;
        assert_eq!(rev(&held.text()), seen, "nothing changed");
        // Another page arrives: the first is told with the place.
        let wait_uri = format!("{room}?client=a1&seen={seen}&wait=5000");
        let waiting = r.call("GET", &wait_uri, &alice, b"");
        let arrives = async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            r.call(
                "POST",
                &room,
                &alice,
                here("a2", "3,4").replace("Otter", "Fox").as_bytes(),
            )
            .await
        };
        let (told, _) = tokio::join!(waiting, arrives);
        assert!(
            told.text().contains(r#""name":"Fox""#) && told.text().contains("3,4"),
            "{}",
            told.text()
        );
        assert!(
            !told.text().contains("a1"),
            "a page is not told about itself"
        );
        let seen = rev(&told.text());
        // A save of the document wakes it too.
        let wait_uri = format!("{room}?client=a1&seen={seen}&wait=5000");
        let waiting = r.call("GET", &wait_uri, &alice, b"");
        let saves = async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            r.call(
                "POST",
                &format!("/lore-office/api/docs/{id}/ops?base=0"),
                &alice,
                br#"{"cells":[]}"#,
            )
            .await
        };
        let (woken, _) = tokio::join!(waiting, saves);
        assert_ne!(rev(&woken.text()), seen);
        // Leaving takes the page out at once.
        let seen = rev(&woken.text());
        let gone = r"{\x22client\x22:\x22a2\x22,\x22gone\x22:true}".replace(r"\x22", "\"");
        r.call("POST", &room, &alice, gone.as_bytes()).await;
        let after = r
            .call(
                "GET",
                &format!("{room}?client=a1&seen={seen}&wait=0"),
                &alice,
                b"",
            )
            .await;
        assert!(after.text().contains(r#""peers":[]"#), "{}", after.text());
    });
}
