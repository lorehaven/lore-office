use std::fmt;

/// Why a package or one of its parts could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The zip container is damaged.
    Zip(String),
    /// A part is not well-formed XML (or not UTF-8).
    Xml(String),
    /// A part or relationship that must be there is not.
    Missing(String),
    /// The file is larger than what is read from untrusted input.
    Limit(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip(m) => write!(f, "zip: {m}"),
            Self::Xml(m) => write!(f, "xml: {m}"),
            Self::Missing(m) => write!(f, "missing: {m}"),
            Self::Limit(m) => write!(f, "too large: {m}"),
        }
    }
}

impl std::error::Error for Error {}
