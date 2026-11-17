//! The cloud API: documents kept per user and the links that share them.
//!
//! Under `/api` (the caller strips the base path):
//!
//! | | |
//! |---|---|
//! | `GET /status` | who is signed in |
//! | `GET /usage` | space used and allowed |
//! | `GET /docs?kind=` | the person's documents |
//! | `PUT /docs?name=&kind=` | a new document (the body is the file) |
//! | `GET\|PUT\|DELETE /docs/{id}` | one document; `PUT` takes `If-Match` and answers 409 when it is stale |
//! | `POST /docs/{id}/share?rights=view\|edit&days=` | a link; its address is in `X-Link` |
//! | `GET /shares`, `DELETE /shares/{token}` | the person's links |
//! | `GET\|PUT /s/{token}` | what a link opens (no sign-in); `PUT` only for a link with edit rights |

#![allow(
    clippy::result_large_err,
    clippy::or_fun_call,
    clippy::redundant_closure
)]

pub mod site;
pub use site::{Modules, Site, SiteConfig};

use office_cloud::{Error, Rights, Share, Storage, new_id, now};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_http::prelude::{Endpoint, Request, Response, http::StatusCode};
use std::sync::Arc;

/// Who is asking.
pub async fn user_of(req: &Request, jwt: &JwtConfig) -> Option<String> {
    if jwt.auth_enabled {
        return quench_auth::http::routers::ui::get_user_from_req(req, jwt)
            .await
            .map(|c| c.sub);
    }
    // No realm (development, tests): everyone is `local`, unless a header says who, so several people can be tried.
    Some(
        req.header("x-lore-user")
            .filter(|u| !u.is_empty() && u.len() < 80)
            .unwrap_or("local")
            .to_string(),
    )
}

/// Signed in at the realm, but not allowed into this service: who they are, so the page can say so.
pub async fn denied_user(req: &Request, jwt: &JwtConfig) -> Option<String> {
    if !jwt.auth_enabled {
        return None;
    }
    let token = quench_auth::http::domain::cookies::cookie_value(
        req,
        &quench_auth::domain::realm::session_cookie_name(),
    )?;
    let claims = jwt.decode_claims(&token).await.ok()?;
    (!claims.allows(&jwt.service_name)).then_some(claims.sub)
}

fn query(req: &Request, key: &str) -> Option<String> {
    let q = req.uri().query()?;
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| quench_auth::domain::percent::decode(v).unwrap_or_default())
    })
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    quench_serde::json::to_string(&text.to_string())
}

fn reply(status: StatusCode, text: &str) -> Response {
    Response::text(status, text)
}

fn failed(e: &Error) -> Response {
    match e {
        Error::NotFound => reply(StatusCode::NOT_FOUND, "not found"),
        Error::Conflict { etag } => reply(
            StatusCode::CONFLICT,
            "the document was changed by someone else",
        )
        .header("x-etag", etag),
        Error::Quota { .. } => reply(
            StatusCode::from_u16(507).unwrap_or(StatusCode::PAYLOAD_TOO_LARGE),
            &e.to_string(),
        ),
        Error::Behind { head } => {
            Response::text(StatusCode::CONFLICT, format!("{{\"head\":{head}}}"))
                .header("content-type", "application/json")
                .header("x-head", head.to_string())
        }
        Error::Denied => reply(StatusCode::FORBIDDEN, &e.to_string()),
        Error::Invalid(_) => reply(StatusCode::BAD_REQUEST, &e.to_string()),
        Error::Io(why) => {
            quench_log::error!("cloud storage: {why}");
            reply(StatusCode::BAD_GATEWAY, "the storage did not answer")
        }
    }
}

fn json<T: quench_serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn file(meta: &office_cloud::DocMeta, bytes: Vec<u8>) -> Response {
    // Names go as percent-encoded UTF-8 (headers are ASCII).
    let name: String = meta
        .name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b' ') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    Response::from_bytes(StatusCode::OK, Bytes::from(bytes))
        .header("content-type", "application/octet-stream")
        .header("x-name", name)
        .header("x-kind", &meta.kind)
        .header("x-etag", &meta.etag)
        .header("cache-control", "no-store")
        .header(
            "access-control-expose-headers",
            "x-name, x-kind, x-etag, x-rights, x-link, x-version, x-head",
        )
}

