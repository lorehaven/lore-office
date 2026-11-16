//! Reading `.xls` files.

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

mod biff;
mod ole;

pub use biff::read;

/// What went wrong with a file.
#[derive(Debug)]
pub enum Error {
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Invalid(m) = self;
        write!(f, "not a readable .xls file: {m}")
    }
}

impl std::error::Error for Error {}
