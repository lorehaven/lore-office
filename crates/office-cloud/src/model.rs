//! The things the cloud holds, and why a request fails.

use quench_serde::{Deserialize, Serialize};
use std::fmt;

/// 10 GiB: what a person may keep unless the operator says otherwise.
pub const DEFAULT_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// A document without its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocMeta {
    pub id: String,
    pub name: String,
    /// The app that opens it: `docs`, `sheets`, `notes` or `pdf`.
    pub kind: String,
    pub size: u64,
    /// Seconds since 1970.
    pub modified: u64,
    /// Changes when the bytes do.
    pub etag: String,
    /// The folder it is in (`Reports/2026`); empty for the top.
    #[serde(default)]
    pub folder: String,
}

/// What a share link allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rights {
    View,
    Edit,
}

impl Rights {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "view" => Some(Self::View),
            "edit" => Some(Self::Edit),
            _ => None,
        }
    }

    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
        }
    }
}

/// A link: whoever has `token` has `rights` on the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub token: String,
    pub owner: String,
    pub doc: String,
    pub name: String,
    pub kind: String,
    pub rights: Rights,
    pub created: u64,
    /// When the link stops working (seconds since 1970); `None` for never.
    pub expires: Option<u64>,
}

impl Share {
    #[must_use]
    pub fn expired(&self, now: u64) -> bool {
        self.expires.is_some_and(|e| e <= now)
    }
}

/// A document shared with one named person: it stays in the owner's cloud and shows in that person's panel under "shared with
/// me", with the rights given.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub owner: String,
    pub doc: String,
    /// Who it is shared with.
    pub with: String,
    pub name: String,
    pub kind: String,
    pub rights: Rights,
    pub created: u64,
}

/// A folder path made fit to keep: no empty parts, no `.` or `..`, nothing a file name cannot have, at most 8 levels and 64
/// characters a level. The top is the empty string.
///
/// # Errors
///
/// [`Error::Invalid`] when the path is not one.
pub fn clean_folder(path: &str) -> Result<String, Error> {
    let mut parts: Vec<String> = Vec::new();
    for part in path.split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part == "."
            || part == ".."
            || part.chars().count() > 64
            || part
                .chars()
                .any(|c| c.is_control() || "\\:*?\"<>|".contains(c))
        {
            return Err(Error::Invalid(
                "a folder name has no / \\ : * ? \" < > | and is at most 64 characters".into(),
            ));
        }
        parts.push(part.to_string());
    }
    if parts.len() > 8 {
        return Err(Error::Invalid("folders go at most 8 levels deep".into()));
    }
    Ok(parts.join("/"))
}

/// Space used and allowed, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub used: u64,
    pub quota: u64,
}

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NotFound,
    /// The document changed since the `etag` the caller started from.
    Conflict {
        etag: String,
    },
    /// There is not room: this much is used of this much.
    Quota {
        used: u64,
        quota: u64,
    },
    /// A link that only views.
    Denied,
    Invalid(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Conflict { .. } => f.write_str("the document was changed by someone else"),
            Self::Quota { used, quota } => write!(f, "no room: {used} of {quota} bytes used"),
            Self::Denied => f.write_str("this link only lets you view"),
            Self::Invalid(why) => write!(f, "invalid: {why}"),
            Self::Io(why) => write!(f, "storage failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// Seconds since 1970.
#[must_use]
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A random identifier: `bytes` bytes as hex (8 for a document, 32 for a link's token).
#[must_use]
pub fn new_id(bytes: usize) -> String {
    let mut buf = vec![0_u8; bytes];
    quench_core::random::fill(&mut buf);
    buf.iter()
        .fold(String::with_capacity(bytes * 2), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// A name fit for a file or a storage: letters, digits and `-_.`, never empty, never longer than 64.
#[must_use]
pub fn safe_name(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    out.truncate(64);
    if out.is_empty() || out.chars().all(|c| c == '.') {
        out = "_".into();
    }
    out
}
