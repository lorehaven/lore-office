//! XML parts as trees (`quench_convert::xml`), with the declaration OOXML expects when writing.
//!
//! Comments and processing instructions are dropped on reading; attributes keep their order and prefixes, and no
//! namespace is resolved, so unknown elements and attributes written back are the ones read.

use crate::error::Error;
use quench_convert::xml;
pub use quench_convert::xml::{attr, child, children, element, elements, tag, text};
pub use quench_serde::{Map, Value};

/// Parse a part. `keep_whitespace` keeps text that is only whitespace (needed for `xml:space="preserve"` strings).
pub fn parse(bytes: &[u8], keep_whitespace: bool) -> Result<Value, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Xml("not UTF-8".into()))?;
    xml::parse_with(text, &xml::Options { keep_whitespace }).map_err(|e| Error::Xml(e.to_string()))
}

/// The part's bytes: declaration, then the tree.
#[must_use]
pub fn write(root: &Value) -> Vec<u8> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str(&xml::to_string(root));
    out.into_bytes()
}

/// An element (or a whole tree) as XML text, without a declaration.
#[must_use]
pub fn to_text(root: &Value) -> String {
    xml::to_string(root)
}

/// An element from its XML text.
pub fn from_text(text: &str) -> Result<Value, Error> {
    parse(text.as_bytes(), false)
}

/// An element with attributes given as pairs and children.
#[must_use]
pub fn el(name: &str, attrs: &[(&str, &str)], kids: Vec<Value>) -> Value {
    let mut m = Map::new();
    for (k, v) in attrs {
        m.insert(*k, Value::String((*v).to_string()));
    }
    element(name, m, kids)
}

/// A text node.
#[must_use]
pub fn txt(s: &str) -> Value {
    Value::String(s.to_string())
}

/// The first child element with this tag, mutably.
pub fn child_mut<'a>(parent: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    let Value::Object(map) = parent else {
        return None;
    };
    let Value::Array(kids) = map.get_mut("children")? else {
        return None;
    };
    kids.iter_mut().find(|k| tag(k) == Some(name))
}

/// Every child of an element, mutably.
pub fn children_mut(parent: &mut Value) -> Option<&mut Vec<Value>> {
    let Value::Object(map) = parent else {
        return None;
    };
    match map.get_mut("children")? {
        Value::Array(kids) => Some(kids),
        _ => None,
    }
}

/// Set an attribute (adds it at the end when new).
pub fn set_attr(elem: &mut Value, name: &str, value: &str) {
    let Value::Object(map) = elem else { return };
    if let Some(Value::Object(attrs)) = map.get_mut("attrs") {
        attrs.insert(name, Value::String(value.to_string()));
    }
}

/// An attribute by its local name: `id` finds `r:id` (the prefix differs between files).
#[must_use]
pub fn attr_local<'a>(e: &'a Value, local: &str) -> Option<&'a str> {
    let Some(Value::Object(attrs)) = e.get("attrs") else {
        return None;
    };
    attrs
        .iter()
        .find(|(k, _)| k.as_str() == local || k.rsplit_once(':').is_some_and(|(_, l)| l == local))
        .and_then(|(_, v)| v.as_str())
}

/// Every attribute of an element, in order.
#[must_use]
pub fn attrs(e: &Value) -> Vec<(&str, &str)> {
    let Some(Value::Object(attrs)) = e.get("attrs") else {
        return Vec::new();
    };
    attrs
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|v| (k.as_str(), v)))
        .collect()
}

/// Remove an attribute.
pub fn remove_attr(elem: &mut Value, name: &str) {
    if let Value::Object(map) = elem
        && let Some(Value::Object(attrs)) = map.get_mut("attrs")
    {
        attrs.remove(name);
    }
}
