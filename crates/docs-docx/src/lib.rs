//! `.docx` files and the text model.

#![allow(
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::explicit_counter_loop
)]

mod read;
mod write;

pub use read::{Docx, Report, read};
pub use write::{write, write_new};

/// What went wrong with a file.
#[derive(Debug)]
pub enum Error {
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Invalid(m) = self;
        write!(f, "not a readable document: {m}")
    }
}

impl std::error::Error for Error {}

impl From<office_fmt::Error> for Error {
    fn from(e: office_fmt::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}

pub(crate) const MAIN_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub(crate) const REL_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const DOCUMENT: &str = "word/document.xml";
