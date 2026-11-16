//! Markdown in and out of a document.
//!
//! Out: headings become `#`, bullet and numbered paragraphs list items, bold `**`, italic `*`, strikethrough `~~`, a link
//! `[text](address)`, a table a pipe table whose first row is the header. In: the same, plus fenced code (set in the mono
//! face), quotes (as plain paragraphs) and paragraphs written over several lines (joined).

#![allow(clippy::too_many_lines, clippy::missing_const_for_fn)]

mod html;

pub use html::to_html;

use docs_text::{Cell, Doc, Family, Kind, Mark, Pos, Style};

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '~' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
}

fn inline(doc: &Doc, para: usize, out: &mut String) {
    for r in &doc.paras()[para].runs {
        let s = r.style;
        let (mut open, mut close) = (String::new(), String::new());
        if s.bold {
            open.push_str("**");
            close.insert_str(0, "**");
        }
        if s.italic {
            open.push('*');
            close.insert(0, '*');
        }
        if s.strike {
            open.push_str("~~");
            close.insert_str(0, "~~");
        }
        // Space at the edges would stop a marker from working.
        let trimmed = r.text.trim();
        if trimmed.is_empty() {
            out.push_str(&r.text);
            continue;
        }
        let lead = &r.text[..r.text.len() - r.text.trim_start().len()];
        let trail = &r.text[r.text.trim_end().len()..];
        out.push_str(lead);
        out.push_str(&open);
        let link = s.link.and_then(|l| doc.url(l));
        if link.is_some() {
            out.push('[');
        }
        if s.family == Some(Family::Mono) && !s.bold && !s.italic {
            out.push('`');
            out.push_str(&trimmed.replace('`', "'"));
            out.push('`');
        } else {
            escape(trimmed, out);
        }
        if let Some(u) = link {
            out.push_str("](");
            out.push_str(&u.replace(')', "%29").replace(' ', "%20"));
            out.push(')');
        }
        out.push_str(&close);
        out.push_str(trail);
    }
}

/// The document as Markdown.
#[must_use]
pub fn to_markdown(doc: &Doc) -> String {
    let mut out = String::new();
    let paras = doc.paras();
    let mut number = 0;
    let mut i = 0;
    let mut previous_list = false;
    while i < paras.len() {
        let p = &paras[i];
        if let Some(cell) = p.cell {
            // A table: pipe rows, a rule under the first.
            let mut j = i;
            while j < paras.len() && paras[j].cell.is_some_and(|c| c.table == cell.table) {
                j += 1;
            }
            let cols = doc.table_cols(cell.table).max(1);
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push('\n');
            }
            for (n, row) in paras[i..j].chunks(cols).enumerate() {
                out.push('|');
                for k in i + n * cols..(i + n * cols + row.len()) {
                    out.push(' ');
                    let mut cell_text = String::new();
                    inline(doc, k, &mut cell_text);
                    let cell_text = cell_text.replace(docs_text::LINE_BREAK, " ");
                    out.push_str(&cell_text);
                    out.push_str(" |");
                }
                out.push('\n');
                if n == 0 {
                    out.push('|');
                    for _ in 0..cols {
                        out.push_str(" --- |");
                    }
                    out.push('\n');
                }
            }
            out.push('\n');
            previous_list = false;
            number = 0;
            i = j;
            continue;
        }
        if p.picture.is_some() {
            out.push_str("![image]\n\n");
            i += 1;
            continue;
        }
        let listed = matches!(p.kind, Kind::Bullet | Kind::Numbered);
        match p.kind {
            Kind::Heading(n) => {
                out.push_str(&"#".repeat(usize::from(n.clamp(1, 6))));
                out.push(' ');
            }
            Kind::Bullet => {
                out.push_str("- ");
                number = 0;
            }
            Kind::Numbered => {
                number += 1;
                out.push_str(&number.to_string());
                out.push_str(". ");
            }
            Kind::Paragraph => number = 0,
        }
        if p.kind != Kind::Numbered {
            number = 0;
        }
        let from = out.len();
        inline(doc, i, &mut out);
        if out[from..].contains(docs_text::LINE_BREAK) {
            // A line break is two spaces and a new line in a paragraph, and a space anywhere else.
            let flat = out.split_off(from);
            out.push_str(&flat.replace(
                docs_text::LINE_BREAK,
                if p.kind == Kind::Paragraph {
                    "  \n"
                } else {
                    " "
                },
            ));
        }
        out.push('\n');
        // List items follow each other; everything else is a paragraph of its own.
        let next_listed = paras.get(i + 1).is_some_and(|n| n.kind == p.kind);
        if !(listed && next_listed) {
            out.push('\n');
        }
        previous_list = listed;
        i += 1;
    }
    let _ = previous_list;
    out.trim_end().to_string() + "\n"
}

