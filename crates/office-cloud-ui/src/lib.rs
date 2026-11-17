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

use office_cloud::{DocMeta, Grant, Share, Usage};
use office_ui::{MenuItem, t};
use quench_serde::{Value, json};
use quench_ui::prelude::*;

mod log;
use log::LOG_QUIET_MS;
mod panel;
mod presence;
pub use presence::Peer;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// What the server told the page (`Page::state`): where the API is, who is signed in, and the link the page was opened by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// `""` when there is no cloud: the apps then have no cloud menu at all.
    pub api: String,
    pub user: Option<String>,
    /// Signed in at the realm as this person, but the account has no access to this service.
    pub denied: Option<String>,
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
            denied: s("denied"),
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
        if let Some(d) = &self.denied {
            out.push_str(&format!(",\"denied\":{}", json::to_string(d.as_str())));
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

/// What an app that describes its changes has to send since the last time it was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A change a page can apply (JSON the app reads back): the cells that were written, say.
    Ops(Vec<u8>),
    /// Something no such change describes: the whole file is what the document is now.
    Whole,
}

/// How an app takes part in a document's log of changes (see the guide "Sync and presence").
#[derive(Clone)]
pub struct DeltaHooks {
    /// What changed since this was last called; `None` for nothing.
    pub take: Rc<dyn Fn() -> Option<Change>>,
    /// Make a change someone else made.
    pub apply: Rc<dyn Fn(&[u8]) -> Result<(), String>>,
    /// For changes that say where they apply by position (paragraph 7), so that they mean nothing once others' changes came in
    /// before them: the page then sends one change at a time, and when others' changes arrive first it drops its unsent change
    /// and asks `take` again (the app has merged its own edits into the document). When it is off, the unsent changes are
    /// applied again after the others' (right for changes that name what they write, like cells).
    pub recompute: bool,
    /// The change last taken was accepted by the cloud.
    pub accepted: Rc<dyn Fn()>,
}

/// What the app gives the cloud.
#[derive(Clone)]
pub struct Binding {
    /// `docs`, `sheets`, `notes` or `pdf`.
    pub kind: &'static str,
    /// Where the person is in the document (a cell range, a paragraph), as text the app reads back from the others' pages; empty
    /// when it has nothing to show.
    pub position: Option<Rc<dyn Fn() -> String>>,
    /// The open document as a file: its name (with extension) and bytes; `None` when there is none to save.
    pub export: Rc<dyn Fn() -> Option<(String, Vec<u8>)>>,
    /// Open a file the cloud gave.
    pub import: Rc<dyn Fn(&str, &[u8]) -> Result<(), String>>,
    /// A number that changes whenever the document does (read inside an effect, it makes the effect run again): how the
    /// cloud knows there are changes to save, and that the person has not changed anything since an update arrived.
    pub revision: Rc<dyn Fn() -> u64>,
    /// Whether changes to a document that is in the cloud are saved there by themselves, a moment after the last one.
    pub autosave: bool,
    /// Present for an app whose changes can be sent one by one: the document is then followed through its log of changes
    /// and the whole file is sent only now and then.
    pub delta: Option<DeltaHooks>,
}

/// How often a page looks at its document for changes (milliseconds).
pub(crate) const TICK_MS: u32 = 400;
/// How often a page asks whether someone else saved the open document (milliseconds).
const POLL_MS: u32 = 2000;
/// How long after the last change a document in the cloud is saved (milliseconds).
const AUTOSAVE_MS: u32 = 1000;
/// The longest changes wait to be saved when the person keeps typing without a pause (milliseconds).
const MAX_WAIT_MS: u32 = 8000;

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
    /// Opened from another person's cloud, shared with this one (and saves there).
    pub owner: Option<String>,
    /// Whether this page may save it: a document of one's own, an edit link, an edit share.
    pub edit: bool,
}

