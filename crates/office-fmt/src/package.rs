use crate::error::Error;
use quench_archive::zip;
use std::io::Cursor;

/// The most parts a package may have.
pub const MAX_PARTS: usize = 20_000;
/// The most one part may expand to.
pub const MAX_PART_BYTES: u64 = 256 << 20;
/// The most all parts together may expand to.
pub const MAX_TOTAL_BYTES: u64 = 1 << 30;

/// The parts of a package, in the order the file had them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    parts: Vec<(String, Vec<u8>)>,
}

impl Package {
    #[must_use]
    pub const fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Read a zip. Every expanded size is bounded, so a small hostile file cannot fill memory.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let mut archive =
            zip::Archive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
        let entries: Vec<_> = archive
            .entries()
            .iter()
            .map(|e| (e.name.clone(), e.is_dir()))
            .collect();
        if entries.len() > MAX_PARTS {
            return Err(Error::Limit(format!("{} parts", entries.len())));
        }
        let (mut parts, mut total) = (Vec::new(), 0u64);
        for (i, (name, dir)) in entries.into_iter().enumerate() {
            if dir {
                continue;
            }
            let data = archive
                .read(i, MAX_PART_BYTES.min(MAX_TOTAL_BYTES - total))
                .map_err(|e| Error::Zip(format!("{name}: {e}")))?;
            total += data.len() as u64;
            parts.push((name, data));
        }
        Ok(Self { parts })
    }

    /// The zip file: parts in order, `[Content_Types].xml` first when there is one (some readers need it early).
    pub fn write(&self) -> Result<Vec<u8>, Error> {
        let mut w = zip::Writer::new();
        // `[Content_Types].xml` first for OOXML, `mimetype` first for OpenDocument.
        let is_first = |n: &str| n == "[Content_Types].xml" || n == "mimetype";
        let first = self.parts.iter().find(|(n, _)| is_first(n));
        for (name, data) in first
            .into_iter()
            .chain(self.parts.iter().filter(|(n, _)| !is_first(n)))
        {
            w.add(name, data)
                .map_err(|e| Error::Zip(format!("{name}: {e}")))?;
        }
        w.finish().map_err(|e| Error::Zip(e.to_string()))
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.parts
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_slice())
    }

    /// Replace a part where it is, or add it at the end.
    pub fn set(&mut self, name: &str, data: Vec<u8>) {
        match self.parts.iter_mut().find(|(n, _)| n == name) {
            Some(p) => p.1 = data,
            None => self.parts.push((name.to_string(), data)),
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.parts.len();
        self.parts.retain(|(n, _)| n != name);
        self.parts.len() != before
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().map(|(n, _)| n.as_str())
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.parts.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}
