//! What a cell holds once calculated, and how values turn into one another.

use std::fmt;

/// A spreadsheet error value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellError {
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    Null,
    /// A formula that depends on itself. Not an xlsx error: written out as 0.
    Circular,
    /// A formula that gives several values has no room: something is in the way.
    Spill,
}

impl CellError {
    /// The text a sheet shows: `#DIV/0!`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NA => "#N/A",
            Self::Null => "#NULL!",
            Self::Circular => "#CIRC!",
            Self::Spill => "#SPILL!",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Div0,
            Self::Value,
            Self::Ref,
            Self::Name,
            Self::Num,
            Self::NA,
            Self::Null,
            Self::Circular,
            Self::Spill,
        ]
        .into_iter()
        .find(|e| e.as_str().eq_ignore_ascii_case(s))
    }
}

impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A calculated cell value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
}

impl Value {
    /// As a number, the way arithmetic reads it: empty is 0, `TRUE` is 1, text that looks like a number is that number.
    pub fn to_number(&self) -> Result<f64, CellError> {
        match self {
            Self::Empty => Ok(0.0),
            Self::Number(n) => Ok(*n),
            Self::Bool(b) => Ok(f64::from(u8::from(*b))),
            Self::Text(t) => parse_number(t).ok_or(CellError::Value),
            Self::Error(e) => Err(*e),
        }
    }

    /// As text: numbers with up to 15 significant digits, booleans as `TRUE`/`FALSE`.
    pub fn to_text(&self) -> Result<String, CellError> {
        match self {
            Self::Empty => Ok(String::new()),
            Self::Number(n) => Ok(number_text(*n)),
            Self::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),
            Self::Text(t) => Ok(t.clone()),
            Self::Error(e) => Err(*e),
        }
    }

    /// As a condition: numbers are true unless 0, text must be `TRUE` or `FALSE`.
    pub fn to_bool(&self) -> Result<bool, CellError> {
        match self {
            Self::Empty => Ok(false),
            Self::Number(n) => Ok(*n != 0.0),
            Self::Bool(b) => Ok(*b),
            Self::Text(t) if t.eq_ignore_ascii_case("true") => Ok(true),
            Self::Text(t) if t.eq_ignore_ascii_case("false") => Ok(false),
            Self::Text(_) => Err(CellError::Value),
            Self::Error(e) => Err(*e),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    #[must_use]
    pub const fn error(&self) -> Option<CellError> {
        match self {
            Self::Error(e) => Some(*e),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(e) => f.write_str(e.as_str()),
            other => f.write_str(&other.to_text().unwrap_or_default()),
        }
    }
}

fn parse_number(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("nan") || t.to_ascii_lowercase().contains("inf") {
        return None;
    }
    let n: f64 = t.parse().ok()?;
    n.is_finite().then_some(n)
}

/// A number the way a sheet writes it: up to 15 significant digits, no trailing zeros, `-0` is `0`.
#[must_use]
pub fn number_text(n: f64) -> String {
    if n == 0.0 {
        return "0".to_string();
    }
    // Round to 15 significant digits through scientific notation, then print the shortest form.
    let rounded: f64 = format!("{n:.14e}").parse().unwrap_or(n);
    let s = format!("{rounded}");
    if s.len() > 20 {
        format!("{rounded:e}")
    } else {
        s
    }
}
