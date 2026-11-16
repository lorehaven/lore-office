#![allow(
    clippy::significant_drop_tightening,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::or_fun_call
)]
//! The same storage on a warehouse: one dynamic storage per user, made the first time the user saves.
//!
//! Warehouse's files API: `POST /api/v1/files` makes a storage (`name`, `owner`, `quota_bytes`), `PUT|GET|DELETE
//! /api/v1/files/{storage}/file?path=` keep files, `GET /api/v1/files/{storage}?prefix=` lists them. Documents are `docs/<id>.bin`
//! and `docs/<id>.json`; links are in one storage of their own, `shares/<token>.json`, so a link can be found from its token
//! alone. The calls are made as the service's own machine identity (a gatehouse client with `warehouse:write`); which user
//! may touch which storage is decided here, by the name of the storage, never by what the caller asks for.

use crate::model::{
    DEFAULT_QUOTA_BYTES, DocMeta, Error, Grant, Share, Usage, clean_folder, new_id, now, safe_name,
};
use crate::store::{user_key, with_ancestors};
use crate::{Result, Storage};
use quench_client::transport::Client;
use quench_core::async_trait;
use quench_serde::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
struct Token {
    value: String,
    until: std::time::Instant,
}

/// The warehouse behind the cloud.
#[derive(Debug, Clone)]
pub struct WarehouseStore {
    http: Client,
    /// `https://host/warehouse`
    base: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    quota: u64,
    shares_storage: String,
    token: Arc<Mutex<Option<Token>>>,
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Io(e.to_string())
}

fn etag_of(bytes: &[u8]) -> String {
    format!(
        "{:08x}-{}",
        quench_archive::checksum::crc32(bytes),
        bytes.len()
    )
}

impl WarehouseStore {
    /// From the environment: `WAREHOUSE_URL`, `GATEHOUSE_URL`, `WAREHOUSE_CLIENT_ID` / `WAREHOUSE_CLIENT_SECRET`, and
    /// `USER_QUOTA_BYTES` (10 GiB).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when an address is missing; [`Error::Io`] when the client cannot be made.
    pub fn from_env() -> Result<Self> {
        let get = |k: &str| quench_core::env::get_or(k, "");
        let (base, gate) = (get("WAREHOUSE_URL"), get("GATEHOUSE_URL"));
        if base.is_empty() || gate.is_empty() {
            return Err(Error::Invalid(
                "WAREHOUSE_URL and GATEHOUSE_URL are needed".into(),
            ));
        }
        Ok(Self {
            http: Client::builder().build().map_err(err)?,
            base: base.trim_end_matches('/').to_string(),
            token_url: format!("{}/api/v1/token", gate.trim_end_matches('/')),
            client_id: get("WAREHOUSE_CLIENT_ID"),
            client_secret: get("WAREHOUSE_CLIENT_SECRET"),
            quota: get("USER_QUOTA_BYTES")
                .parse()
                .unwrap_or(DEFAULT_QUOTA_BYTES),
            shares_storage: safe_name(&quench_core::env::get_or(
                "SHARES_STORAGE",
                "lore-office-shares",
            )),
            token: Arc::new(Mutex::new(None)),
        })
    }

    /// The name of a user's storage.
    #[must_use]
    pub fn storage_of(user: &str) -> String {
        // Warehouse allows letters, digits, `-` and `_` in a storage name (no dots, which `safe_name` keeps), so a name like
        // `ada.lovelace` is made `ada_lovelace`; the checksum of the real name keeps two names that look alike apart.
        let part: String = user
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .take(48)
            .collect();
        format!(
            "lore-{}-{:08x}",
            if part.is_empty() { "user" } else { &part },
            quench_archive::checksum::crc32(user.as_bytes())
        )
    }

