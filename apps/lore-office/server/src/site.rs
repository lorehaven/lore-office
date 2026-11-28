//! The whole suite behind one address: the launcher, the four apps, the assets, sign-in and the cloud API.

#![allow(clippy::type_complexity, clippy::unused_self)]

use crate::{CloudApi, denied_user, user_of};
use office_cloud::Storage;
use office_cloud_ui::{Context, SharedWith};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::sso_client::SsoConfig;
use quench_core::async_trait;
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::body::InboundBody;
use quench_http::prelude::{Endpoint, Request, Response, http::StatusCode};
use std::sync::Arc;

/// The cookie that remembers which page a person was on when they went to sign in. The sign-in library always ends at
/// `/ui/home`, so that route reads this and sends them back.
const RETURN_COOKIE: &str = "lore_return";

/// Whether `to` is a page of this service a person may be sent back to: under the base path, no way out of it.
#[must_use]
pub fn valid_return(base: &str, to: &str) -> bool {
    let rooted = format!("{base}/");
    to.starts_with(&rooted)
        && to.len() <= 300
        && !to.contains("//")
        && !to.contains('\\')
        && !to.chars().any(char::is_control)
}

/// The `set-cookie` value that remembers `to` for ten minutes.
#[must_use]
pub fn return_cookie(to: &str) -> String {
    format!(
        "{RETURN_COOKIE}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=600",
        quench_auth::domain::percent::encode(to)
    )
}

/// The page remembered in a request's `cookie` header, if it is still one the service allows.
#[must_use]
pub fn returned_to(base: &str, cookies: &str) -> Option<String> {
    let raw = cookies.split(';').find_map(|c| {
        let (name, value) = c.trim().split_once('=')?;
        (name == RETURN_COOKIE).then_some(value)
    })?;
    quench_auth::domain::percent::decode(raw).filter(|to| valid_return(base, to))
}

/// The compiled wasm of each page.
#[derive(Clone, Debug, Default)]
pub struct Modules {
    pub launcher: Vec<u8>,
    pub docs: Vec<u8>,
    pub sheets: Vec<u8>,
    pub notes: Vec<u8>,
    pub pdf: Vec<u8>,
}

/// What the service is made of.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct SiteConfig {
    pub modules: Modules,
    pub store: Arc<dyn Storage>,
    pub jwt: JwtConfig,
    /// `None`: no gatehouse, so no sign-in route.
    pub sso: Option<SsoConfig>,
    /// Where the service is served (`/lore-office`, or `""`).
    pub base: String,
    pub max_upload: usize,
    pub dev: bool,
    /// Where warehouse is (`https://host/warehouse`): what `/open` reads a person's files from. `None`: there is no `/open`.
    pub warehouse: Option<String>,
    /// Where gatehouse is, for the links of the bar every page has when someone is signed in.
    pub gatehouse: String,
}

const APPS: [&str; 4] = ["docs", "sheets", "notes", "pdf"];

/// The request header the dispatcher puts the page's context in (always replaced, never the client's).
const CONTEXT: &str = "x-lore-context";

