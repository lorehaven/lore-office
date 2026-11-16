//! Laying a slide's markdown out on the stage: a list of things to draw, then drawing them.

use crate::look::Look;
use notes_md::{Piece, callout, label, pieces};
use pdf_write::{Family, Font, Page, text_width};
use quench_html::markdown::{Alignment, Block, Inline, List, Options, Table, parse};

/// What to draw, in pixels of the stage from its top left.
#[derive(Clone, Debug)]
enum Op {
    Text {
        x: f64,
        y: f64,
        text: String,
        font: Font,
        size: f64,
        color: u32,
        underline: bool,
        strike: bool,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: u32,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
        color: u32,
    },
}

/// A laid-out slide.
#[derive(Clone, Debug, Default)]
pub struct Flowed {
    ops: Vec<Op>,
}

const PAD_X: f64 = 64.0;
const PAD_TOP: f64 = 48.0;
const BASE: f64 = 30.0;
const LEADING: f64 = 1.35;

/// A stretch of text with its style.
#[derive(Clone, Debug)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    mono: bool,
    link: bool,
    strike: bool,
}

/// The parser cuts text at every bracket; wikilinks need it whole.
fn joined(list: &[Inline]) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for i in list {
        match (out.last_mut(), i) {
            (Some(Inline::Text(last)), Inline::Text(next)) => last.push_str(next),
            _ => out.push(i.clone()),
        }
    }
    out
}

fn runs(list: &[Inline], base: &Run, out: &mut Vec<Run>) {
    for i in joined(list) {
        let with = |f: &dyn Fn(&mut Run)| {
            let mut r = base.clone();
            f(&mut r);
            r
        };
        match i {
            Inline::Text(t) => {
                for p in pieces(&t) {
                    match p {
                        Piece::Text(t) => out.push(Run {
                            text: t,
                            ..base.clone()
                        }),
                        Piece::Link(l) => out.push(with(&|r| {
                            r.text = label(&l);
                            r.link = true;
                        })),
                    }
                }
            }
            Inline::Code(c) => out.push(with(&|r| {
                r.text.clone_from(&c);
                r.mono = true;
            })),
            Inline::Math(t) => out.push(with(&|r| {
                r.text.clone_from(&t);
                r.italic = true;
            })),
            Inline::Emph(c) => runs(&c, &with(&|r| r.italic = true), out),
            Inline::Strong(c) => runs(&c, &with(&|r| r.bold = true), out),
            Inline::Strikethrough(c) => runs(&c, &with(&|r| r.strike = true), out),
            Inline::Link { children, .. } => runs(&children, &with(&|r| r.link = true), out),
            Inline::Image { children, .. } => {
                let mut alt = Vec::new();
                runs(&children, base, &mut alt);
                let text: String = alt.iter().map(|r| r.text.as_str()).collect();
                out.push(Run {
                    text: format!("[{text}]"),
                    ..base.clone()
                });
            }
            Inline::Html(h) => out.push(Run {
                text: h,
                ..base.clone()
            }),
            Inline::SoftBreak => out.push(Run {
                text: " ".into(),
                ..base.clone()
            }),
            Inline::HardBreak => out.push(Run {
                text: "\n".into(),
                ..base.clone()
            }),
            Inline::FootnoteReference { number, .. } => out.push(Run {
                text: format!("[{number}]"),
                ..base.clone()
            }),
        }
    }
}

struct Cx<'a> {
    ops: Vec<Op>,
    look: &'a Look,
}

