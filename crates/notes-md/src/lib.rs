//! Markdown notes: the parts lore-notes reads itself.
//!
//! The text stays the source of truth. Everything here reads it and gives back plain data (with the line each thing was on,
//! so the editor can jump there); nothing writes it except [`set_front_matter_key`].

mod front;
mod links;
mod slides;

pub use front::{FrontMatter, parse_front, set_front_matter_key};
pub use links::{
    Callout, Heading, Link, Piece, callout, flatten_wikilinks, label, outline, pieces,
    rename_links, slug, tags, title_of, wikilinks,
};
pub use slides::{Slide, slides};

/// The lines of a text, each with whether it is inside a fenced code block (the fence lines themselves count as inside).
pub(crate) fn fenced_lines(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.split('\n') {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |m| trimmed.chars().take_while(|c| *c == m).count());
        match fence {
            None => {
                if let (Some(m), true) = (marker, run >= 3) {
                    fence = Some((m, run));
                    out.push((line, true));
                } else {
                    out.push((line, false));
                }
            }
            Some((m, n)) => {
                out.push((line, true));
                if marker == Some(m) && run >= n && trimmed[run..].trim().is_empty() {
                    fence = None;
                }
            }
        }
    }
    out
}
