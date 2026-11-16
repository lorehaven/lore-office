//! OpenDocument spreadsheets (`.ods`).
//!
//! Read: sheets, cell values and formulas, text styles (bold, italic, underline, colour), fill, alignment and wrapping,
//! percent, number and date formats, column widths and row heights, merged cells, hidden rows and columns. Written: the same.
//! Charts, validation, conditional formats and the like in a file are not read, and so are not kept.

#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::style,
    clippy::complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

mod formula;
mod read;
mod write;

pub use formula::{from_odf, to_odf};
pub use read::read;
pub use write::write;

/// What went wrong with a file.
#[derive(Debug)]
pub enum Error {
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Invalid(m) = self;
        write!(f, "not a readable spreadsheet: {m}")
    }
}

impl std::error::Error for Error {}

impl From<office_fmt::Error> for Error {
    fn from(e: office_fmt::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}

/// Pixels per centimetre and per inch (CSS).
pub(crate) const PX_PER_CM: f64 = 96.0 / 2.54;
