//! Following a document through its log of changes.
//!
//! For an app that describes its changes (`Binding::delta`) a page sends each change as it is made, after a short quiet, and
//! asks every few seconds for the changes others made. The service numbers every change, so all pages apply them in the same
//! order. A change that began from an old number is refused with the latest; the page then catches up (applies what it
//! missed, then its own unsent changes again, so theirs come first and the page's after) and sends again. Now and then a page
//! that is up to date sends the whole file as the new snapshot at the latest number, and the service drops the older changes.

#![allow(clippy::redundant_pub_crate)]

use crate::{Change, Cloud, DocMeta, Opened, enc, t};
use quench_serde::{Value as Json, json};

/// How long the app must be quiet before its changes are sent (milliseconds), and the longest they wait.
pub const LOG_QUIET_MS: u32 = 400;
const LOG_MAX_WAIT_MS: u32 = 2000;
/// After this many changes beyond the stored file, a page that is up to date sends the whole file, once it has been quiet this
/// long (milliseconds).
const COMPACT_EVERY: u64 = 50;
const COMPACT_QUIET_MS: u32 = 3000;

fn number(v: Option<&Json>) -> Option<u64> {
    match v? {
        Json::UInt(n) => Some(*n),
        Json::Int(n) => u64::try_from(*n).ok(),
        _ => None,
    }
}

fn header_number(r: &crate::Reply, name: &str) -> Option<u64> {
    r.header(name)?.parse().ok()
}

impl Cloud {
    /// Whether the open document is followed through its log: the app describes its changes and a document is open.
    pub(crate) fn following(&self) -> bool {
        self.opened.get_untracked().is_some()
            && self
                .binding
                .borrow()
                .as_ref()
                .is_some_and(|b| b.delta.is_some())
    }

    fn hooks(&self) -> Option<crate::DeltaHooks> {
        self.binding.borrow().as_ref().and_then(|b| b.delta.clone())
    }

    /// Where the open document's calls begin: `/docs/{id}`, `/s/{token}` or `/shared/{owner}/{id}`.
    fn prefix(o: &Opened) -> String {
        match (&o.token, &o.owner) {
            (Some(token), _) => format!("/s/{token}"),
            (None, Some(owner)) => format!("/shared/{}/{}", enc(owner), o.id),
            (None, None) => format!("/docs/{}", o.id),
        }
    }

    /// The document was fetched or saved at `version` (and the log may hold more): start from there.
    pub(crate) fn begin_log(&self, version: u64, head: u64) {
        self.version.set(version);
        self.head.set(version);
        self.want_whole.set(false);
        if head > version {
            self.catch_up();
        }
    }

    /// Learn the numbers of a response about the open document.
    pub(crate) fn note_log(&self, r: &crate::Reply) {
        let version = header_number(r, "x-version").unwrap_or(0);
        let head = header_number(r, "x-head").unwrap_or(version);
        self.begin_log(version, head);
    }

    /// One look: take what the app changed, and send what is ready.
    pub(crate) fn look_log(&self) {
        let Some(hooks) = self.hooks() else {
            return;
        };
        let writable = self.writable();
        let mut changed = false;
        if writable {
            match (hooks.take)() {
                Some(Change::Ops(bytes)) => {
                    self.outbox.borrow_mut().push(bytes);
                    changed = true;
                }
                Some(Change::Whole) => {
                    // The whole file says it all: the changes before it are in it.
                    self.outbox.borrow_mut().clear();
                    self.want_whole.set(true);
                    changed = true;
                }
                None => {}
            }
        }
        if changed {
            self.quiet.set(0);
        } else {
            self.quiet
                .set(self.quiet.get().saturating_add(crate::TICK_MS));
        }
        let pending = self.want_whole.get() || !self.outbox.borrow().is_empty();
        if self.dirty.get_untracked() != pending {
            self.dirty.set(pending);
        }
        if pending {
            self.waiting
                .set(self.waiting.get().saturating_add(crate::TICK_MS));
        } else {
            self.waiting.set(0);
        }
        if !writable || self.saving.get() || self.catching.get() || self.paused.get() {
            return;
        }
        let quiet = self.quiet.get();
        if self.want_whole.get() {
            if quiet >= LOG_QUIET_MS {
                self.send_snapshot();
            }
        } else if !self.outbox.borrow().is_empty() {
            if quiet >= LOG_QUIET_MS || self.waiting.get() >= LOG_MAX_WAIT_MS {
                self.send_change();
            }
        } else if self.head.get().saturating_sub(self.version.get()) >= COMPACT_EVERY
            && quiet >= COMPACT_QUIET_MS
        {
            // Up to date and idle: fold the changes into the stored file.
            self.want_whole.set(true);
        }
    }

