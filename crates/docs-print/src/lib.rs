//! Laying a document out on pages, and writing them as a PDF.
//!
//! [`layout`] breaks each paragraph into lines (words measured in the PDF fonts, so what is measured is what is drawn), sets
//! the lines left, centred, right or justified, numbers list items, and fills pages top to bottom. The result is plain data
//! ([`Layout`]) that a viewer could draw as well; [`to_pdf`] draws it.

#![allow(
    clippy::useless_let_if_seq,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::unreadable_literal,
    clippy::semicolon_if_nothing_returned
)]

use docs_text::{Align, Doc, Family, Kind, Para, Style};
use pdf_write::{Document, Font, Page, text_width};

/// A paper size in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub width: f64,
    pub height: f64,
}

pub const A4: Paper = Paper {
    width: 595.28,
    height: 841.89,
};
pub const LETTER: Paper = Paper {
    width: 612.0,
    height: 792.0,
};

impl Paper {
    /// The sheet a paper size is, in points.
    #[must_use]
    pub fn of(size: docs_text::PaperSize) -> Self {
        let (w, h) = size.twips();
        Self {
            width: f64::from(w) / 20.0,
            height: f64::from(h) / 20.0,
        }
    }
}

/// How pages are set up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    pub paper: Paper,
    /// Margins in points: top, right, bottom, left.
    pub margins: (f64, f64, f64, f64),
    /// The size of text that has none of its own, in points.
    pub body_size: f64,
    /// Print the page number at the foot of each page.
    pub page_numbers: bool,
}

impl Setup {
    /// The setup a document asks for: its paper size.
    #[must_use]
    pub fn for_doc(doc: &Doc) -> Self {
        Self {
            paper: Paper::of(doc.paper()),
            ..Self::default()
        }
    }
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            paper: A4,
            margins: (72.0, 72.0, 72.0, 72.0),
            body_size: 12.0,
            page_numbers: true,
        }
    }
}

/// A piece of text on a page: its baseline starts at (`x`, `y`) from the top left of the paper.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub font: Font,
    pub size: f64,
    pub color: u32,
    pub underline: bool,
    pub strike: bool,
    /// The colour behind the text.
    pub highlight: Option<u32>,
    /// The link the text is (the number of its address in the document).
    pub link: Option<u32>,
}

/// A picture on a page: its top left, its size, and the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub mime: &'static str,
    pub data: std::rc::Rc<Vec<u8>>,
}

/// The pages of a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub pages: Vec<Vec<Item>>,
    /// The lines to draw on each page (table borders).
    pub rules: Vec<Vec<Rule>>,
    /// The pictures on each page.
    pub images: Vec<Vec<Image>>,
    /// The paragraphs a new page starts at, in order (a paragraph that runs over a page is listed where it starts to):
    /// what an editor needs to show where the pages end.
    pub breaks: Vec<usize>,
}

const INK: u32 = 0x0000_0000;
const LIST_INDENT: f64 = 22.0;

fn font_of(para: &Para, style: Style) -> Font {
    let heading = matches!(para.kind, Kind::Heading(_));
    let family = match style.family {
        Some(Family::Sans) => pdf_write::Family::Sans,
        Some(Family::Serif) => pdf_write::Family::Serif,
        Some(Family::Mono) => pdf_write::Family::Mono,
        None if heading => pdf_write::Family::Sans,
        None => pdf_write::Family::Serif,
    };
    Font::in_family(family, style.bold || heading, style.italic)
}

fn size_of(para: &Para, style: Style, body: f64) -> f64 {
    style.size.map_or_else(
        || match para.kind {
            Kind::Heading(1) => body * 1.7,
            Kind::Heading(2) => body * 1.35,
            Kind::Heading(3) => body * 1.15,
            Kind::Heading(4) => body * 1.1,
            Kind::Heading(_) => body * 1.05,
            _ => body,
        },
        |s| f64::from(s) / 2.0,
    )
}

/// A word (or the spaces between words) with how it is drawn.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct Token {
    text: String,
    font: Font,
    size: f64,
    color: u32,
    underline: bool,
    strike: bool,
    highlight: Option<u32>,
    link: Option<u32>,
    /// How far the baseline is raised (lowered when negative).
    rise: f64,
    width: f64,
    space: bool,
    /// A forced line break (it has no width and is not drawn).
    newline: bool,
}

