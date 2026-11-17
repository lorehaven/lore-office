//! Who else has the open document open, and where they are in it.
//!
//! The page tells the service where it is (a text its app writes: a cell range, a paragraph) when that changes and every few
//! seconds, and keeps one question open: "tell me when anything about this document changes". The service answers it at once
//! when someone arrives, leaves or moves, or when the document is saved or changed, and the page asks again. So an update
//! reaches the others in a moment, and the page does not need to ask every 2 s whether the document changed (that stays as a
//! slow safety net, and as the way it works when the question cannot be held open).

use crate::{Cloud, Opened, TICK_MS, enc};
use quench_serde::Value;
use quench_serde::json;
use std::cell::{Cell, RefCell};

/// One other page that has the document open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub client: String,
    pub name: String,
    pub color: String,
    /// Where it is, as its app wrote it (may be empty).
    pub pos: String,
}

const COLOURS: [(&str, &str); 8] = [
    ("Red", "#d9534f"),
    ("Orange", "#e8833a"),
    ("Amber", "#c99a14"),
    ("Green", "#3fa34d"),
    ("Teal", "#1fa3a3"),
    ("Blue", "#3b82d6"),
    ("Violet", "#8a5fd1"),
    ("Pink", "#d6619b"),
];

const ANIMALS: [&str; 20] = [
    "Otter", "Fox", "Heron", "Lynx", "Badger", "Owl", "Seal", "Hare", "Wolf", "Crane", "Newt",
    "Stoat", "Finch", "Moose", "Gecko", "Bison", "Ibex", "Tapir", "Raven", "Panda",
];

/// How long the question is held open at most (milliseconds).
const HOLD_MS: u32 = 20_000;
/// How often the page says again where it is though it has not moved.
const BEAT_MS: u32 = 8_000;
/// How long to wait before asking again after a failure.
const RETRY_MS: u32 = 3_000;

/// This page's name for the others: a colour and an animal (or the person's own name when signed in), and its colour.
#[derive(Clone, Debug)]
pub struct Me {
    pub client: String,
    pub name: String,
    pub color: String,
}

/// What the page keeps about its place in the room.
#[derive(Default)]
pub struct Presence {
    me: RefCell<Option<Me>>,
    /// The document the room is for: its id, link and owner.
    room: RefCell<Option<(String, Option<String>, Option<String>)>>,
    /// Counts the rooms, so an answer from an earlier one is ignored.
    generation: Cell<u32>,
    rev: Cell<u64>,
    docs: Cell<u64>,
    busy: Cell<bool>,
    retry: Cell<u32>,
    since: Cell<u32>,
    sent: RefCell<Option<String>>,
    /// Whether the held question is working (then the slow safety net is enough).
    pub(crate) pushed: Cell<bool>,
    pub(crate) slow: Cell<u32>,
}

impl Cloud {
    fn me(&self) -> Me {
        if let Some(me) = self.presence.me.borrow().clone() {
            return me;
        }
        let client = random_id();
        let byte = |i: usize| {
            usize::from_str_radix(client.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0)
        };
        let (colour, hex) = COLOURS[byte(0) % COLOURS.len()];
        let name = self
            .context
            .user
            .clone()
            .unwrap_or_else(|| format!("{colour} {}", ANIMALS[byte(1) % ANIMALS.len()]));
        let me = Me {
            client,
            name,
            color: hex.to_string(),
        };
        *self.presence.me.borrow_mut() = Some(me.clone());
        me
    }

    /// This page's colour, for what it shows of itself.
    #[must_use]
    pub fn my_color(&self) -> String {
        self.me().color
    }

    /// The others, with names told apart: two with the same name get a number.
    #[must_use]
    pub fn others(&self) -> Vec<Peer> {
        let mine = self.me().name;
        let mut seen: Vec<String> = vec![mine];
        let mut list = self.peers.get();
        list.sort_by(|a, b| a.client.cmp(&b.client));
        for p in &mut list {
            let same = seen.iter().filter(|n| n.starts_with(&p.name)).count();
            seen.push(p.name.clone());
            if same > 0 {
                p.name = format!("{} {}", p.name, same + 1);
            }
        }
        list
    }

    fn position(&self) -> String {
        self.binding
            .borrow()
            .as_ref()
            .and_then(|b| b.position.as_ref().map(|f| f()))
            .unwrap_or_default()
    }

