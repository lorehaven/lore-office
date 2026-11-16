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
    DEFAULT_QUOTA_BYTES, DocMeta, Error, Rights, Share, Usage, new_id, now, safe_name,
};
#[cfg(feature = "server")]
pub use warehouse::WarehouseStore;

#[cfg(feature = "server")]
mod store;
#[cfg(feature = "server")]
pub use store::{Result, Storage, open_share, save_share};