/// Which documents the panel lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// The person's own, in folders.
    Mine,
    /// What other people have shared with them.
    Shared,
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
    /// Every folder the person has.
    pub folders: Signal<Vec<String>>,
    /// The folder the panel shows (`""` is the top) and which documents.
    pub folder: Signal<String>,
    pub section: Signal<Section>,
    /// Whether documents are shown as a grid of cards (else as a list); kept in the browser.
    pub grid: Signal<bool>,
    /// The documents picked in the panel: their ids, or `owner/id` in the shared section.
    pub selected: Signal<Vec<String>>,
    /// What others shared with the person.
    pub grants: Signal<Vec<Grant>>,
    /// Who the document being shared is already shared with.
    pub doc_grants: Signal<Vec<Grant>>,
    /// The name typed to share with, and whether they may change it.
    pub person: Signal<String>,
    pub person_edit: Signal<bool>,
    /// The name typed for a new folder.
    pub folder_name: Signal<String>,
    /// There are changes the cloud has not got yet.
    pub dirty: Signal<bool>,
    /// The other pages that have the open document open.
    pub peers: Signal<Vec<Peer>>,
    presence: Rc<presence::Presence>,
    /// A newer version someone else saved, found while there were changes of the person's own.
    pub newer: Signal<Option<DocMeta>>,
    binding: Rc<RefCell<Option<Binding>>>,
    /// The revision the cloud's copy has (what the document was at the last open or save).
    baseline: Rc<Cell<u64>>,
    /// The revision seen at the last look, and how long it has stayed the same (milliseconds).
    seen: Rc<Cell<u64>>,
    quiet: Rc<Cell<u32>>,
    /// How long there have been changes not yet sent (milliseconds).
    waiting: Rc<Cell<u32>>,
    /// What the browser's storage says the open document is in the cloud (so a reload saves to the same place), and how long
    /// a change of document has waited to be written there.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    kept: Rc<RefCell<Option<Opened>>>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    kept_wait: Rc<Cell<u32>>,
    /// The log of the open document: the version of the file it was fetched or saved at, the latest change this page has made
    /// or applied, and the changes taken from the app that are not sent yet.
    version: Rc<Cell<u64>>,
    head: Rc<Cell<u64>>,
    outbox: Rc<RefCell<Vec<Vec<u8>>>>,
    /// The whole file is to be sent (the app changed something no change describes, or the person saved), and whether to say so.
    want_whole: Rc<Cell<bool>>,
    announce: Rc<Cell<bool>>,
    catching: Rc<Cell<bool>>,
    /// Saving by itself is off after a failure, until the person saves.
    paused: Rc<Cell<bool>>,
    saving: Rc<Cell<bool>>,
    started: Rc<Cell<bool>>,
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
            folders: create_signal(Vec::new()),
            folder: create_signal(String::new()),
            section: create_signal(Section::Mine),
            grid: create_signal(saved_grid()),
            selected: create_signal(Vec::new()),
            grants: create_signal(Vec::new()),
            doc_grants: create_signal(Vec::new()),
            person: create_signal(String::new()),
            person_edit: create_signal(false),
            folder_name: create_signal(String::new()),
            dirty: create_signal(false),
            newer: create_signal(None),
            peers: create_signal(Vec::new()),
            presence: Rc::default(),
            binding: Rc::default(),
            baseline: Rc::default(),
            seen: Rc::default(),
            quiet: Rc::default(),
            waiting: Rc::default(),
            kept: Rc::default(),
            kept_wait: Rc::default(),
            version: Rc::default(),
            head: Rc::default(),
            outbox: Rc::default(),
            want_whole: Rc::default(),
            announce: Rc::default(),
            catching: Rc::default(),
            paused: Rc::default(),
            saving: Rc::default(),
            started: Rc::default(),
            transport,
        }
    }

    /// Make this the cloud the menus and the overlay show (one for the page).
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    pub fn bind(&self, binding: Binding) {
        *self.binding.borrow_mut() = Some(binding);
        self.watch();
    }

    /// Whether the open document may be saved to the cloud by this page: it is the person's own (signed in), or it was
    /// opened through a link that lets them edit.
    fn writable(&self) -> bool {
        self.opened.get_untracked().is_some_and(|o| o.edit)
    }

    /// The document's revision now (untracked).
    fn revision(&self) -> u64 {
        self.binding.borrow().as_ref().map_or(0, |b| (b.revision)())
    }

    /// Say the document and the cloud's copy are the same: what it is now, or what it was when it was sent.
    fn mark_clean(&self, revision: u64) {
        self.baseline.set(revision);
        self.dirty.set(self.revision() != revision);
    }

    /// Start looking after a document in the cloud: note when it changes (and save it a moment after the last change), and ask
    /// now and then whether someone else saved. Once, when the app has bound itself.
    ///
    /// It looks on a timer rather than reacting to the revision: that works wherever the page runs, with nothing to keep alive.
    fn watch(&self) {
        if self.started.replace(true) {
            return;
        }
        // Without a clock (the server drawing the page) there is nothing to wait on.
        if quench_ui::platform::installed() {
            self.tick();
            self.poll();
        }
    }

    fn tick(&self) {
        let me = self.clone();
        quench_ui::platform::after(TICK_MS, move || {
            me.look();
            me.tick();
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn kept_key(&self) -> Option<String> {
        let kind = self.binding.borrow().as_ref()?.kind;
        Some(format!("lore-office.opened.{kind}"))
    }

    /// Write down where the open document is in the cloud, so that after a reload (the app brings back its last text) a save
    /// goes to that document and does not make a new one.
    ///
    /// A different document is written only after it has stayed open for 2 s, by when the app has kept its text too: a reload
    /// before that finds nothing written (a new document), never another document's place beside this one's text.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::unused_self)]
    fn keep_opened(&self) {}

    #[cfg(target_arch = "wasm32")]
    fn keep_opened(&self) {
        let Some(key) = self.kept_key() else { return };
        let now = self.opened.get_untracked();
        let kept = self.kept.borrow().clone();
        if kept == now {
            self.kept_wait.set(0);
            return;
        }
        let write = |o: &Option<Opened>| match o {
            Some(o) => quench_ui_web::browser::storage_set(
                &key,
                Some(&format!(
                    "{}\n{}\n{}\n{}\n{}",
                    o.id,
                    o.etag,
                    o.token.as_deref().unwrap_or(""),
                    o.owner.as_deref().unwrap_or(""),
                    u8::from(o.edit)
                )),
            ),
            None => quench_ui_web::browser::storage_set(&key, None),
        };
        let same_place = match (&kept, &now) {
            (Some(a), Some(b)) => {
                (&a.id, &a.token, &a.owner, a.edit) == (&b.id, &b.token, &b.owner, b.edit)
            }
            _ => false,
        };
        if same_place {
            write(&now);
            *self.kept.borrow_mut() = now;
            return;
        }
        if kept.is_some() {
            write(&None);
            *self.kept.borrow_mut() = None;
        }
        let waited = self.kept_wait.get().saturating_add(TICK_MS);
        self.kept_wait.set(waited);
        if now.is_none() || waited >= 2000 {
            write(&now);
            *self.kept.borrow_mut() = now;
            self.kept_wait.set(0);
        }
    }

    /// The app has brought back its last text: it is the open document of the cloud that storage names (if any), so that a save
    /// goes there. Whatever the cloud has changed since shows as a conflict, as it does for any save.
    pub fn restore_opened(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(key) = self.kept_key() else { return };
            let Some(text) = quench_ui_web::browser::storage_get(&key) else {
                return;
            };
            let mut p = text.split('\n');
            let (Some(id), Some(etag), Some(token), Some(owner), Some(edit)) =
                (p.next(), p.next(), p.next(), p.next(), p.next())
            else {
                return;
            };
            let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
            let o = Opened {
                id: id.to_string(),
                etag: etag.to_string(),
                token: some(token),
                owner: some(owner),
                edit: edit == "1",
            };
            *self.kept.borrow_mut() = Some(o.clone());
            self.opened.set(Some(o));
        }
    }

    /// One look at the document: has it changed, and has it been quiet long enough to send?
    fn look(&self) {
        self.keep_opened();
        self.presence_step();
        if self.following() {
            self.look_log();
            return;
        }
        let now = self.revision();
        if now == self.seen.get() {
            self.quiet.set(self.quiet.get().saturating_add(TICK_MS));
        } else {
            self.seen.set(now);
            self.quiet.set(0);
        }
        if self.opened.get_untracked().is_none() || now == self.baseline.get() {
            self.waiting.set(0);
            return;
        }
        if !self.dirty.get_untracked() {
            self.dirty.set(true);
        }
        self.waiting.set(self.waiting.get().saturating_add(TICK_MS));
        let by_itself = self.binding.borrow().as_ref().is_some_and(|b| b.autosave);
        // After a pause, or - for someone who types without one - after the longest wait.
        if by_itself
            && (self.quiet.get() >= AUTOSAVE_MS || self.waiting.get() >= MAX_WAIT_MS)
            && !self.paused.get()
            && self.writable()
            && self.newer.get_untracked().is_none()
            && !self.saving.get()
        {
            self.quiet.set(0);
            self.waiting.set(0);
            self.save_inner(true);
        }
    }

    fn poll(&self) {
        let me = self.clone();
        quench_ui::platform::after(POLL_MS, move || {
            // While the held question works, news comes by itself: this is only the safety net.
            let slow = me.presence.slow.get().wrapping_add(1);
            me.presence.slow.set(slow);
            if me.presence.pushed.get() && !slow.is_multiple_of(8) {
                me.poll();
                return;
            }
            if me.following() {
                me.catch_up();
            } else {
                me.check_remote();
            }
            me.poll();
        });
    }

    /// Ask whether the open document has a newer version.
    pub fn check_remote(&self) {
        let Some(o) = self.opened.get_untracked() else {
            return;
        };
        if self.saving.get() || o.etag.is_empty() {
            return;
        }
        let path = match (&o.token, &o.owner) {
            (Some(token), _) => format!("/s/{token}/meta"),
            (None, Some(owner)) => format!("/shared/{}/{}/meta", enc(owner), o.id),
            (None, None) => format!("/docs/{}/meta", o.id),
        };
        self.fetch("GET", &path, Vec::new(), Vec::new(), move |me, r| {
            if r.status != 200 {
                return;
            }
            let Ok(meta) = json::from_str::<DocMeta>(&String::from_utf8_lossy(&r.body)) else {
                return;
            };
            me.remote_seen(meta);
        });
    }

    /// What was found out about the open document in the cloud.
    fn remote_seen(&self, meta: DocMeta) {
        let Some(o) = self.opened.get_untracked() else {
            return;
        };
        if self.saving.get() || meta.etag == o.etag {
            return;
        }
        if self.dirty.get_untracked() || self.revision() != self.baseline.get() {
            // The person has changes of their own: they choose (the banner).
            self.dirty.set(true);
            self.newer.set(Some(meta));
        } else {
            self.reload();
        }
    }

    /// Take the cloud's version of the open document in place of the one on the screen.
    pub fn reload(&self) {
        let Some(o) = self.opened.get_untracked() else {
            return;
        };
        let path = match (&o.token, &o.owner) {
            (Some(token), _) => format!("/s/{token}"),
            (None, Some(owner)) => format!("/shared/{}/{}", enc(owner), o.id),
            (None, None) => format!("/docs/{}", o.id),
        };
        let (token, owner, edit, id) = (o.token, o.owner, o.edit, o.id);
        self.fetch("GET", &path, Vec::new(), Vec::new(), move |me, r| {
            if r.status != 200 {
                return;
            }
            let name = r.header("x-name").unwrap_or("document").to_string();
            let etag = r.header("x-etag").unwrap_or_default().to_string();
            let Some(b) = me.binding.borrow().clone() else {
                return;
            };
            if (b.import)(&name, &r.body).is_ok() {
                me.opened.set(Some(Opened {
                    id,
                    etag,
                    token,
                    owner,
                    edit,
                }));
                me.newer.set(None);
                me.mark_clean((b.revision)());
                me.note_log(&r);
                me.status
                    .set(t("Updated: someone else changed this document"));
            }
        });
    }

    /// Keep the person's own version over the newer one found: the next save goes over it.
    pub fn keep_mine(&self) {
        let Some(newer) = self.newer.get_untracked() else {
            return;
        };
        if let Some(mut o) = self.opened.get_untracked() {
            o.etag = newer.etag;
            self.opened.set(Some(o));
        }
        self.newer.set(None);
        self.save();
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
        self.fetch(method, path, headers, body, move |me, reply| {
            me.busy.set(false);
            done(me, reply);
        });
    }

    /// As `call`, without showing the page as busy: for what goes on in the background.
    fn fetch(
        &self,
        method: &str,
        path: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        done: impl FnOnce(&Self, Reply) + 'static,
    ) {
        let me = self.clone();
        self.transport.send(
            method,
            &self.url(path),
            headers,
            body,
            Box::new(move |reply| done(&me, reply)),
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
        self.fetch("GET", "/folders", Vec::new(), Vec::new(), |me, r| {
            if r.status == 200
                && let Ok(list) = json::from_str::<Vec<String>>(&String::from_utf8_lossy(&r.body))
            {
                me.folders.set(list);
            }
        });
        self.fetch("GET", "/grants", Vec::new(), Vec::new(), |me, r| {
            if r.status == 200
                && let Ok(list) = json::from_str::<Vec<Grant>>(&String::from_utf8_lossy(&r.body))
            {
                me.grants.set(list);
            }
        });
    }

    /// Show a folder of the person's own documents.
    pub fn go(&self, folder: &str) {
        self.section.set(Section::Mine);
        self.folder.set(folder.to_string());
        self.selected.set(Vec::new());
    }

    /// Show what others shared with the person.
    pub fn go_shared(&self) {
        self.section.set(Section::Shared);
        self.selected.set(Vec::new());
    }

    /// Make the folder named in the field, inside the folder shown.
    pub fn make_folder(&self) {
        let name = self.folder_name.get_untracked();
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let here = self.folder.get_untracked();
        let path = if here.is_empty() {
            name.to_string()
        } else {
            format!("{here}/{name}")
        };
        self.call(
            "POST",
            &format!("/folders?path={}", enc(&path)),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200 {
                    me.folder_name.set(String::new());
                    me.refresh();
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    /// Take away an empty folder.
    pub fn delete_folder(&self, path: &str) {
        self.call(
            "DELETE",
            &format!("/folders?path={}", enc(path)),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 204 || r.status == 200 {
                    me.refresh();
                } else if r.status == 400 {
                    me.status.set(t("The folder is not empty"));
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    /// Move the documents picked into `folder` (`""` is the top).
    pub fn move_selected(&self, folder: &str) {
        let ids = self.selected.get_untracked();
        if ids.is_empty() || self.section.get_untracked() != Section::Mine {
            return;
        }
        let left = Rc::new(Cell::new(ids.len()));
        for id in ids {
            let left = left.clone();
            self.fetch(
                "PUT",
                &format!("/docs/{id}/move?folder={}", enc(folder)),
                Vec::new(),
                Vec::new(),
                move |me, r| {
                    if r.status != 200 {
                        me.failed(&r);
                    }
                    left.set(left.get() - 1);
                    if left.get() == 0 {
                        me.selected.set(Vec::new());
                        me.refresh();
                    }
                },
            );
        }
    }

    /// Delete the documents picked.
    pub fn delete_selected(&self) {
        if self.section.get_untracked() != Section::Mine {
            return;
        }
        let ids = self.selected.get_untracked();
        self.selected.set(Vec::new());
        for id in ids {
            self.delete_doc(&id);
        }
    }

    /// Open the document picked (the one, when exactly one is).
    pub fn open_selected(&self) {
        let ids = self.selected.get_untracked();
        let [one] = ids.as_slice() else {
            return;
        };
        match self.section.get_untracked() {
            Section::Mine => self.open_doc(one),
            Section::Shared => {
                if let Some((owner, id)) = one.split_once('/') {
                    self.open_granted(owner, id);
                }
            }
        }
    }

    /// Open a document another person shared with this one.
    pub fn open_granted(&self, owner: &str, id: &str) {
        let (owner, id) = (owner.to_string(), id.to_string());
        self.call(
            "GET",
            &format!("/shared/{}/{id}", enc(&owner)),
            Vec::new(),
            Vec::new(),
            move |me, r| {
                if r.status != 200 {
                    me.failed(&r);
                    return;
                }
                let name = r.header("x-name").unwrap_or("document").to_string();
                let etag = r.header("x-etag").unwrap_or_default().to_string();
                let edit = r.header("x-rights") == Some("edit");
                let Some(b) = me.binding.borrow().clone() else {
                    return;
                };
                match (b.import)(&name, &r.body) {
                    Ok(()) => {
                        me.opened.set(Some(Opened {
                            id,
                            etag,
                            token: None,
                            owner: Some(owner.clone()),
                            edit,
                        }));
                        me.newer.set(None);
                        me.mark_clean(me.revision());
                        me.note_log(&r);
                        me.panel.set(Panel::Closed);
                        me.status.set(format!(
                            "{} {name} ({owner}, {})",
                            t("Opened from the cloud of"),
                            if edit {
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

    /// Share document `id` with the person named in the field.
    pub fn share_with(&self, id: &str) {
        let with = self.person.get_untracked();
        let with = with.trim();
        if with.is_empty() {
            return;
        }
        let rights = if self.person_edit.get_untracked() {
            "edit"
        } else {
            "view"
        };
        let id = id.to_string();
        self.call(
            "POST",
            &format!("/docs/{id}/grant?user={}&rights={rights}", enc(with)),
            Vec::new(),
            Vec::new(),
            move |me, r| {
                if r.status == 200 {
                    me.person.set(String::new());
                    me.list_doc_grants(&id);
                    me.status.set(t("Shared: it shows in their cloud"));
                } else if r.status == 400 {
                    me.status.set(String::from_utf8_lossy(&r.body).to_string());
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    /// Who document `id` is shared with.
    pub fn list_doc_grants(&self, id: &str) {
        self.fetch(
            "GET",
            &format!("/docs/{id}/grants"),
            Vec::new(),
            Vec::new(),
            |me, r| {
                if r.status == 200
                    && let Ok(list) =
                        json::from_str::<Vec<Grant>>(&String::from_utf8_lossy(&r.body))
                {
                    me.doc_grants.set(list);
                }
            },
        );
    }

    /// Stop sharing document `id` with `user`.
    pub fn revoke(&self, id: &str, user: &str) {
        let id = id.to_string();
        self.call(
            "DELETE",
            &format!("/docs/{id}/grant?user={}", enc(user)),
            Vec::new(),
            Vec::new(),
            move |me, r| {
                if r.status == 204 || r.status == 200 {
                    me.list_doc_grants(&id);
                } else {
                    me.failed(&r);
                }
            },
        );
    }

    /// Save the open document: over the one it came from, else as a new one.
    pub fn save(&self) {
        self.save_inner(false);
    }

    /// `quiet`: a save the person did not ask for (changes sent by themselves), which says nothing when it works.
    fn save_inner(&self, quiet: bool) {
        // A document followed through its log is saved by sending the whole file at the latest version.
        if !quiet && self.following() && self.writable() {
            self.announce.set(true);
            self.want_whole.set(true);
            self.paused.set(false);
            self.quiet.set(LOG_QUIET_MS);
            return;
        }
        let Some(b) = self.binding.borrow().clone() else {
            return;
        };
        let at = (b.revision)();
        // Making the document in the cloud sends the file whole: what the app had waiting is in it.
        if self.opened.get_untracked().is_none()
            && let Some(delta) = &b.delta
        {
            let _ = (delta.take)();
        }
        let Some((name, bytes)) = (b.export)() else {
            if !quiet {
                self.status.set(t("There is nothing to save"));
            }
            return;
        };
        let opened = self.opened.get_untracked();
        // A name with letters outside ASCII is sent as UTF-8 bytes.
        let name_q = enc(&name);
        // A new document goes into the folder the panel shows (when it shows the person's own).
        let folder = if self.section.get_untracked() == Section::Mine {
            self.folder.get_untracked()
        } else {
            String::new()
        };
        let (path, headers) = match &opened {
            Some(o) if o.token.is_some() => (
                format!("/s/{}", o.token.clone().unwrap_or_default()),
                vec![("if-match".to_string(), o.etag.clone())],
            ),
            Some(o) if o.owner.is_some() => (
                format!(
                    "/shared/{}/{}",
                    enc(o.owner.as_deref().unwrap_or_default()),
                    o.id
                ),
                vec![("if-match".to_string(), o.etag.clone())],
            ),
            Some(o) => (
                format!("/docs/{}?name={name_q}&kind={}", o.id, b.kind),
                vec![("if-match".to_string(), o.etag.clone())],
            ),
            None if folder.is_empty() => {
                (format!("/docs?name={name_q}&kind={}", b.kind), Vec::new())
            }
            None => (
                format!(
                    "/docs?name={name_q}&kind={}&folder={}",
                    b.kind,
                    enc(&folder)
                ),
                Vec::new(),
            ),
        };
        let (token, owner) = opened
            .as_ref()
            .map_or((None, None), |o| (o.token.clone(), o.owner.clone()));
        self.saving.set(true);
        self.call("PUT", &path, headers, bytes, move |me, r| {
            me.saving.set(false);
            if r.status == 200 {
                if let Ok(meta) = json::from_str::<DocMeta>(&String::from_utf8_lossy(&r.body)) {
                    me.opened.set(Some(Opened {
                        id: meta.id,
                        etag: meta.etag,
                        token,
                        owner,
                        edit: true,
                    }));
                    // What was typed while this was on its way is still to be sent.
                    me.mark_clean(at);
                    me.note_log(&r);
                    me.paused.set(false);
                    if !quiet {
                        me.status.set(t("Saved to the cloud"));
                    }
                    if me.panel.get_untracked() == Panel::Documents {
                        me.refresh();
                    }
                }
            } else if r.status == 409 && opened.is_some() {
                // Someone saved first: look at what they saved and let the person choose.
                me.dirty.set(true);
                me.check_remote();
            } else {
                if quiet {
                    // Not again and again: the person is told once, and saves when they are ready.
                    me.paused.set(true);
                }
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
                            owner: None,
                            edit: true,
                        }));
                        me.newer.set(None);
                        me.mark_clean(me.revision());
                        me.note_log(&r);
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
                            owner: None,
                            edit: share.edit,
                        }));
                        me.mark_clean(me.revision());
                        me.note_log(&r);
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
        let hint = if saves_to_cloud() { "Ctrl+S" } else { "" };
        items.push(MenuItem::action("Save to the cloud", hint, move || {
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

/// Whether Ctrl+S saves to the cloud on this page.
///
/// That is so when the person is signed in (the cloud is the default place for what they make), or the page was opened by a
/// link that lets them edit. Otherwise Ctrl+S saves a file as before.
#[must_use]
pub fn saves_to_cloud() -> bool {
    current().is_some_and(|c| {
        c.enabled()
            && (c.context.user.is_some() || c.context.share.as_ref().is_some_and(|s| s.edit))
    })
}

/// The Ctrl+S of an app: saves to the cloud when [`saves_to_cloud`], and says it did; else the app saves a file itself.
#[allow(clippy::must_use_candidate)]
pub fn save_shortcut() -> bool {
    if !saves_to_cloud() {
        return false;
    }
    if let Some(cloud) = current() {
        cloud.save();
    }
    true
}

/// A "Cloud" button for the top line (nothing when the page has no cloud): opens the person's documents.
#[must_use]
pub fn cloud_button() -> View {
    let Some(cloud) = current() else {
        return View::Empty;
    };
    let title = cloud.clone();
    let (dot_on, dot_pending) = (cloud.opened.clone(), cloud.dirty.clone());
    let here = cloud.clone();
    let who = span().class("who").child(View::dynamic(move || {
        let list = here.others();
        let mut row = span().class("who-row");
        for p in list.iter().take(4) {
            let color = p.color.clone();
            row = row.child(
                span()
                    .class("who-chip")
                    .attr(
                        "title",
                        format!("{} {}", p.name, t("has this document open")),
                    )
                    .css("background", move || color.clone())
                    .text(p.name.chars().next().map(String::from).unwrap_or_default()),
            );
        }
        if list.len() > 4 {
            row = row.child(
                span()
                    .class("who-more")
                    .text(format!("+{}", list.len() - 4)),
            );
        }
        row.into_view()
    }));
    let cloud_tool = button()
        .attr("type", "button")
        // What the button says follows the document: in the cloud and sent, or with changes still to send.
        .attr_fn("title", move || {
            if title.opened.get().is_none() {
                t("Your documents in the cloud")
            } else if title.dirty.get() {
                t("Your documents in the cloud: changes not sent yet")
            } else {
                t("Your documents in the cloud: this document is up to date")
            }
        })
        .class("tool cloud-button")
        .child(span().class("icon icon-cloud"))
        .child(span().text(t("Cloud")))
        .child(
            span()
                .class("sync-dot")
                .class_if("on", move || dot_on.get().is_some())
                .class_if("pending", move || dot_pending.get()),
        )
        .on_click(move || cloud.open_panel());
    span()
        .class("cloud-wrap")
        .child(who)
        .child(cloud_tool)
        .into_view()
}

/// Where a lore-office server keeps an app, from the address a person typed.
///
/// The page is the address then `/<kind>`. Without a scheme it is `https://`; with only a host (nothing after it) the
/// suite's usual place, `/lore-office`, is added. Empty when nothing was typed.
#[must_use]
pub fn server_page(address: &str, kind: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.is_empty() {
        return String::new();
    }
    let with_scheme = if address.contains("://") {
        address.to_string()
    } else {
        format!("https://{address}")
    };
    let after_scheme = with_scheme.split_once("://").map_or("", |(_, rest)| rest);
    let base = if after_scheme.contains('/') {
        with_scheme
    } else {
        format!("{with_scheme}/lore-office")
    };
    format!("{base}/{kind}")
}

/// Text as it goes in a query: letters, digits and `-_.` as they are, every other byte as `%XX`.
pub(crate) fn enc(text: &str) -> String {
    text.bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.') {
                char::from(c).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect()
}

/// Whether the panel showed a grid last time (kept in the browser's storage).
fn saved_grid() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        quench_ui_web::browser::storage_get("cloud.grid").is_some_and(|v| v == "1")
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(crate) fn keep_grid(grid: bool) {
    #[cfg(target_arch = "wasm32")]
    quench_ui_web::browser::storage_set("cloud.grid", Some(if grid { "1" } else { "0" }));
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

pub(crate) fn button_for(label: &str, tip: &str, action: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .class("tool")
        .attr("title", t(tip))
        .text(t(label))
        .on_click(action)
}

pub(crate) fn size(bytes: u64) -> String {
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
            // Someone else saved while there were changes of the person's own: they choose which to keep.
            if c.newer.get().is_some() {
                let (theirs, mine) = (c.clone(), c.clone());
                return div()
                    .class("cloud-banner")
                    .attr("role", "alert")
                    .child(span().text(t(
                        "Someone else changed this document while you were editing it",
                    )))
                    .child(button_for(
                        "Load their version",
                        "Replace what is on the screen with their version",
                        move || theirs.reload(),
                    ))
                    .child(button_for(
                        "Keep mine",
                        "Save what is on the screen over their version",
                        move || mine.keep_mine(),
                    ))
                    .into_view();
            }
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
/// Sharing with a named person: it stays in the owner's cloud and shows in theirs, with the rights given.
fn with_a_person(c: &Cloud, id: &str) -> Element {
    let (typed, shown, edit_set, edit_shown, share) = (
        c.person.clone(),
        c.person.clone(),
        c.person_edit.clone(),
        c.person_edit.clone(),
        c.clone(),
    );
    let (list, revoke_all) = (c.clone(), c.clone());
    let shared_id = id.to_string();
    let existing = View::dynamic(move || {
        let rows: Vec<View> = list
            .doc_grants
            .get()
            .into_iter()
            .map(|g| {
                let (me, doc, who) = (revoke_all.clone(), g.doc.clone(), g.with.clone());
                div()
                    .class("cloud-row")
                    .child(span().class("cloud-name").text(format!(
                        "{} \u{b7} {}",
                        g.with,
                        if g.rights == office_cloud::Rights::Edit {
                            t("can edit")
                        } else {
                            t("view only")
                        }
                    )))
                    .child(button_for(
                        "Withdraw",
                        "Stop sharing it with this person",
                        move || me.revoke(&doc, &who),
                    ))
                    .into_view()
            })
            .collect();
        div().class("cloud-list").children(rows).into_view()
    });
    div()
        .class("cloud-person")
        .child(h3().text(t("Share with a person")))
        .child(p().text(t(
            "It stays in your cloud and shows in theirs. Give the name they sign in with.",
        )))
        .child(
            input()
                .attr("type", "text")
                .class("cloud-link")
                .attr("placeholder", t("Their user name"))
                .attr("aria-label", t("Their user name"))
                .attr_fn("value", move || shown.get())
                .on_input(move |v| typed.set(v.to_string())),
        )
        .child(
            label()
                .class("check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr_bool("checked", move || edit_shown.get())
                        .on_click(move || edit_set.set(!edit_set.get_untracked())),
                )
                .child(span().text(t("They may change it (otherwise they only view)"))),
        )
        .child(button_for(
            "Share with them",
            "Share the document with this person",
            move || share.share_with(&shared_id),
        ))
        .child(existing)
}

pub(crate) fn account(c: &Cloud) -> View {
    let mut bar = div().class("cloud-account");
    let home = c.home();
    if c.enabled() {
        let who = match (&c.context.user, c.context.login.is_empty()) {
            (None, _) if c.context.denied.is_some() => format!(
                "{} {}: {}",
                t("Signed in as"),
                c.context.denied.clone().unwrap_or_default(),
                t("this account has no access to the cloud; ask an administrator")
            ),
            (Some(u), _) => format!("{} {u}", t("Signed in as")),
            (None, false) => t("Not signed in"),
            (None, true) => t("Local use: no sign-in on this server"),
        };
        bar = bar.child(span().class("cloud-who").text(who));
        if (c.context.user.is_some() || c.context.denied.is_some()) && !c.context.login.is_empty() {
            bar = bar.child(
                a().class("tool")
                    .attr("href", format!("{home}ui/logout"))
                    .text(t("Sign out")),
            );
        } else if c.context.user.is_none()
            && c.context.denied.is_none()
            && !c.context.login.is_empty()
        {
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
                    .attr("placeholder", "https://ennor.ddns.net/lore-office")
                    .attr("aria-label", t("Address of the lore-office server"))
                    .attr("value", c.server.get_untracked())
                    .on_input(move |v| {
                        typed.set(v.trim().to_string());
                        keep_server(v.trim());
                    }),
            )
            .child(
                a().class("tool")
                    .attr_fn("href", move || server_page(&link.get(), kind))
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
    panel::documents(c)
}

fn sharing(c: &Cloud, id: &str) -> View {
    let id_for_people = id.to_string();
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
        .child(with_a_person(c, &id_for_people))
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
