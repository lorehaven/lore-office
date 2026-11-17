//! Changes to paragraphs, as a document in the cloud sends and receives them.
//!
//! A change replaces some paragraphs by others: from paragraph `at`, `del` of them become the paragraphs in `ins`. It is found by
//! comparing the document with the last one the cloud is known to have (the paragraphs that are the same at the start and at
//! the end are left out), so no editing step needs to be tracked. The paragraphs are sent in full: text, kind, alignment,
//! spacing and runs with their look.
//!
//! What a change cannot carry makes the page send the whole file instead: anything that points into the document's own lists
//! (a font by name, a link, a picture, a table cell) or is not a paragraph (header, footer, paper, tables).
//!
//! Positions only mean something against the paragraphs the sender had, so when others' changes come first this page merges
//! them into its document (theirs where the page did not edit, the page's own where it did), takes them as the cloud's
//! state, and works its own change out again from the difference.

use crate::state::DocState;
use docs_text::{Align, Doc, Family, Kind, Para, ParaFmt, Run, Style};
use office_cloud_ui::Change;
use quench_serde::{Value as Json, json};

/// What this page knows of the cloud's document.
#[derive(Default)]
pub struct Track {
    /// The document as the cloud has it, as far as this page knows; `None` until the first look.
    base: Option<Doc>,
    /// The document as it was when the change now being sent was taken: the cloud's, once that is accepted.
    sent: Option<Doc>,
}

/// Where two paragraph lists differ: from `at`, `del` of the first are `ins` of the second.
fn diff(from: &[Para], to: &[Para]) -> Option<(usize, usize, usize)> {
    let most = from.len().min(to.len());
    let head = (0..most).take_while(|i| from[*i].same(&to[*i])).count();
    let tail = (0..most - head)
        .take_while(|i| from[from.len() - 1 - i].same(&to[to.len() - 1 - i]))
        .count();
    let (del, ins) = (from.len() - head - tail, to.len() - head - tail);
    (del != 0 || ins != 0).then_some((head, del, ins))
}

/// Whether a paragraph can be sent in a change.
fn plain(p: &Para) -> bool {
    p.cell.is_none()
        && p.picture.is_none()
        && p.runs
            .iter()
            .all(|r| r.style.font.is_none() && r.style.link.is_none())
}

fn kind_word(k: Kind) -> String {
    match k {
        Kind::Paragraph => "p".into(),
        Kind::Heading(n) => format!("h{n}"),
        Kind::Bullet => "b".into(),
        Kind::Numbered => "n".into(),
    }
}

fn kind_of(word: &str) -> Kind {
    match word {
        "b" => Kind::Bullet,
        "n" => Kind::Numbered,
        w if w.starts_with('h') => Kind::Heading(w[1..].parse().unwrap_or(1)),
        _ => Kind::Paragraph,
    }
}

const fn align_word(a: Align) -> &'static str {
    match a {
        Align::Left => "l",
        Align::Center => "c",
        Align::Right => "r",
        Align::Justify => "j",
    }
}

fn align_of(word: &str) -> Align {
    match word {
        "c" => Align::Center,
        "r" => Align::Right,
        "j" => Align::Justify,
        _ => Align::Left,
    }
}

fn opt(n: Option<u32>) -> i64 {
    n.map_or(-1, i64::from)
}

fn quoted(text: &str) -> String {
    json::to_string(&text.to_string())
}

fn encode_para(p: &Para) -> String {
    let runs: Vec<String> = p
        .runs
        .iter()
        .map(|r| {
            let s = &r.style;
            let bits = u8::from(s.bold)
                | u8::from(s.italic) << 1
                | u8::from(s.underline) << 2
                | u8::from(s.strike) << 3
                | u8::from(s.sup) << 4
                | u8::from(s.sub) << 5;
            let family = match s.family {
                None => 0,
                Some(Family::Sans) => 1,
                Some(Family::Serif) => 2,
                Some(Family::Mono) => 3,
            };
            format!(
                "[{},{bits},{},{},{},{family}]",
                quoted(&r.text),
                opt(s.highlight),
                opt(s.size.map(u32::from)),
                opt(s.color)
            )
        })
        .collect();
    let f = &p.fmt;
    format!(
        "{{\"k\":\"{}\",\"a\":\"{}\",\"f\":[{},{},{},{},{},{}],\"r\":[{}]}}",
        kind_word(p.kind),
        align_word(p.align),
        f.indent,
        f.first,
        opt(f.after.map(u32::from)),
        f.line,
        u8::from(f.page_break),
        f.level,
        runs.join(",")
    )
}

fn int(v: Option<&Json>) -> Option<i64> {
    v.and_then(Json::as_i64)
}