fn tokens(para: &Para, body: f64) -> Vec<Token> {
    let mut out = Vec::new();
    for r in &para.runs {
        let (font, full) = (font_of(para, r.style), size_of(para, r.style, body));
        // Raised and lowered text is set smaller.
        let raised = r.style.sup || r.style.sub;
        let size = if raised { full * 0.65 } else { full };
        let rise = if r.style.sup {
            full * 0.35
        } else if r.style.sub {
            -full * 0.15
        } else {
            0.0
        };
        let linked = r.style.link.is_some();
        let color = r
            .style
            .color
            .unwrap_or(if linked { 0x0005_63C1 } else { INK });
        let mut word = String::new();
        let mut is_space = false;
        let push = |word: &mut String, space: bool, out: &mut Vec<Token>| {
            if !word.is_empty() {
                let text = std::mem::take(word);
                out.push(Token {
                    width: text_width(&text, font, size),
                    text,
                    font,
                    size,
                    color,
                    underline: r.style.underline || linked,
                    strike: r.style.strike,
                    highlight: r.style.highlight,
                    link: r.style.link,
                    rise,
                    space,
                    newline: false,
                });
            }
        };
        for c in r.text.chars() {
            if c == docs_text::LINE_BREAK {
                push(&mut word, is_space, &mut out);
                is_space = false;
                out.push(Token {
                    text: String::new(),
                    font,
                    size,
                    color,
                    underline: false,
                    strike: false,
                    highlight: None,
                    link: None,
                    rise: 0.0,
                    width: 0.0,
                    space: false,
                    newline: true,
                });
                continue;
            }
            let space = c == ' ' || c == '\t';
            if space != is_space {
                push(&mut word, is_space, &mut out);
                is_space = space;
            }
            word.push(if c == '\t' { ' ' } else { c });
        }
        push(&mut word, is_space, &mut out);
    }
    out
}

/// The lines of a paragraph for a measure of `width`: each is its tokens (trailing spaces dropped).
fn break_lines(tokens: Vec<Token>, width: f64, first: f64) -> Vec<Vec<Token>> {
    let mut lines: Vec<Vec<Token>> = vec![Vec::new()];
    let mut used = 0.0;
    for t in tokens {
        if t.newline {
            lines.push(Vec::new());
            used = 0.0;
            continue;
        }
        let limit = if lines.len() == 1 { first } else { width };
        if !t.space && used + t.width > limit && used > 0.0 {
            lines.push(Vec::new());
            used = 0.0;
        }
        if t.space && used == 0.0 {
            continue;
        }
        used += t.width;
        if let Some(l) = lines.last_mut() {
            l.push(t);
        }
    }
    for l in &mut lines {
        while l.last().is_some_and(|t| t.space) {
            l.pop();
        }
    }
    lines
}

/// A straight line (a table's border), from (`x1`, `y1`) to (`x2`, `y2`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Where a paragraph goes: its left edge, its width, the top it starts at, and where pages break (`None`: they do not).
struct Spot {
    left: f64,
    measure: f64,
    y: f64,
    pages: Option<(f64, f64)>,
}

