//! A fast reader for the part of a worksheet that is huge and regular: `<sheetData>`. The generic XML tree costs more than
//! the rest of loading a big file put together, so the rows and cells are scanned straight out of the text, and the tree is
//! built only for what is left of the sheet (columns, merged cells, views, ...), which is small.

use std::borrow::Cow;

/// A `<row>`: its number if it has one, and its other attributes (height, hidden, ...) as written.
#[derive(Debug, Default)]
pub struct Row {
    pub number: Option<u32>,
    pub attrs: Vec<(String, String)>,
}

/// A formula element: its text, and the sharing markers.
#[derive(Debug, Default)]
pub struct Formula {
    pub text: String,
    pub kind: Option<String>,
    pub shared_index: Option<String>,
    /// The range an array formula fills (`ref`).
    pub reference: Option<String>,
}

/// A `<c>` with what the suite reads from it.
#[derive(Debug, Default)]
pub struct Cell {
    pub address: Option<String>,
    pub style: Option<String>,
    pub kind: Option<String>,
    pub formula: Option<Formula>,
    pub value: Option<String>,
    /// The text of an inline string (`<is>`), phonetic hints left out.
    pub inline: Option<String>,
}

/// The byte range of `<sheetData>...</sheetData>` in a sheet, from the start of the opening tag to the end of the closing
/// one; `None` when there is none (an empty `<sheetData/>` is a range too).
#[must_use]
pub fn data_range(xml: &[u8]) -> Option<(usize, usize)> {
    let start = find(xml, b"<sheetData", 0)?;
    let after = start + b"<sheetData".len();
    // `<sheetDataFoo` is not it.
    if !matches!(
        xml.get(after),
        Some(b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')
    ) {
        return None;
    }
    let open_end = find(xml, b">", after)?;
    if xml[open_end - 1] == b'/' {
        return Some((start, open_end + 1));
    }
    let close = find(xml, b"</sheetData>", open_end)?;
    Some((start, close + b"</sheetData>".len()))
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The text with `&amp;` and the other references decoded.
fn unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return Cow::Owned(out);
        };
        let name = &rest[1..end];
        match name {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            n if n.starts_with("#x") || n.starts_with("#X") => out.extend(
                u32::from_str_radix(&n[2..], 16)
                    .ok()
                    .and_then(char::from_u32),
            ),
            n if n.starts_with('#') => {
                out.extend(n[1..].parse::<u32>().ok().and_then(char::from_u32));
            }
            _ => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The attributes of a tag (`name="value"` pairs, either quote), values not yet decoded.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let b = tag.as_bytes();
    let mut i = tag.find(|c: char| c.is_whitespace()).unwrap_or(b.len());
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'/' {
            i += 1;
        }
        let name = &tag[name_start..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(&quote) = b.get(i).filter(|q| **q == b'"' || **q == b'\'') else {
            break;
        };
        let value_start = i + 1;
        let Some(len) = tag[value_start..].find(quote as char) else {
            break;
        };
        if !name.is_empty() {
            out.push((name, &tag[value_start..value_start + len]));
        }
        i = value_start + len + 1;
    }
    out
}

fn attr(attrs: &[(&str, &str)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| unescape(v).into_owned())
}

/// A cursor over the text.
struct Scanner<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Scanner<'a> {
    /// The next tag: its text between `<` and `>` (without them). Comments, processing instructions and CDATA are skipped.
    fn next_tag(&mut self) -> Result<Option<&'a str>, String> {
        loop {
            let Some(lt) = self.text[self.at..].find('<') else {
                return Ok(None);
            };
            let start = self.at + lt;
            let rest = &self.text[start..];
            if rest.starts_with("<!--") {
                let end = rest.find("-->").ok_or("a comment is not closed")?;
                self.at = start + end + 3;
            } else if rest.starts_with("<?") {
                let end = rest
                    .find("?>")
                    .ok_or("a processing instruction is not closed")?;
                self.at = start + end + 2;
            } else if rest.starts_with("<![CDATA[") {
                let end = rest.find("]]>").ok_or("CDATA is not closed")?;
                self.at = start + end + 3;
            } else {
                let end = rest.find('>').ok_or("a tag is not closed")?;
                if rest[1..end].contains('<') {
                    return Err("a tag is not closed".to_string());
                }
                self.at = start + end + 1;
                return Ok(Some(&rest[1..end]));
            }
        }
    }

    /// The text up to the next tag, with CDATA included and references decoded.
    fn text(&mut self) -> Result<String, String> {
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
                let end = cdata.find("]]>").ok_or("CDATA is not closed")?;
                out.push_str(&cdata[..end]);
                self.at += "<![CDATA[".len() + end + 3;
                continue;
            }
            let end = rest.find('<').unwrap_or(rest.len());
            out.push_str(&unescape(&rest[..end]));
            self.at += end;
            return Ok(out);
        }
    }

    /// The text of an element whose opening tag was just read, and its closing tag, which must be `name`'s.
    fn element_text(&mut self, name: &str) -> Result<String, String> {
        let text = self.text()?;
        match self.next_tag()? {
            Some(t) if t.strip_prefix('/').is_some_and(|n| n.trim() == name) => Ok(text),
            _ => Err(format!("<{name}> is not closed")),
        }
    }

    /// Skip to the end of the element whose opening tag was just read.
    fn skip(&mut self) -> Result<(), String> {
        let mut depth = 1;
        while depth > 0 {
            let tag = self.next_tag()?.ok_or("an element is not closed")?;
            if tag.starts_with('/') {
                depth -= 1;
            } else if !tag.ends_with('/') {
                depth += 1;
            }
        }
        Ok(())
    }
}

