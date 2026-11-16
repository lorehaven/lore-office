//! A PDF made of chosen pages of other PDFs.

#![allow(
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::useless_let_if_seq,
    clippy::option_if_let_else
)]

use quench_pdf::{Dict, Document, Object};
use std::collections::HashMap;
use std::fmt::Write as _;

/// One page of the new file: a page of one of the sources, and how many quarter turns clockwise to turn it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// Which of the sources.
    pub source: usize,
    /// The page in it, from 0.
    pub page: usize,
    /// Added to the page's own rotation (any multiple of 90, also negative).
    pub turn: i32,
}

/// A markup to put on a page of the new file, in the page's own coordinates (y up).
#[derive(Debug, Clone, PartialEq)]
pub struct NewMarkup {
    /// `Highlight`, `Underline`, `StrikeOut`, `Square` or `Text` (a note).
    pub kind: String,
    /// The boxes it covers (x0, y0, x1, y1): a line of text each.
    pub boxes: Vec<(f64, f64, f64, f64)>,
    pub color: (u8, u8, u8),
    /// The comment (the text of a note).
    pub contents: String,
}

fn utf16_hex(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in text.encode_utf16() {
        let _ = write!(out, "{u:04X}");
    }
    out.push('>');
    out
}

impl NewMarkup {
    fn body(&self) -> String {
        let bounds = self
            .boxes
            .iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |a, b| {
                (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
            });
        let (r, g, b) = self.color;
        let mut quads = String::new();
        for (x0, y0, x1, y1) in &self.boxes {
            let _ = write!(
                quads,
                "{x0:.2} {y1:.2} {x1:.2} {y1:.2} {x0:.2} {y0:.2} {x1:.2} {y0:.2} "
            );
        }
        let mut body = format!(
            "<</Type/Annot/Subtype/{}/Rect[{:.2} {:.2} {:.2} {:.2}]/C[{:.3} {:.3} {:.3}]/F 4/T(lore-pdf)",
            self.kind,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0
        );
        if matches!(self.kind.as_str(), "Highlight" | "Underline" | "StrikeOut") {
            let _ = write!(body, "/QuadPoints[{}]", quads.trim_end());
        }
        if self.kind == "Text" {
            body.push_str("/Name/Note/Open false");
        }
        if !self.contents.is_empty() {
            let _ = write!(body, "/Contents {}", utf16_hex(&self.contents));
        }
        body.push_str(">>");
        body
    }
}

/// Text to put on a page of the new file, where the page has room for it (a form's blank, say): set in the embedded font.
#[derive(Debug, Clone, PartialEq)]
pub struct NewText {
    /// The start of the baseline, in the page's own coordinates (y up).
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub color: (u8, u8, u8),
    pub text: String,
}

/// Why a file could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A part names a source or a page that is not there.
    NoSuchPage { source: usize, page: usize },
    /// There is nothing to put in the file.
    Empty,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchPage { source, page } => {
                write!(f, "page {} of file {} does not exist", page + 1, source + 1)
            }
            Self::Empty => f.write_str("no pages to put in the file"),
        }
    }
}

impl std::error::Error for Error {}

/// The objects of the new file, numbered as they are copied.
struct Writer<'a> {
    sources: &'a [Document],
    /// (source, object number) -> number in the new file.
    map: HashMap<(usize, u32), u32>,
    /// What is left to write: (source, old number, new number).
    queue: Vec<(usize, u32, u32)>,
    objects: Vec<(u32, Vec<u8>)>,
    next: u32,
}

fn write_name(name: &str, out: &mut Vec<u8>) {
    out.push(b'/');
    for b in name.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_'
                    | b'.'
                    | b'+'
                    | b'*'
                    | b','
                    | b':'
                    | b';'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'='
                    | b'?'
                    | b'^'
                    | b'|'
                    | b'~'
            )
        {
            out.push(b);
        } else {
            let _ = write!(Hex(out), "#{b:02X}");
        }
    }
}

