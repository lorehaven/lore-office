//! Cell addresses: zero-based rows and columns, A1 notation, rectangular ranges.

use std::fmt;

/// Rows a sheet can address (as in xlsx).
pub const MAX_ROWS: u32 = 1_048_576;
/// Columns a sheet can address (`A` to `XFD`).
pub const MAX_COLS: u32 = 16_384;

/// The letters of a zero-based column: 0 is `A`, 25 is `Z`, 26 is `AA`.
#[must_use]
pub fn col_name(col: u32) -> String {
    let mut n = col + 1;
    let mut out = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        out.push(b'A' + u8::try_from(r).unwrap_or(0));
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The zero-based column of letters (any case); `None` when empty, not letters, or past `XFD`.
#[must_use]
pub fn col_index(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut n: u32 = 0;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1;
    }
    (n <= MAX_COLS).then(|| n - 1)
}

/// Rows or columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

/// Where a position `i` on an axis goes when `delta` rows or columns are inserted (positive) at `at`, or `-delta` are
/// deleted from `at` (negative). `None` when it was deleted.
#[must_use]
pub fn shift_point(i: u32, at: u32, delta: i64) -> Option<u32> {
    if delta >= 0 {
        let d = u32::try_from(delta).unwrap_or(u32::MAX);
        Some(if i >= at { i.saturating_add(d) } else { i })
    } else {
        let n = u32::try_from(-delta).unwrap_or(u32::MAX);
        if i < at {
            Some(i)
        } else if i < at.saturating_add(n) {
            None
        } else {
            Some(i - n)
        }
    }
}

/// The same for the span `lo..=hi`: it grows when rows are inserted inside it, shrinks when some of it is deleted, and is
/// gone (`None`) when all of it is.
#[must_use]
pub fn shift_span(lo: u32, hi: u32, at: u32, delta: i64) -> Option<(u32, u32)> {
    if delta >= 0 {
        let d = u32::try_from(delta).unwrap_or(u32::MAX);
        let f = |i: u32| if i >= at { i.saturating_add(d) } else { i };
        return Some((f(lo), f(hi)));
    }
    let n = u32::try_from(-delta).unwrap_or(u32::MAX);
    let cut_end = at.saturating_add(n);
    if lo >= at && hi < cut_end {
        return None;
    }
    let new_lo = if lo < at {
        lo
    } else if lo < cut_end {
        at
    } else {
        lo - n
    };
    let new_hi = if hi < at {
        hi
    } else if hi < cut_end {
        at.checked_sub(1)?
    } else {
        hi - n
    };
    Some((new_lo, new_hi))
}

/// One cell's address, zero-based. Orders by row, then column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    pub row: u32,
    pub col: u32,
}

impl CellRef {
    #[must_use]
    pub const fn new(row: u32, col: u32) -> Self {
        Self { row, col }
    }

    /// `A1`, `b7`, `$C$3`: the address, and whether the column and the row are anchored with `$`.
    #[must_use]
    pub fn parse_anchored(s: &str) -> Option<(Self, bool, bool)> {
        let (col_abs, rest) = s.strip_prefix('$').map_or((false, s), |r| (true, r));
        let split = rest.find(|c: char| !c.is_ascii_alphabetic())?;
        let (letters, rest) = rest.split_at(split);
        let (row_abs, digits) = rest.strip_prefix('$').map_or((false, rest), |r| (true, r));
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let row: u32 = digits.parse().ok()?;
        if row == 0 || row > MAX_ROWS {
            return None;
        }
        Some((Self::new(row - 1, col_index(letters)?), col_abs, row_abs))
    }

    /// `A1`, `b7`, `$C$3` (the anchors are accepted and dropped).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::parse_anchored(s).map(|(c, _, _)| c)
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", col_name(self.col), self.row + 1)
    }
}

/// A rectangle of cells, both corners included, always normalised so `start` is the top left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Range {
    pub start: CellRef,
    pub end: CellRef,
}

impl Range {
    /// The rectangle between two corners given in any order.
    #[must_use]
    pub fn new(a: CellRef, b: CellRef) -> Self {
        Self {
            start: CellRef::new(a.row.min(b.row), a.col.min(b.col)),
            end: CellRef::new(a.row.max(b.row), a.col.max(b.col)),
        }
    }

    /// `A1:C3`, or a single cell `B2` (a range of one).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.split_once(':') {
            Some((a, b)) => Some(Self::new(CellRef::parse(a)?, CellRef::parse(b)?)),
            None => CellRef::parse(s).map(|c| Self::new(c, c)),
        }
    }

    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.end.row - self.start.row + 1
    }

    #[must_use]
    pub const fn cols(&self) -> u32 {
        self.end.col - self.start.col + 1
    }

    #[must_use]
    pub const fn contains(&self, c: CellRef) -> bool {
        c.row >= self.start.row
            && c.row <= self.end.row
            && c.col >= self.start.col
            && c.col <= self.end.col
    }

    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = CellRef> + '_ {
        (self.start.row..=self.end.row)
            .flat_map(move |r| (self.start.col..=self.end.col).map(move |c| CellRef::new(r, c)))
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}:{}", self.start, self.end)
        }
    }
}
