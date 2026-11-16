//! lore-pdf: read the text of a PDF, page by page, and open it in the browser's viewer to see the pages drawn.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::type_complexity,
    clippy::missing_const_for_fn,
    clippy::semicolon_if_nothing_returned,
    clippy::match_wildcard_for_single_variants,
    clippy::option_if_let_else
)]

pub mod i18n;
mod state;
mod view;

/// How the reader looks.
pub const STYLE: &str = include_str!("style.css");

pub use state::{Hit, PdfState};
pub use view::pdf_view;
