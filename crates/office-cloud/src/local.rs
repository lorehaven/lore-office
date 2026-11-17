//! A directory per user, with a quota.

use crate::model::{
    DEFAULT_QUOTA_BYTES, DocMeta, Error, Grant, Share, Usage, clean_folder, new_id, now, safe_name,
};
use crate::store::{user_key, with_ancestors};
use crate::{Result, Storage};
use quench_core::async_trait;
use quench_serde::json;
use std::path::PathBuf;

/// Documents and shares in a directory: `users/<user>/<id>.bin` and `<id>.json` beside it, and `shares/<token>.json`.
#[derive(Clone, Debug)]
pub struct LocalStore {
    root: PathBuf,
    quota: u64,
}

fn io(e: &std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        Error::NotFound
    } else {
        Error::Io(e.to_string())
    }
}

fn etag_of(bytes: &[u8]) -> String {
    format!(
        "{:08x}-{}",
        quench_archive::checksum::crc32(bytes),
        bytes.len()
    )
}

impl LocalStore {
    /// A store under `root` (made when needed) that lets each user keep `quota` bytes.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, quota: u64) -> Self {
        Self {
            root: root.into(),
            quota,
        }
    }

    /// As [`LocalStore::new`] with the default quota, or `USER_QUOTA_BYTES` from the environment.
    #[must_use]
    pub fn from_env(root: impl Into<PathBuf>) -> Self {
        let quota = quench_core::env::get_or("USER_QUOTA_BYTES", "")
            .parse()
            .unwrap_or(DEFAULT_QUOTA_BYTES);
        Self::new(root, quota)
    }

    /// The directory of a user: a safe form of the name and a checksum of the real one, so two names never meet.
    fn user_dir(&self, user: &str) -> PathBuf {
        let tag = quench_archive::checksum::crc32(user.as_bytes());
        self.root
            .join("users")
            .join(format!("{}-{tag:08x}", safe_name(user)))
    }

    fn check_id(id: &str) -> Result<()> {
        if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(())
        } else {
            Err(Error::Invalid("a document id is hexadecimal".into()))
        }
    }

    fn read_meta(dir: &std::path::Path, id: &str) -> Result<DocMeta> {
        let text = std::fs::read_to_string(dir.join(format!("{id}.json"))).map_err(|e| io(&e))?;
        json::from_str(&text).map_err(|e| Error::Io(e.to_string()))
    }

    fn used(dir: &std::path::Path) -> u64 {
        std::fs::read_dir(dir).map_or(0, |d| {
            d.filter_map(std::result::Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "bin"))
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
    }

    fn ops_dir(&self, user: &str, id: &str) -> PathBuf {
        self.user_dir(user).join(format!("{id}.ops"))
    }

    fn grant_paths(&self, owner: &str, doc: &str, with: &str) -> Result<[PathBuf; 2]> {
        Self::check_id(doc)?;
        let (o, w) = (user_key(owner), user_key(with));
        Ok([
            self.root
                .join("grants")
                .join("to")
                .join(&w)
                .join(format!("{o}-{doc}.json")),
            self.root
                .join("grants")
                .join("by")
                .join(&o)
                .join(format!("{doc}-{w}.json")),
        ])
    }

    fn read_grants(dir: &std::path::Path, keep: impl Fn(&Grant) -> bool) -> Vec<Grant> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut out: Vec<Grant> = entries
            .filter_map(std::result::Result::ok)
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| json::from_str::<Grant>(&t).ok())
            .filter(|g| keep(g))
            .collect();
        out.sort_by_key(|g| std::cmp::Reverse(g.created));
        out
    }

    fn folders_file(&self, user: &str) -> PathBuf {
        self.user_dir(user).join("folders.json")
    }

    fn made_folders(&self, user: &str) -> Vec<String> {
        std::fs::read_to_string(self.folders_file(user))
            .ok()
            .and_then(|t| json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default()
    }

    fn share_path(&self, token: &str) -> Result<PathBuf> {
        if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(self.root.join("shares").join(format!("{token}.json")))
        } else {
            Err(Error::NotFound)
        }
    }
}

