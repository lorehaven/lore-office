//! Links between notes, tags, callouts and the outline.

use crate::{fenced_lines, front::parse_front};

/// `[[Note]]`, `[[Note#Heading]]`, `[[Note|alias]]` or `![[image.png]]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    /// `![[...]]`: shown in place instead of linked.
    pub embed: bool,
    pub line: usize,
}

/// The text of a span of the line outside inline code.
fn outside_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else {
            out.push(if in_code { ' ' } else { c });
        }
    }
    out
}

/// The wikilinks of a note, outside code, in order.
#[must_use]
pub fn wikilinks(text: &str) -> Vec<Link> {
    let mut out = Vec::new();
    for (n, (line, fenced)) in fenced_lines(text).into_iter().enumerate() {
        if fenced {
            continue;
        }
        let masked = outside_code(line);
        let mut at = 0;
        while let Some(open) = masked[at..].find("[[") {
            let start = at + open;
            let Some(close) = masked[start + 2..].find("]]") else {
                break;
            };
            let end = start + 2 + close;
            let inner = &line[start + 2..end];
            at = end + 2;
            if inner.trim().is_empty() || inner.contains('[') {
                continue;
            }
            let embed = start > 0 && line[..start].ends_with('!');
            let (target, alias) = inner
                .split_once('|')
                .map_or((inner, None), |(t, a)| (t, Some(a.trim().to_string())));
            let (name, heading) = target
                .split_once('#')
                .map_or((target, None), |(t, h)| (t, Some(h.trim().to_string())));
            out.push(Link {
                target: name.trim().to_string(),
                heading: heading.filter(|h| !h.is_empty()),
                alias: alias.filter(|a| !a.is_empty()),
                embed,
                line: n,
            });
        }
    }
    out
}

/// The `#tag` words outside code: lower case, without the `#`, each once, in order.
///
/// A tag starts with a letter and may hold letters, digits, `_`, `-` and `/`; a `#` in the middle of a word or a heading
/// marker is not one.
#[must_use]
pub fn tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (line, fenced) in fenced_lines(parse_front(text).1) {
        if fenced
            || line.trim_start().starts_with('#')
                && line.trim_start().chars().find(|c| *c != '#') == Some(' ')
        {
            continue;
        }
        let masked = outside_code(line);
        let chars: Vec<char> = masked.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if *c != '#' || i > 0 && !chars[i - 1].is_whitespace() {
                continue;
            }
            let word: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .collect();
            if word.chars().next().is_some_and(char::is_alphabetic) {
                let tag = word.to_lowercase();
                if !out.contains(&tag) {
                    out.push(tag);
                }
            }
        }
    }
    out
}

/// A heading of a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

/// The ATX headings (`# x`) outside code blocks.
#[must_use]
pub fn outline(text: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    for (n, (line, fenced)) in fenced_lines(text).into_iter().enumerate() {
        if fenced {
            continue;
        }
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            let text = trimmed[level..]
                .trim()
                .trim_end_matches('#')
                .trim()
                .to_string();
            if !text.is_empty() {
                out.push(Heading {
                    level: u8::try_from(level).unwrap_or(6),
                    text,
                    line: n,
                });
            }
        }
    }
    out
}

/// A heading's text as an address: lower case, words joined by `-`.
#[must_use]
pub fn slug(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The title of a note: the front matter's, else the first heading, else its name.
#[must_use]
pub fn title_of(name: &str, text: &str) -> String {
    let (front, body) = parse_front(text);
    front
        .title
        .filter(|t| !t.is_empty())
        .or_else(|| {
            outline(body)
                .into_iter()
                .find(|h| h.level == 1)
                .map(|h| h.text)
        })
        .unwrap_or_else(|| name.trim_end_matches(".md").to_string())
}

/// A block quote that starts with `[!note]`, `[!warning] Title`...
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Callout {
    /// Lower case: `note`, `tip`, `warning`...
    pub kind: String,
    /// What follows the marker on that line; empty when there is nothing.
    pub title: String,
}

/// The callout a block quote's first line makes, if it does (the line without its `>`).
#[must_use]
pub fn callout(first_line: &str) -> Option<Callout> {
    let rest = first_line.trim().strip_prefix("[!")?;
    let (kind, after) = rest.split_once(']')?;
    (!kind.is_empty() && kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then(|| {
        Callout {
            kind: kind.to_lowercase(),
            title: after.trim_start_matches(['+', '-']).trim().to_string(),
        }
    })
}

/// A stretch of a text: plain, or a wikilink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Link(Link),
}

/// A line of text cut into plain stretches and wikilinks (`line` is left 0).
#[must_use]
pub fn pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open + 2..].find("]]") else {
            break;
        };
        let inner = &rest[open + 2..open + 2 + close];
        let embed = rest[..open].ends_with('!');
        let before = &rest[..open - usize::from(embed)];
        if inner.trim().is_empty() || inner.contains('[') {
            out.push(Piece::Text(rest[..open + 2].to_string()));
            rest = &rest[open + 2..];
            continue;
        }
        if !before.is_empty() {
            out.push(Piece::Text(before.to_string()));
        }
        let (target, alias) = inner
            .split_once('|')
            .map_or((inner, None), |(t, a)| (t, Some(a.trim().to_string())));
        let (name, heading) = target
            .split_once('#')
            .map_or((target, None), |(t, h)| (t, Some(h.trim().to_string())));
        out.push(Piece::Link(Link {
            target: name.trim().to_string(),
            heading: heading.filter(|h| !h.is_empty()),
            alias: alias.filter(|a| !a.is_empty()),
            embed,
            line: 0,
        }));
        rest = &rest[open + 2 + close + 2..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    out
}

/// What a link shows: its alias, else its target (with the heading after a ` > `).
#[must_use]
pub fn label(link: &Link) -> String {
    if let Some(alias) = &link.alias {
        return alias.clone();
    }
    link.heading
        .as_ref()
        .map_or_else(|| link.target.clone(), |h| format!("{} > {h}", link.target))
}

/// The text with every `[[wikilink]]` written as what it shows (`![[pic.png]]` as the file's name), for a place that has no
/// notes to link to, such as an exported page.
#[must_use]
pub fn flatten_wikilinks(text: &str) -> String {
    let mut out = String::new();
    let mut fence = false;
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        if fence {
            out.push_str(line);
            continue;
        }
        for piece in pieces(line) {
            match piece {
                Piece::Text(t) => out.push_str(&t),
                Piece::Link(l) => out.push_str(&label(&l)),
            }
        }
    }
    out
}

/// The text with the links to `old` pointed at `new` (`[[old]]`, `[[old#h]]`, `[[old|alias]]`, embeds too); code is left alone.
#[must_use]
pub fn rename_links(text: &str, old: &str, new: &str) -> String {
    let mut out = String::new();
    let mut fence = false;
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        if fence || !line.contains("[[") {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find("[[") {
            let Some(close) = rest[open + 2..].find("]]") else {
                break;
            };
            let inner = &rest[open + 2..open + 2 + close];
            out.push_str(&rest[..open + 2]);
            let (target, tail) = inner
                .find(['#', '|'])
                .map_or((inner, ""), |i| (&inner[..i], &inner[i..]));
            if target.trim().eq_ignore_ascii_case(old.trim()) {
                out.push_str(new);
                out.push_str(tail);
            } else {
                out.push_str(inner);
            }
            out.push_str("]]");
            rest = &rest[open + 2 + close + 2..];
        }
        out.push_str(rest);
    }
    out
}
