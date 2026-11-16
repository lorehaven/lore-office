//! Relationships: how one part names the others (`xl/_rels/workbook.xml.rels`).

use crate::error::Error;
use crate::xml::{self, Value};

pub const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
pub const SHARED_STRINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
pub const STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rel {
    pub id: String,
    pub kind: String,
    pub target: String,
    /// `TargetMode="External"`: the target is a link, not a part.
    pub external: bool,
}

/// The relationships part for a part: `xl/workbook.xml` has `xl/_rels/workbook.xml.rels`; the package itself (`""`)
/// has `_rels/.rels`.
#[must_use]
pub fn rels_part(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None if part.is_empty() => "_rels/.rels".to_string(),
        None => format!("_rels/{part}.rels"),
    }
}

/// The part a relationship of `base` points to: relative to `base`'s folder, or absolute with a leading `/`. `..` and
/// `.` are resolved; the result has no leading slash, as zip entry names do.
#[must_use]
pub fn resolve(base: &str, target: &str) -> String {
    let mut stack: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(d, _)| d.split('/').collect())
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Rel>, Error> {
    let root = xml::parse(bytes, false)?;
    Ok(xml::elements(&root, "Relationship")
        .map(|r| Rel {
            id: xml::attr(r, "Id").unwrap_or_default().to_string(),
            kind: xml::attr(r, "Type").unwrap_or_default().to_string(),
            target: xml::attr(r, "Target").unwrap_or_default().to_string(),
            external: xml::attr(r, "TargetMode") == Some("External"),
        })
        .collect())
}

#[must_use]
pub fn to_bytes(rels: &[Rel]) -> Vec<u8> {
    let kids: Vec<Value> = rels
        .iter()
        .map(|r| {
            let mut attrs = vec![
                ("Id", r.id.as_str()),
                ("Type", r.kind.as_str()),
                ("Target", r.target.as_str()),
            ];
            if r.external {
                attrs.push(("TargetMode", "External"));
            }
            xml::el("Relationship", &attrs, Vec::new())
        })
        .collect();
    xml::write(&xml::el("Relationships", &[("xmlns", NS)], kids))
}