// ---------------------------------------------------------------- reading

/// Styled text pieces of one line: the text, and the style it has (with the address of a link, kept apart).
struct Piece {
    text: String,
    style: Style,
    link: Option<String>,
}

fn parse_inline(line: &str) -> Vec<Piece> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<Piece> = Vec::new();
    let (mut bold, mut italic, mut strike) = (false, false, false);
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String,
                 out: &mut Vec<Piece>,
                 bold: bool,
                 italic: bool,
                 strike: bool,
                 link: Option<String>,
                 mono: bool| {
        if !text.is_empty() {
            out.push(Piece {
                text: std::mem::take(text),
                style: Style {
                    bold,
                    italic,
                    strike,
                    family: mono.then_some(Family::Mono),
                    ..Style::default()
                },
                link,
            });
        }
    };
    let alnum = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\\' if next.is_some() => {
                text.push(next.unwrap_or('\\'));
                i += 2;
            }
            '*' if next == Some('*') => {
                flush(&mut text, &mut out, bold, italic, strike, None, false);
                bold = !bold;
                i += 2;
            }
            '*' => {
                flush(&mut text, &mut out, bold, italic, strike, None, false);
                italic = !italic;
                i += 1;
            }
            '_' if !alnum(i.checked_sub(1).and_then(|k| chars.get(k)))
                || !alnum(chars.get(i + 1)) =>
            {
                let double = next == Some('_');
                flush(&mut text, &mut out, bold, italic, strike, None, false);
                if double {
                    bold = !bold;
                    i += 2;
                } else {
                    italic = !italic;
                    i += 1;
                }
            }
            '~' if next == Some('~') => {
                flush(&mut text, &mut out, bold, italic, strike, None, false);
                strike = !strike;
                i += 2;
            }
            '`' => {
                if let Some(end) = chars[i + 1..].iter().position(|c| *c == '`') {
                    flush(&mut text, &mut out, bold, italic, strike, None, false);
                    let code: String = chars[i + 1..i + 1 + end].iter().collect();
                    out.push(Piece {
                        text: code,
                        style: Style {
                            family: Some(Family::Mono),
                            ..Style::default()
                        },
                        link: None,
                    });
                    i += end + 2;
                } else {
                    text.push(c);
                    i += 1;
                }
            }
            '!' if next == Some('[') => {
                // A picture: its alt text.
                if let Some((label, _, after)) = link_at(&chars, i + 1) {
                    text.push_str(&label);
                    i = after;
                } else {
                    text.push(c);
                    i += 1;
                }
            }
            '[' => {
                if let Some((label, url, after)) = link_at(&chars, i) {
                    flush(&mut text, &mut out, bold, italic, strike, None, false);
                    for mut piece in parse_inline(&label) {
                        piece.style.bold |= bold;
                        piece.style.italic |= italic;
                        piece.style.strike |= strike;
                        piece.link = Some(url.clone());
                        out.push(piece);
                    }
                    i = after;
                } else {
                    text.push(c);
                    i += 1;
                }
            }
            c => {
                text.push(c);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut out, bold, italic, strike, None, false);
    out
}

/// `[label](url)` starting at `at` (a `[`): the label, the address, and where it ends.
fn link_at(chars: &[char], at: usize) -> Option<(String, String, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (k, c) in chars.iter().enumerate().skip(at) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(k);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = chars[close + 2..].iter().position(|c| *c == ')')? + close + 2;
    let label: String = chars[at + 1..close].iter().collect();
    let url: String = chars[close + 2..end].iter().collect();
    Some((label, url.trim().to_string(), end + 1))
}

/// Add styled pieces to the end of paragraph `at`.
fn add_pieces(doc: &mut Doc, at: usize, pieces: &[Piece]) {
    for piece in pieces {
        let start = Pos::new(at, doc.len_of(at));
        doc.replace(start, start, &piece.text);
        let end = Pos::new(at, doc.len_of(at));
        if end == start {
            continue;
        }
        // Typed text takes the style before it, so each mark is set to what the piece says.
        for (wanted, mark) in [
            (piece.style.bold, Mark::Bold),
            (piece.style.italic, Mark::Italic),
            (piece.style.strike, Mark::Strike),
        ] {
            if doc.all_marked(start, end, mark) != wanted {
                doc.toggle(start, end, mark);
            }
        }
        let family = piece.style.family;
        doc.restyle(start, end, |s| Style { family, ..s });
        doc.set_link(start, end, piece.link.as_deref());
    }
}