fn decode_para(v: &Json) -> Option<Para> {
    let Some(Json::Array(f)) = v.get("f") else {
        return None;
    };
    let Some(Json::Array(runs)) = v.get("r") else {
        return None;
    };
    let n = |i: usize| int(f.get(i));
    let fmt = ParaFmt {
        indent: u16::try_from(n(0)?).ok()?,
        first: i16::try_from(n(1)?).ok()?,
        after: u16::try_from(n(2)?).ok(),
        line: u16::try_from(n(3)?).ok()?,
        page_break: n(4)? != 0,
        level: u8::try_from(n(5)?).ok()?,
    };
    let mut made = Vec::new();
    for r in runs {
        let Json::Array(r) = r else { return None };
        let text = r.first()?.as_str()?.to_string();
        let bits = int(r.get(1))?;
        let at = |i: usize| int(r.get(i)).filter(|n| *n >= 0);
        made.push(Run {
            text,
            style: Style {
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                underline: bits & 4 != 0,
                strike: bits & 8 != 0,
                sup: bits & 16 != 0,
                sub: bits & 32 != 0,
                highlight: at(2).and_then(|n| u32::try_from(n).ok()),
                size: at(3).and_then(|n| u16::try_from(n).ok()),
                color: at(4).and_then(|n| u32::try_from(n).ok()),
                family: match int(r.get(5))? {
                    1 => Some(Family::Sans),
                    2 => Some(Family::Serif),
                    3 => Some(Family::Mono),
                    _ => None,
                },
                font: None,
                link: None,
            },
        });
    }
    Some(Para {
        id: 0,
        kind: kind_of(v.get("k")?.as_str()?),
        align: align_of(v.get("a")?.as_str()?),
        cell: None,
        picture: None,
        fmt,
        runs: made,
    })
}

impl Track {
    /// Start from this document: it is what the cloud has (it was just opened from there, or just saved).
    pub fn reset(&mut self, doc: &Doc) {
        self.base = Some(doc.clone());
        self.sent = None;
    }
}

impl DocState {
    /// What changed since this was last called, for the cloud: the paragraphs that differ, or the whole file.
    pub fn take_changes(&self) -> Option<Change> {
        let doc = self.doc.borrow();
        let mut track = self.track.borrow_mut();
        let Some(base) = track.base.clone() else {
            track.base = Some(doc.clone());
            return None;
        };
        if !doc.same_apart_from_paras(&base) {
            track.base = Some(doc.clone());
            return Some(Change::Whole);
        }
        let (at, del, ins) = diff(base.paras(), doc.paras())?;
        let added = &doc.paras()[at..at + ins];
        if !added.iter().all(plain) {
            track.base = Some(doc.clone());
            return Some(Change::Whole);
        }
        track.sent = Some(doc.clone());
        let list: Vec<String> = added.iter().map(encode_para).collect();
        Some(Change::Ops(
            format!("{{\"at\":{at},\"del\":{del},\"ins\":[{}]}}", list.join(",")).into_bytes(),
        ))
    }

    /// The change last taken was accepted: it is the cloud's now.
    pub fn change_accepted(&self) {
        let mut track = self.track.borrow_mut();
        if let Some(sent) = track.sent.take() {
            track.base = Some(sent);
        }
    }

    /// Make a change someone else made: into the cloud's state, and into this page's document where it did not edit.
    ///
    /// # Errors
    /// When the change cannot be read.
    pub fn apply_remote(&self, op: &[u8]) -> Result<(), String> {
        let value = json::parse(&String::from_utf8_lossy(op)).map_err(|e| e.to_string())?;
        let number = |k: &str| {
            value
                .get(k)
                .and_then(Json::as_i64)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| format!("a change needs {k}"))
        };
        let (at, del) = (number("at")?, number("del")?);
        let Some(Json::Array(items)) = value.get("ins") else {
            return Err("a change needs ins".into());
        };
        let ins: Vec<Para> = items
            .iter()
            .map(|p| decode_para(p).ok_or_else(|| "a paragraph could not be read".to_string()))
            .collect::<Result<_, _>>()?;
        {
            let mut doc = self.doc.borrow_mut();
            let mut track = self.track.borrow_mut();
            let base = track.base.take().unwrap_or_else(|| doc.clone());
            let local = diff(base.paras(), doc.paras());
            let mut base_now = base;
            base_now.splice_paras(at, del, ins.clone());
            match local {
                // Nothing of this page's own, or its edits are after the change: the change goes in at the same place.
                None => doc.splice_paras(at, del, ins),
                Some((l_at, ..)) if at + del <= l_at => doc.splice_paras(at, del, ins),
                // Its edits are before the change: the change goes in after them.
                Some((l_at, l_del, l_ins)) if at >= l_at + l_del => {
                    // Where it lands in this page's document: moved by what its own edits added or took out.
                    let to = (at + l_ins).saturating_sub(l_del);
                    doc.splice_paras(to, del, ins);
                }
                // Both changed the same paragraphs: this page's stay, and are sent over the others'.
                Some(_) => {}
            }
            track.base = Some(base_now);
            track.sent = None;
        }
        self.bump();
        Ok(())
    }
}