/// The name of a tag; a closing tag keeps its slash (`/row`).
fn tag_name(tag: &str) -> &str {
    let body = tag.strip_prefix('/').unwrap_or(tag);
    let len = body
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(body.len());
    &tag[..tag.len() - body.len() + len]
}

/// Receives what [`scan`] finds. Returning `false` stops the scan.
pub trait Visitor {
    fn row(&mut self, row: Row) -> bool;
    fn cell(&mut self, cell: Cell) -> bool;
}

/// The attributes of every row that has any besides its number, by row number, without reading the cells.
#[must_use]
pub fn row_attributes(data: &str) -> std::collections::HashMap<u32, Vec<(String, String)>> {
    let mut out = std::collections::HashMap::new();
    let mut s = Scanner { text: data, at: 0 };
    while let Ok(Some(tag)) = s.next_tag() {
        if tag_name(tag) != "row" {
            continue;
        }
        let attrs = attributes(tag);
        if let Some(n) = attr(&attrs, "r").and_then(|v| v.parse::<u32>().ok()) {
            let kept: Vec<(String, String)> = attrs
                .iter()
                .filter(|(k, _)| *k != "r" && *k != "spans")
                .map(|(k, v)| ((*k).to_string(), unescape(v).into_owned()))
                .collect();
            if !kept.is_empty() {
                out.insert(n, kept);
            }
        }
        if !tag.ends_with('/') {
            match s.text[s.at..].find("</row>") {
                Some(end) => s.at += end + "</row>".len(),
                None => break,
            }
        }
    }
    out
}

/// Scan the inside of `<sheetData>` (or the whole element): the visitor is told of each `<row>` and each `<c>`.
pub fn scan(data: &str, v: &mut dyn Visitor) -> Result<(), String> {
    let mut s = Scanner { text: data, at: 0 };
    while let Some(tag) = s.next_tag()? {
        match tag_name(tag) {
            "row" => {
                let attrs = attributes(tag);
                let row = Row {
                    number: attr(&attrs, "r").and_then(|n| n.parse().ok()),
                    attrs: attrs
                        .iter()
                        .filter(|(n, _)| *n != "r" && *n != "spans")
                        .map(|(n, val)| ((*n).to_string(), unescape(val).into_owned()))
                        .collect(),
                };
                if !v.row(row) {
                    return Ok(());
                }
            }
            "c" => {
                let attrs = attributes(tag);
                let mut cell = Cell {
                    address: attr(&attrs, "r"),
                    style: attr(&attrs, "s"),
                    kind: attr(&attrs, "t"),
                    ..Cell::default()
                };
                if !tag.ends_with('/') {
                    cell_children(&mut s, &mut cell)?;
                }
                if !v.cell(cell) {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn cell_children(s: &mut Scanner, cell: &mut Cell) -> Result<(), String> {
    loop {
        let tag = s.next_tag()?.ok_or("a cell is not closed")?;
        let name = tag_name(tag);
        match name {
            "/c" => return Ok(()),
            "v" if !tag.ends_with('/') => cell.value = Some(s.element_text("v")?),
            "f" => {
                let attrs = attributes(tag);
                let text = if tag.ends_with('/') {
                    String::new()
                } else {
                    s.text()?
                };
                cell.formula = Some(Formula {
                    text,
                    kind: attr(&attrs, "t"),
                    shared_index: attr(&attrs, "si"),
                    reference: attr(&attrs, "ref"),
                });
            }
            "is" if !tag.ends_with('/') => cell.inline = Some(inline_text(s)?),
            "v" => {}
            other if !tag.ends_with('/') && !other.starts_with('/') => s.skip()?,
            _ => {}
        }
    }
}

/// The text of an `<is>`: every `<t>`, except inside phonetic hints (`<rPh>`).
fn inline_text(s: &mut Scanner) -> Result<String, String> {
    let mut out = String::new();
    loop {
        let tag = s.next_tag()?.ok_or("an inline string is not closed")?;
        match tag_name(tag) {
            "/is" => return Ok(out),
            "t" if !tag.ends_with('/') => out.push_str(&s.element_text("t")?),
            "rPh" if !tag.ends_with('/') => s.skip()?,
            _ => {}
        }
    }
}
