//! The storage behind the cloud, and the rules over it.

use crate::{DocMeta, Error, Grant, Rights, Share, Usage, now, safe_name};
use quench_core::async_trait;

pub type Result<T> = std::result::Result<T, Error>;

/// Where documents and shares are kept.
#[async_trait]
pub trait Storage: Send + Sync {
    /// The user's documents, newest first.
    async fn list(&self, user: &str) -> Result<Vec<DocMeta>>;
    /// A document and its bytes.
    async fn get(&self, user: &str, id: &str) -> Result<(DocMeta, Vec<u8>)>;
    /// A document without its bytes: what a page asks every few seconds to see whether someone else saved.
    async fn meta(&self, user: &str, id: &str) -> Result<DocMeta>;
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
    /// Every folder the user has: the ones they made and the ones their documents are in, with the ones above them, sorted.
    async fn folders(&self, user: &str) -> Result<Vec<String>>;
    /// Make a folder (and those above it).
    async fn make_folder(&self, user: &str, path: &str) -> Result<()>;
    /// Take a folder away; it must hold no document and no folder.
    async fn delete_folder(&self, user: &str, path: &str) -> Result<()>;
    /// Keep change number `n` of a document's log (a new file: it is never rewritten).
    async fn put_op(&self, owner: &str, id: &str, n: u64, bytes: &[u8]) -> Result<()>;
    /// The changes after number `after`, oldest first.
    async fn ops_after(&self, owner: &str, id: &str, after: u64) -> Result<Vec<(u64, Vec<u8>)>>;
    /// The number of the latest change kept, or 0 when there is none.
    async fn ops_head(&self, owner: &str, id: &str) -> Result<u64>;
    /// Take away the changes up to and including `upto` (they are in the stored file now).
    async fn drop_ops(&self, owner: &str, id: &str, upto: u64) -> Result<()>;
    /// Say which version of the log the stored file stands for.
    async fn set_version(&self, owner: &str, id: &str, version: u64) -> Result<DocMeta>;
    /// Put a document in a folder (`""` is the top). The bytes and the `etag` stay as they are.
    async fn move_doc(&self, user: &str, id: &str, folder: &str) -> Result<DocMeta>;
    /// Keep a grant (the document it names must exist).
    async fn put_grant(&self, grant: &Grant) -> Result<()>;
    /// What others have shared with the user.
    async fn grants_to(&self, user: &str) -> Result<Vec<Grant>>;
    /// Who the owner has shared the document with.
    async fn grants_of(&self, owner: &str, doc: &str) -> Result<Vec<Grant>>;
    /// The grant of `owner`'s document to `with`.
    async fn grant(&self, owner: &str, doc: &str, with: &str) -> Result<Grant>;
    async fn delete_grant(&self, owner: &str, doc: &str, with: &str) -> Result<()>;
}

/// A name as it is kept on disk and in paths: safe characters, and a checksum of the real name so two names that look alike
/// stay apart.
#[must_use]
pub fn user_key(user: &str) -> String {
    format!(
        "{}-{:08x}",
        safe_name(user),
        quench_archive::checksum::crc32(user.as_bytes())
    )
}

/// Share the owner's document with `with`.
///
/// # Errors
///
/// [`Error::Invalid`] for oneself or an empty name; [`Error::NotFound`] when the owner has no such document.
pub async fn grant_doc(
    store: &dyn Storage,
    owner: &str,
    id: &str,
    with: &str,
    rights: Rights,
) -> Result<Grant> {
    let with = with.trim();
    if with.is_empty() || with.len() > 80 || with.chars().any(char::is_control) {
        return Err(Error::Invalid("name the person to share with".into()));
    }
    if with == owner {
        return Err(Error::Invalid("it is already yours".into()));
    }
    let meta = store.meta(owner, id).await?;
    let grant = Grant {
        owner: owner.to_string(),
        doc: meta.id,
        with: with.to_string(),
        name: meta.name,
        kind: meta.kind,
        rights,
        created: now(),
    };
    store.put_grant(&grant).await?;
    Ok(grant)
}

/// What `user` may do with `owner`'s document `id`, resolved: the grant, the document and its bytes.
///
/// # Errors
///
/// [`Error::NotFound`] when nothing was shared with `user`.
pub async fn open_granted(
    store: &dyn Storage,
    user: &str,
    owner: &str,
    id: &str,
) -> Result<(Grant, DocMeta, Vec<u8>)> {
    let grant = store.grant(owner, id, user).await?;
    let (meta, bytes) = store.get(owner, id).await?;
    Ok((grant, meta, bytes))
}

/// As [`open_granted`], without the bytes.
///
/// # Errors
///
/// As [`open_granted`].
pub async fn granted_meta(
    store: &dyn Storage,
    user: &str,
    owner: &str,
    id: &str,
) -> Result<(Grant, DocMeta)> {
    let grant = store.grant(owner, id, user).await?;
    let meta = store.meta(owner, id).await?;
    Ok((grant, meta))
}

/// Save into `owner`'s document `id` as `user`: only a grant with edit rights may; it is saved in the owner's cloud, under the
/// owner's quota.
///
/// # Errors
///
/// [`Error::NotFound`] without a grant, [`Error::Denied`] for a view grant; otherwise as [`Storage::put`].
pub async fn save_granted(
    store: &dyn Storage,
    user: &str,
    owner: &str,
    id: &str,
    bytes: &[u8],
    if_match: Option<&str>,
) -> Result<DocMeta> {
    let grant = store.grant(owner, id, user).await?;
    if grant.rights != Rights::Edit {
        return Err(Error::Denied);
    }
    let meta = store.meta(owner, id).await?;
    store
        .put(owner, Some(id), &meta.name, &meta.kind, bytes, if_match)
        .await
}

/// The folders of the user from what they made and the folders of their documents.
pub fn with_ancestors(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut all = std::collections::BTreeSet::new();
    for path in paths {
        let mut here = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            if !here.is_empty() {
                here.push('/');
            }
            here.push_str(part);
            all.insert(here.clone());
        }
    }
    all.into_iter().collect()
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

/// As [`open_share`], without the bytes.
///
/// # Errors
///
/// As [`open_share`].
pub async fn share_meta(store: &dyn Storage, token: &str) -> Result<(Share, DocMeta)> {
    let share = store.share(token).await?;
    if share.expired(now()) {
        return Err(Error::NotFound);
    }
    let meta = store.meta(&share.owner, &share.doc).await?;
    Ok((share, meta))
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