pub struct Site {
    launcher: UiApp,
    apps: Vec<(&'static str, UiApp)>,
    cloud: CloudApi,
    sso: Option<SsoConfig>,
    base: String,
    files: Option<office_cloud::WarehouseFiles>,
    gatehouse: String,
    /// Sessions renewed a moment ago, by the refresh token they came from: a page and its API calls arrive together, and
    /// the realm accepts a refresh token once.
    renewed: tokio::sync::Mutex<Vec<(std::time::Instant, String, Renewed)>>,
    /// People a renewed session did not let in, and when that was found.
    refused: tokio::sync::Mutex<Vec<(std::time::Instant, String)>>,
}

/// How long the session of someone without access is not renewed again. A grant given meanwhile takes this long to show
/// (signing out and in shows it at once).
const NO_ACCESS: std::time::Duration = std::time::Duration::from_secs(60);

/// A fresh pair of tokens and the `set-cookie` values that keep them.
#[derive(Clone)]
struct Renewed {
    access: String,
    cookies: Vec<String>,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Site")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

fn loader(icon: &str) -> String {
    format!(
        "{}\n{}\n{}",
        office_theme::icon_script(icon),
        office_theme::SCRIPT,
        quench_ui_web::QUENCH_JS
    )
}

/// Two JSON objects as one: the launcher's page carries both its links and the page's context.
fn with_context(links: &str, context: &str) -> String {
    match (links.strip_suffix('}'), context.strip_prefix('{')) {
        (Some(l), Some(c)) if c != "}" => format!("{l},{c}"),
        _ => links.to_string(),
    }
}

/// The start page, with the same top bar as the apps: its page carries both the links to them and the page's context.
fn launcher_page(ui: Ui, style: String, links: lore_launcher_app::Links) -> UiApp {
    UiApp::new(ui).page("/", move |req| {
        let context = context_of(req);
        // As for the apps: set and used with no `.await` between.
        office_cloud_ui::set_server_context(context.clone());
        let _ = office_cloud_ui::setup();
        Page::new("lore-office", lore_launcher_app::launcher(&links))
            .stylesheet(&style)
            .state(with_context(&links.to_json(), &context.to_json()))
    })
}

fn context_of(req: &Request) -> Context {
    req.header(CONTEXT)
        .map_or_else(Context::default, Context::from_json)
}

impl Site {
    #[must_use]
    pub fn new(config: SiteConfig) -> Self {
        let base = config.base.trim_end_matches('/').to_string();
        let ui_for = |icon: &str, wasm: &[u8], css: String| {
            let mut ui = Ui::with_script(loader(icon), wasm.to_vec());
            if config.dev {
                ui = ui.dev_reload();
            }
            let style = ui.add_asset("site.css", css, "text/css");
            (ui, style)
        };
        let theme = |own: &str| format!("{}\n{}\n{}", office_theme::STYLE, office_ui::STYLE, own);
        let links = lore_launcher_app::Links {
            docs: format!("{base}/docs"),
            sheets: format!("{base}/sheets"),
            notes: format!("{base}/notes"),
            pdf: format!("{base}/pdf"),
        };
        let (ui, style) = ui_for(
            include_str!("../../../lore-launcher/icon.svg"),
            &config.modules.launcher,
            theme(lore_launcher_app::STYLE),
        );
        let launcher = launcher_page(ui, style, links);
        let mut apps = Vec::new();
        let table: [(
            &str,
            &str,
            &Vec<u8>,
            String,
            fn() -> quench_ui::prelude::View,
        ); 4] = [
            (
                "docs",
                include_str!("../../../lore-docs/icon.svg"),
                &config.modules.docs,
                docs_app::STYLE.to_string(),
                lore_docs_app::app,
            ),
            (
                "sheets",
                include_str!("../../../lore-sheets/icon.svg"),
                &config.modules.sheets,
                sheets_app::STYLE.to_string(),
                lore_sheets_app::app,
            ),
            (
                "notes",
                include_str!("../../../lore-notes/icon.svg"),
                &config.modules.notes,
                notes_app::STYLE.to_string(),
                lore_notes_app::app,
            ),
            (
                "pdf",
                include_str!("../../../lore-pdf/icon.svg"),
                &config.modules.pdf,
                pdf_app::STYLE.to_string(),
                lore_pdf_app::app,
            ),
        ];
        for (name, icon, wasm, css, build) in table {
            let (ui, style) = ui_for(icon, wasm, theme(&css));
            let title = format!("lore-{name}");
            let app = UiApp::new(ui).page("/", move |req| {
                let context = context_of(req);
                // The page is built and rendered with no `.await` after this, so the thread's value is the request's.
                office_cloud_ui::set_server_context(context.clone());
                Page::new(&title, build())
                    .stylesheet(&style)
                    .state(context.to_json())
            });
            apps.push((name, app));
        }
        let cloud = CloudApi {
            log: office_cloud::OpLog::new(),
            hub: office_cloud::Hub::new(),
            store: config.store,
            jwt: config.jwt,
            base: base.clone(),
            max_upload: config.max_upload,
            login: format!("{base}/ui/login"),
        };
        Self {
            launcher,
            apps,
            cloud,
            sso: config.sso,
            base,
            files: config
                .warehouse
                .as_deref()
                .filter(|w| !w.is_empty())
                .and_then(|w| office_cloud::WarehouseFiles::new(w).ok()),
            gatehouse: config.gatehouse.trim_end_matches('/').to_string(),
            renewed: tokio::sync::Mutex::default(),
            refused: tokio::sync::Mutex::default(),
        }
    }