impl Cx<'_> {
    fn font(r: &Run, force_bold: bool) -> Font {
        let family = if r.mono { Family::Mono } else { Family::Sans };
        Font::in_family(family, r.bold || force_bold, r.italic)
    }

    /// Set runs in a box of `width` starting at (`x`, `y`); returns the height used.
    fn paragraph(
        &mut self,
        list: &[Inline],
        x: f64,
        y: f64,
        width: f64,
        size: f64,
        bold: bool,
        color: u32,
        align_center: bool,
    ) -> f64 {
        let mut all = Vec::new();
        let base = Run {
            text: String::new(),
            bold,
            italic: false,
            mono: false,
            link: false,
            strike: false,
        };
        runs(list, &base, &mut all);
        self.runs_in(&all, x, y, width, size, color, align_center)
    }

    fn runs_in(
        &mut self,
        all: &[Run],
        x: f64,
        y: f64,
        width: f64,
        size: f64,
        color: u32,
        center: bool,
    ) -> f64 {
        // Words with their styles; a line is a list of (run index, word, width).
        let mut lines: Vec<Vec<(Run, String, f64)>> = vec![Vec::new()];
        let mut used = 0.0;
        for r in all {
            let font = Self::font(r, false);
            for (n, piece) in r.text.split('\n').enumerate() {
                if n > 0 {
                    lines.push(Vec::new());
                    used = 0.0;
                }
                for token in split_keep_spaces(piece) {
                    let w = text_width(&token, font, size);
                    let is_space = token.trim().is_empty();
                    if !is_space && used + w > width && used > 0.0 {
                        lines.push(Vec::new());
                        used = 0.0;
                    }
                    if is_space && used == 0.0 {
                        continue;
                    }
                    used += w;
                    if let Some(l) = lines.last_mut() {
                        l.push((r.clone(), token, w));
                    }
                }
            }
        }
        let line_h = size * LEADING;
        let mut top = y;
        for line in &lines {
            // Neighbouring words of one style are drawn as one piece of text, so a viewer reads them with their spaces.
            let mut pieces: Vec<(Run, String, f64)> = Vec::new();
            for (r, token, w) in line {
                match pieces.last_mut() {
                    Some((last, text, width)) if same_style(last, r) => {
                        text.push_str(token);
                        *width += w;
                    }
                    _ => pieces.push((r.clone(), token.clone(), *w)),
                }
            }
            while pieces.last().is_some_and(|(_, t, _)| t.trim().is_empty()) {
                pieces.pop();
            }
            if let Some((_, t, w)) = pieces.last_mut() {
                let trimmed = t.trim_end().len();
                let cut = t.len() - trimmed;
                if cut > 0 {
                    *w -= text_width(&t[trimmed..], Font::Helvetica, size);
                    t.truncate(trimmed);
                }
            }
            let total: f64 = pieces.iter().map(|(_, _, w)| w).sum();
            let mut cursor = x + if center {
                ((width - total) / 2.0).max(0.0)
            } else {
                0.0
            };
            for (r, text, w) in &pieces {
                if !text.trim().is_empty() {
                    let ink = if r.link { self.look.accent } else { color };
                    if r.mono {
                        self.ops.push(Op::Rect {
                            x: cursor,
                            y: top + size * 0.05,
                            w: *w,
                            h: size * 1.15,
                            color: self.look.tint,
                        });
                    }
                    self.ops.push(Op::Text {
                        x: cursor,
                        y: top + size,
                        text: text.clone(),
                        font: Self::font(r, false),
                        size,
                        color: ink,
                        underline: r.link,
                        strike: r.strike,
                    });
                }
                cursor += w;
            }
            top += line_h;
        }
        top - y
    }

    fn blocks(&mut self, blocks: &[Block], x: f64, mut y: f64, width: f64, tight: bool) -> f64 {
        let start = y;
        let ink = self.look.ink;
        for b in blocks {
            let gap = BASE * 0.5;
            y += match b {
                Block::Paragraph(content) => {
                    self.paragraph(content, x, y, width, BASE, false, ink, false)
                        + if tight { 0.0 } else { gap }
                }
                Block::Heading { level, content } => {
                    let size = match level {
                        1 => BASE * 1.9,
                        2 => BASE * 1.5,
                        3 => BASE * 1.2,
                        _ => BASE,
                    };
                    let color = if *level <= 2 { self.look.accent } else { ink };
                    self.paragraph(content, x, y, width, size, true, color, false) + gap * 0.8
                }
                Block::ThematicBreak => {
                    self.ops.push(Op::Line {
                        x1: x,
                        y1: y + 6.0,
                        x2: x + width,
                        y2: y + 6.0,
                        width: 2.0,
                        color: self.look.rule,
                    });
                    gap + 12.0
                }
                Block::CodeBlock { literal, .. } => {
                    let size = BASE * 0.72;
                    let lines: Vec<&str> = literal.trim_end_matches('\n').split('\n').collect();
                    let h = lines.len() as f64 * size * LEADING + 20.0;
                    self.ops.push(Op::Rect {
                        x,
                        y,
                        w: width,
                        h,
                        color: self.look.tint,
                    });
                    for (i, l) in lines.iter().enumerate() {
                        self.ops.push(Op::Text {
                            x: x + 14.0,
                            y: y + 10.0 + (i as f64 + 1.0) * size * LEADING - size * 0.3,
                            text: (*l).to_string(),
                            font: Font::Courier,
                            size,
                            color: ink,
                            underline: false,
                            strike: false,
                        });
                    }
                    h + gap
                }
                Block::Html(_) => 0.0,
                Block::BlockQuote(inner) => self.quote(inner, x, y, width) + gap,
                Block::List(l) => self.list(l, x, y, width) + gap,
                Block::Table(t) => self.table(t, x, y, width) + gap,
                Block::Math(tex) => {
                    let run = Run {
                        text: tex.trim().to_string(),
                        bold: false,
                        italic: true,
                        mono: false,
                        link: false,
                        strike: false,
                    };
                    self.runs_in(&[run], x, y, width, BASE * 1.1, ink, true) + gap
                }
            };
        }
        y - start
    }

    fn quote(&mut self, inner: &[Block], x: f64, y: f64, width: f64) -> f64 {
        // A callout: a quote whose first line is `[!kind] Title`.
        let mut title = None;
        let mut body: Vec<Block> = inner.to_vec();
        if let Some(Block::Paragraph(first)) = inner.first() {
            let first = joined(first);
            if let Some(Inline::Text(head)) = first.first()
                && let Some(c) = callout(head.lines().next().unwrap_or_default())
            {
                title = Some(if c.title.is_empty() { c.kind } else { c.title });
                let mut rest: Vec<Inline> = Vec::new();
                if let Some((_, after)) = head.split_once('\n').filter(|(_, a)| !a.is_empty()) {
                    rest.push(Inline::Text(after.to_string()));
                }
                let mut skipped = false;
                for i in first.iter().skip(1) {
                    if !skipped && rest.is_empty() && matches!(i, Inline::SoftBreak) {
                        skipped = true;
                        continue;
                    }
                    rest.push(i.clone());
                }
                body = if rest.is_empty() {
                    Vec::new()
                } else {
                    vec![Block::Paragraph(rest)]
                };
                body.extend(inner.iter().skip(1).cloned());
            }
        }
        let mark = self.ops.len();
        let mut used = 0.0;
        if let Some(t) = &title {
            let run = Run {
                text: capitalise(t),
                bold: true,
                italic: false,
                mono: false,
                link: false,
                strike: false,
            };
            used += self.runs_in(
                &[run],
                x + 24.0,
                y + 8.0,
                width - 36.0,
                BASE,
                self.look.ink,
                false,
            );
        }
        used += self.blocks(&body, x + 24.0, y + 8.0 + used, width - 36.0, false);
        let h = used + 12.0;
        // The tint and the bar go behind what was just drawn.
        self.ops.insert(
            mark,
            Op::Rect {
                x,
                y,
                w: width,
                h,
                color: if title.is_some() {
                    self.look.tint
                } else {
                    self.look.background
                },
            },
        );
        self.ops.insert(
            mark + 1,
            Op::Rect {
                x,
                y,
                w: 6.0,
                h,
                color: self.look.accent,
            },
        );
        h
    }

    fn list(&mut self, l: &List, x: f64, y: f64, width: f64) -> f64 {
        let start = y;
        let mut y = y;
        for (n, item) in l.items.iter().enumerate() {
            let indent = if item.task.is_some() {
                BASE * 1.7
            } else {
                BASE * 1.2
            };
            let marker = match (item.task, l.ordered) {
                (Some(true), _) => "[x]".to_string(),
                (Some(false), _) => "[ ]".to_string(),
                (None, true) => format!("{}.", l.start + n),
                (None, false) => "\u{2022}".to_string(),
            };
            self.ops.push(Op::Text {
                x,
                y: y + BASE,
                text: marker,
                font: Font::Helvetica,
                size: BASE,
                color: self.look.accent,
                underline: false,
                strike: false,
            });
            y += self
                .blocks(&item.blocks, x + indent, y, width - indent, true)
                .max(BASE * LEADING);
        }
        y - start
    }

    fn table(&mut self, t: &Table, x: f64, y: f64, width: f64) -> f64 {
        let size = BASE * 0.85;
        let cols = t.header.len().max(1);
        let flat = |cell: &[Inline]| -> String {
            let mut all = Vec::new();
            let base = Run {
                text: String::new(),
                bold: false,
                italic: false,
                mono: false,
                link: false,
                strike: false,
            };
            runs(cell, &base, &mut all);
            all.iter().map(|r| r.text.as_str()).collect()
        };
        let mut natural = vec![0.0_f64; cols];
        let rows: Vec<&Vec<Vec<Inline>>> =
            std::iter::once(&t.header).chain(t.rows.iter()).collect();
        for r in &rows {
            for (i, c) in r.iter().enumerate().take(cols) {
                natural[i] = natural[i].max(text_width(&flat(c), Font::HelveticaBold, size) + 28.0);
            }
        }
        let sum: f64 = natural.iter().sum();
        let scale = if sum > width { width / sum } else { 1.0 };
        let widths: Vec<f64> = natural.iter().map(|w| w * scale).collect();
        let row_h = size * LEADING + 12.0;
        let mut top = y;
        for (ri, r) in rows.iter().enumerate() {
            let mut left = x;
            if ri == 0 {
                self.ops.push(Op::Rect {
                    x,
                    y: top,
                    w: widths.iter().sum(),
                    h: row_h,
                    color: self.look.tint,
                });
            }
            for (i, w) in widths.iter().enumerate() {
                let text = r.get(i).map(|c| flat(c)).unwrap_or_default();
                let align = t.alignments.get(i).copied().unwrap_or(Alignment::None);
                let font = if ri == 0 {
                    Font::HelveticaBold
                } else {
                    Font::Helvetica
                };
                let tw = text_width(&text, font, size);
                let tx = match align {
                    Alignment::Right => left + w - tw - 14.0,
                    Alignment::Center => left + (w - tw) / 2.0,
                    _ => left + 14.0,
                };
                self.ops.push(Op::Text {
                    x: tx,
                    y: top + 6.0 + size,
                    text,
                    font,
                    size,
                    color: self.look.ink,
                    underline: false,
                    strike: false,
                });
                self.ops.push(Op::Line {
                    x1: left,
                    y1: top,
                    x2: left,
                    y2: top + row_h,
                    width: 1.0,
                    color: self.look.rule,
                });
                left += w;
            }
            self.ops.push(Op::Line {
                x1: left,
                y1: top,
                x2: left,
                y2: top + row_h,
                width: 1.0,
                color: self.look.rule,
            });
            self.ops.push(Op::Line {
                x1: x,
                y1: top,
                x2: left,
                y2: top,
                width: 1.0,
                color: self.look.rule,
            });
            top += row_h;
        }
        self.ops.push(Op::Line {
            x1: x,
            y1: top,
            x2: x + widths.iter().sum::<f64>(),
            y2: top,
            width: 1.0,
            color: self.look.rule,
        });
        top - y
    }
}