    async fn bearer(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref()
            && t.until > std::time::Instant::now()
        {
            return Ok(t.value.clone());
        }
        let form = format!(
            "grant_type=client_credentials&client_id={}&client_secret={}",
            self.client_id, self.client_secret
        );
        let res = self
            .http
            .post(&self.token_url)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.into_bytes())
            .send()
            .await
            .map_err(err)?;
        if !res.status().is_success() {
            return Err(Error::Io(format!("gatehouse answered {}", res.status())));
        }
        let body = res.text().await.map_err(err)?;
        let value = json::parse(&body).map_err(err)?;
        let access = value
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Io("no access token".into()))?
            .to_string();
        let secs = value
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(300)
            .max(60);
        *guard = Some(Token {
            value: access.clone(),
            until: std::time::Instant::now()
                + std::time::Duration::from_secs(u64::try_from(secs - 30).unwrap_or(30)),
        });
        Ok(access)
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: &str,
    ) -> Result<(u16, Vec<u8>)> {
        let token = self.bearer().await?;
        let url = format!("{}{path}", self.base);
        let mut req = match method {
            "GET" => self.http.get(&url),
            "PUT" => self.http.put(&url),
            "POST" => self.http.post(&url),
            _ => self.http.delete(&url),
        }
        .bearer_auth(token);
        if let Some(b) = body {
            req = req.header("content-type", content_type).body(b);
        }
        let res = req.send().await.map_err(err)?;
        let status = res.status().as_u16();
        let bytes = res.bytes().await.map_err(err)?.to_vec();
        Ok((status, bytes))
    }

    fn grant_paths(owner: &str, doc: &str, with: &str) -> [String; 2] {
        let (o, w) = (user_key(owner), user_key(with));
        [
            format!("grants/to/{w}/{o}-{doc}.json"),
            format!("grants/by/{o}/{doc}-{w}.json"),
        ]
    }

    /// The grants kept under `prefix` in the shares storage.
    async fn read_grants(&self, prefix: &str) -> Result<Vec<Grant>> {
        let mut out = Vec::new();
        for path in self.names(&self.shares_storage, prefix).await? {
            if let Ok(bytes) = self.read(&self.shares_storage, &path).await
                && let Ok(grant) = json::from_str::<Grant>(&String::from_utf8_lossy(&bytes))
            {
                out.push(grant);
            }
        }
        out.sort_by_key(|g| std::cmp::Reverse(g.created));
        Ok(out)
    }

    /// The folders the user made (the others are those their documents are in).
    async fn made_folders(&self, user: &str) -> Result<Vec<String>> {
        match self.read(&Self::storage_of(user), "folders.json").await {
            Ok(bytes) => Ok(json::from_str(&String::from_utf8_lossy(&bytes)).unwrap_or_default()),
            Err(Error::NotFound) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn file_path(storage: &str, path: &str) -> String {
        format!("/api/v1/files/{storage}/file?path={}", Self::quote(path))
    }

    /// Make the user's storage if it is not there.
    ///
    /// It is asked for first: only a storage that is missing (a 404) is made, so a save into a storage that exists does not
    /// depend on how warehouse answers a second request to make it.
    async fn ensure(&self, storage: &str, owner: &str) -> Result<()> {
        let (status, _) = self
            .call("GET", &format!("/api/v1/files/{storage}?n=1"), None, "")
            .await?;
        match status {
            200..=299 => return Ok(()),
            404 => {}
            s => return Err(Error::Io(format!("warehouse answered {s} for {storage}"))),
        }
        let body = format!(
            "{{\"name\":{},\"owner\":{},\"quota_bytes\":{}}}",
            json::to_string(storage),
            json::to_string(owner),
            self.quota
        );
        let (status, body) = self
            .call(
                "POST",
                "/api/v1/files",
                Some(body.into_bytes()),
                "application/json",
            )
            .await?;
        match status {
            // A 409 is a storage someone else made a moment ago.
            200..=299 | 409 => Ok(()),
            s => Err(Error::Io(format!(
                "warehouse would not make the storage {storage} for {owner} ({s}): {}",
                String::from_utf8_lossy(&body)
                    .chars()
                    .take(200)
                    .collect::<String>()
            ))),
        }
    }

    async fn read(&self, storage: &str, path: &str) -> Result<Vec<u8>> {
        let (status, body) = self
            .call("GET", &Self::file_path(storage, path), None, "")
            .await?;
        match status {
            200..=299 => Ok(body),
            404 => Err(Error::NotFound),
            s => Err(Error::Io(format!(
                "warehouse answered {s} reading {storage}/{path}: {}",
                String::from_utf8_lossy(&body)
                    .chars()
                    .take(200)
                    .collect::<String>()
            ))),
        }
    }

    async fn write(&self, storage: &str, path: &str, bytes: Vec<u8>) -> Result<()> {
        let (status, body) = self
            .call(
                "PUT",
                &Self::file_path(storage, path),
                Some(bytes),
                "application/octet-stream",
            )
            .await?;
        match status {
            200..=299 => Ok(()),
            // Warehouse refuses what is over the quota with 413 or 507.
            413 | 507 => Err(Error::Quota {
                used: 0,
                quota: self.quota,
            }),
            s => Err(Error::Io(format!(
                "warehouse answered {s}: {}",
                String::from_utf8_lossy(&body)
            ))),
        }
    }

    /// The files under `prefix` with their sizes, every page of them. Warehouse answers
    /// `{"entries":[{"name","path","kind","size"}]}` and pages with `n` and `last` (the previous page's final path).
    async fn listing(&self, storage: &str, prefix: &str) -> Result<Vec<(String, u64)>> {
        let mut out: Vec<(String, u64)> = Vec::new();
        for _ in 0..200 {
            let mut path = format!("/api/v1/files/{storage}?prefix={prefix}&n=1000");
            if let Some((last, _)) = out.last() {
                path = format!("{path}&last={}", Self::quote(last));
            }
            let (status, body) = self.call("GET", &path, None, "").await?;
            if status == 404 {
                return Ok(Vec::new());
            }
            if !(200..300).contains(&status) {
                return Err(Error::Io(format!("warehouse answered {status}")));
            }
            let value = json::parse(&String::from_utf8_lossy(&body)).map_err(err)?;
            let page: Vec<(String, u64)> = value
                .get("entries")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter(|i| i.get("kind").and_then(Value::as_str) != Some("dir"))
                        .filter_map(|i| {
                            let path = i.get("path").and_then(Value::as_str)?.to_string();
                            let size = i
                                .get("size")
                                .and_then(Value::as_i64)
                                .and_then(|n| u64::try_from(n).ok())
                                .unwrap_or(0);
                            Some((path, size))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if page.is_empty() {
                break;
            }
            out.extend(page);
        }
        Ok(out)
    }

    /// The paths under `prefix`.
    async fn names(&self, storage: &str, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .listing(storage, prefix)
            .await?
            .into_iter()
            .map(|(path, _)| path)
            .collect())
    }

    /// Percent-encoding for a path in a query.
    fn quote(path: &str) -> String {
        path.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/') {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }
}

#[async_trait]
impl Storage for WarehouseStore {
    async fn list(&self, user: &str) -> Result<Vec<DocMeta>> {
        let storage = Self::storage_of(user);
        let mut out = Vec::new();
        for path in self.names(&storage, "docs/").await? {
            if path.ends_with(".json")
                && let Ok(bytes) = self.read(&storage, &path).await
                && let Ok(meta) = json::from_str::<DocMeta>(&String::from_utf8_lossy(&bytes))
            {
                out.push(meta);
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    async fn get(&self, user: &str, id: &str) -> Result<(DocMeta, Vec<u8>)> {
        if !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Invalid("a document id is hexadecimal".into()));
        }
        let storage = Self::storage_of(user);
        let meta: DocMeta = json::from_str(&String::from_utf8_lossy(
            &self.read(&storage, &format!("docs/{id}.json")).await?,
        ))
        .map_err(err)?;
        let bytes = self.read(&storage, &format!("docs/{id}.bin")).await?;
        Ok((meta, bytes))
    }

    async fn meta(&self, user: &str, id: &str) -> Result<DocMeta> {
        if !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Invalid("a document id is hexadecimal".into()));
        }
        let storage = Self::storage_of(user);
        json::from_str(&String::from_utf8_lossy(
            &self.read(&storage, &format!("docs/{id}.json")).await?,
        ))
        .map_err(err)
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
        let storage = Self::storage_of(user);
        self.ensure(&storage, user).await?;
        let (id, folder) = match id {
            Some(id) => {
                let (old, _) = self.get(user, id).await?;
                if let Some(want) = if_match
                    && want != old.etag
                {
                    return Err(Error::Conflict { etag: old.etag });
                }
                (id.to_string(), old.folder)
            }
            None => (new_id(8), String::new()),
        };
        let meta = DocMeta {
            id: id.clone(),
            name: name.trim().to_string(),
            kind: kind.to_string(),
            size: bytes.len() as u64,
            modified: now(),
            etag: etag_of(bytes),
            folder,
        };
        self.write(&storage, &format!("docs/{id}.bin"), bytes.to_vec())
            .await?;
        self.write(
            &storage,
            &format!("docs/{id}.json"),
            json::to_string(&meta).into_bytes(),
        )
        .await?;
        Ok(meta)
    }

    async fn delete(&self, user: &str, id: &str) -> Result<()> {
        let storage = Self::storage_of(user);
        self.get(user, id).await?;
        for path in [format!("docs/{id}.bin"), format!("docs/{id}.json")] {
            let (status, _) = self
                .call("DELETE", &Self::file_path(&storage, &path), None, "")
                .await?;
            if !(200..300).contains(&status) && status != 404 {
                return Err(Error::Io(format!("warehouse answered {status}")));
            }
        }
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
        // What the person keeps is what is under `docs/`, added up from the listing: the answer for one storage carries
        // no totals (only the list of all storages does, and that is the operator's).
        let storage = Self::storage_of(user);
        let used = self
            .listing(&storage, "docs/")
            .await?
            .iter()
            .map(|(_, size)| *size)
            .sum();
        Ok(Usage {
            used,
            quota: self.quota,
        })
    }

    async fn put_share(&self, share: &Share) -> Result<()> {
        self.get(&share.owner, &share.doc).await?;
        self.ensure(&self.shares_storage, "admin").await.ok();
        self.write(
            &self.shares_storage,
            &format!("{}.json", share.token),
            json::to_string(share).into_bytes(),
        )
        .await
    }

    async fn share(&self, token: &str) -> Result<Share> {
        if token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::NotFound);
        }
        json::from_str(&String::from_utf8_lossy(
            &self
                .read(&self.shares_storage, &format!("{token}.json"))
                .await?,
        ))
        .map_err(err)
    }

    async fn shares_of(&self, user: &str) -> Result<Vec<Share>> {
        let mut out = Vec::new();
        for path in self.names(&self.shares_storage, "").await? {
            if path.ends_with(".json")
                && let Ok(bytes) = self.read(&self.shares_storage, &path).await
                && let Ok(share) = json::from_str::<Share>(&String::from_utf8_lossy(&bytes))
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
        let (status, _) = self
            .call(
                "DELETE",
                &Self::file_path(&self.shares_storage, &format!("{token}.json")),
                None,
                "",
            )
            .await?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(Error::Io(format!("warehouse answered {status}")))
        }
    }

    async fn folders(&self, user: &str) -> Result<Vec<String>> {
        let mut all = self.made_folders(user).await?;
        all.extend(self.list(user).await?.into_iter().map(|d| d.folder));
        Ok(with_ancestors(all))
    }

    async fn make_folder(&self, user: &str, path: &str) -> Result<()> {
        let path = clean_folder(path)?;
        if path.is_empty() {
            return Err(Error::Invalid("a folder needs a name".into()));
        }
        let mut made = self.made_folders(user).await?;
        if !made.contains(&path) {
            made.push(path);
        }
        let storage = Self::storage_of(user);
        self.ensure(&storage, user).await?;
        self.write(
            &storage,
            "folders.json",
            json::to_string(&made).into_bytes(),
        )
        .await
    }

    async fn delete_folder(&self, user: &str, path: &str) -> Result<()> {
        let path = clean_folder(path)?;
        let inside = |p: &str| p == path || p.starts_with(&format!("{path}/"));
        if path.is_empty() || self.list(user).await?.iter().any(|d| inside(&d.folder)) {
            return Err(Error::Invalid("the folder is not empty".into()));
        }
        let made: Vec<String> = self
            .made_folders(user)
            .await?
            .into_iter()
            .filter(|p| !inside(p))
            .collect();
        self.write(
            &Self::storage_of(user),
            "folders.json",
            json::to_string(&made).into_bytes(),
        )
        .await
    }

    async fn move_doc(&self, user: &str, id: &str, folder: &str) -> Result<DocMeta> {
        let folder = clean_folder(folder)?;
        let mut meta = self.meta(user, id).await?;
        meta.folder = folder;
        self.write(
            &Self::storage_of(user),
            &format!("docs/{id}.json"),
            json::to_string(&meta).into_bytes(),
        )
        .await?;
        Ok(meta)
    }

    async fn put_grant(&self, grant: &Grant) -> Result<()> {
        self.meta(&grant.owner, &grant.doc).await?;
        self.ensure(&self.shares_storage, "admin").await.ok();
        for path in Self::grant_paths(&grant.owner, &grant.doc, &grant.with) {
            self.write(
                &self.shares_storage,
                &path,
                json::to_string(grant).into_bytes(),
            )
            .await?;
        }
        Ok(())
    }

    async fn grants_to(&self, user: &str) -> Result<Vec<Grant>> {
        let prefix = format!("grants/to/{}/", user_key(user));
        Ok(self
            .read_grants(&prefix)
            .await?
            .into_iter()
            .filter(|g| g.with == user)
            .collect())
    }

    async fn grants_of(&self, owner: &str, doc: &str) -> Result<Vec<Grant>> {
        let prefix = format!("grants/by/{}/{doc}-", user_key(owner));
        Ok(self
            .read_grants(&prefix)
            .await?
            .into_iter()
            .filter(|g| g.owner == owner && g.doc == doc)
            .collect())
    }

    async fn grant(&self, owner: &str, doc: &str, with: &str) -> Result<Grant> {
        if !doc.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::NotFound);
        }
        let [to, _] = Self::grant_paths(owner, doc, with);
        let grant: Grant = json::from_str(&String::from_utf8_lossy(
            &self.read(&self.shares_storage, &to).await?,
        ))
        .map_err(err)?;
        if grant.owner == owner && grant.with == with {
            Ok(grant)
        } else {
            Err(Error::NotFound)
        }
    }

    async fn delete_grant(&self, owner: &str, doc: &str, with: &str) -> Result<()> {
        for path in Self::grant_paths(owner, doc, with) {
            let (status, _) = self
                .call(
                    "DELETE",
                    &Self::file_path(&self.shares_storage, &path),
                    None,
                    "",
                )
                .await?;
            if !(200..300).contains(&status) && status != 404 {
                return Err(Error::Io(format!("warehouse answered {status}")));
            }
        }
        Ok(())
    }
}
