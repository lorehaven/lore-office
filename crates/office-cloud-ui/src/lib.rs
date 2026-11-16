//! The cloud on the screen.

#![allow(
    clippy::module_name_repetitions,
    clippy::type_complexity,
    clippy::format_push_string,
    clippy::missing_const_for_fn,
    clippy::semicolon_if_nothing_returned,
    clippy::redundant_clone,
    clippy::needless_collect,
    clippy::or_fun_call,
    missing_debug_implementations
)]

use office_cloud::{DocMeta, Share, Usage};
use office_ui::{MenuItem, t};
use quench_serde::{Value, json};
use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// What the server told the page (`Page::state`): where the API is, who is signed in, and the link the page was opened by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// `""` when there is no cloud: the apps then have no cloud menu at all.
    pub api: String,
    pub user: Option<String>,
    /// Where signing in starts.
    pub login: String,
    pub share: Option<SharedWith>,
}

/// A page opened from a share link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedWith {
    pub token: String,
    pub edit: bool,
}

impl Context {
    /// From the page state's JSON; anything missing or unreadable is "no cloud".
    #[must_use]
    pub fn from_json(text: &str) -> Self {
        let Ok(v) = json::parse(text) else {
            return Self::default();
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            api: s("api").unwrap_or_default(),
            user: s("user"),
            login: s("login").unwrap_or_default(),
            share: v.get("share").and_then(|sh| {
                Some(SharedWith {
                    token: sh.get("token")?.as_str()?.to_string(),
                    edit: sh.get("rights")?.as_str()? == "edit",
                })
            }),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = format!(
            "{{\"api\":{},\"login\":{}",
            json::to_string(self.api.as_str()),
            json::to_string(self.login.as_str())
        );
        if let Some(u) = &self.user {
            out.push_str(&format!(",\"user\":{}", json::to_string(u.as_str())));
        }
        if let Some(sh) = &self.share {
            out.push_str(&format!(
                ",\"share\":{{\"token\":{},\"rights\":\"{}\"}}",
                json::to_string(sh.token.as_str()),
                if sh.edit { "edit" } else { "view" }
            ));
        }
        out.push('}');
        out
    }
}

/// A response: the status, the headers (lower-cased names) and the body.
#[derive(Clone, Debug, Default)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// How a request reaches the server.
pub trait Transport {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        done: Box<dyn FnOnce(Reply)>,
    );
}

/// What the app gives the cloud.
#[derive(Clone)]
pub struct Binding {
    /// `docs`, `sheets`, `notes` or `pdf`.
    pub kind: &'static str,
    /// The open document as a file: its name (with extension) and bytes; `None` when there is none to save.
    pub export: Rc<dyn Fn() -> Option<(String, Vec<u8>)>>,
    /// Open a file the cloud gave.
    pub import: Rc<dyn Fn(&str, &[u8]) -> Result<(), String>>,
}

/// Which part of the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Panel {
    Closed,
    Documents,
    /// Sharing the document with this id.
    Sharing(String),
}

/// The document open now and where it is in the cloud, so a save goes to the same place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    pub id: String,
    pub etag: String,
    /// Opened through a link (and saves through it).
    pub token: Option<String>,
}

#[derive(Clone)]
pub struct Cloud {
    pub context: Context,
    pub panel: Signal<Panel>,
    pub docs: Signal<Vec<DocMeta>>,
    pub shares: Signal<Vec<Share>>,
    pub usage: Signal<Option<Usage>>,
    pub status: Signal<String>,
    pub opened: Signal<Option<Opened>>,
    /// The link just made, as an address for the person to copy.
    pub link: Signal<String>,
    pub rights_edit: Signal<bool>,
    /// Days until a new link stops working; empty means never.
    pub days: Signal<String>,
    /// The lore-office server the person means to use (an address; empty: the one this page came from).
    pub server: Signal<String>,
    pub busy: Signal<bool>,
    binding: Rc<RefCell<Option<Binding>>>,
    transport: Rc<dyn Transport>,
}

thread_local! {
    static CURRENT: RefCell<Option<Cloud>> = const { RefCell::new(None) };
}

