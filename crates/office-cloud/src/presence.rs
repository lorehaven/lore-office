//! Who has a document open, and where in it: kept in memory only, advisory only.
//!
//! Each page that has a document open sends where it is (a short text its app understands: a cell range, a paragraph) now
//! and then, and asks to be told when anything about the document changes: someone else's place, someone arriving or
//! leaving, or the document itself (the service tells the room when a save or a change is made). That question is held open
//! until there is something to tell, so an update reaches the others at once and nothing needs asking every few seconds.
//!
//! Nothing here is written down: a restart of the service empties it, and the pages send themselves again. It assumes one
//! replica, like the change log.

#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};

/// How long a page that has not been heard from stays in the list.
const EXPIRES: Duration = Duration::from_secs(12);

/// One page that has the document open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub client: String,
    pub name: String,
    pub color: String,
    /// Where it is in the document, as its app wrote it (may be empty).
    pub pos: String,
}

struct Seen {
    peer: Peer,
    at: Instant,
}

struct Room {
    /// Changes whenever something about the room does: a page, a place, the document.
    rev: u64,
    /// How many times the document itself changed.
    docs: u64,
    peers: HashMap<String, Seen>,
    wake: Arc<Notify>,
}

impl Room {
    fn new() -> Self {
        Self {
            rev: 1,
            docs: 0,
            peers: HashMap::new(),
            wake: Arc::new(Notify::new()),
        }
    }

    fn changed(&mut self) {
        self.rev += 1;
        self.wake.notify_waiters();
    }

    fn prune(&mut self) {
        let before = self.peers.len();
        self.peers.retain(|_, s| s.at.elapsed() < EXPIRES);
        if self.peers.len() != before {
            self.changed();
        }
    }
}

/// The rooms, one per document.
#[derive(Default)]
pub struct Hub {
    rooms: Mutex<HashMap<(String, String), Room>>,
}

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub").finish_non_exhaustive()
    }
}

/// A short name: printable characters, at most 24.
fn clean_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
    n.trim().to_string()
}

/// `#rgb` or `#rrggbb`, else grey.
fn clean_color(color: &str) -> String {
    let hex = color.strip_prefix('#').unwrap_or("");
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        color.to_string()
    } else {
        "#888888".to_string()
    }
}

impl Hub {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The document changed (a save, a change added, deleted): tell those who are waiting.
    pub async fn touch(&self, owner: &str, id: &str) {
        let mut rooms = self.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&(owner.to_string(), id.to_string())) {
            room.docs += 1;
            room.changed();
        }
    }

    /// A page says it is here (or, with `gone`, that it left), and where. Returns the room's number.
    pub async fn update(&self, owner: &str, id: &str, peer: Peer, gone: bool) -> u64 {
        let peer = Peer {
            name: clean_name(&peer.name),
            color: clean_color(&peer.color),
            pos: peer.pos.chars().take(200).collect(),
            client: peer
                .client
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(32)
                .collect(),
        };
        let mut rooms = self.rooms.lock().await;
        let room = rooms
            .entry((owner.to_string(), id.to_string()))
            .or_insert_with(Room::new);
        if gone {
            if room.peers.remove(&peer.client).is_some() {
                room.changed();
            }
        } else {
            let differs = room.peers.get(&peer.client).is_none_or(|s| s.peer != peer);
            room.peers.insert(
                peer.client.clone(),
                Seen {
                    peer,
                    at: Instant::now(),
                },
            );
            if differs {
                room.changed();
            }
        }
        let rev = room.rev;
        if room.peers.is_empty() {
            rooms.remove(&(owner.to_string(), id.to_string()));
        }
        rev
    }

    /// The room's number, how many times the document changed, and the others in the room: when the number is not `seen`
    /// (now if it already differs, else when something changes), else after `limit` (the same number then). `client` is kept alive by asking.
    pub async fn wait(
        &self,
        owner: &str,
        id: &str,
        client: &str,
        seen: u64,
        limit: Duration,
    ) -> (u64, u64, Vec<Peer>) {
        let key = (owner.to_string(), id.to_string());
        let deadline = Instant::now() + limit;
        loop {
            let wake = {
                let mut rooms = self.rooms.lock().await;
                let room = rooms.entry(key.clone()).or_insert_with(Room::new);
                if let Some(me) = room.peers.get_mut(client) {
                    me.at = Instant::now();
                }
                room.prune();
                let others = || {
                    let mut list: Vec<Peer> = room
                        .peers
                        .values()
                        .filter(|s| s.peer.client != client)
                        .map(|s| s.peer.clone())
                        .collect();
                    list.sort_by(|a, b| a.client.cmp(&b.client));
                    list
                };
                if room.rev != seen || Instant::now() >= deadline {
                    let result = (room.rev, room.docs, others());
                    if room.peers.is_empty() {
                        rooms.remove(&key);
                    }
                    return result;
                }
                let wake = room.wake.clone();
                let mut n = Box::pin(wake.clone().notified_owned());
                n.as_mut().enable();
                drop(rooms);
                n
            };
            let slice = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(1));
            let _ = tokio::time::timeout(slice, wake).await;
        }
    }
}
