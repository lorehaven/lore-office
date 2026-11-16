//! The shared string table.

use office_fmt::xml::{self, Value};
use std::collections::HashMap;

/// The text of a `<si>`: its `<t>`, or its runs' `<t>`s joined; phonetic hints are not text.
pub fn si_text(si: &Value) -> String {
    let mut out = String::new();
    for k in xml::children(si) {
        match xml::tag(k) {
            Some("t") => out.push_str(&xml::text(k)),
            Some("r") => {
                for t in xml::elements(k, "t") {
                    out.push_str(&xml::text(t));
                }
            }
            _ => {}
        }
    }
    out
}

pub fn plain_si(text: &str) -> Value {
    let keep = text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.contains('\n');
    let attrs: &[(&str, &str)] = if keep {
        &[("xml:space", "preserve")]
    } else {
        &[]
    };
    xml::el("si", &[], vec![xml::el("t", attrs, vec![xml::txt(text)])])
}

/// Strings being collected for a write: each distinct text once, in first-use order.
#[derive(Debug, Default)]
pub struct Table {
    index: HashMap<String, usize>,
    items: Vec<String>,
    uses: usize,
}

impl Table {
    pub fn add(&mut self, text: &str) -> usize {
        self.uses += 1;
        if let Some(i) = self.index.get(text) {
            return *i;
        }
        self.items.push(text.to_string());
        self.index.insert(text.to_string(), self.items.len() - 1);
        self.items.len() - 1
    }

    /// The `<sst>`; a string the file already had keeps its original element (and so its rich-text runs).
    pub fn to_xml(&self, original: &[Value]) -> Value {
        let mut by_text: HashMap<String, &Value> = HashMap::new();
        for si in original {
            by_text.entry(si_text(si)).or_insert(si);
        }
        let kids = self
            .items
            .iter()
            .map(|t| by_text.get(t).map_or_else(|| plain_si(t), |v| (*v).clone()))
            .collect();
        xml::el(
            "sst",
            &[
                (
                    "xmlns",
                    "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
                ),
                ("count", &self.uses.to_string()),
                ("uniqueCount", &self.items.len().to_string()),
            ],
            kids,
        )
    }
}