fn same_style(a: &Run, b: &Run) -> bool {
    a.bold == b.bold
        && a.italic == b.italic
        && a.mono == b.mono
        && a.link == b.link
        && a.strike == b.strike
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Words and the spaces between them, each its own token.
fn split_keep_spaces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_space = false;
    for c in text.chars() {
        let space = c == ' ';
        if !current.is_empty() && space != in_space {
            out.push(std::mem::take(&mut current));
        }
        in_space = space;
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// A slide's markdown laid out on a stage of `stage` pixels.
#[must_use]
pub fn slide(markdown: &str, layout: Option<&str>, stage: (f64, f64), look: &Look) -> Flowed {
    let doc = parse(markdown, Options::ALL);
    let mut cx = Cx {
        ops: Vec::new(),
        look,
    };
    let width = stage.0 - 2.0 * PAD_X;
    let used = cx.blocks(&doc.blocks, PAD_X, PAD_TOP, width, false);
    // `center` and `title` put the text in the middle of the page.
    if matches!(layout, Some("center" | "title")) {
        let shift = ((stage.1 - 2.0 * PAD_TOP - used) / 2.0).max(0.0);
        for op in &mut cx.ops {
            match op {
                Op::Text { y, .. } => *y += shift,
                Op::Rect { y, .. } => *y += shift,
                Op::Line { y1, y2, .. } => {
                    *y1 += shift;
                    *y2 += shift;
                }
            }
        }
    }
    if layout == Some("center") {
        // Centred text: the lines were set left; shift them by half the free space of their own line.
        let mut lines: std::collections::BTreeMap<i64, (f64, f64)> =
            std::collections::BTreeMap::new();
        for op in &cx.ops {
            if let Op::Text {
                x,
                y,
                text,
                font,
                size,
                ..
            } = op
            {
                let key = *y as i64;
                let e = lines.entry(key).or_insert((*x, *x));
                e.0 = e.0.min(*x);
                e.1 = e.1.max(x + text_width(text, *font, *size));
            }
        }
        for op in &mut cx.ops {
            if let Op::Text { x, y, .. } = op
                && let Some((l, r)) = lines.get(&(*y as i64))
            {
                *x += ((stage.0 - PAD_X * 2.0 - (r - l)) / 2.0).max(0.0) - (*l - PAD_X);
            }
        }
    }
    Flowed { ops: cx.ops }
}

/// Draw a laid-out slide on a page; `scale` turns stage pixels into points.
pub fn paint(page: &mut Page, flowed: &Flowed, scale: f64) {
    for op in &flowed.ops {
        match op {
            Op::Rect { x, y, w, h, color } => {
                page.fill_rect(x * scale, y * scale, w * scale, h * scale, *color)
            }
            Op::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color,
            } => page.line(
                x1 * scale,
                y1 * scale,
                x2 * scale,
                y2 * scale,
                width * scale,
                *color,
            ),
            Op::Text {
                x,
                y,
                text,
                font,
                size,
                color,
                underline,
                strike,
            } => {
                page.text(x * scale, y * scale, text, *font, size * scale, *color);
                let w = text_width(text, *font, size * scale);
                if *underline {
                    page.line(
                        x * scale,
                        y * scale + 2.0,
                        x * scale + w,
                        y * scale + 2.0,
                        0.8,
                        *color,
                    );
                }
                if *strike {
                    page.line(
                        x * scale,
                        y * scale - size * scale * 0.3,
                        x * scale + w,
                        y * scale - size * scale * 0.3,
                        0.8,
                        *color,
                    );
                }
            }
        }
    }
}
