use crate::error::Error;
use crate::xml::{self, Value};

const NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

/// `[Content_Types].xml`: the media type of each part, by extension or by part name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentTypes {
    /// `(extension, type)`, extension without the dot, lower case.
    pub defaults: Vec<(String, String)>,
    /// `(part name with a leading slash, type)`.
    pub overrides: Vec<(String, String)>,
}

impl ContentTypes {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let root = xml::parse(bytes, false)?;
        let get = |e: &Value, a: &str| xml::attr(e, a).unwrap_or_default().to_string();
        Ok(Self {
            defaults: xml::elements(&root, "Default")
                .map(|e| (get(e, "Extension").to_lowercase(), get(e, "ContentType")))
                .collect(),
            overrides: xml::elements(&root, "Override")
                .map(|e| (get(e, "PartName"), get(e, "ContentType")))
                .collect(),
        })
    }

    /// The media type of a part (name without a leading slash).
    #[must_use]
    pub fn of(&self, part: &str) -> Option<&str> {
        let slashed = format!("/{part}");
        if let Some((_, t)) = self.overrides.iter().find(|(n, _)| *n == slashed) {
            return Some(t);
        }
        let ext = part.rsplit_once('.')?.1.to_lowercase();
        self.defaults
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, t)| t.as_str())
    }

    /// Declare a part's type by name (replacing an earlier declaration).
    pub fn set_override(&mut self, part: &str, content_type: &str) {
        let slashed = format!("/{part}");
        match self.overrides.iter_mut().find(|(n, _)| *n == slashed) {
            Some(o) => o.1 = content_type.to_string(),
            None => self.overrides.push((slashed, content_type.to_string())),
        }
    }

    pub fn remove_override(&mut self, part: &str) {
        let slashed = format!("/{part}");
        self.overrides.retain(|(n, _)| *n != slashed);
    }

    pub fn set_default(&mut self, ext: &str, content_type: &str) {
        let ext = ext.to_lowercase();
        if !self.defaults.iter().any(|(e, _)| *e == ext) {
            self.defaults.push((ext, content_type.to_string()));
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut kids: Vec<Value> = self
            .defaults
            .iter()
            .map(|(e, t)| {
                xml::el(
                    "Default",
                    &[("Extension", e), ("ContentType", t)],
                    Vec::new(),
                )
            })
            .collect();
        kids.extend(self.overrides.iter().map(|(n, t)| {
            xml::el(
                "Override",
                &[("PartName", n), ("ContentType", t)],
                Vec::new(),
            )
        }));
        xml::write(&xml::el("Types", &[("xmlns", NS)], kids))
    }
}