    /// The same request for another path, with the page's context attached.
    fn rewritten(req: &mut Request, path: &str, context: Option<&Context>) -> Request {
        let uri = path.parse().unwrap_or_else(|_| req.uri().clone());
        let mut headers = req.headers().clone();
        headers.remove(CONTEXT);
        if let Some(c) = context
            && let Ok(v) = c.to_json().parse()
        {
            headers.insert(CONTEXT, v);
        }
        let body = req.take_body();
        Request::new(
            req.method().clone(),
            uri,
            headers,
            body,
            req.container().clone(),
        )
    }

    async fn context(&self, req: &Request, app: &str) -> Context {
        let user = user_of(req, &self.cloud.jwt).await;
        let denied = if user.is_none() {
            denied_user(req, &self.cloud.jwt).await
        } else {
            None
        };
        let login_back = format!("{}/{app}", self.base);
        // Without gatehouse there is no signing in: the cloud is the local person's, and the page has no link to follow.
        let login = if self.sso.is_some() {
            format!("{}/ui/login?redirect={login_back}", self.base)
        } else {
            String::new()
        };
        let share = match req
            .uri()
            .query()
            .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("share=")))
        {
            Some(token) => match self.cloud.store.share(token).await {
                Ok(s) if !s.expired(office_cloud::now()) && s.kind == app => Some(SharedWith {
                    token: token.to_string(),
                    edit: s.rights == office_cloud::Rights::Edit,
                }),
                _ => None,
            },
            None => None,
        };
        // A document made by `/open`, to open at once. Its id is only ever one of the person's own: the API checks it.
        let open = req
            .uri()
            .query()
            .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("doc=")))
            .filter(|id| {
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
            .filter(|_| user.is_some())
            .map(str::to_string);
        Context {
            api: format!("{}/api", self.base),
            user,
            denied,
            login,
            share,
            open,
            gatehouse: self.gatehouse.clone(),
        }
    }

    /// `/open?app=docs|sheets&storage=&path=`: copies a file of the person's from warehouse into their cloud here and goes
    /// to the app with it open. The file is read with the person's own token, so only what they may read comes through.
    async fn open_from_warehouse(&self, req: &Request) -> Response {
        use quench_auth::domain::percent::decode;
        let arg = |key: &str| {
            req.uri().query().and_then(|q| {
                q.split('&').find_map(|p| {
                    let (k, v) = p.split_once('=')?;
                    (k == key).then(|| decode(v).unwrap_or_default())
                })
            })
        };
        let (Some(app), Some(storage), Some(path)) = (arg("app"), arg("storage"), arg("path"))
        else {
            return Response::text(StatusCode::BAD_REQUEST, "app, storage and path are needed");
        };
        if !APPS.contains(&app.as_str()) {
            return Response::text(StatusCode::BAD_REQUEST, "no such app");
        }
        let Some(files) = &self.files else {
            return Response::text(StatusCode::NOT_FOUND, "there is no warehouse to open from");
        };
        let Some(user) = user_of(req, &self.cloud.jwt).await else {
            // Sign in, and come back to this very address.
            let back = req.uri().to_string();
            return if self.sso.is_some() && valid_return(&self.base, &back) {
                self.go(&format!("{}/ui/login?redirect={back}", self.base))
            } else {
                Response::text(StatusCode::UNAUTHORIZED, "sign in first")
            };
        };
        let token = if self.cloud.jwt.auth_enabled {
            quench_auth::http::domain::cookies::cookie_value(
                req,
                &quench_auth::domain::realm::session_cookie_name(),
            )
            .unwrap_or_default()
        } else {
            String::new()
        };
        let bytes = match files.read(&token, &storage, &path).await {
            Ok(b) => b,
            Err(office_cloud::Error::Denied) => {
                return Response::text(
                    StatusCode::FORBIDDEN,
                    "the warehouse does not let you read that file",
                );
            }
            Err(office_cloud::Error::NotFound) => {
                return Response::text(StatusCode::NOT_FOUND, "no such file in the warehouse");
            }
            Err(e) => {
                quench_log::error!("open from warehouse: {e}");
                return Response::text(StatusCode::BAD_GATEWAY, "the warehouse did not answer");
            }
        };
        let name = path.rsplit('/').next().unwrap_or("document");
        match self
            .cloud
            .store
            .put(&user, None, name, &app, &bytes, None)
            .await
        {
            Ok(meta) => self.go(&format!("{}/{app}?doc={}", self.base, meta.id)),
            Err(office_cloud::Error::Quota { .. }) => Response::text(
                StatusCode::INSUFFICIENT_STORAGE,
                "your lore-office storage is full",
            ),
            Err(e) => {
                quench_log::error!("open from warehouse: {e}");
                Response::text(StatusCode::BAD_GATEWAY, "the document could not be kept")
            }
        }
    }

    fn go(&self, to: &str) -> Response {
        Response::new(StatusCode::FOUND).header("location", to)
    }
}