    fn say_where(&self, o: &Opened, pos: &str, gone: bool) {
        let me = self.me();
        let body = format!(
            "{{\"client\":{},\"name\":{},\"color\":{},\"pos\":{},\"gone\":{gone}}}",
            quoted(&me.client),
            quoted(&me.name),
            quoted(&me.color),
            quoted(pos)
        );
        let path = format!("{}/presence", Self::prefix(o));
        self.fetch(
            "POST",
            &path,
            vec![("content-type".into(), "application/json".into())],
            body.into_bytes(),
            |_, _| {},
        );
    }

    /// One step, every tick: follow the open document (say where the page is, keep the question open), or leave the room.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn presence_step(&self) {
        let p = &self.presence;
        let opened = self.opened.get_untracked();
        let room = opened
            .as_ref()
            .map(|o| (o.id.clone(), o.token.clone(), o.owner.clone()));
        if *p.room.borrow() != room {
            // A different document (or none): leave the old room, start the new one.
            let old = p.room.borrow().clone();
            if let Some((id, token, owner)) = old {
                let gone = Opened {
                    id,
                    etag: String::new(),
                    token,
                    owner,
                    edit: false,
                };
                self.say_where(&gone, "", true);
            }
            p.generation.set(p.generation.get().wrapping_add(1));
            p.rev.set(0);
            p.docs.set(0);
            p.busy.set(false);
            p.retry.set(0);
            p.pushed.set(false);
            *p.sent.borrow_mut() = None;
            *p.room.borrow_mut() = room;
            if !self.peers.get_untracked().is_empty() {
                self.peers.set(Vec::new());
            }
        }
        let Some(o) = opened else { return };
        let pos = self.position();
        p.since.set(p.since.get().saturating_add(TICK_MS));
        let moved = p.sent.borrow().as_deref() != Some(pos.as_str());
        if moved || p.since.get() >= BEAT_MS {
            p.since.set(0);
            *p.sent.borrow_mut() = Some(pos.clone());
            self.say_where(&o, &pos, false);
        }
        p.retry.set(p.retry.get().saturating_sub(TICK_MS));
        if p.busy.get() || p.retry.get() > 0 {
            return;
        }
        p.busy.set(true);
        let (generation, me) = (p.generation.get(), self.me());
        let path = format!(
            "{}/presence?client={}&seen={}&wait={HOLD_MS}",
            Self::prefix(&o),
            enc(&me.client),
            p.rev.get()
        );
        self.fetch("GET", &path, Vec::new(), Vec::new(), move |cloud, r| {
            let p = &cloud.presence;
            if p.generation.get() != generation {
                return;
            }
            p.busy.set(false);
            let parsed = (r.status == 200)
                .then(|| json::parse(&String::from_utf8_lossy(&r.body)).ok())
                .flatten();
            let Some(v) = parsed else {
                p.pushed.set(false);
                p.retry.set(RETRY_MS);
                return;
            };
            p.pushed.set(true);
            let number = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
            let (rev, docs) = (number("rev"), number("doc"));
            if rev == p.rev.get() {
                return;
            }
            p.rev.set(rev);
            let list: Vec<Peer> = v
                .get("peers")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            let s = |k: &str| {
                                x.get(k)
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string()
                            };
                            Peer {
                                client: s("client"),
                                name: s("name"),
                                color: s("color"),
                                pos: s("pos"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if cloud.peers.get_untracked() != list {
                cloud.peers.set(list);
            }
            if docs != p.docs.get() {
                p.docs.set(docs);
                // The document changed: look at once, not at the next poll.
                if cloud.following() {
                    cloud.catch_up();
                } else {
                    cloud.check_remote();
                }
            }
        });
    }
}

fn quoted(text: &str) -> String {
    json::to_string(&text.to_string())
}

/// 16 hex digits to tell this page from the others. The browser build has no OS randomness here, so it is made from the clock
/// (to the millisecond) mixed with a counter; two pages opened in the same millisecond are the only way to meet.
#[cfg(target_arch = "wasm32")]
fn random_id() -> String {
    thread_local! { static COUNT: Cell<u64> = const { Cell::new(0) }; }
    let n = COUNT.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    #[allow(clippy::cast_sign_loss)]
    let mut x = (quench_ui_web::browser::wall_ms() as u64) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // splitmix64
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    format!("{x:016x}")
}

#[cfg(not(target_arch = "wasm32"))]
fn random_id() -> String {
    office_cloud::new_id(8)
}