/// Put a paragraph on the page (new pages are added when it runs past the foot); returns the `y` below it. `spacing` is
/// whether the space after the paragraph is kept (it is not inside a table cell).
fn place(
    para: &Para,
    marker: Option<&str>,
    setup: &Setup,
    at: &Spot,
    spacing: bool,
    pages: &mut Vec<Vec<Item>>,
) -> f64 {
    let Spot {
        left,
        measure,
        mut y,
        pages: breaks,
    } = *at;
    let listed = marker.is_some();
    let fmt = para.fmt;
    let indent = (if listed {
        LIST_INDENT * (1.0 + f64::from(fmt.level.min(8)))
    } else {
        0.0
    }) + f64::from(fmt.indent) / 20.0;
    let first_extra = f64::from(fmt.first) / 20.0;
    let toks = tokens(para, setup.body_size);
    let line_size = toks
        .iter()
        .map(|t| t.size)
        .fold(0.0, f64::max)
        .max(if toks.is_empty() {
            size_of(para, Style::default(), setup.body_size)
        } else {
            0.0
        });
    let line_height = line_size
        * 1.2
        * if fmt.line == 0 {
            1.0
        } else {
            f64::from(fmt.line) / 240.0
        };
    let heading = matches!(para.kind, Kind::Heading(_));
    let before = if heading && spacing {
        line_size * 0.6
    } else {
        0.0
    };
    let after = match (heading, spacing) {
        (_, false) => 0.0,
        (true, _) => line_size * 0.3,
        _ => setup.body_size * 0.5,
    };
    let after = if spacing {
        fmt.after.map_or(after, |a| f64::from(a) / 20.0)
    } else {
        after
    };
    let lines = break_lines(toks, measure - indent, measure - indent - first_extra);
    y += before;
    let last = lines.len() - 1;
    for (n, line) in lines.iter().enumerate() {
        let shift = if n == 0 { first_extra } else { 0.0 };
        if let Some((top, floor)) = breaks
            && y + line_height > floor
            && y > top
        {
            pages.push(Vec::new());
            y = top;
        }
        let baseline = y + line_size;
        let page = pages.last_mut().unwrap_or_else(|| unreachable!());
        if n == 0 && listed {
            let marker = marker.unwrap_or_default().to_string();
            let font = Font::in_family(pdf_write::Family::Serif, false, false);
            let w = text_width(&marker, font, setup.body_size);
            page.push(Item {
                x: left + indent + shift - 6.0 - w,
                y: baseline,
                text: marker,
                font,
                size: setup.body_size,
                color: INK,
                underline: false,
                strike: false,
                highlight: None,
                link: None,
            });
        }
        let natural: f64 = line.iter().map(|t| t.width).sum();
        let spaces = line.iter().filter(|t| t.space).count();
        let free = (measure - indent - shift - natural).max(0.0);
        let (start, stretch) = match para.align {
            Align::Left => (0.0, 0.0),
            Align::Center => (free / 2.0, 0.0),
            Align::Right => (free, 0.0),
            Align::Justify if n != last && spaces > 0 => (0.0, free / spaces as f64),
            Align::Justify => (0.0, 0.0),
        };
        let mut x = left + indent + shift + start;
        let mut group: Option<Item> = None;
        let flush = |g: &mut Option<Item>, page: &mut Vec<Item>| {
            if let Some(item) = g.take() {
                page.push(item);
            }
        };
        for t in line {
            let advance = t.width + if t.space { stretch } else { 0.0 };
            if stretch > 0.0 && t.space {
                // A justified gap: the pieces on both sides are drawn apart.
                flush(&mut group, page);
                x += advance;
                continue;
            }
            match &mut group {
                Some(g)
                    if g.font == t.font
                        && g.size == t.size
                        && g.color == t.color
                        && g.underline == t.underline
                        && g.strike == t.strike
                        && g.highlight == t.highlight
                        && g.link == t.link
                        && (g.y - (baseline - t.rise)).abs() < 1e-9 =>
                {
                    g.text.push_str(&t.text)
                }
                _ => {
                    flush(&mut group, page);
                    group = Some(Item {
                        x,
                        y: baseline - t.rise,
                        text: t.text.clone(),
                        font: t.font,
                        size: t.size,
                        color: t.color,
                        underline: t.underline,
                        strike: t.strike,
                        highlight: t.highlight,
                        link: t.link,
                    });
                }
            }
            x += advance;
        }
        flush(&mut group, page);
        y += line_height;
    }
    y + after
}

const CELL_PAD: f64 = 4.0;