#[async_trait]
impl Storage for LocalStore {
    async fn list(&self, user: &str) -> Result<Vec<DocMeta>> {
        let dir = self.user_dir(user);
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(out);
        };
        for e in entries.filter_map(std::result::Result::ok) {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "json")
                && let Some(id) = path.file_stem().and_then(|s| s.to_str())
                && let Ok(meta) = Self::read_meta(&dir, id)
            {
                out.push(meta);
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    async fn get(&self, user: &str, id: &str) -> Result<(DocMeta, Vec<u8>)> {
        Self::check_id(id)?;
        let dir = self.user_dir(user);
        let meta = Self::read_meta(&dir, id)?;
        let bytes = std::fs::read(dir.join(format!("{id}.bin"))).map_err(|e| io(&e))?;
        Ok((meta, bytes))
    }

    async fn meta(&self, user: &str, id: &str) -> Result<DocMeta> {
        Self::check_id(id)?;
        Self::read_meta(&self.user_dir(user), id)
    }

    async fn put(
        &self,
        user: &str,
        id: Option<&str>,
        name: &str,
        kind: &str,
        bytes: &[u8],
        if_match: Option<&str>,
    ) -> Result<DocMeta> {
        if !matches!(kind, "docs" | "sheets" | "notes" | "pdf") {
            return Err(Error::Invalid("kind is docs, sheets, notes or pdf".into()));
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(Error::Invalid("a name of 1 to 200 characters".into()));
        }
        let dir = self.user_dir(user);
        std::fs::create_dir_all(&dir).map_err(|e| io(&e))?;
        let (id, before, folder, old_version) = match id {
            Some(id) => {
                Self::check_id(id)?;
                // A document that is gone (deleted elsewhere, or its description lost) is made again under the same id:
                // the person's text is worth more than the refusal.
                match Self::read_meta(&dir, id) {
                    Ok(old) => {
                        if let Some(want) = if_match
                            && want != old.etag
                        {
                            return Err(Error::Conflict { etag: old.etag });
                        }
                        (id.to_string(), old.size, old.folder, old.version)
                    }
                    Err(Error::NotFound) => (id.to_string(), 0, String::new(), 0),
                    Err(e) => return Err(e),
                }
            }
            None => (new_id(8), 0, String::new(), 0),
        };
        let used = Self::used(&dir).saturating_sub(before);
        if used + bytes.len() as u64 > self.quota {
            return Err(Error::Quota {
                used,
                quota: self.quota,
            });
        }
        let meta = DocMeta {
            id: id.clone(),
            name: name.to_string(),
            kind: kind.to_string(),
            size: bytes.len() as u64,
            modified: now(),
            etag: etag_of(bytes),
            folder,
            version: old_version,
        };
        // The bytes first, under a name of their own, then in place: a failure part way leaves the old document whole.
        let tmp = dir.join(format!("{id}.tmp"));
        std::fs::write(&tmp, bytes).map_err(|e| io(&e))?;
        std::fs::rename(&tmp, dir.join(format!("{id}.bin"))).map_err(|e| io(&e))?;
        std::fs::write(dir.join(format!("{id}.json")), json::to_string(&meta))
            .map_err(|e| io(&e))?;
        Ok(meta)
    }

    async fn delete(&self, user: &str, id: &str) -> Result<()> {
        Self::check_id(id)?;
        let dir = self.user_dir(user);
        Self::read_meta(&dir, id)?;
        std::fs::remove_file(dir.join(format!("{id}.bin"))).map_err(|e| io(&e))?;
        std::fs::remove_file(dir.join(format!("{id}.json"))).map_err(|e| io(&e))?;
        let _ = std::fs::remove_dir_all(self.ops_dir(user, id));
        // The links and the grants to it go with it.
        for share in self.shares_of(user).await? {
            if share.doc == id {
                let _ = self.delete_share(user, &share.token).await;
            }
        }
        for grant in self.grants_of(user, id).await? {
            let _ = self.delete_grant(user, id, &grant.with).await;
        }
        Ok(())
    }

    async fn usage(&self, user: &str) -> Result<Usage> {
        Ok(Usage {
            used: Self::used(&self.user_dir(user)),
            quota: self.quota,
        })
    }

    async fn put_share(&self, share: &Share) -> Result<()> {
        let path = self.share_path(&share.token)?;
        Self::read_meta(&self.user_dir(&share.owner), &share.doc)?;
        std::fs::create_dir_all(path.parent().unwrap_or(&self.root)).map_err(|e| io(&e))?;
        std::fs::write(path, json::to_string(share)).map_err(|e| io(&e))
    }

    async fn share(&self, token: &str) -> Result<Share> {
        let text = std::fs::read_to_string(self.share_path(token)?).map_err(|e| io(&e))?;
        json::from_str(&text).map_err(|e| Error::Io(e.to_string()))
    }

    async fn shares_of(&self, user: &str) -> Result<Vec<Share>> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.root.join("shares")) else {
            return Ok(out);
        };
        for e in entries.filter_map(std::result::Result::ok) {
            if let Ok(text) = std::fs::read_to_string(e.path())
                && let Ok(share) = json::from_str::<Share>(&text)
                && share.owner == user
            {
                out.push(share);
            }
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.created));
        Ok(out)
    }

    async fn delete_share(&self, user: &str, token: &str) -> Result<()> {
        let share = self.share(token).await?;
        if share.owner != user {
            return Err(Error::NotFound);
        }
        std::fs::remove_file(self.share_path(token)?).map_err(|e| io(&e))
    }

    async fn put_op(&self, owner: &str, id: &str, n: u64, bytes: &[u8]) -> Result<()> {
        Self::check_id(id)?;
        Self::read_meta(&self.user_dir(owner), id)?;
        let dir = self.ops_dir(owner, id);
        std::fs::create_dir_all(&dir).map_err(|e| io(&e))?;
        std::fs::write(dir.join(format!("{n:010}.json")), bytes).map_err(|e| io(&e))
    }

    async fn ops_after(&self, owner: &str, id: &str, after: u64) -> Result<Vec<(u64, Vec<u8>)>> {
        Self::check_id(id)?;
        let Ok(entries) = std::fs::read_dir(self.ops_dir(owner, id)) else {
            return Ok(Vec::new());
        };
        let mut out: Vec<(u64, Vec<u8>)> = entries
            .filter_map(std::result::Result::ok)
            .filter_map(|e| {
                let n: u64 = e.path().file_stem()?.to_str()?.parse().ok()?;
                (n > after).then(|| std::fs::read(e.path()).ok().map(|b| (n, b)))?
            })
            .collect();
        out.sort_by_key(|(n, _)| *n);
        Ok(out)
    }

    async fn ops_head(&self, owner: &str, id: &str) -> Result<u64> {
        Self::check_id(id)?;
        Ok(
            std::fs::read_dir(self.ops_dir(owner, id)).map_or(0, |entries| {
                entries
                    .filter_map(std::result::Result::ok)
                    .filter_map(|e| e.path().file_stem()?.to_str()?.parse::<u64>().ok())
                    .max()
                    .unwrap_or(0)
            }),
        )
    }

    async fn drop_ops(&self, owner: &str, id: &str, upto: u64) -> Result<()> {
        Self::check_id(id)?;
        let Ok(entries) = std::fs::read_dir(self.ops_dir(owner, id)) else {
            return Ok(());
        };
        for e in entries.filter_map(std::result::Result::ok) {
            if e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok())
                .is_some_and(|n| n <= upto)
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
        Ok(())
    }

    async fn set_version(&self, owner: &str, id: &str, version: u64) -> Result<DocMeta> {
        Self::check_id(id)?;
        let dir = self.user_dir(owner);
        let mut meta = Self::read_meta(&dir, id)?;
        meta.version = version;
        std::fs::write(dir.join(format!("{id}.json")), json::to_string(&meta))
            .map_err(|e| io(&e))?;
        Ok(meta)
    }

    async fn folders(&self, user: &str) -> Result<Vec<String>> {
        let mut all = self.made_folders(user);
        all.extend(self.list(user).await?.into_iter().map(|d| d.folder));
        Ok(with_ancestors(all))
    }

    async fn make_folder(&self, user: &str, path: &str) -> Result<()> {
        let path = clean_folder(path)?;
        if path.is_empty() {
            return Err(Error::Invalid("a folder needs a name".into()));
        }
        let mut made = self.made_folders(user);
        if !made.contains(&path) {
            made.push(path);
        }
        std::fs::create_dir_all(self.user_dir(user)).map_err(|e| io(&e))?;
        std::fs::write(self.folders_file(user), json::to_string(&made)).map_err(|e| io(&e))
    }

    async fn delete_folder(&self, user: &str, path: &str) -> Result<()> {
        let path = clean_folder(path)?;
        let inside = |p: &str| p == path || p.starts_with(&format!("{path}/"));
        if path.is_empty() || self.list(user).await?.iter().any(|d| inside(&d.folder)) {
            return Err(Error::Invalid("the folder is not empty".into()));
        }
        let made: Vec<String> = self
            .made_folders(user)
            .into_iter()
            .filter(|p| !inside(p))
            .collect();
        std::fs::write(self.folders_file(user), json::to_string(&made)).map_err(|e| io(&e))
    }

    async fn move_doc(&self, user: &str, id: &str, folder: &str) -> Result<DocMeta> {
        Self::check_id(id)?;
        let folder = clean_folder(folder)?;
        let dir = self.user_dir(user);
        let mut meta = Self::read_meta(&dir, id)?;
        meta.folder = folder;
        std::fs::write(dir.join(format!("{id}.json")), json::to_string(&meta))
            .map_err(|e| io(&e))?;
        Ok(meta)
    }

    async fn put_grant(&self, grant: &Grant) -> Result<()> {
        Self::read_meta(&self.user_dir(&grant.owner), &grant.doc)?;
        for path in self.grant_paths(&grant.owner, &grant.doc, &grant.with)? {
            std::fs::create_dir_all(path.parent().unwrap_or(&self.root)).map_err(|e| io(&e))?;
            std::fs::write(path, json::to_string(grant)).map_err(|e| io(&e))?;
        }
        Ok(())
    }

    async fn grants_to(&self, user: &str) -> Result<Vec<Grant>> {
        let dir = self.root.join("grants").join("to").join(user_key(user));
        Ok(Self::read_grants(&dir, |g| g.with == user))
    }

    async fn grants_of(&self, owner: &str, doc: &str) -> Result<Vec<Grant>> {
        let dir = self.root.join("grants").join("by").join(user_key(owner));
        Ok(Self::read_grants(&dir, |g| {
            g.owner == owner && g.doc == doc
        }))
    }

    async fn grant(&self, owner: &str, doc: &str, with: &str) -> Result<Grant> {
        let [to, _] = self.grant_paths(owner, doc, with)?;
        let text = std::fs::read_to_string(to).map_err(|e| io(&e))?;
        let grant: Grant = json::from_str(&text).map_err(|e| Error::Io(e.to_string()))?;
        if grant.owner == owner && grant.with == with {
            Ok(grant)
        } else {
            Err(Error::NotFound)
        }
    }

    async fn delete_grant(&self, owner: &str, doc: &str, with: &str) -> Result<()> {
        let [to, by] = self.grant_paths(owner, doc, with)?;
        if !to.exists() {
            return Err(Error::NotFound);
        }
        let _ = std::fs::remove_file(by);
        std::fs::remove_file(to).map_err(|e| io(&e))
    }
}