impl Site {
    /// A session this service cannot use, renewed from the refresh token as the forge services do.
    ///
    /// That is an access token that ran out, or one made for another service (signing in there gives a token good for that
    /// service only). The request goes on with the new access token and the response sets the new cookies, so a person who
    /// is signed in at the realm is signed in here too, without being sent to sign in. Someone the renewed token does not
    /// let in either has no access: their session is then left alone for [`NO_ACCESS`].
    async fn renew(&self, req: Request) -> (Request, Vec<String>) {
        use quench_auth::domain::{realm, sso_client};
        use quench_auth::http::domain::cookies::cookie_value;
        let jwt = &self.cloud.jwt;
        if !jwt.auth_enabled || user_of(&req, jwt).await.is_some() {
            return (req, Vec::new());
        }
        if let Some(user) = denied_user(&req, jwt).await {
            let mut refused = self.refused.lock().await;
            refused.retain(|(at, _)| at.elapsed() < NO_ACCESS);
            if refused.iter().any(|(_, u)| *u == user) {
                return (req, Vec::new());
            }
        }
        let Some(refresh) = cookie_value(&req, &realm::refresh_cookie_name()) else {
            return (req, Vec::new());
        };
        let renewed = {
            let mut recent = self.renewed.lock().await;
            recent.retain(|(at, ..)| at.elapsed() < std::time::Duration::from_secs(30));
            if let Some((.., r)) = recent.iter().find(|(_, from, _)| *from == refresh) {
                r.clone()
            } else {
                let Some(tokens) = sso_client::refresh(&refresh).await else {
                    return (req, Vec::new());
                };
                let r = Renewed {
                    access: tokens.access_token.clone(),
                    cookies: vec![
                        realm::session_cookie(tokens.access_token).to_string(),
                        realm::refresh_cookie(tokens.refresh_token).to_string(),
                    ],
                };
                recent.push((std::time::Instant::now(), refresh, r.clone()));
                r
            }
        };
        if let Ok(claims) = jwt.decode_claims(&renewed.access).await
            && !claims.allows(&jwt.service_name)
        {
            self.refused
                .lock()
                .await
                .push((std::time::Instant::now(), claims.sub));
        }
        // The request carries the new access token in place of the old one.
        let (session, refresh_name) = (realm::session_cookie_name(), realm::refresh_cookie_name());
        let kept: Vec<String> = req
            .header("cookie")
            .unwrap_or_default()
            .split(';')
            .map(str::trim)
            .filter(|c| {
                let name = c.split('=').next().unwrap_or_default();
                name != session && name != refresh_name && !c.is_empty()
            })
            .map(str::to_string)
            .collect();
        let cookie = [kept, vec![format!("{session}={}", renewed.access)]]
            .concat()
            .join("; ");
        let mut headers = req.headers().clone();
        if let Ok(v) = cookie.parse() {
            headers.insert("cookie", v);
        }
        let mut req = req;
        let renewed_req = Request::new(
            req.method().clone(),
            req.uri().clone(),
            headers,
            req.take_body(),
            req.container().clone(),
        );
        (renewed_req, renewed.cookies)
    }
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, req: Request) -> Response {
        let (req, cookies) = self.renew(req).await;
        let mut response = self.route(req).await;
        for c in cookies {
            response = response.append_header("set-cookie", c);
        }
        response
    }
}

