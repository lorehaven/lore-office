//! Reading a file out of a warehouse storage as the person who asked (their own access token, not the service's), so a
//! document kept there can be opened in the suite.

use crate::{Error, Result};
use quench_client::transport::Client;

/// The most a file taken from a warehouse may be (a document, not an archive).
const MAX_BYTES: usize = 256 << 20;

fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Where files are read from: a warehouse's address (`https://host/warehouse`).
#[derive(Debug, Clone)]
pub struct WarehouseFiles {
    http: Client,
    base: String,
}

impl WarehouseFiles {
    /// # Errors
    /// [`Error::Io`] when the client cannot be made.
    pub fn new(base: &str) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .build()
                .map_err(|e| Error::Io(e.to_string()))?,
            base: base.trim_end_matches('/').to_string(),
        })
    }

    /// `path` of `storage`, read with the person's `token`.
    ///
    /// # Errors
    /// [`Error::Denied`] when the warehouse refuses them, [`Error::NotFound`] for no such file, [`Error::Invalid`] for a
    /// file too big, [`Error::Io`] when the warehouse does not answer.
    pub async fn read(&self, token: &str, storage: &str, path: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/v1/files/{}/file?path={}",
            self.base,
            percent(storage),
            percent(path)
        );
        let res = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        match res.status().as_u16() {
            200 => {}
            401 | 403 => return Err(Error::Denied),
            404 => return Err(Error::NotFound),
            other => return Err(Error::Io(format!("the warehouse answered {other}"))),
        }
        let bytes = res.bytes().await.map_err(|e| Error::Io(e.to_string()))?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid("the file is too big".into()));
        }
        Ok(bytes.to_vec())
    }
}
