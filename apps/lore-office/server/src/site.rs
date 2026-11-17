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
        let launcher = UiApp::new(ui).page("/", move |_| {
            Page::new("lore-office", lore_launcher_app::launcher(&links))
                .stylesheet(&style)
                .state(links.to_json())
        });
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
        Context {
            api: format!("{}/api", self.base),
            user,
            denied,
            login,
            share,
        }
    }

    fn go(&self, to: &str) -> Response {
        Response::new(StatusCode::FOUND).header("location", to)
    }
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, mut req: Request) -> Response {
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
                let r = Self::rewritten(&mut req, "/", None);
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