impl Site {
    async fn route(&self, mut req: Request) -> Response {
        let full = req.uri().path().to_string();
        let path = if self.base.is_empty() {
            full.as_str()
        } else {
            full.strip_prefix(self.base.as_str())
                .filter(|r| r.is_empty() || r.starts_with('/'))
                .unwrap_or(full.as_str())
        };
        let path = if path.is_empty() { "/" } else { path };
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let response = match segments.as_slice() {
            ["health"] | ["health", _] => Response::text(StatusCode::OK, "ok"),
            ["_quench", ..] => {
                // Each app's assets are hashed, so the one whose hash it is answers.
                let mut found = None;
                let own = Self::rewritten(&mut req, &full, None);
                for app in std::iter::once(&self.launcher).chain(self.apps.iter().map(|(_, a)| a)) {
                    let probe = Request::new(
                        own.method().clone(),
                        own.uri().clone(),
                        own.headers().clone(),
                        InboundBody::empty(),
                        own.container().clone(),
                    );
                    let r = app.call(probe).await;
                    if r.status() != StatusCode::NOT_FOUND {
                        found = Some(r);
                        break;
                    }
                }
                found.unwrap_or_else(Response::not_found)
            }
            ["api", ..] => self.cloud.call(req).await,
            ["open"] => self.open_from_warehouse(&req).await,
            ["ui", "login"] => match &self.sso {
                Some(sso) => {
                    let mut response =
                        quench_auth::http::routers::ui::pages::auth::login_delegation(&req, sso)
                            .await;
                    // Remember the page they came from: the library ends at /ui/home, which sends them back to it.
                    if let Some(to) =
                        quench_auth::http::routers::ui::pages::auth::redirect_target(&req)
                            .filter(|to| valid_return(&self.base, to))
                    {
                        response = response.append_header("set-cookie", return_cookie(&to));
                    }
                    response
                }
                None => Response::text(StatusCode::NOT_FOUND, "sign-in is not set up"),
            },
            ["ui", "auth", "callback"] => match &self.sso {
                Some(sso) => {
                    quench_auth::http::routers::ui::pages::auth::auth_callback(&req, sso).await
                }
                None => Response::text(StatusCode::NOT_FOUND, "sign-in is not set up"),
            },
            ["ui", "logout"] => {
                quench_auth::http::routers::ui::pages::auth::logout_delegation(&req)
            }
            ["ui", "home"] => {
                let back = req
                    .headers()
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|c| returned_to(&self.base, c));
                // Back to the page they were on (and the note is thrown away), else the launcher.
                back.map_or_else(
                    || self.go(&format!("{}/", self.base)),
                    |to| {
                        self.go(&to).append_header(
                            "set-cookie",
                            format!(
                                "{RETURN_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0"
                            ),
                        )
                    },
                )
            }
            [] => {
                let context = self.context(&req, "").await;
                let r = Self::rewritten(&mut req, "/", Some(&context));
                self.launcher.call(r).await
            }
            [app] if APPS.contains(app) => {
                let context = self.context(&req, app).await;
                let r = Self::rewritten(&mut req, "/", Some(&context));
                let Some((_, ui)) = self.apps.iter().find(|(n, _)| n == app) else {
                    return Response::not_found();
                };
                ui.call(r).await
            }
            _ => Response::not_found(),
        };
        response.header("x-content-type-options", "nosniff")
    }
}
