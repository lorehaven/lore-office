//! The log of changes beside a document's stored file.
//!
//! A document in the cloud is its last full file (the snapshot, at a `version`) and the changes made since, numbered one after
//! the other. The service numbers them, so every page applies them in the same order; a change that began from a version that
//! is no longer the latest is refused with the latest (`Error::Behind`), and its maker catches up first. Now and then a page
//! uploads a new full file as the snapshot at the latest version and the older changes are dropped.
//!
//! One service replica is assumed: the latest numbers are kept in memory, and one lock orders the appends.

#![allow(clippy::significant_drop_tightening)]

use crate::model::{DocMeta, Error};
use crate::{Result, Storage};
use std::collections::HashMap;
use tokio::sync::Mutex;

/// What a page asking for the changes after some number is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Since {
    /// The changes after it (maybe none), with the latest number and the version of the stored file.
    Ops {
        head: u64,
        version: u64,
        ops: Vec<(u64, Vec<u8>)>,
    },
    /// The changes it needs were folded into the stored file: it should fetch the file again.
    Gone { head: u64, version: u64 },
}

/// The latest number of each document's log, and the order of changes to it.
#[derive(Debug, Default)]
pub struct OpLog {
    heads: Mutex<HashMap<(String, String), u64>>,
}

impl OpLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    async fn head_in(
        heads: &mut HashMap<(String, String), u64>,
        store: &dyn Storage,
        owner: &str,
        id: &str,
    ) -> Result<u64> {
        let key = (owner.to_string(), id.to_string());
        if let Some(h) = heads.get(&key) {
            return Ok(*h);
        }
        let version = store.meta(owner, id).await?.version;
        let head = store.ops_head(owner, id).await?.max(version);
        heads.insert(key, head);
        Ok(head)
    }

    /// The latest number of the document's log: the version of the stored file when no change was made since.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when there is no such document.
    pub async fn head(&self, store: &dyn Storage, owner: &str, id: &str) -> Result<u64> {
        Self::head_in(&mut *self.heads.lock().await, store, owner, id).await
    }

    /// Add a change that began from version `base`; the number it gets.
    ///
    /// # Errors
    ///
    /// [`Error::Behind`] when `base` is not the latest.
    pub async fn append(
        &self,
        store: &dyn Storage,
        owner: &str,
        id: &str,
        base: u64,
        op: &[u8],
    ) -> Result<u64> {
        let mut heads = self.heads.lock().await;
        let head = Self::head_in(&mut heads, store, owner, id).await?;
        if base != head {
            return Err(Error::Behind { head });
        }
        let n = head + 1;
        store.put_op(owner, id, n, op).await?;
        heads.insert((owner.to_string(), id.to_string()), n);
        Ok(n)
    }

    /// The changes after number `after`.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when there is no such document.
    pub async fn since(
        &self,
        store: &dyn Storage,
        owner: &str,
        id: &str,
        after: u64,
    ) -> Result<Since> {
        let mut heads = self.heads.lock().await;
        let head = Self::head_in(&mut heads, store, owner, id).await?;
        let version = store.meta(owner, id).await?.version;
        if after < version {
            return Ok(Since::Gone { head, version });
        }
        let ops = if after >= head {
            Vec::new()
        } else {
            store.ops_after(owner, id, after).await?
        };
        Ok(Since::Ops { head, version, ops })
    }

    /// Save a full file as the new snapshot.
    ///
    /// With `at`, it is the document at that version of the log (what a page that applied every change sends now and then):
    /// refused with [`Error::Behind`] when changes were added since, else the changes up to it are dropped. Without, it
    /// replaces the document whole: it is a new version after every change, so pages that follow the log fetch it again.
    ///
    /// # Errors
    ///
    /// [`Error::Behind`], and those of [`Storage::put`].
    #[allow(clippy::too_many_arguments)]
    pub async fn snapshot(
        &self,
        store: &dyn Storage,
        owner: &str,
        id: &str,
        name: &str,
        kind: &str,
        bytes: &[u8],
        at: Option<u64>,
        if_match: Option<&str>,
    ) -> Result<(DocMeta, u64)> {
        let mut heads = self.heads.lock().await;
        let head = Self::head_in(&mut heads, store, owner, id).await?;
        if let Some(at) = at
            && at != head
        {
            return Err(Error::Behind { head });
        }
        let mut meta = store
            .put(owner, Some(id), name, kind, bytes, if_match)
            .await?;
        // A document that never had a log needs no version: nobody follows it.
        if at.is_none() && head == 0 {
            return Ok((meta, 0));
        }
        let version = at.unwrap_or(head + 1);
        if meta.version != version {
            meta = store.set_version(owner, id, version).await?;
        }
        store.drop_ops(owner, id, version).await?;
        heads.insert((owner.to_string(), id.to_string()), version);
        Ok((meta, version))
    }

    /// Forget what is known of a document (it was deleted).
    pub async fn forget(&self, owner: &str, id: &str) {
        self.heads
            .lock()
            .await
            .remove(&(owner.to_string(), id.to_string()));
    }
}