/// Lay a document out on pages.
#[must_use]
pub fn layout(doc: &Doc, setup: &Setup) -> Layout {
    let (top, right, bottom, left) = setup.margins;
    let measure = setup.paper.width - left - right;
    let floor = setup.paper.height - bottom;
    let mut pages: Vec<Vec<Item>> = vec![Vec::new()];
    let mut rules: Vec<Vec<Rule>> = vec![Vec::new()];
    let mut images: Vec<Vec<Image>> = vec![Vec::new()];
    let mut y = top;
    let mut breaks: Vec<usize> = Vec::new();
    let mark = |breaks: &mut Vec<usize>, at: usize| {
        if breaks.last() != Some(&at) {
            breaks.push(at);
        }
    };
    let markers = doc.list_markers();
    let paras = doc.paras();
    let mut i = 0;
    while i < paras.len() {
        let para = &paras[i];
        if let Some(cell) = para.cell {
            // A table: its rows are laid out whole (a row that does not fit goes to the next page).
            let mut j = i;
            while j < paras.len() && paras[j].cell.is_some_and(|c| c.table == cell.table) {
                j += 1;
            }
            let rows = paras[i..j]
                .iter()
                .filter_map(|p| p.cell)
                .map(|c| usize::from(c.row) + 1)
                .max()
                .unwrap_or(1);
            // Where the columns start (their widths are fractions of the measure).
            let mut bounds = vec![0.0_f64];
            for w in doc.table_widths(cell.table) {
                let last = bounds[bounds.len() - 1];
                bounds.push(last + measure * f64::from(w) / 1000.0);
            }
            let header = doc.table_header(cell.table);
            let mut r = 0;
            let mut resume: Option<usize> = None;
            while r < rows {
                let cells: Vec<(usize, &Para)> = paras[i..j]
                    .iter()
                    .filter(|p| p.cell.is_some_and(|c| usize::from(c.row) == r))
                    .filter_map(|p| p.cell.map(|c| (usize::from(c.col), p)))
                    .collect();
                // How tall the row is: the tallest cell, measured on scratch pages.
                let mut height = 0.0_f64;
                for (c, p) in &cells {
                    let mut scratch = vec![Vec::new()];
                    let at = Spot {
                        left: 0.0,
                        measure: bounds[c + 1] - bounds[*c] - 2.0 * CELL_PAD,
                        y: 0.0,
                        pages: None,
                    };
                    height = height.max(place(p, None, setup, &at, false, &mut scratch));
                }
                let height = height + 2.0 * CELL_PAD;
                let mut top_row = r == 0 || resume.is_some();
                if y + height > floor && y > top {
                    mark(&mut breaks, i);
                    pages.push(Vec::new());
                    rules.push(Vec::new());
                    images.push(Vec::new());
                    y = top;
                    // A header row is set again at the top of the page the table goes on to.
                    if header && r > 0 && resume.is_none() {
                        resume = Some(r);
                        r = 0;
                        continue;
                    }
                    top_row = true;
                }
                for (c, p) in &cells {
                    let at = Spot {
                        left: left + bounds[*c] + CELL_PAD,
                        measure: bounds[c + 1] - bounds[*c] - 2.0 * CELL_PAD,
                        y: y + CELL_PAD,
                        pages: None,
                    };
                    place(p, None, setup, &at, false, &mut pages);
                }
                let page = rules.last_mut().unwrap_or_else(|| unreachable!());
                if top_row {
                    page.push(Rule {
                        x1: left,
                        y1: y,
                        x2: left + measure,
                        y2: y,
                    });
                }
                page.push(Rule {
                    x1: left,
                    y1: y + height,
                    x2: left + measure,
                    y2: y + height,
                });
                for x in &bounds {
                    page.push(Rule {
                        x1: left + x,
                        y1: y,
                        x2: left + x,
                        y2: y + height,
                    });
                }
                y += height;
                r = match (r, resume.take()) {
                    (0, Some(back)) => back,
                    (_, other) => {
                        resume = other;
                        r + 1
                    }
                };
            }
            y += setup.body_size * 0.5;
            i = j;
            continue;
        }
        if let Some(pic) = para.picture.and_then(|n| doc.picture(n)) {
            // A picture is a block: scaled down to the measure if it is wider, set by the paragraph's alignment, and moved
            // to the next page when it does not fit.
            let (mut w, mut h) = pic.size();
            if w > measure {
                (w, h) = (measure, h * measure / w);
            }
            if y + h > floor && y > top {
                mark(&mut breaks, i);
                pages.push(Vec::new());
                rules.push(Vec::new());
                images.push(Vec::new());
                y = top;
            }
            let x = match para.align {
                Align::Center => left + (measure - w) / 2.0,
                Align::Right => left + measure - w,
                _ => left,
            };
            images
                .last_mut()
                .unwrap_or_else(|| unreachable!())
                .push(Image {
                    x,
                    y,
                    width: w,
                    height: h,
                    mime: pic.mime,
                    data: pic.data.clone(),
                });
            y += h + setup.body_size * 0.5;
            i += 1;
            continue;
        }
        if para.fmt.page_break && y > top {
            mark(&mut breaks, i);
            pages.push(Vec::new());
            rules.push(Vec::new());
            images.push(Vec::new());
            y = top;
        }
        let at = Spot {
            left,
            measure,
            y,
            pages: Some((top, floor)),
        };
        let before = pages.len();
        y = place(para, markers[i].as_deref(), setup, &at, true, &mut pages);
        if pages.len() > before {
            mark(&mut breaks, i);
        }
        while rules.len() < pages.len() {
            rules.push(Vec::new());
        }
        while images.len() < pages.len() {
            images.push(Vec::new());
        }
        i += 1;
    }
    // The header and footer on every page; with no footer, the page number.
    let total = pages.len();
    let stamp = |text: &str, n: usize| {
        text.replace("{pages}", &total.to_string())
            .replace("{page}", &(n + 1).to_string())
    };
    for (n, page) in pages.iter_mut().enumerate() {
        let mut lines: Vec<(String, f64)> = Vec::new();
        let header: Vec<&str> = doc.header().split('\n').filter(|l| !l.is_empty()).collect();
        for (k, line) in header.iter().enumerate() {
            lines.push((
                stamp(line, n),
                (top / 2.0 - 4.0 * header.len() as f64 + 11.0 * k as f64).max(14.0),
            ));
        }
        let footer: Vec<&str> = doc.footer().split('\n').filter(|l| !l.is_empty()).collect();
        if footer.is_empty() && setup.page_numbers {
            lines.push((
                format!("{} / {total}", n + 1),
                setup.paper.height - bottom / 2.0,
            ));
        }
        for (k, line) in footer.iter().enumerate() {
            lines.push((
                stamp(line, n),
                setup.paper.height - bottom / 2.0 - 11.0 * (footer.len() - 1 - k) as f64,
            ));
        }
        for (text, y) in lines {
            let w = text_width(&text, Font::Helvetica, 9.0);
            page.push(Item {
                x: (setup.paper.width - w) / 2.0,
                y,
                text,
                font: Font::Helvetica,
                size: 9.0,
                color: 0x0066_6666,
                underline: false,
                strike: false,
                highlight: None,
                link: None,
            });
        }
    }
    while rules.len() < pages.len() {
        rules.push(Vec::new());
    }
    while images.len() < pages.len() {
        images.push(Vec::new());
    }
    Layout {
        pages,
        rules,
        images,
        breaks,
    }
}