/// `write!` into a byte buffer.
struct Hex<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Hex<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

impl Writer<'_> {
    /// The number in the new file for an object of a source (it is queued to be written the first time).
    fn number(&mut self, source: usize, old: u32) -> u32 {
        if let Some(n) = self.map.get(&(source, old)) {
            return *n;
        }
        let n = self.next;
        self.next += 1;
        self.map.insert((source, old), n);
        self.queue.push((source, old, n));
        n
    }

    fn write_object(
        &mut self,
        source: usize,
        object: &Object,
        out: &mut Vec<u8>,
        skip_parent: bool,
    ) {
        match object {
            Object::Null => out.extend_from_slice(b"null"),
            Object::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            Object::Int(n) => out.extend_from_slice(n.to_string().as_bytes()),
            Object::Real(x) => {
                let mut s = format!("{x:.6}");
                if s.contains('.') {
                    s = s.trim_end_matches('0').trim_end_matches('.').to_string();
                }
                out.extend_from_slice(if s.is_empty() || s == "-" {
                    b"0"
                } else {
                    s.as_bytes()
                });
            }
            Object::Name(n) => write_name(n, out),
            Object::String(bytes) => {
                out.push(b'<');
                for b in bytes {
                    let _ = write!(Hex(out), "{b:02X}");
                }
                out.push(b'>');
            }
            Object::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b' ');
                    }
                    self.write_object(source, item, out, false);
                }
                out.push(b']');
            }
            Object::Dict(d) => self.write_dict(source, d, out, skip_parent),
            Object::Stream(s) => {
                // A stream is only ever an object of its own; reaching one here is a stream inside a direct object.
                self.write_dict(source, &s.dict, out, skip_parent);
            }
            Object::Ref(n, _) => {
                let new = self.number(source, *n);
                let _ = write!(Hex(out), "{new} 0 R");
            }
            Object::Op(_) => out.extend_from_slice(b"null"),
        }
    }

    fn write_dict(&mut self, source: usize, dict: &Dict, out: &mut Vec<u8>, skip_parent: bool) {
        out.extend_from_slice(b"<<");
        for (k, v) in dict {
            // Links back to the old page tree would drag it along.
            if (skip_parent && k == "Parent") || k == "P" || k == "Length" {
                continue;
            }
            write_name(k, out);
            out.push(b' ');
            self.write_object(source, v, out, false);
            out.push(b' ');
        }
        out.extend_from_slice(b">>");
    }

    /// Write the queued objects (and what they refer to) until none is left.
    fn drain(&mut self) {
        while let Some((source, old, new)) = self.queue.pop() {
            let object = self.sources[source].get_object(old).unwrap_or(Object::Null);
            let mut body = Vec::new();
            match &object {
                Object::Stream(s) => {
                    self.write_dict(source, &s.dict, &mut body, false);
                    // The dictionary is written without its length; put the right one back.
                    body.truncate(body.len() - 2);
                    let _ = write!(Hex(&mut body), "/Length {}>>\nstream\n", s.raw.len());
                    body.extend_from_slice(&s.raw);
                    body.extend_from_slice(b"\nendstream");
                }
                other => self.write_object(source, other, &mut body, false),
            }
            self.objects.push((new, body));
        }
    }
}

/// Make a file of the parts, in order.
///
/// # Errors
///
/// [`Error::Empty`] with no parts, [`Error::NoSuchPage`] for a part that names a page that is not there.
pub fn assemble(sources: &[Document], parts: &[Part]) -> Result<Vec<u8>, Error> {
    assemble_marked(sources, parts, &[])
}

/// Make a file of the parts, in order, with markup added to some of them: each entry of `marks` is the index of a part (not
/// of a source page) and what goes on it.
///
/// # Errors
///
/// As [`assemble`].
pub fn assemble_marked(
    sources: &[Document],
    parts: &[Part],
    marks: &[(usize, NewMarkup)],
) -> Result<Vec<u8>, Error> {
    assemble_full(sources, parts, marks, &[])
}

