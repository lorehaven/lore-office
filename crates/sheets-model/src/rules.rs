//! Rules attached to ranges of a sheet: data validation and conditional formatting.

use crate::refs::Range;

/// Which way up the paper is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    /// Portrait, unless the sheet is wider than the page.
    #[default]
    Auto,
    Portrait,
    Landscape,
}

/// The paper sizes that can be chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

/// How a sheet is laid out on paper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSetup {
    pub orientation: Orientation,
    pub paper: Paper,
    /// The same margin on all four sides, in millimetres.
    pub margin_mm: u32,
    /// Shrink the sheet so that its width fits the page.
    pub fit_width: bool,
    /// Draw the lines between cells.
    pub gridlines: bool,
    /// Rows (first, last) repeated at the top of every page.
    pub repeat_rows: Option<(u32, u32)>,
    /// Print only this part of the sheet.
    pub print_area: Option<Range>,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            orientation: Orientation::Auto,
            paper: Paper::A4,
            margin_mm: 13,
            fit_width: false,
            gridlines: true,
            repeat_rows: None,
            print_area: None,
        }
    }
}

/// How a value is compared with the bounds of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

impl Compare {
    /// Whether `x` satisfies the comparison with `a` (and `b` for the two-sided ones).
    #[must_use]
    pub fn holds(self, x: f64, a: f64, b: f64) -> bool {
        let (lo, hi) = (a.min(b), a.max(b));
        match self {
            Self::Between => x >= lo && x <= hi,
            Self::NotBetween => x < lo || x > hi,
            Self::Equal => (x - a).abs() < 1e-12,
            Self::NotEqual => (x - a).abs() >= 1e-12,
            Self::Greater => x > a,
            Self::Less => x < a,
            Self::GreaterOrEqual => x >= a,
            Self::LessOrEqual => x <= a,
        }
    }

    /// The word the file format uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Between => "between",
            Self::NotBetween => "notBetween",
            Self::Equal => "equal",
            Self::NotEqual => "notEqual",
            Self::Greater => "greaterThan",
            Self::Less => "lessThan",
            Self::GreaterOrEqual => "greaterThanOrEqual",
            Self::LessOrEqual => "lessThanOrEqual",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "between" => Self::Between,
            "notBetween" => Self::NotBetween,
            "equal" => Self::Equal,
            "notEqual" => Self::NotEqual,
            "greaterThan" => Self::Greater,
            "lessThan" => Self::Less,
            "greaterThanOrEqual" => Self::GreaterOrEqual,
            "lessThanOrEqual" => Self::LessOrEqual,
            _ => return None,
        })
    }
}

/// What kind of value a number rule checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberKind {
    Whole,
    Decimal,
    Date,
    Time,
    Length,
}

/// What a cell may hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Allow {
    /// One of these texts.
    List(Vec<String>),
    /// One of the values in a range or name (`Sheet1!$A$1:$A$9`, written as in a formula).
    ListFrom(String),
    /// A number (or date, time, text length) compared with bounds that are values or formulas.
    Number {
        kind: NumberKind,
        op: Compare,
        a: String,
        b: String,
    },
    /// Whatever makes this formula true; it is written for the top left cell of the range.
    Custom(String),
}

/// How hard a rule is enforced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Severity {
    /// The entry is refused.
    #[default]
    Stop,
    /// The person is told and the entry is kept.
    Warning,
    Information,
}

/// A data validation rule on a range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validation {
    pub range: Range,
    pub allow: Allow,
    pub allow_blank: bool,
    /// Show the list as a drop-down in the cell (lists only).
    pub dropdown: bool,
    pub severity: Severity,
    pub title: String,
    pub message: String,
}

/// The look a conditional format gives a cell on top of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dxf {
    pub bold: bool,
    pub italic: bool,
    pub color: Option<u32>,
    pub fill: Option<u32>,
}

/// What makes a conditional format apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    /// The cell's number compared with bounds (values or formulas).
    CellIs {
        op: Compare,
        a: String,
        b: String,
    },
    Contains(String),
    NotContains(String),
    BeginsWith(String),
    EndsWith(String),
    /// A formula written for the top left cell of the range.
    Expression(String),
    Duplicates,
    Unique,
    Blanks,
    NotBlanks,
    Errors,
    /// The `n` biggest (or smallest) values, or percent of them.
    Top {
        n: u32,
        bottom: bool,
        percent: bool,
    },
    AboveAverage {
        below: bool,
    },
    /// Shades from `low` to `high` (through `mid`) by where the value falls between the smallest and largest.
    ColorScale {
        low: u32,
        mid: Option<u32>,
        high: u32,
    },
    /// A bar in the cell as long as the value is big.
    DataBar {
        color: u32,
    },
}

/// A conditional format on a range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CondRule {
    pub range: Range,
    pub cond: Cond,
    pub dxf: Dxf,
    /// Lower numbers win when rules disagree.
    pub priority: u32,
    pub stop_if_true: bool,
}