/// The document as a PDF.
#[must_use]
pub fn to_pdf(doc: &Doc, setup: &Setup, title: &str) -> Vec<u8> {
    let laid = layout(doc, setup);
    let mut out = Document::new();
    out.title(title);
    for (n, items) in laid.pages.iter().enumerate() {
        let mut page = Page::new(setup.paper.width, setup.paper.height);
        for r in laid.rules.get(n).map_or(&[][..], Vec::as_slice) {
            page.line(r.x1, r.y1, r.x2, r.y2, 0.5, 0x0080_8080);
        }
        for im in laid.images.get(n).map_or(&[][..], Vec::as_slice) {
            if let Ok(px) = quench_image::decode(&im.data) {
                page.image(
                    im.x, im.y, im.width, im.height, px.width, px.height, &px.rgba,
                );
            }
        }
        for i in items {
            let w = text_width(&i.text, i.font, i.size);
            if let Some(h) = i.highlight {
                page.fill_rect(i.x, i.y - i.size * 0.85, w, i.size * 1.15, h);
            }
        }
        for i in items {
            page.text(i.x, i.y, &i.text, i.font, i.size, i.color);
            if i.strike {
                let w = text_width(&i.text, i.font, i.size);
                page.line(
                    i.x,
                    i.y - i.size * 0.3,
                    i.x + w,
                    i.y - i.size * 0.3,
                    i.size * 0.06,
                    i.color,
                );
            }
            if let Some(url) = i.link.and_then(|l| doc.url(l)) {
                let w = text_width(&i.text, i.font, i.size);
                page.link(i.x, i.y - i.size * 0.85, w, i.size * 1.1, url);
            }
            if i.underline {
                let w = text_width(&i.text, i.font, i.size);
                page.line(
                    i.x,
                    i.y + i.size * 0.12,
                    i.x + w,
                    i.y + i.size * 0.12,
                    i.size * 0.06,
                    i.color,
                );
            }
        }
        out.add(page);
    }
    out.finish()
}