fn split_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_string()),
            c => cell.push(c),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

fn is_rule(line: &str) -> bool {
    let cells = split_row(line);
    !cells.is_empty()
        && cells.iter().all(|c| {
            !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')) && c.contains('-')
        })
}

/// The lines of a paragraph joined: with a space, unless one ends in a line break.
fn join_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for (n, l) in lines.iter().enumerate() {
        if n > 0 && !out.ends_with(docs_text::LINE_BREAK) {
            out.push(' ');
        }
        out.push_str(l);
    }
    out.trim_end_matches(docs_text::LINE_BREAK).to_string()
}

/// A document from Markdown.
#[must_use]
pub fn from_markdown(text: &str) -> Doc {
    let mut doc = Doc::new();
    let mut first = true;
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut paragraph: Vec<String> = Vec::new();
    // Make a new paragraph and return its number.
    let fresh = |doc: &mut Doc, first: &mut bool| {
        if *first {
            *first = false;
            0
        } else {
            doc.append_para()
        }
    };
    macro_rules! flush {
        () => {
            if !paragraph.is_empty() {
                let at = fresh(&mut doc, &mut first);
                add_pieces(&mut doc, at, &parse_inline(&join_lines(&paragraph)));
                paragraph.clear();
            }
        };
    }
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            flush!();
            i += 1;
            continue;
        }
        if let Some(fence) = trimmed.strip_prefix("```") {
            let _ = fence;
            flush!();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                let at = fresh(&mut doc, &mut first);
                add_pieces(
                    &mut doc,
                    at,
                    &[Piece {
                        text: lines[i].to_string(),
                        style: Style {
                            family: Some(Family::Mono),
                            ..Style::default()
                        },
                        link: None,
                    }],
                );
                i += 1;
            }
            i += 1;
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush!();
            let at = fresh(&mut doc, &mut first);
            add_pieces(&mut doc, at, &parse_inline(trimmed[hashes + 1..].trim()));
            doc.set_kind(at, Kind::Heading(u8::try_from(hashes.min(6)).unwrap_or(6)));
            i += 1;
            continue;
        }
        let bullet = ["- ", "* ", "+ "]
            .iter()
            .find_map(|m| trimmed.strip_prefix(m));
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        let numbered =
            (digits > 0 && trimmed[digits..].starts_with(". ")).then(|| &trimmed[digits + 2..]);
        if let Some(item) = bullet.or(numbered) {
            flush!();
            let at = fresh(&mut doc, &mut first);
            let item = item
                .strip_prefix("[ ] ")
                .or_else(|| item.strip_prefix("[x] "))
                .unwrap_or(item);
            add_pieces(&mut doc, at, &parse_inline(item.trim()));
            doc.set_kind(
                at,
                if bullet.is_some() {
                    Kind::Bullet
                } else {
                    Kind::Numbered
                },
            );
            i += 1;
            continue;
        }
        if trimmed.starts_with('|') && lines.get(i + 1).is_some_and(|l| is_rule(l)) {
            flush!();
            let header = split_row(trimmed);
            let cols = header.len().clamp(1, 8);
            let table = doc.new_table_id();
            let mut row = 0_u16;
            let add_row = |doc: &mut Doc, first: &mut bool, cells: &[String], row: u16| {
                for c in 0..cols {
                    let at = fresh(doc, first);
                    add_pieces(
                        doc,
                        at,
                        &parse_inline(cells.get(c).map_or("", String::as_str)),
                    );
                    doc.set_cell(
                        at,
                        Some(Cell {
                            table,
                            row,
                            col: u16::try_from(c).unwrap_or(0),
                        }),
                    );
                }
            };
            add_row(&mut doc, &mut first, &header, row);
            i += 2;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                row += 1;
                add_row(&mut doc, &mut first, &split_row(lines[i]), row);
                i += 1;
            }
            continue;
        }
        if let Some(quoted) = trimmed.strip_prefix('>') {
            paragraph.push(quoted.trim().to_string());
            i += 1;
            continue;
        }
        // Two spaces or a backslash at the end of a line break it inside the paragraph.
        let hard = line.ends_with("  ") || line.ends_with('\\');
        let mut text = trimmed.trim_end().trim_end_matches('\\').to_string();
        if hard {
            text.push(docs_text::LINE_BREAK);
        }
        paragraph.push(text);
        i += 1;
    }
    flush!();
    // A table is never the end of a document: there is a paragraph after it to type in.
    if doc.paras().last().is_some_and(|p| p.cell.is_some()) {
        doc.append_para();
    }
    doc
}
