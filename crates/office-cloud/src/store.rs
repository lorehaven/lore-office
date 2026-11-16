//! The storage behind the cloud, and the rules over it.

use crate::{DocMeta, Error, Rights, Share, Usage, now};
use quench_core::async_trait;

pub type Result<T> = std::result::Result<T, Error>;

/// Where documents and shares are kept.
#[async_trait]
pub trait Storage: Send + Sync {
    /// The user's documents, newest first.
    async fn list(&self, user: &str) -> Result<Vec<DocMeta>>;
    /// A document and its bytes.
    async fn get(&self, user: &str, id: &str) -> Result<(DocMeta, Vec<u8>)>;
    /// Save a document: a new one when `id` is `None`, else the one named, which must still be at `if_match` when that is given.
    async fn put(
        &self,
        user: &str,
        id: Option<&str>,
        name: &str,
        kind: &str,
        bytes: &[u8],
        if_match: Option<&str>,
    ) -> Result<DocMeta>;
    async fn delete(&self, user: &str, id: &str) -> Result<()>;
    /// How much space the user has used and may use.
    async fn usage(&self, user: &str) -> Result<Usage>;
    /// Keep a share (the document it names must exist).
    async fn put_share(&self, share: &Share) -> Result<()>;
    async fn share(&self, token: &str) -> Result<Share>;
    async fn shares_of(&self, user: &str) -> Result<Vec<Share>>;
    async fn delete_share(&self, user: &str, token: &str) -> Result<()>;
}

/// What a share link lets its holder do, resolved: the document and whether it may be changed.
///
/// # Errors
///
/// [`Error::NotFound`] for a link nobody made or that was taken back or has run out.
pub async fn open_share(store: &dyn Storage, token: &str) -> Result<(Share, DocMeta, Vec<u8>)> {
    let share = store.share(token).await?;
    if share.expired(now()) {
        return Err(Error::NotFound);
    }
    let (meta, bytes) = store.get(&share.owner, &share.doc).await?;
    Ok((share, meta, bytes))
}

/// Save through a share link: only a link with edit rights may.
///
/// # Errors
///
/// [`Error::Denied`] for a view link; otherwise as [`Storage::put`].
pub async fn save_share(
    store: &dyn Storage,
    token: &str,
    bytes: &[u8],
    if_match: Option<&str>,
) -> Result<DocMeta> {
    let share = store.share(token).await?;
    if share.expired(now()) {
        return Err(Error::NotFound);
    }
    if share.rights != Rights::Edit {
        return Err(Error::Denied);
    }
    let (meta, _) = store.get(&share.owner, &share.doc).await?;
    store
        .put(
            &share.owner,
            Some(&share.doc),
            &meta.name,
            &meta.kind,
            bytes,
            if_match,
        )
        .await
}
