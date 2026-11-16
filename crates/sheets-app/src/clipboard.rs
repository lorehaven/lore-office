//! What copying a range puts on the clipboard, and reading what was pasted.

use crate::history::CellState;
use sheets_model::{CellRef, Range};

/// A copied range, kept inside the app so that pasting it here keeps formulas (moved to the new place) and formats.
#[derive(Clone, Debug)]
pub struct Clip {
    /// The text that went on the system clipboard; a paste of the same text is this clip.
    pub text: String,
    /// The sheet the cells came from.
    pub sheet: usize,
    /// Where the top-left cell came from.
    pub origin: CellRef,
    pub cells: Vec<Vec<CellState>>,
    /// For a cut: the sheet and range to empty once it is pasted.
    pub cut: Option<(usize, Range)>,
}

/// Tab-separated text, one line per row; fields with a tab, quote or line break are quoted.
#[must_use]
pub fn to_text(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        for (i, field) in row.iter().enumerate() {
            if i > 0 {
                out.push('\t');
            }
            sheets_csv::quote(field, '\t', &mut out);
        }
        out.push('\n');
    }
    out
}

/// A table for programs that paste HTML (a word processor, a mail editor).
#[must_use]
pub fn to_html(rows: &[Vec<String>]) -> String {
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut out = String::from("<table>");
    for row in rows {
        out.push_str("<tr>");
        for field in row {
            out.push_str("<td>");
            out.push_str(&escape(field));
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// The rows and fields of pasted text: tab-separated, quotes undone, a final line break ignored.
#[must_use]
pub fn from_text(text: &str) -> Vec<Vec<String>> {
    let text = text.replace("\r\n", "\n");
    sheets_csv::records(&text, b'\t').unwrap_or_default()
}