/// The API.
pub struct CloudApi {
    pub store: Arc<dyn Storage>,
    pub jwt: JwtConfig,
    /// The base path the service is served under (`/lore-office`), for the addresses it hands out.
    pub base: String,
    pub max_upload: usize,
    /// Where signing in starts.
    pub login: String,
    /// The numbering of each document's changes.
    pub log: office_cloud::OpLog,
    /// Who has each document open, and the waiting for news of it.
    pub hub: office_cloud::Hub,
}

impl std::fmt::Debug for CloudApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl CloudApi {
    fn link_for(&self, req: &Request, share: &Share) -> String {
        let proto = req.header("x-forwarded-proto").unwrap_or("https");
        let host = req
            .header("x-forwarded-host")
            .or_else(|| req.header("host"))
            .unwrap_or("localhost");
        format!(
            "{proto}://{host}{}/{}?share={}",
            self.base, share.kind, share.token
        )
    }

    /// Whether `user` was given the right to change `owner`'s document `id`.
    async fn edit_grant(&self, user: &str, owner: &str, id: &str) -> Result<(), Error> {
        let grant = self.store.grant(owner, id, user).await?;
        if grant.rights == Rights::Edit {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }

    /// The link, when it still works and lets its holder change the document.
    async fn edit_share(&self, token: &str) -> Result<Share, Error> {
        let share = self.store.share(token).await?;
        if share.expired(now()) {
            return Err(Error::NotFound);
        }
        if share.rights != Rights::Edit {
            return Err(Error::Denied);
        }
        Ok(share)
    }

    /// `response` with the version of the stored file and the latest change number of the document.
    async fn stamped(&self, response: Response, owner: &str, id: &str) -> Response {
        let version = self.store.meta(owner, id).await.map_or(0, |m| m.version);
        let head = self
            .log
            .head(&*self.store, owner, id)
            .await
            .unwrap_or(version);
        response
            .header("x-version", version.to_string())
            .header("x-head", head.to_string())
    }

    /// Save a full file over `owner`'s document `id`: as the snapshot at version `at` of its log when that is given, else whole.
    #[allow(clippy::too_many_arguments)]
    async fn replace(
        &self,
        owner: &str,
        id: &str,
        name_kind: Option<(String, String)>,
        bytes: &[u8],
        at: Option<u64>,
        if_match: Option<&str>,
    ) -> Response {
        let (name, kind) = match name_kind {
            Some(nk) => nk,
            None => match self.store.meta(owner, id).await {
                Ok(m) => (m.name, m.kind),
                Err(e) => return failed(&e),
            },
        };
        match self
            .log
            .snapshot(&*self.store, owner, id, &name, &kind, bytes, at, if_match)
            .await
        {
            Ok((meta, head)) => {
                self.hub.touch(owner, id).await;
                json(&meta)
                    .header("x-version", meta.version.to_string())
                    .header("x-head", head.to_string())
            }
            Err(e) => failed(&e),
        }
    }

    /// `POST .../ops?base=n`: add a change that began from version `n` of the log.
    async fn ops_post(&self, req: &mut Request, owner: &str, id: &str) -> Response {
        let Some(base) = query(req, "base").and_then(|b| b.parse::<u64>().ok()) else {
            return reply(
                StatusCode::BAD_REQUEST,
                "base is the version the change began from",
            );
        };
        let bytes = match self.body(req).await {
            Ok(b) => b,
            Err(r) => return r,
        };
        if quench_serde::json::parse(&String::from_utf8_lossy(&bytes)).is_err() {
            return reply(StatusCode::BAD_REQUEST, "a change is JSON");
        }
        match self.log.append(&*self.store, owner, id, base, &bytes).await {
            Ok(n) => {
                self.hub.touch(owner, id).await;
                Response::text(StatusCode::OK, format!("{{\"n\":{n}}}"))
                    .header("content-type", "application/json")
                    .header("x-head", n.to_string())
            }
            Err(e) => failed(&e),
        }
    }

    /// `POST .../presence`: this page is here, at this place (`{"client","name","color","pos","gone"}`).
    async fn presence_post(&self, req: &mut Request, owner: &str, id: &str) -> Response {
        let bytes = match self.body(req).await {
            Ok(b) => b,
            Err(r) => return r,
        };
        let Ok(value) = quench_serde::json::parse(&String::from_utf8_lossy(&bytes)) else {
            return reply(StatusCode::BAD_REQUEST, "presence is JSON");
        };
        let text = |k: &str| {
            value
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let client = text("client");
        if client.is_empty() {
            return reply(StatusCode::BAD_REQUEST, "client names the page");
        }
        let gone = value
            .get("gone")
            .and_then(quench_serde::Value::as_bool)
            .unwrap_or(false);
        let peer = office_cloud::Peer {
            client,
            name: text("name"),
            color: text("color"),
            pos: text("pos"),
        };
        let rev = self.hub.update(owner, id, peer, gone).await;
        Response::text(StatusCode::OK, format!("{{\"rev\":{rev}}}"))
            .header("content-type", "application/json")
    }

    /// `GET .../presence?client=c&seen=n&wait=ms`: the others here, held until something changes from number `n` (or `wait`).
    async fn presence_get(&self, req: &Request, owner: &str, id: &str) -> Response {
        let client = query(req, "client").unwrap_or_default();
        let seen = query(req, "seen")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let wait = query(req, "wait")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
            .min(25_000);
        let (rev, docs, peers) = self
            .hub
            .wait(
                owner,
                id,
                &client,
                seen,
                std::time::Duration::from_millis(wait),
            )
            .await;
        let list: Vec<String> = peers
            .iter()
            .map(|p| {
                format!(
                    "{{\"client\":{},\"name\":{},\"color\":{},\"pos\":{}}}",
                    quoted(&p.client),
                    quoted(&p.name),
                    quoted(&p.color),
                    quoted(&p.pos),
                )
            })
            .collect();
        Response::text(
            StatusCode::OK,
            format!(
                "{{\"rev\":{rev},\"doc\":{docs},\"peers\":[{}]}}",
                list.join(",")
            ),
        )
        .header("content-type", "application/json")
    }

    /// `GET .../ops?after=n`: the changes after number `n`.
    async fn ops_get(&self, req: &Request, owner: &str, id: &str) -> Response {
        let after = query(req, "after")
            .and_then(|a| a.parse::<u64>().ok())
            .unwrap_or(0);
        match self.log.since(&*self.store, owner, id, after).await {
            Ok(office_cloud::Since::Ops { head, version, ops }) => {
                let list: Vec<String> = ops
                    .iter()
                    .map(|(n, op)| format!("{{\"n\":{n},\"op\":{}}}", String::from_utf8_lossy(op)))
                    .collect();
                Response::text(
                    StatusCode::OK,
                    format!(
                        "{{\"head\":{head},\"version\":{version},\"ops\":[{}]}}",
                        list.join(",")
                    ),
                )
                .header("content-type", "application/json")
                .header("cache-control", "no-store")
            }
            Ok(office_cloud::Since::Gone { head, version }) => Response::text(
                StatusCode::GONE,
                format!("{{\"head\":{head},\"version\":{version}}}"),
            )
            .header("content-type", "application/json"),
            Err(e) => failed(&e),
        }
    }

    async fn body(&self, req: &mut Request) -> Result<Vec<u8>, Response> {
        req.take_body()
            .collect_limited(self.max_upload)
            .await
            .map(|b| b.to_vec())
            .map_err(|_| reply(StatusCode::PAYLOAD_TOO_LARGE, "the file is too large"))
    }

    async fn shared(&self, req: &mut Request, token: &str) -> Response {
        match req.method().as_str() {
            "GET" => match office_cloud::open_share(&*self.store, token).await {
                Ok((share, meta, bytes)) => {
                    let response = file(&meta, bytes).header("x-rights", share.rights.word());
                    self.stamped(response, &share.owner, &share.doc).await
                }
                Err(e) => failed(&e),
            },
            "PUT" => {
                let if_match = req.header("if-match").map(str::to_string);
                let at = query(req, "at").and_then(|a| a.parse::<u64>().ok());
                let share = match self.edit_share(token).await {
                    Ok(s) => s,
                    Err(e) => return failed(&e),
                };
                let bytes = match self.body(req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                self.replace(
                    &share.owner,
                    &share.doc,
                    None,
                    &bytes,
                    at,
                    if_match.as_deref(),
                )
                .await
            }
            _ => reply(StatusCode::METHOD_NOT_ALLOWED, "GET or PUT"),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn handle(&self, mut req: Request) -> Response {
        let path = req.uri().path().trim_end_matches('/').to_string();
        let unbased = path.strip_prefix(self.base.as_str()).unwrap_or(&path);
        let rest = unbased.strip_prefix("/api").unwrap_or(unbased).to_string();
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        let method = req.method().as_str().to_string();
        // What a link opens needs no sign-in.
        if let ["s", token] = parts.as_slice() {
            return self.shared(&mut req, token).await;
        }
        if let ["s", token, "ops"] = parts.as_slice() {
            return match method.as_str() {
                "GET" => match office_cloud::share_meta(&*self.store, token).await {
                    Ok((share, _)) => self.ops_get(&req, &share.owner, &share.doc).await,
                    Err(e) => failed(&e),
                },
                "POST" => match self.edit_share(token).await {
                    Ok(share) => self.ops_post(&mut req, &share.owner, &share.doc).await,
                    Err(e) => failed(&e),
                },
                _ => reply(StatusCode::METHOD_NOT_ALLOWED, "GET or POST"),
            };
        }
        if let ["s", token, "presence"] = parts.as_slice() {
            return match office_cloud::share_meta(&*self.store, token).await {
                Ok((share, _)) => match method.as_str() {
                    "GET" => self.presence_get(&req, &share.owner, &share.doc).await,
                    "POST" => self.presence_post(&mut req, &share.owner, &share.doc).await,
                    _ => reply(StatusCode::METHOD_NOT_ALLOWED, "GET or POST"),
                },
                Err(e) => failed(&e),
            };
        }
        if let ["s", token, "meta"] = parts.as_slice() {
            return match office_cloud::share_meta(&*self.store, token).await {
                Ok((share, meta)) => {
                    let response = json(&meta).header("x-rights", share.rights.word());
                    self.stamped(response, &share.owner, &share.doc).await
                }
                Err(e) => failed(&e),
            };
        }
        let user = user_of(&req, &self.jwt).await;
        if let ["status"] = parts.as_slice() {
            let body = format!(
                "{{\"user\":{},\"login\":{}}}",
                user.as_deref()
                    .map_or("null".to_string(), |u| quench_serde::json::to_string(u)),
                quench_serde::json::to_string(self.login.as_str())
            );
            return Response::from_bytes(StatusCode::OK, Bytes::from(body))
                .header("content-type", "application/json");
        }
        let Some(user) = user else {
            return reply(StatusCode::UNAUTHORIZED, "sign in first")
                .header("www-authenticate", "Bearer");
        };
        let store = &*self.store;
        match (method.as_str(), parts.as_slice()) {
            ("GET", ["usage"]) => store
                .usage(&user)
                .await
                .map_or_else(|e| failed(&e), |u| json(&u)),
            ("GET", ["docs"]) => match store.list(&user).await {
                Ok(list) => {
                    let kind = query(&req, "kind");
                    let list: Vec<_> = list
                        .into_iter()
                        .filter(|d| kind.as_deref().is_none_or(|k| k.is_empty() || d.kind == k))
                        .collect();
                    json(&list)
                }
                Err(e) => failed(&e),
            },
            ("PUT", ["docs"]) => {
                let (Some(name), Some(kind)) = (query(&req, "name"), query(&req, "kind")) else {
                    return reply(StatusCode::BAD_REQUEST, "name and kind are needed");
                };
                let folder = query(&req, "folder").unwrap_or_default();
                if let Err(e) = office_cloud::clean_folder(&folder) {
                    return failed(&e);
                }
                let bytes = match self.body(&mut req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                match store.put(&user, None, &name, &kind, &bytes, None).await {
                    // A new document starts at the top; into a folder is a second step.
                    Ok(meta) if !folder.is_empty() => store
                        .move_doc(&user, &meta.id, &folder)
                        .await
                        .map_or_else(|e| failed(&e), |m| json(&m)),
                    Ok(meta) => json(&meta),
                    Err(e) => failed(&e),
                }
            }
            ("GET", ["folders"]) => store
                .folders(&user)
                .await
                .map_or_else(|e| failed(&e), |f| json(&f)),
            ("POST", ["folders"]) => {
                let Some(path) = query(&req, "path") else {
                    return reply(StatusCode::BAD_REQUEST, "path is needed");
                };
                match store.make_folder(&user, &path).await {
                    Ok(()) => store
                        .folders(&user)
                        .await
                        .map_or_else(|e| failed(&e), |f| json(&f)),
                    Err(e) => failed(&e),
                }
            }
            ("DELETE", ["folders"]) => {
                let Some(path) = query(&req, "path") else {
                    return reply(StatusCode::BAD_REQUEST, "path is needed");
                };
                store
                    .delete_folder(&user, &path)
                    .await
                    .map_or_else(|e| failed(&e), |()| Response::new(StatusCode::NO_CONTENT))
            }
            ("PUT", ["docs", id, "move"]) => store
                .move_doc(&user, id, &query(&req, "folder").unwrap_or_default())
                .await
                .map_or_else(|e| failed(&e), |m| json(&m)),
            ("POST", ["docs", id, "grant"]) => {
                let Some(rights) =
                    Rights::parse(&query(&req, "rights").unwrap_or_else(|| "view".into()))
                else {
                    return reply(StatusCode::BAD_REQUEST, "rights are view or edit");
                };
                let with = query(&req, "user").unwrap_or_default();
                office_cloud::grant_doc(store, &user, id, &with, rights)
                    .await
                    .map_or_else(|e| failed(&e), |g| json(&g))
            }
            ("GET", ["docs", id, "grants"]) => store
                .grants_of(&user, id)
                .await
                .map_or_else(|e| failed(&e), |g| json(&g)),
            ("DELETE", ["docs", id, "grant"]) => store
                .delete_grant(&user, id, &query(&req, "user").unwrap_or_default())
                .await
                .map_or_else(|e| failed(&e), |()| Response::new(StatusCode::NO_CONTENT)),
            ("GET", ["grants"]) => store
                .grants_to(&user)
                .await
                .map_or_else(|e| failed(&e), |g| json(&g)),
            ("GET", ["shared", owner, id]) => {
                match office_cloud::open_granted(store, &user, owner, id).await {
                    Ok((grant, meta, bytes)) => {
                        let response = file(&meta, bytes).header("x-rights", grant.rights.word());
                        self.stamped(response, owner, id).await
                    }
                    Err(e) => failed(&e),
                }
            }
            ("GET", ["shared", owner, id, "meta"]) => {
                match office_cloud::granted_meta(store, &user, owner, id).await {
                    Ok((grant, meta)) => {
                        let response = json(&meta).header("x-rights", grant.rights.word());
                        self.stamped(response, owner, id).await
                    }
                    Err(e) => failed(&e),
                }
            }
            ("GET" | "POST", ["shared", owner, id, "presence"]) => {
                match office_cloud::granted_meta(store, &user, owner, id).await {
                    Ok(_) if method == "GET" => self.presence_get(&req, owner, id).await,
                    Ok(_) => self.presence_post(&mut req, owner, id).await,
                    Err(e) => failed(&e),
                }
            }
            ("GET", ["docs", id, "presence"]) => match store.meta(&user, id).await {
                Ok(_) => self.presence_get(&req, &user, id).await,
                Err(e) => failed(&e),
            },
            ("POST", ["docs", id, "presence"]) => match store.meta(&user, id).await {
                Ok(_) => self.presence_post(&mut req, &user, id).await,
                Err(e) => failed(&e),
            },
            ("GET", ["shared", owner, id, "ops"]) => {
                match office_cloud::granted_meta(store, &user, owner, id).await {
                    Ok(_) => self.ops_get(&req, owner, id).await,
                    Err(e) => failed(&e),
                }
            }
            ("POST", ["shared", owner, id, "ops"]) => {
                match self.edit_grant(&user, owner, id).await {
                    Ok(()) => self.ops_post(&mut req, owner, id).await,
                    Err(e) => failed(&e),
                }
            }
            ("PUT", ["shared", owner, id]) => {
                let if_match = req.header("if-match").map(str::to_string);
                let at = query(&req, "at").and_then(|a| a.parse::<u64>().ok());
                if let Err(e) = self.edit_grant(&user, owner, id).await {
                    return failed(&e);
                }
                let bytes = match self.body(&mut req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                self.replace(owner, id, None, &bytes, at, if_match.as_deref())
                    .await
            }
            ("GET", ["docs", id, "meta"]) => match store.meta(&user, id).await {
                Ok(m) => self.stamped(json(&m), &user, id).await,
                Err(e) => failed(&e),
            },
            ("GET", ["docs", id, "ops"]) => self.ops_get(&req, &user, id).await,
            ("POST", ["docs", id, "ops"]) => self.ops_post(&mut req, &user, id).await,
            ("GET", ["docs", id]) => match store.get(&user, id).await {
                Ok((m, b)) => self.stamped(file(&m, b), &user, id).await,
                Err(e) => failed(&e),
            },
            ("PUT", ["docs", id]) => {
                let (Some(name), Some(kind)) = (query(&req, "name"), query(&req, "kind")) else {
                    return reply(StatusCode::BAD_REQUEST, "name and kind are needed");
                };
                let if_match = req.header("if-match").map(str::to_string);
                let at = query(&req, "at").and_then(|a| a.parse::<u64>().ok());
                let bytes = match self.body(&mut req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                self.replace(
                    &user,
                    id,
                    Some((name, kind)),
                    &bytes,
                    at,
                    if_match.as_deref(),
                )
                .await
            }
            ("DELETE", ["docs", id]) => {
                let done = store.delete(&user, id).await;
                self.log.forget(&user, id).await;
                self.hub.touch(&user, id).await;
                done.map_or_else(|e| failed(&e), |()| Response::new(StatusCode::NO_CONTENT))
            }
            ("POST", ["docs", id, "share"]) => {
                let Some(rights) =
                    Rights::parse(&query(&req, "rights").unwrap_or_else(|| "view".into()))
                else {
                    return reply(StatusCode::BAD_REQUEST, "rights are view or edit");
                };
                let days: Option<u64> = query(&req, "days")
                    .and_then(|d| d.parse().ok())
                    .filter(|d| *d > 0);
                let (meta, _) = match store.get(&user, id).await {
                    Ok(x) => x,
                    Err(e) => return failed(&e),
                };
                let share = Share {
                    token: new_id(32),
                    owner: user.clone(),
                    doc: meta.id,
                    name: meta.name,
                    kind: meta.kind,
                    rights,
                    created: now(),
                    expires: days.map(|d| now() + d * 86_400),
                };
                match store.put_share(&share).await {
                    Ok(()) => json(&share).header("x-link", self.link_for(&req, &share)),
                    Err(e) => failed(&e),
                }
            }
            ("GET", ["shares"]) => store
                .shares_of(&user)
                .await
                .map_or_else(|e| failed(&e), |l| json(&l)),
            ("DELETE", ["shares", token]) => store
                .delete_share(&user, token)
                .await
                .map_or_else(|e| failed(&e), |()| Response::new(StatusCode::NO_CONTENT)),
            _ => reply(StatusCode::NOT_FOUND, "no such call"),
        }
    }
}

#[async_trait]
impl Endpoint for CloudApi {
    async fn call(&self, req: Request) -> Response {
        let response = self.handle(req).await;
        // Nothing here is for other sites' pages to read, and nothing is cached.
        response
            .header("x-content-type-options", "nosniff")
            .header("referrer-policy", "no-referrer")
    }
}
