//! Documents in the cloud, and the links that share them.
//!
//! A document is bytes (a `.docx`, `.xlsx`, `.md`, `.pdf`) with a name, a kind and an `etag` that changes when the bytes do; a save
//! says which `etag` it started from (`if_match`), so two people saving one document do not silently overwrite each other.
//! Everything a person has lives under their own user name, with a quota (10 GiB unless told otherwise). A [`Share`] is a random
//! token that opens one document with [`Rights::View`] or [`Rights::Edit`] for whoever has the link, signed in or not.

#![allow(clippy::module_name_repetitions)]

#[cfg(feature = "server")]
mod local;
mod model;
#[cfg(feature = "server")]
mod warehouse;

#[cfg(feature = "server")]
pub use local::LocalStore;
pub use model::{
    DEFAULT_QUOTA_BYTES, DocMeta, Error, Grant, Rights, Share, Usage, clean_folder, new_id, now,
    safe_name,
};
#[cfg(feature = "server")]
pub use warehouse::WarehouseStore;

#[cfg(feature = "server")]
mod oplog;
#[cfg(feature = "server")]
mod presence;
#[cfg(feature = "server")]
pub use presence::{Hub, Peer};
#[cfg(feature = "server")]
mod store;
#[cfg(feature = "server")]
pub use oplog::{OpLog, Since};
#[cfg(feature = "server")]
pub use store::{
    Result, Storage, grant_doc, granted_meta, open_granted, open_share, save_granted, save_share,
    share_meta, user_key,
};