    /// Send the oldest change not yet sent, from the latest number this page knows.
    fn send_change(&self) {
        let Some(o) = self.opened.get_untracked() else {
            return;
        };
        let Some(op) = self.outbox.borrow().first().cloned() else {
            return;
        };
        let path = format!("{}/ops?base={}", Self::prefix(&o), self.head.get());
        self.saving.set(true);
        self.fetch("POST", &path, Vec::new(), op, move |me, r| {
            me.saving.set(false);
            match r.status {
                200 => {
                    let n = json::parse(&String::from_utf8_lossy(&r.body))
                        .ok()
                        .and_then(|v| number(v.get("n")))
                        .unwrap_or_else(|| me.head.get() + 1);
                    me.head.set(n);
                    let mut outbox = me.outbox.borrow_mut();
                    if !outbox.is_empty() {
                        outbox.remove(0);
                    }
                    me.waiting.set(0);
                }
                // Behind: others made changes first.
                409 => me.catch_up(),
                _ => {
                    me.paused.set(true);
                    me.failed(&r);
                }
            }
        });
    }

    /// Send the whole file as the snapshot at the latest number this page knows.
    fn send_snapshot(&self) {
        let Some(o) = self.opened.get_untracked() else {
            return;
        };
        let Some(b) = self.binding.borrow().clone() else {
            return;
        };
        let Some((name, bytes)) = (b.export)() else {
            self.want_whole.set(false);
            return;
        };
        let at = self.head.get();
        let path = match (&o.token, &o.owner) {
            (Some(_), _) | (None, Some(_)) => format!("{}?at={at}", Self::prefix(&o)),
            (None, None) => format!("/docs/{}?name={}&kind={}&at={at}", o.id, enc(&name), b.kind),
        };
        self.saving.set(true);
        let announce = self.announce.replace(false);
        self.fetch("PUT", &path, Vec::new(), bytes, move |me, r| {
            me.saving.set(false);
            match r.status {
                200 => {
                    me.want_whole.set(false);
                    me.outbox.borrow_mut().clear();
                    me.version.set(at);
                    if let Ok(meta) = json::from_str::<DocMeta>(&String::from_utf8_lossy(&r.body))
                        && let Some(mut now) = me.opened.get_untracked()
                    {
                        now.etag = meta.etag;
                        me.opened.set(Some(now));
                    }
                    me.paused.set(false);
                    if announce {
                        me.status.set(t("Saved to the cloud"));
                    }
                    if me.panel.get_untracked() == crate::Panel::Documents {
                        me.refresh();
                    }
                }
                409 => {
                    me.announce.set(announce);
                    me.catch_up();
                }
                _ => {
                    me.paused.set(true);
                    me.failed(&r);
                }
            }
        });
    }

    /// Ask for the changes after the latest this page knows, and apply them.
    pub(crate) fn catch_up(&self) {
        if self.catching.get() || self.saving.get() {
            return;
        }
        let (Some(o), Some(_)) = (self.opened.get_untracked(), self.hooks()) else {
            return;
        };
        let path = format!("{}/ops?after={}", Self::prefix(&o), self.head.get());
        self.catching.set(true);
        self.fetch("GET", &path, Vec::new(), Vec::new(), move |me, r| {
            me.catching.set(false);
            match r.status {
                200 => me.apply_log(&r.body),
                // What it needs was folded into the stored file: fetch that.
                410 => me.reload(),
                _ => {}
            }
        });
    }

    /// Apply an answer to "the changes after n": theirs first, then this page's own that are not sent yet.
    fn apply_log(&self, body: &[u8]) {
        let Some(hooks) = self.hooks() else {
            return;
        };
        let Ok(answer) = json::parse(&String::from_utf8_lossy(body)) else {
            return;
        };
        let Some(Json::Array(items)) = answer.get("ops") else {
            return;
        };
        let mut last = self.head.get();
        let mut applied = false;
        for item in items {
            let (Some(n), Some(op)) = (number(item.get("n")), item.get("op")) else {
                continue;
            };
            let text = json::to_string(op);
            if (hooks.apply)(text.as_bytes()).is_err() {
                // A change this page cannot read: start again from the stored file.
                self.reload();
                return;
            }
            last = n;
            applied = true;
        }
        if let Some(version) = number(answer.get("version")) {
            self.version.set(version);
        }
        let head = number(answer.get("head")).unwrap_or(last).max(last);
        self.head.set(head);
        if applied {
            // Their changes are in; this page's own that wait to be sent come after them.
            for own in self.outbox.borrow().iter() {
                let _ = (hooks.apply)(own);
            }
            self.status
                .set(t("Updated: someone else changed this document"));
        }
    }
}
