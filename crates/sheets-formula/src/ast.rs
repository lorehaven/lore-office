//! The syntax tree of a formula.

use sheets_model::{Axis, CellError, CellRef, shift_point, shift_span};

/// A cell reference as written: optional sheet, and which parts are anchored with `$`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub sheet: Option<String>,
    pub cell: CellRef,
    pub col_abs: bool,
    pub row_abs: bool,
}

/// `A1:B2`, optionally on another sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeRef {
    pub sheet: Option<String>,
    pub start: Reference,
    pub end: Reference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Expr {
    /// Rewrite the references to `target` (a sheet name) for rows or columns inserted at `at` (`delta` > 0) or deleted
    /// (`delta` < 0). `own` is the sheet the formula is on, which unqualified references mean. A reference to something
    /// deleted becomes `#REF!`. Returns whether anything changed.
    pub fn shift_axis(&mut self, own: &str, target: &str, axis: Axis, at: u32, delta: i64) -> bool {
        let hits =
            |sheet: &Option<String>| sheet.as_deref().unwrap_or(own).eq_ignore_ascii_case(target);
        let pick = |c: CellRef| if axis == Axis::Row { c.row } else { c.col };
        let with = |c: CellRef, i: u32| {
            if axis == Axis::Row {
                CellRef::new(i, c.col)
            } else {
                CellRef::new(c.row, i)
            }
        };
        match self {
            Self::Ref(r) if hits(&r.sheet) => match shift_point(pick(r.cell), at, delta) {
                Some(i) if i == pick(r.cell) => false,
                Some(i) => {
                    r.cell = with(r.cell, i);
                    true
                }
                None => {
                    *self = Self::Error(CellError::Ref);
                    true
                }
            },
            Self::Range(r) if hits(&r.sheet.clone().or_else(|| r.start.sheet.clone())) => {
                let (a, b) = (pick(r.start.cell), pick(r.end.cell));
                let (lo, hi) = (a.min(b), a.max(b));
                match shift_span(lo, hi, at, delta) {
                    Some((nl, nh)) if (nl, nh) == (lo, hi) => false,
                    Some((nl, nh)) => {
                        let (first, second) = if a <= b { (nl, nh) } else { (nh, nl) };
                        r.start.cell = with(r.start.cell, first);
                        r.end.cell = with(r.end.cell, second);
                        true
                    }
                    None => {
                        *self = Self::Error(CellError::Ref);
                        true
                    }
                }
            }
            Self::Unary(_, x) | Self::Percent(x) => x.shift_axis(own, target, axis, at, delta),
            Self::Binary(_, l, r) => {
                let (a, b) = (
                    l.shift_axis(own, target, axis, at, delta),
                    r.shift_axis(own, target, axis, at, delta),
                );
                a || b
            }
            Self::Call(_, args) => {
                let mut any = false;
                for a in args {
                    any |= a.shift_axis(own, target, axis, at, delta);
                }
                any
            }
            _ => false,
        }
    }

    /// Point every reference to sheet `old` (compared without regard to case) at `new` instead. Returns whether any did.
    pub fn rename_sheet(&mut self, old: &str, new: &str) -> bool {
        let mut changed = false;
        let mut point = |sheet: &mut Option<String>| {
            if sheet
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(old))
            {
                *sheet = Some(new.to_string());
                changed = true;
            }
        };
        match self {
            Self::Ref(r) => point(&mut r.sheet),
            Self::Range(r) => {
                point(&mut r.sheet);
                point(&mut r.start.sheet);
                point(&mut r.end.sheet);
            }
            Self::Unary(_, x) | Self::Percent(x) => return x.rename_sheet(old, new),
            Self::Binary(_, l, r) => {
                let (a, b) = (l.rename_sheet(old, new), r.rename_sheet(old, new));
                return a || b;
            }
            Self::Call(_, args) => {
                let mut any = false;
                for a in args {
                    any |= a.rename_sheet(old, new);
                }
                return any;
            }
            _ => {}
        }
        changed
    }

    /// Whether the formula's value can change without anything it reads changing (`NOW`, `RAND`, ...).
    #[must_use]
    pub fn is_volatile(&self) -> bool {
        match self {
            Self::Call(name, args) => {
                crate::functions::VOLATILE.contains(&name.as_str())
                    || args.iter().any(Self::is_volatile)
            }
            Self::Unary(_, x) | Self::Percent(x) => x.is_volatile(),
            Self::Binary(_, l, r) => l.is_volatile() || r.is_volatile(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
    Ref(Reference),
    Range(RangeRef),
    /// A defined name (not resolved yet: evaluates to `#NAME?`).
    Name(String),
    Unary(UnOp, Box<Self>),
    Binary(BinOp, Box<Self>, Box<Self>),
    /// `50%`
    Percent(Box<Self>),
    /// The function name is upper case.
    Call(String, Vec<Self>),
}