/// Make a file of the parts, in order, with markup and text added to some of them (the numbers are indexes of parts).
///
/// # Errors
///
/// As [`assemble`].
pub fn assemble_full(
    sources: &[Document],
    parts: &[Part],
    marks: &[(usize, NewMarkup)],
    texts: &[(usize, NewText)],
) -> Result<Vec<u8>, Error> {
    if parts.is_empty() {
        return Err(Error::Empty);
    }
    let all: Vec<Vec<quench_pdf::Page>> = sources.iter().map(Document::pages).collect();
    let mut w = Writer {
        sources,
        map: HashMap::new(),
        queue: Vec::new(),
        objects: Vec::new(),
        // 1 is the catalog, 2 the page tree; pages and what they use follow.
        next: 3,
    };
    let mut page_numbers = Vec::new();
    let mut used = pdf_write::embedded::Used::new();
    let mut font_at: Option<u32> = None;
    for (index, part) in parts.iter().enumerate() {
        let page =
            all.get(part.source)
                .and_then(|p| p.get(part.page))
                .ok_or(Error::NoSuchPage {
                    source: part.source,
                    page: part.page,
                })?;
        let mut dict = page.dict.clone();
        dict.remove("Parent");
        let rotate = sources[part.source]
            .get(&dict, "Rotate")
            .and_then(|r| r.as_int())
            .unwrap_or(0);
        let turned = (rotate + i64::from(part.turn)).rem_euclid(360);
        if turned == 0 {
            dict.remove("Rotate");
        } else {
            dict.insert("Rotate".to_string(), Object::Int(turned));
        }
        let existing = dict.remove("Annots");
        // Text added to the page goes in a content stream of its own after the page's, with the font in the resources.
        let my_texts: Vec<&NewText> = texts
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, t)| t)
            .collect();
        let mut text_parts: Option<(u32, Vec<u8>, Vec<u8>)> = None;
        if !my_texts.is_empty() {
            let fu = *font_at.get_or_insert_with(|| {
                let at = w.next;
                w.next += 5;
                at
            });
            let mut drawing = String::new();
            for t in &my_texts {
                let (r, g, b) = t.color;
                let hex = pdf_write::embedded::encode(&t.text, &mut used);
                let _ = writeln!(
                    drawing,
                    "q BT {:.3} {:.3} {:.3} rg /LoreU {:.2} Tf {:.2} {:.2} Td {hex} Tj ET Q",
                    f64::from(r) / 255.0,
                    f64::from(g) / 255.0,
                    f64::from(b) / 255.0,
                    t.size,
                    t.x,
                    t.y
                );
            }
            let at = w.next;
            w.next += 1;
            let mut stream = format!("<< /Length {} >>\nstream\n", drawing.len()).into_bytes();
            stream.extend_from_slice(drawing.as_bytes());
            stream.extend_from_slice(b"endstream");
            w.objects.push((at, stream));
            // The page's own contents (one stream or a list of them), then ours.
            let mut contents = Vec::new();
            match dict.remove("Contents") {
                Some(Object::Array(items)) => {
                    for item in &items {
                        w.write_object(part.source, item, &mut contents, false);
                        contents.push(b' ');
                    }
                }
                Some(other) => {
                    w.write_object(part.source, &other, &mut contents, false);
                    contents.push(b' ');
                }
                None => {}
            }
            let _ = write!(Hex(&mut contents), "{at} 0 R");
            // The resources with the font among the page's own.
            let resources = match sources[part.source].get(&dict, "Resources") {
                Some(Object::Dict(d)) => d,
                _ => Dict::new(),
            };
            dict.remove("Resources");
            let mut others = resources.clone();
            let fonts = match others.remove("Font") {
                Some(f) => match sources[part.source].resolve(&f) {
                    Object::Dict(d) => d,
                    _ => Dict::new(),
                },
                None => Dict::new(),
            };
            let mut res = Vec::new();
            w.write_dict(part.source, &others, &mut res, false);
            res.truncate(res.len() - 2);
            res.extend_from_slice(b"/Font<<");
            let mut inner = Vec::new();
            w.write_dict(part.source, &fonts, &mut inner, false);
            res.extend_from_slice(&inner[2..inner.len() - 2]);
            let _ = write!(Hex(&mut res), "/LoreU {fu} 0 R>>>>");
            text_parts = Some((0, contents, res));
        }
        let n = w.next;
        w.next += 1;
        let mut body = Vec::new();
        body.extend_from_slice(b"<</Parent 2 0 R ");
        // Written as the dictionary it is, with the pointer to its parent put in front.
        let mut rest = Vec::new();
        w.write_dict(part.source, &dict, &mut rest, true);
        rest.truncate(rest.len() - 2);
        body.extend_from_slice(&rest[2..]);
        if let Some((_, contents, resources)) = &text_parts {
            body.extend_from_slice(b"/Contents[");
            body.extend_from_slice(contents);
            body.extend_from_slice(b"]/Resources");
            body.extend_from_slice(resources);
        }
        // The page's own annotations, then the new ones.
        let mine: Vec<&NewMarkup> = marks
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m)
            .collect();
        if existing.is_some() || !mine.is_empty() {
            body.extend_from_slice(b"/Annots[");
            if let Some(existing) = existing {
                let resolved = sources[part.source].resolve(&existing);
                if let Object::Array(items) = resolved {
                    for item in &items {
                        w.write_object(part.source, item, &mut body, false);
                        body.push(b' ');
                    }
                }
            }
            for m in mine {
                let at = w.next;
                w.next += 1;
                w.objects.push((at, m.body().into_bytes()));
                let _ = write!(Hex(&mut body), "{at} 0 R ");
            }
            body.push(b']');
        }
        body.extend_from_slice(b">>");
        w.objects.push((n, body));
        page_numbers.push(n);
        w.drain();
    }
    if let Some(fu) = font_at {
        for (k, body) in pdf_write::embedded::objects(&used, fu as usize)
            .into_iter()
            .enumerate()
        {
            w.objects.push((fu + u32::try_from(k).unwrap_or(0), body));
        }
    }
    let mut out: Vec<u8> = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    let mut put = |out: &mut Vec<u8>, n: u32, body: &[u8]| {
        offsets.push((n, out.len()));
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    put(&mut out, 1, b"<</Type/Catalog/Pages 2 0 R>>");
    let kids: Vec<String> = page_numbers.iter().map(|n| format!("{n} 0 R")).collect();
    put(
        &mut out,
        2,
        format!(
            "<</Type/Pages/Kids[{}]/Count {}>>",
            kids.join(" "),
            page_numbers.len()
        )
        .as_bytes(),
    );
    w.objects.sort_by_key(|(n, _)| *n);
    for (n, body) in &w.objects {
        put(&mut out, *n, body);
    }
    let size = w.next;
    let xref = out.len();
    let mut table = format!("xref\n0 {size}\n0000000000 65535 f \n");
    offsets.sort_unstable();
    for n in 1..size {
        match offsets.iter().find(|(k, _)| *k == n) {
            Some((_, at)) => {
                let _ = writeln!(table, "{at:010} 00000 n ");
            }
            None => table.push_str("0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(table.as_bytes());
    out.extend_from_slice(
        format!("trailer\n<</Size {size}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    Ok(out)
}

/// Every page of every source, in order.
#[must_use]
pub fn all_parts(sources: &[Document]) -> Vec<Part> {
    sources
        .iter()
        .enumerate()
        .flat_map(|(source, d)| {
            (0..d.page_count()).map(move |page| Part {
                source,
                page,
                turn: 0,
            })
        })
        .collect()
}