impl Cloud {
    #[must_use]
    pub fn new(context: Context, transport: Rc<dyn Transport>) -> Self {
        Self {
            context,
            panel: create_signal(Panel::Closed),
            docs: create_signal(Vec::new()),
            shares: create_signal(Vec::new()),
            usage: create_signal(None),
            status: create_signal(String::new()),
            opened: create_signal(None),
            link: create_signal(String::new()),
            rights_edit: create_signal(false),
            days: create_signal(String::new()),
            server: create_signal(saved_server()),
            busy: create_signal(false),
            binding: Rc::default(),
            transport,
        }
    }

    /// Make this the cloud the menus and the overlay show (one for the page).
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    pub fn bind(&self, binding: Binding) {
        *self.binding.borrow_mut() = Some(binding);
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        !self.context.api.is_empty()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.context.api)
    }

    fn call(
        &self,
        method: &str,
        path: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        done: impl FnOnce(&Self, Reply) + 'static,
    ) {
        self.busy.set(true);
        let me = self.clone();
        self.transport.send(
            method,
            &self.url(path),
            headers,
            body,
            Box::new(move |reply| {
                me.busy.set(false);
                done(&me, reply);
            }),
        );
    }

    fn failed(&self, reply: &Reply) {
        let why = String::from_utf8_lossy(&reply.body).to_string();
        let words = match reply.status {
            0 => t("The cloud cannot be reached"),
            401 => t("Sign in first"),
            403 => t("This link only lets you view"),
            404 => t("Not found"),
            409 => t("Someone else changed this document: open it again, or save it as a new one"),
            413 | 507 => t("There is no room left in your cloud storage"),
            s => format!(
                "{} ({s}) {}",
                t("The cloud said no"),
                why.chars().take(120).collect::<String>()
            ),
        };
        self.status.set(words);
    }

    /// Show the documents panel (and fetch the list when signed in).
    pub fn open_panel(&self) {
        self.panel.set(Panel::Documents);
        if self.context.user.is_some() {
            self.refresh();
        }
    }

    /// Save the open document as a new one, whatever it was opened from (what to do after a conflict).
    pub fn save_copy(&self) {
        self.opened.set(None);
        self.save();
    }

    pub fn close(&self) {
        self.panel.set(Panel::Closed);
    }

    /// Fetch the list of documents (of this app's kind) and the usage.
    pub fn refresh(&self) {
        let kind = self.binding.borrow().as_ref().map_or("", |b| b.kind);
        self.call(
            "GET",
            &format!("/docs?kind={kind}"),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200 {
                    if let Ok(list) =
                        json::from_str::<Vec<DocMeta>>(&String::from_utf8_lossy(&r.body))
                    {
                        me.docs.set(list);
                    }
                } else {
                    me.failed(&r);
                }
            },
        );
        self.call("GET", "/usage", Vec::new(), Vec::new(), |me, r| {
            if r.status == 200 {
                me.usage
                    .set(json::from_str::<Usage>(&String::from_utf8_lossy(&r.body)).ok());
            }
        });
    }

    /// Save the open document: over the one it came from, else as a new one.
    pub fn save(&self) {
        let Some(b) = self.binding.borrow().clone() else {
            return;
        };
        let Some((name, bytes)) = (b.export)() else {
            self.status.set(t("There is nothing to save"));
            return;
        };
        let opened = self.opened.get_untracked();
        // A name with letters outside ASCII is sent as UTF-8 bytes.
        let enc: String = name
            .bytes()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.') {
                    char::from(c).to_string()
                } else {
                    format!("%{c:02X}")
                }
            })
            .collect();
        let (path, headers) = match &opened {
            Some(o) if o.token.is_some() => (
                format!("/s/{}", o.token.clone().unwrap_or_default()),
                vec![("if-match".to_string(), o.etag.clone())],
            ),
            Some(o) => (
                format!("/docs/{}?name={enc}&kind={}", o.id, b.kind),
                vec![("if-match".to_string(), o.etag.clone())],
            ),
            None => (format!("/docs?name={enc}&kind={}", b.kind), Vec::new()),
        };
        let token = opened.as_ref().and_then(|o| o.token.clone());
        self.call("PUT", &path, headers, bytes, move |me, r| {
            if r.status == 200 {
                if let Ok(meta) = json::from_str::<DocMeta>(&String::from_utf8_lossy(&r.body)) {
                    me.opened.set(Some(Opened {
                        id: meta.id,
                        etag: meta.etag,
                        token,
                    }));
                    me.status.set(t("Saved to the cloud"));
                    if me.panel.get_untracked() == Panel::Documents {
                        me.refresh();
                    }
                }
            } else {
                me.failed(&r);
            }
        });
    }

    /// Open document `id` of the person.
    pub fn open_doc(&self, id: &str) {
        let id = id.to_string();
        self.call(
            "GET",
            &format!("/docs/{id}"),
            Vec::new(),
            Vec::new(),
            move |me, r| {
                if r.status != 200 {
                    me.failed(&r);
                    return;
                }
                let name = r.header("x-name").unwrap_or("document").to_string();
                let etag = r.header("x-etag").unwrap_or_default().to_string();
                let Some(b) = me.binding.borrow().clone() else {
                    return;
                };
                match (b.import)(&name, &r.body) {
                    Ok(()) => {
                        me.opened.set(Some(Opened {
                            id,
                            etag,
                            token: None,
                        }));
                        me.panel.set(Panel::Closed);
                        me.status
                            .set(format!("{} {name}", t("Opened from the cloud:")));
                    }
                    Err(e) => me.status.set(e),
                }
            },
        );
    }

    /// Open what the share link of this page names.
    pub fn open_share(&self) {
        let Some(share) = self.context.share.clone() else {
            return;
        };
        self.call(
            "GET",
            &format!("/s/{}", share.token),
            Vec::new(),
            Vec::new(),
            move |me, r| {
                if r.status != 200 {
                    me.failed(&r);
                    return;
                }
                let name = r.header("x-name").unwrap_or("document").to_string();
                let etag = r.header("x-etag").unwrap_or_default().to_string();
                let Some(b) = me.binding.borrow().clone() else {
                    return;
                };
                match (b.import)(&name, &r.body) {
                    Ok(()) => {
                        me.opened.set(Some(Opened {
                            id: String::new(),
                            etag,
                            token: Some(share.token.clone()),
                        }));
                        me.status.set(format!(
                            "{} {name} ({})",
                            t("Opened from a link:"),
                            if share.edit {
                                t("you can edit")
                            } else {
                                t("view only")
                            }
                        ));
                    }
                    Err(e) => me.status.set(e),
                }
            },
        );
    }

    pub fn delete_doc(&self, id: &str) {
        self.call(
            "DELETE",
            &format!("/docs/{id}"),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200 || r.status == 204 {
                    me.refresh();
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    /// Make a link to document `id` with the rights chosen.
    pub fn share(&self, id: &str, edit: bool) {
        let rights = if edit { "edit" } else { "view" };
        let days = self.days.get_untracked();
        let ends = days
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|d| *d > 0)
            .map_or_else(String::new, |d| format!("&days={d}"));
        self.call(
            "POST",
            &format!("/docs/{id}/share?rights={rights}{ends}"),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200
                    && let Ok(share) = json::from_str::<Share>(&String::from_utf8_lossy(&r.body))
                {
                    me.link.set(
                        r.header("x-link")
                            .map_or_else(|| share.token.clone(), str::to_string),
                    );
                    me.status.set(t("The link is ready: copy it below"));
                    me.list_shares();
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    pub fn list_shares(&self) {
        self.call("GET", "/shares", Vec::new(), Vec::new(), |me, r| {
            if r.status == 200
                && let Ok(list) = json::from_str::<Vec<Share>>(&String::from_utf8_lossy(&r.body))
            {
                me.shares.set(list);
            }
        });
    }

    pub fn withdraw(&self, token: &str) {
        self.call(
            "DELETE",
            &format!("/shares/{token}"),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200 || r.status == 204 {
                    me.list_shares();
                } else {
                    me.failed(&r);
                }
            },
        );
    }
}

/// The cloud of this page, if one was installed.
#[must_use]
pub fn current() -> Option<Cloud> {
    CURRENT.with(|c| c.borrow().clone())
}

/// The File menu's items with the cloud's after them (when the page has a cloud).
#[must_use]
pub fn with_menu(mut items: Vec<MenuItem>) -> Vec<MenuItem> {
    items.extend(menu_items());
    items
}

/// What the File menu gets: nothing when the page has no cloud.
#[must_use]
pub fn menu_items() -> Vec<MenuItem> {
    let Some(cloud) = current() else {
        return Vec::new();
    };
    let (open, save, copy) = (cloud.clone(), cloud.clone(), cloud.clone());
    let has_account = cloud.context.user.is_some();
    let signed_in =
        cloud.context.user.is_some() || cloud.context.share.as_ref().is_some_and(|s| s.edit);
    let mut items = vec![
        MenuItem::Separator,
        MenuItem::action("Cloud\u{2026}", "", move || open.open_panel()),
    ];
    if !cloud.enabled() {
        return items;
    }
    if signed_in {
        items.push(MenuItem::action("Save to the cloud", "", move || {
            save.save()
        }));
    }
    if has_account {
        items.push(MenuItem::action(
            "Save a copy to the cloud",
            "",
            move || {
                copy.save_copy();
            },
        ));
    }
    items
}

/// A "Cloud" button for the top line (nothing when the page has no cloud): opens the person's documents.
#[must_use]
pub fn cloud_button() -> View {
    let Some(cloud) = current() else {
        return View::Empty;
    };
    button()
        .attr("type", "button")
        .attr("title", t("Your documents in the cloud"))
        .class("tool cloud-button")
        .child(span().class("icon icon-cloud"))
        .child(span().text(t("Cloud")))
        .on_click(move || cloud.open_panel())
        .into_view()
}

/// The server kept in the browser's storage, if any.
fn saved_server() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        quench_ui_web::browser::storage_get("cloud.server").unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn keep_server(address: &str) {
    #[cfg(target_arch = "wasm32")]
    quench_ui_web::browser::storage_set("cloud.server", Some(address).filter(|a| !a.is_empty()));
}

impl Cloud {
    /// Where this server's pages start (`/lore-office/`), from the API's address.
    fn home(&self) -> String {
        self.context
            .api
            .strip_suffix("api")
            .unwrap_or("/")
            .to_string()
    }
}

fn button_for(label: &str, tip: &str, action: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .class("tool")
        .attr("title", t(tip))
        .text(t(label))
        .on_click(action)
}

fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{:.0} kB", (b / 1e3).max(1.0))
    }
}

/// The cloud's own screen parts: a banner when the page was opened by a link, and the panel when it is asked for.
#[must_use]
pub fn overlay() -> View {
    // The same two slots whether or not there is a cloud, so the server's page and the browser's agree.
    let (panel, banner) = (current(), current().filter(Cloud::enabled));
    let banner_view = {
        let c = banner;
        View::dynamic(move || {
            let Some(c) = c.clone() else {
                return View::Empty;
            };
            let msg = c.status.get();
            let shared = c.context.share.clone();
            if msg.is_empty() && shared.is_none() {
                return View::Empty;
            }
            let c2 = c.clone();
            div()
                .class("cloud-banner")
                .attr("role", "status")
                .child(span().text(if msg.is_empty() {
                    shared.map_or(String::new(), |s| {
                        if s.edit {
                            t("Opened from a link: you can edit")
                        } else {
                            t("Opened from a link: view only")
                        }
                    })
                } else {
                    msg
                }))
                .child(button_for("Close", "Hide this", move || {
                    c2.status.set(String::new())
                }))
                .into_view()
        })
    };
    let panel_view = View::dynamic(move || {
        let Some(c) = panel.clone() else {
            return View::Empty;
        };
        let which = c.panel.get();
        if which == Panel::Closed {
            return View::Empty;
        }
        let close = c.clone();
        let body = match which {
            Panel::Closed => View::Empty,
            Panel::Documents => documents(&c),
            Panel::Sharing(id) => sharing(&c, &id),
        };
        div()
            .class("cloud-backdrop")
            .child(
                div()
                    .class("cloud-panel")
                    .attr("role", "dialog")
                    .attr("aria-label", t("Cloud"))
                    .child(
                        div()
                            .class("cloud-head")
                            .child(h2().text(t("Cloud")))
                            .child(button_for("Close", "Close", move || close.close())),
                    )
                    .child(body),
            )
            .into_view()
    });
    div()
        .class("cloud-root")
        .child(banner_view)
        .child(panel_view)
        .into_view()
}

/// Where the person is signed in and on which server: sign in or out, and move to another lore-office server.
fn account(c: &Cloud) -> View {
    let mut bar = div().class("cloud-account");
    let home = c.home();
    if c.enabled() {
        let who = match (&c.context.user, c.context.login.is_empty()) {
            (Some(u), _) => format!("{} {u}", t("Signed in as")),
            (None, false) => t("Not signed in"),
            (None, true) => t("Local use: no sign-in on this server"),
        };
        bar = bar.child(span().class("cloud-who").text(who));
        if c.context.user.is_some() && !c.context.login.is_empty() {
            bar = bar.child(
                a().class("tool")
                    .attr("href", format!("{home}ui/logout"))
                    .text(t("Sign out")),
            );
        } else if c.context.user.is_none() && !c.context.login.is_empty() {
            bar = bar.child(
                a().class("tool primary")
                    .attr("href", c.context.login.clone())
                    .text(t("Sign in")),
            );
        }
    } else {
        bar = bar.child(
            span()
                .class("cloud-who")
                .text(t("This app is not connected to a lore-office server.")),
        );
    }
    let (typed, link, kind) = (
        c.server.clone(),
        c.server.clone(),
        c.binding.borrow().as_ref().map_or("docs", |b| b.kind),
    );
    bar.child(
        label()
            .class("cloud-server")
            .child(span().text(t("Server")))
            .child(
                input()
                    .attr("type", "url")
                    .attr("placeholder", "https://example.org/lore-office")
                    .attr("aria-label", t("Address of the lore-office server"))
                    .attr("value", c.server.get_untracked())
                    .on_input(move |v| {
                        typed.set(v.trim().to_string());
                        keep_server(v.trim());
                    }),
            )
            .child(
                a().class("tool")
                    .attr_fn("href", move || {
                        let s = link.get();
                        if s.is_empty() {
                            String::new()
                        } else {
                            format!("{}/{kind}", s.trim_end_matches('/'))
                        }
                    })
                    .text(t("Connect")),
            ),
    )
    .into_view()
}

fn documents(c: &Cloud) -> View {
    if !c.enabled() {
        return div().class("cloud-body").child(account(c)).into_view();
    }
    if c.context.user.is_none() {
        return div()
            .class("cloud-body")
            .child(account(c))
            .child(p().text(t(
                "Sign in to keep your documents in the cloud and to share them.",
            )))
            .into_view();
    }
    let (save, list) = (c.clone(), c.clone());
    let usage = c.usage.clone();
    let rows = View::dynamic(move || {
        let docs = list.docs.get();
        if docs.is_empty() {
            return p()
                .class("empty")
                .text(t("Nothing here yet: save a document to the cloud."))
                .into_view();
        }
        let items: Vec<View> = docs
            .into_iter()
            .map(|d| {
                let (open, del, share) = (list.clone(), list.clone(), list.clone());
                let (id1, id2, id3) = (d.id.clone(), d.id.clone(), d.id.clone());
                div()
                    .class("cloud-row")
                    .child(span().class("cloud-name").text(d.name.clone()))
                    .child(span().class("cloud-size").text(size(d.size)))
                    .child(button_for("Open", "Open this document", move || {
                        open.open_doc(&id1)
                    }))
                    .child(button_for(
                        "Share\u{2026}",
                        "Make a link to this document",
                        move || {
                            share.link.set(String::new());
                            share.panel.set(Panel::Sharing(id3.clone()));
                            share.list_shares();
                        },
                    ))
                    .child(button_for(
                        "Delete",
                        "Delete this document from the cloud",
                        move || del.delete_doc(&id2),
                    ))
                    .into_view()
            })
            .collect();
        div().class("cloud-list").children(items).into_view()
    });
    div()
        .class("cloud-body")
        .child(account(c))
        .child(span().class("cloud-usage").text_fn(move || {
            usage.get().map_or(String::new(), |u| {
                format!("{} / {}", size(u.used), size(u.quota))
            })
        }))
        .child(button_for(
            "Save the open document here",
            "Save the open document to the cloud",
            move || save.save(),
        ))
        .child(rows)
        .into_view()
}

fn sharing(c: &Cloud, id: &str) -> View {
    let days_signal = c.days.clone();
    let (back, make, edit_signal, link) =
        (c.clone(), c.clone(), c.rights_edit.clone(), c.link.clone());
    let (shown, set) = (c.rights_edit.clone(), c.rights_edit.clone());
    let id = id.to_string();
    let existing = {
        let c = c.clone();
        let id = id.clone();
        View::dynamic(move || {
            let mine: Vec<Share> = c.shares.get().into_iter().filter(|s| s.doc == id).collect();
            let items: Vec<View> = mine
                .into_iter()
                .map(|s| {
                    let w = c.clone();
                    let token = s.token.clone();
                    div()
                        .class("cloud-row")
                        .child(span().class("cloud-name").text(format!(
                            "{} \u{b7} {}",
                            if s.rights == office_cloud::Rights::Edit {
                                t("can edit")
                            } else {
                                t("view only")
                            },
                            s.expires.map_or(t("no end date"), |_| t("ends"))
                        )))
                        .child(button_for(
                            "Withdraw",
                            "Stop this link from working",
                            move || w.withdraw(&token),
                        ))
                        .into_view()
                })
                .collect();
            div().class("cloud-list").children(items).into_view()
        })
    };
    div()
        .class("cloud-body")
        .child(p().text(t(
            "Anyone with the link can open the document, signed in or not.",
        )))
        .child(
            label()
                .class("check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr_bool("checked", move || shown.get())
                        .on_click(move || set.set(!set.get_untracked())),
                )
                .child(span().text(t("They may change it (otherwise they only view)"))),
        )
        .child(
            label()
                .class("check")
                .child(span().text(t("Ends after this many days (empty: never)")))
                .child(
                    input()
                        .attr("type", "number")
                        .attr("min", "1")
                        .attr("aria-label", t("Days until the link ends"))
                        .on_input(move |v| days_signal.set(v.to_string())),
                ),
        )
        .child(button_for(
            "Make a link",
            "Make a link to this document",
            move || make.share(&id, edit_signal.get_untracked()),
        ))
        .child(
            input()
                .attr("type", "text")
                .class("cloud-link")
                .attr("readonly", "readonly")
                .attr("aria-label", t("The link"))
                .attr_fn("value", move || link.get()),
        )
        .child(existing)
        .child(button_for("Back", "Back to the documents", move || {
            back.panel.set(Panel::Documents)
        }))
        .into_view()
}

thread_local! {
    static SERVER: RefCell<Context> = RefCell::default();
}

/// On the server: what the page being rendered knows (who is signed in, the link it was opened by).
pub fn set_server_context(context: Context) {
    SERVER.with(|c| *c.borrow_mut() = context);
}

#[cfg(not(target_arch = "wasm32"))]
struct NoTransport;

#[cfg(not(target_arch = "wasm32"))]
impl Transport for NoTransport {
    fn send(
        &self,
        _: &str,
        _: &str,
        _: Vec<(String, String)>,
        _: Vec<u8>,
        done: Box<dyn FnOnce(Reply)>,
    ) {
        done(Reply::default());
    }
}

/// The cloud of this page, made and installed: in the browser from the page state the server sent, on the server from
/// [`set_server_context`]. Call it before building the view; then give it a [`Binding`].
#[must_use]
pub fn setup() -> Cloud {
    #[cfg(target_arch = "wasm32")]
    let cloud = {
        let context = quench_ui_web::initial_state()
            .map_or_else(Context::default, |t| Context::from_json(&t));
        Cloud::new(context, Rc::new(web::Web))
    };
    #[cfg(not(target_arch = "wasm32"))]
    let cloud = Cloud::new(SERVER.with(|c| c.borrow().clone()), Rc::new(NoTransport));
    cloud.install();
    // The suite's home is where the API's path starts: `/lore-office/api` is served by the page at `/lore-office/`.
    office_ui::set_home(
        cloud
            .context
            .api
            .strip_suffix("api")
            .filter(|h| !h.is_empty())
            .map(str::to_string),
    );
    cloud
}

/// The browser's transport (the page's own `fetch`; cookies go with it).
#[cfg(target_arch = "wasm32")]
pub mod web {
    use super::{Reply, Transport};

    pub struct Web;

    impl Transport for Web {
        fn send(
            &self,
            method: &str,
            url: &str,
            headers: Vec<(String, String)>,
            body: Vec<u8>,
            done: Box<dyn FnOnce(Reply)>,
        ) {
            let mut req = quench_ui_web::Request::new(method, url);
            for (k, v) in &headers {
                req = req.header(k, v);
            }
            let fetch = req.body(body).send();
            quench_ui_web::spawn(async move {
                let reply = match fetch.await {
                    Ok(r) => Reply {
                        status: r.status,
                        headers: r
                            .headers
                            .into_iter()
                            .map(|(k, v)| (k.to_lowercase(), v))
                            .collect(),
                        body: r.body,
                    },
                    Err(_) => Reply {
                        status: 0,
                        ..Reply::default()
                    },
                };
                done(reply);
            });
        }
    }
}
