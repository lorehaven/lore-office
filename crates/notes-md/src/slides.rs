//! A note as a deck: slides split by `---`, speaker notes, directives.

use crate::fenced_lines;

/// One slide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slide {
    /// The markdown shown, without the notes and the directives.
    pub markdown: String,
    /// What the speaker sees: everything after a `???` or `Note:` line.
    pub notes: String,
    /// The layout asked for by `<!-- layout: two-col -->`.
    pub layout: Option<String>,
    /// Other directives, `<!-- key: value -->`, in order.
    pub directives: Vec<(String, String)>,
    /// The line of the note (counting from 0 at the start of the text given) the slide starts on.
    pub line: usize,
}

fn directive(line: &str) -> Option<(String, String)> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?;
    // A bare `<!-- fragments -->` is a switch that is on.
    let (key, value) = match inner.split_once(':') {
        Some(kv) => kv,
        None if inner.trim() == "fragments" => ("fragments", "true"),
        None => return None,
    };
    let key = key.trim();
    (!key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then(|| (key.to_lowercase(), value.trim().to_string()))
}

/// The slides of the text after the front matter.
///
/// Lines that are only `---` split it (not inside a code block); empty slides are dropped, but a text with nothing in it
/// is one empty slide, so there is always something to show.
#[must_use]
pub fn slides(body: &str) -> Vec<Slide> {
    let mut out: Vec<Slide> = Vec::new();
    let mut current = Slide::default();
    let mut in_notes = false;
    let mut started = false;
    for (n, (line, fenced)) in fenced_lines(body).into_iter().enumerate() {
        let line = line.trim_end_matches('\r');
        if !fenced && line.trim_end() == "---" {
            if started || !current.markdown.trim().is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.line = n + 1;
            in_notes = false;
            started = false;
            continue;
        }
        if !started {
            if line.trim().is_empty() {
                current.line = n + 1;
                continue;
            }
            started = true;
        }
        if !fenced
            && !in_notes
            && (line.trim_end() == "???" || line.trim().to_lowercase().starts_with("note:"))
        {
            in_notes = true;
            let rest = line
                .trim()
                .get(5..)
                .filter(|_| line.trim().to_lowercase().starts_with("note:"))
                .unwrap_or("");
            if !rest.trim().is_empty() {
                current.notes.push_str(rest.trim());
                current.notes.push('\n');
            }
            continue;
        }
        if !fenced
            && !in_notes
            && let Some((k, v)) = directive(line)
        {
            if k == "layout" {
                current.layout = Some(v);
            } else {
                current.directives.push((k, v));
            }
            continue;
        }
        let target = if in_notes {
            &mut current.notes
        } else {
            &mut current.markdown
        };
        target.push_str(line);
        target.push('\n');
    }
    if started || !current.markdown.trim().is_empty() || out.is_empty() {
        out.push(current);
    }
    for s in &mut out {
        s.markdown = s.markdown.trim_matches('\n').to_string();
        s.notes = s.notes.trim_matches('\n').to_string();
    }
    out
}
