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
            "x-name, x-kind, x-etag, x-rights, x-link",
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
                    file(&meta, bytes).header("x-rights", share.rights.word())
                }
                Err(e) => failed(&e),
            },
            "PUT" => {
                let if_match = req.header("if-match").map(str::to_string);
                let bytes = match self.body(req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                match office_cloud::save_share(&*self.store, token, &bytes, if_match.as_deref())
                    .await
                {
                    Ok(meta) => json(&meta),
                    Err(e) => failed(&e),
                }
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
        if let ["s", token, "meta"] = parts.as_slice() {
            return match office_cloud::share_meta(&*self.store, token).await {
                Ok((share, meta)) => json(&meta).header("x-rights", share.rights.word()),
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
                        file(&meta, bytes).header("x-rights", grant.rights.word())
                    }
                    Err(e) => failed(&e),
                }
            }
            ("GET", ["shared", owner, id, "meta"]) => {
                match office_cloud::granted_meta(store, &user, owner, id).await {
                    Ok((grant, meta)) => json(&meta).header("x-rights", grant.rights.word()),
                    Err(e) => failed(&e),
                }
            }
            ("PUT", ["shared", owner, id]) => {
                let if_match = req.header("if-match").map(str::to_string);
                let bytes = match self.body(&mut req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                office_cloud::save_granted(store, &user, owner, id, &bytes, if_match.as_deref())
                    .await
                    .map_or_else(|e| failed(&e), |m| json(&m))
            }
            ("GET", ["docs", id, "meta"]) => store
                .meta(&user, id)
                .await
                .map_or_else(|e| failed(&e), |m| json(&m)),
            ("GET", ["docs", id]) => store
                .get(&user, id)
                .await
                .map_or_else(|e| failed(&e), |(m, b)| file(&m, b)),
            ("PUT", ["docs", id]) => {
                let (Some(name), Some(kind)) = (query(&req, "name"), query(&req, "kind")) else {
                    return reply(StatusCode::BAD_REQUEST, "name and kind are needed");
                };
                let if_match = req.header("if-match").map(str::to_string);
                let bytes = match self.body(&mut req).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                store
                    .put(&user, Some(id), &name, &kind, &bytes, if_match.as_deref())
                    .await
                    .map_or_else(|e| failed(&e), |m| json(&m))
            }
            ("DELETE", ["docs", id]) => store
                .delete(&user, id)
                .await
                .map_or_else(|e| failed(&e), |()| Response::new(StatusCode::NO_CONTENT)),
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
