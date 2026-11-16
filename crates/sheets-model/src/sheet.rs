//! Sheets and workbooks: sparse storage of what the user typed.

use crate::charts::{Chart, Sparkline};
use crate::refs::{Axis, CellRef, Range, shift_point, shift_span};
use crate::rules::{CondRule, PageSetup, Validation};
use crate::value::Value;
use std::collections::{BTreeMap, BTreeSet};

/// What a cell holds before calculation: a constant, or the text of a formula (without the leading `=`).
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Value(Value),
    Formula(String),
}

/// Horizontal alignment of a cell's content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Numbers to the right, text to the left.
    #[default]
    General,
    Left,
    Center,
    Right,
}

/// Protection of a sheet: its locked cells cannot be changed.
///
/// A password, when there is one, is kept as the 16 bit hash spreadsheets have always used; it keeps honest people from
/// changing a sheet by mistake and is not a secret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Protection {
    pub password_hash: Option<u16>,
    /// A file protected the sheet with a newer, stronger password hash that this suite cannot check: only the program that
    /// made it can lift the protection.
    pub opaque: bool,
}

impl Protection {
    /// The legacy hash of a password.
    #[must_use]
    pub fn hash(password: &str) -> u16 {
        let mut hash: u16 = 0;
        for byte in password.bytes().rev() {
            hash = ((hash >> 14) & 1) | ((hash << 1) & 0x7FFF);
            hash ^= u16::from(byte);
        }
        hash = ((hash >> 14) & 1) | ((hash << 1) & 0x7FFF);
        hash ^= u16::try_from(password.len()).unwrap_or(u16::MAX);
        hash ^ 0xCE4B
    }

    /// Whether `password` unlocks this protection.
    #[must_use]
    pub fn accepts(&self, password: &str) -> bool {
        !self.opaque && self.password_hash.is_none_or(|h| h == Self::hash(password))
    }
}

/// A filter over the rows of a range: its first row holds the headings and the rest are shown or hidden by what each
/// filtered column allows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoFilter {
    pub range: Range,
    /// For each filtered column (zero-based, absolute), the texts of the cells whose rows stay visible.
    pub allowed: BTreeMap<u32, BTreeSet<String>>,
}

/// A filter moved by an insertion or deletion of rows or columns; `None` when its span is gone.
fn shift_filter(f: AutoFilter, axis: Axis, at: u32, delta: i64) -> Option<AutoFilter> {
    let (lo, hi) = if axis == Axis::Row {
        (f.range.start.row, f.range.end.row)
    } else {
        (f.range.start.col, f.range.end.col)
    };
    let (a, b) = shift_span(lo, hi, at, delta)?;
    let (start, end) = if axis == Axis::Row {
        (
            CellRef::new(a, f.range.start.col),
            CellRef::new(b, f.range.end.col),
        )
    } else {
        (
            CellRef::new(f.range.start.row, a),
            CellRef::new(f.range.end.row, b),
        )
    };
    let allowed = f
        .allowed
        .into_iter()
        .filter_map(|(col, set)| {
            let col = if axis == Axis::Col {
                shift_point(col, at, delta)?
            } else {
                col
            };
            Some((col, set))
        })
        .collect();
    Some(AutoFilter {
        range: Range::new(start, end),
        allowed,
    })
}

/// The name of a look made of three colours (heading band, heading text, tint of every other row): `Custom_` and three hex
/// colours. The colours are in the name, so a table needs nothing else to carry them.
#[must_use]
pub fn custom_table_style(head: u32, head_text: u32, band: u32) -> String {
    format!("Custom_{head:06X}_{head_text:06X}_{band:06X}")
}

/// The colours of a look made by [`custom_table_style`].
#[must_use]
pub fn parse_custom_table_style(name: &str) -> Option<(u32, u32, u32)> {
    let mut parts = name.strip_prefix("Custom_")?.split('_');
    let mut next = || {
        let p = parts.next().filter(|p| p.len() == 6)?;
        u32::from_str_radix(p, 16).ok()
    };
    let (a, b, c) = (next()?, next()?, next()?);
    parts.next().is_none().then_some((a, b, c))
}

/// A table: a named block of cells whose first row holds the headings. It carries its own filter (over the whole block,
/// headings first) and may show its rows in bands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub filter: AutoFilter,
    pub banded: bool,
    /// The name of the look, as a `.xlsx` file names it (`TableStyleMedium2`); empty is the plain default, `None` is no look.
    pub style: String,
}

impl Table {
    /// The cells of the table, headings included.
    #[must_use]
    pub const fn range(&self) -> Range {
        self.filter.range
    }
}

/// How a border line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    #[default]
    Thin,
    Medium,
    Thick,
    Dashed,
    Dotted,
    Double,
}

/// One side of a cell's border.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    /// `0xRRGGBB`; `None` for the default (black).
    pub color: Option<u32>,
}

/// The four sides of a cell; `None` for no line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Border {
    pub top: Option<Line>,
    pub bottom: Option<Line>,
    pub left: Option<Line>,
    pub right: Option<Line>,
}

impl Border {
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.top.is_none() && self.bottom.is_none() && self.left.is_none() && self.right.is_none()
    }
}

/// How a cell looks. Index 0 of [`Workbook::styles`] is the default every unstyled cell has.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // bold, italic, underline and wrap are four independent switches
pub struct Style {
    /// A number format as written in the format language (`General`, `#,##0.00`, `yyyy-mm-dd`).
    pub number_format: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Text colour as `0xRRGGBB`; `None` for the default.
    pub color: Option<u32>,
    /// Background colour as `0xRRGGBB`; `None` for none.
    pub fill: Option<u32>,
    pub align: Align,
    /// Long text wraps inside the cell.
    pub wrap: bool,
    /// Lines around the cell.
    pub border: Border,
    /// When the sheet is protected, a locked cell cannot be changed.
    pub locked: bool,
    /// Text size in half points (22 = 11 pt); `None` for the workbook's default.
    pub font_size: Option<u16>,
    /// Font family; `None` for the workbook's default.
    pub font_name: Option<String>,
    /// The cell format of a loaded file this one was made from (an index into the workbook's styles, 0 for none). What
    /// this struct does not describe (borders, theme colours, protection) is kept from that format when the file is
    /// written, so changing a cell's colour does not lose its border.
    pub base: u32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            number_format: "General".into(),
            bold: false,
            italic: false,
            underline: false,
            color: None,
            fill: None,
            align: Align::General,
            wrap: false,
            border: Border::default(),
            locked: true,
            font_size: None,
            font_name: None,
            base: 0,
        }
    }
}

/// One sheet: a name and the cells that are not empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheet {
    pub name: String,
    cells: BTreeMap<CellRef, Content>,
    /// Style index (into [`Workbook::styles`]) of the cells that are not the default style.
    styles: BTreeMap<CellRef, u32>,
    /// Widths in CSS pixels of the columns that are not the default width, by zero-based column.
    col_widths: BTreeMap<u32, f64>,
    /// Heights in CSS pixels of the rows that are not the default height, by zero-based row.
    row_heights: BTreeMap<u32, f64>,
    /// What a file said a formula's value was, for formulas this suite cannot calculate (an unknown function). It is used
    /// only while the formula cannot be calculated, and forgotten when the cell is changed.
    cached: BTreeMap<CellRef, Value>,
    /// Rows and columns above and left of which the sheet does not scroll.
    freeze: (u32, u32),
    /// Ranges shown as one cell: the value of the top left cell fills them.
    merges: Vec<Range>,
    hidden_rows: BTreeSet<u32>,
    hidden_cols: BTreeSet<u32>,
    autofilter: Option<AutoFilter>,
    tables: Vec<Table>,
    /// Outline (group) level of rows and columns that have one: 1 to 7.
    row_levels: BTreeMap<u32, u8>,
    col_levels: BTreeMap<u32, u8>,
    /// Style index of whole rows and columns: what their empty cells look like, and what a cell typed there starts with.
    row_styles: BTreeMap<u32, u32>,
    col_styles: BTreeMap<u32, u32>,
    validations: Vec<Validation>,
    charts: Vec<Chart>,
    protection: Option<Protection>,
    sparklines: Vec<Sparkline>,
    page: PageSetup,
    cond_rules: Vec<CondRule>,
    /// Conditional formatting elements this suite cannot read (icon sets, ...), as the file had them, written back as they are.
    cond_raw: Vec<String>,
    /// Notes on cells.
    comments: BTreeMap<CellRef, String>,
}

impl Sheet {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: BTreeMap::new(),
            styles: BTreeMap::new(),
            col_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            cached: BTreeMap::new(),
            freeze: (0, 0),
            merges: Vec::new(),
            hidden_rows: BTreeSet::new(),
            hidden_cols: BTreeSet::new(),
            autofilter: None,
            tables: Vec::new(),
            row_levels: BTreeMap::new(),
            col_levels: BTreeMap::new(),
            row_styles: BTreeMap::new(),
            col_styles: BTreeMap::new(),
            validations: Vec::new(),
            charts: Vec::new(),
            protection: None,
            sparklines: Vec::new(),
            page: PageSetup::default(),
            cond_rules: Vec::new(),
            cond_raw: Vec::new(),
            comments: BTreeMap::new(),
        }
    }

    /// Store a cell; an empty value removes it, so the sheet only holds what is there.
    pub fn set(&mut self, at: CellRef, content: Content) {
        self.cached.remove(&at);
        if matches!(content, Content::Value(Value::Empty)) {
            self.cells.remove(&at);
        } else {
            self.cells.insert(at, content);
        }
    }

    /// Give a cell a style (an index into the workbook's styles); 0 is the default and is not stored.
    pub fn set_style(&mut self, at: CellRef, style: u32) {
        if style == 0 {
            self.styles.remove(&at);
        } else {
            self.styles.insert(at, style);
        }
    }

    /// The style index of a cell (0 when it has none).
    #[must_use]
    pub fn style(&self, at: CellRef) -> u32 {
        self.styles.get(&at).copied().unwrap_or(0)
    }

    /// The cells that have a style other than the default, by row then column; empty cells included.
    pub fn styled(&self) -> impl Iterator<Item = (CellRef, u32)> + '_ {
        self.styles.iter().map(|(k, v)| (*k, *v))
    }

    /// The width of a column in pixels when it is not the default.
    #[must_use]
    pub fn col_width(&self, col: u32) -> Option<f64> {
        self.col_widths.get(&col).copied()
    }

    /// Set a column's width in pixels; `None` goes back to the default.
    pub fn set_col_width(&mut self, col: u32, width: Option<f64>) {
        match width {
            Some(w) if w.is_finite() && w > 0.0 => {
                self.col_widths.insert(col, w);
            }
            _ => {
                self.col_widths.remove(&col);
            }
        }
    }

    /// The columns with their own width, in order.
    pub fn col_widths(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.col_widths.iter().map(|(c, w)| (*c, *w))
    }

    /// The height of a row in pixels when it is not the default.
    #[must_use]
    pub fn row_height(&self, row: u32) -> Option<f64> {
        self.row_heights.get(&row).copied()
    }

    /// Set a row's height in pixels; `None` goes back to the default.
    pub fn set_row_height(&mut self, row: u32, height: Option<f64>) {
        match height {
            Some(h) if h.is_finite() && h > 0.0 => {
                self.row_heights.insert(row, h);
            }
            _ => {
                self.row_heights.remove(&row);
            }
        }
    }

    /// The rows with their own height, in order.
    pub fn row_heights(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.row_heights.iter().map(|(r, h)| (*r, *h))
    }

    /// The rows and columns kept in view when the sheet scrolls, as (rows, columns).
    #[must_use]
    pub const fn freeze(&self) -> (u32, u32) {
        self.freeze
    }

    pub fn set_freeze(&mut self, rows: u32, cols: u32) {
        self.freeze = (rows.min(crate::MAX_ROWS - 1), cols.min(crate::MAX_COLS - 1));
    }

    /// The merged ranges.
    #[must_use]
    pub fn merges(&self) -> &[Range] {
        &self.merges
    }

    /// Merge a range (anything it overlaps is unmerged first). A range of one cell merges nothing.
    pub fn merge(&mut self, range: Range) {
        self.merges.retain(|m| !overlaps(m, &range));
        if range.start != range.end {
            self.merges.push(range);
            self.merges.sort_by_key(|m| m.start);
        }
    }

    /// Undo the merge that holds `at`. Returns whether there was one.
    pub fn unmerge(&mut self, at: CellRef) -> bool {
        let before = self.merges.len();
        self.merges.retain(|m| !m.contains(at));
        self.merges.len() != before
    }

    /// The merged range that holds `at`.
    #[must_use]
    pub fn merge_at(&self, at: CellRef) -> Option<Range> {
        self.merges.iter().copied().find(|m| m.contains(at))
    }

    /// The style of a whole row (0 for none).
    #[must_use]
    pub fn row_style(&self, row: u32) -> u32 {
        self.row_styles.get(&row).copied().unwrap_or(0)
    }

    pub fn set_row_style(&mut self, row: u32, style: u32) {
        if style == 0 {
            self.row_styles.remove(&row);
        } else {
            self.row_styles.insert(row, style);
        }
    }

    #[must_use]
    pub fn col_style(&self, col: u32) -> u32 {
        self.col_styles.get(&col).copied().unwrap_or(0)
    }

    pub fn set_col_style(&mut self, col: u32, style: u32) {
        if style == 0 {
            self.col_styles.remove(&col);
        } else {
            self.col_styles.insert(col, style);
        }
    }

    pub fn row_styles(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.row_styles.iter().map(|(r, s)| (*r, *s))
    }

    pub fn col_styles(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.col_styles.iter().map(|(c, s)| (*c, *s))
    }

    /// The style a cell shows: its own, or else its row's, or else its column's.
    #[must_use]
    pub fn effective_style(&self, at: CellRef) -> u32 {
        match self.style(at) {
            0 => match self.row_style(at.row) {
                0 => self.col_style(at.col),
                s => s,
            },
            s => s,
        }
    }

    /// The outline level of a row (0 when it is in no group).
    #[must_use]
    pub fn row_level(&self, row: u32) -> u8 {
        self.row_levels.get(&row).copied().unwrap_or(0)
    }

    pub fn set_row_level(&mut self, row: u32, level: u8) {
        if level == 0 {
            self.row_levels.remove(&row);
        } else {
            self.row_levels.insert(row, level.min(7));
        }
    }

    #[must_use]
    pub fn col_level(&self, col: u32) -> u8 {
        self.col_levels.get(&col).copied().unwrap_or(0)
    }

    pub fn set_col_level(&mut self, col: u32, level: u8) {
        if level == 0 {
            self.col_levels.remove(&col);
        } else {
            self.col_levels.insert(col, level.min(7));
        }
    }

    pub fn row_levels(&self) -> impl Iterator<Item = (u32, u8)> + '_ {
        self.row_levels.iter().map(|(r, l)| (*r, *l))
    }

    pub fn col_levels(&self) -> impl Iterator<Item = (u32, u8)> + '_ {
        self.col_levels.iter().map(|(c, l)| (*c, *l))
    }

    /// How the sheet is protected, if it is.
    #[must_use]
    pub const fn protection(&self) -> Option<Protection> {
        self.protection
    }

    pub const fn set_protection(&mut self, p: Option<Protection>) {
        self.protection = p;
    }

    #[must_use]
    pub fn sparklines(&self) -> &[Sparkline] {
        &self.sparklines
    }

    pub fn set_sparklines(&mut self, list: Vec<Sparkline>) {
        self.sparklines = list;
    }

    /// The sparkline in a cell.
    #[must_use]
    pub fn sparkline_at(&self, at: CellRef) -> Option<&Sparkline> {
        self.sparklines.iter().find(|s| s.cell == at)
    }

    #[must_use]
    pub fn charts(&self) -> &[Chart] {
        &self.charts
    }

    pub fn set_charts(&mut self, charts: Vec<Chart>) {
        self.charts = charts;
    }

    #[must_use]
    pub const fn page(&self) -> &PageSetup {
        &self.page
    }

    pub const fn set_page(&mut self, page: PageSetup) {
        self.page = page;
    }

    #[must_use]
    pub fn validations(&self) -> &[Validation] {
        &self.validations
    }

    pub fn set_validations(&mut self, v: Vec<Validation>) {
        self.validations = v;
    }

    /// The first validation rule that covers a cell.
    #[must_use]
    pub fn validation_at(&self, at: CellRef) -> Option<&Validation> {
        self.validations.iter().find(|v| v.range.contains(at))
    }

    #[must_use]
    pub fn cond_rules(&self) -> &[CondRule] {
        &self.cond_rules
    }

    pub fn set_cond_rules(&mut self, rules: Vec<CondRule>) {
        self.cond_rules = rules;
    }

    #[must_use]
    pub fn cond_raw(&self) -> &[String] {
        &self.cond_raw
    }

    pub fn set_cond_raw(&mut self, raw: Vec<String>) {
        self.cond_raw = raw;
    }

    #[must_use]
    pub fn comment(&self, at: CellRef) -> Option<&str> {
        self.comments.get(&at).map(String::as_str)
    }

    pub fn comments(&self) -> impl Iterator<Item = (CellRef, &str)> {
        self.comments.iter().map(|(k, v)| (*k, v.as_str()))
    }

    /// Set (or with empty text remove) the note on a cell.
    pub fn set_comment(&mut self, at: CellRef, text: &str) {
        if text.is_empty() {
            self.comments.remove(&at);
        } else {
            self.comments.insert(at, text.to_string());
        }
    }

    #[must_use]
    pub const fn autofilter(&self) -> Option<&AutoFilter> {
        self.autofilter.as_ref()
    }

    #[must_use]
    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    pub fn set_tables(&mut self, tables: Vec<Table>) {
        self.tables = tables;
    }

    /// The first table that holds a cell.
    #[must_use]
    pub fn table_at(&self, at: CellRef) -> Option<usize> {
        self.tables.iter().position(|t| {
            let r = t.range();
            (r.start.row..=r.end.row).contains(&at.row)
                && (r.start.col..=r.end.col).contains(&at.col)
        })
    }

    pub fn set_autofilter(&mut self, filter: Option<AutoFilter>) {
        self.autofilter = filter;
    }

    pub fn set_row_hidden(&mut self, row: u32, hidden: bool) {
        if hidden {
            self.hidden_rows.insert(row);
        } else {
            self.hidden_rows.remove(&row);
        }
    }

    #[must_use]
    pub fn row_hidden(&self, row: u32) -> bool {
        self.hidden_rows.contains(&row)
    }

    pub fn hidden_rows(&self) -> impl Iterator<Item = u32> + '_ {
        self.hidden_rows.iter().copied()
    }

    pub fn set_col_hidden(&mut self, col: u32, hidden: bool) {
        if hidden {
            self.hidden_cols.insert(col);
        } else {
            self.hidden_cols.remove(&col);
        }
    }

    #[must_use]
    pub fn col_hidden(&self, col: u32) -> bool {
        self.hidden_cols.contains(&col)
    }

    pub fn hidden_cols(&self) -> impl Iterator<Item = u32> + '_ {
        self.hidden_cols.iter().copied()
    }

    #[allow(clippy::too_many_lines)]
    /// Make room for (`delta` > 0) or remove (`delta` < 0) rows or columns at `at`: everything below or to the right moves,
    /// and what was in removed rows or columns is gone. Formulas are not touched here (see `Engine`).
    pub fn shift(&mut self, axis: Axis, at: u32, delta: i64) {
        fn moved<V>(map: &mut BTreeMap<CellRef, V>, axis: Axis, at: u32, delta: i64) {
            let old = std::mem::take(map);
            for (k, v) in old {
                let i = if axis == Axis::Row { k.row } else { k.col };
                if let Some(j) = shift_point(i, at, delta) {
                    let key = if axis == Axis::Row {
                        CellRef::new(j, k.col)
                    } else {
                        CellRef::new(k.row, j)
                    };
                    map.insert(key, v);
                }
            }
        }
        moved(&mut self.cells, axis, at, delta);
        moved(&mut self.styles, axis, at, delta);
        moved(&mut self.cached, axis, at, delta);
        let old = std::mem::take(&mut self.merges);
        for m in old {
            let (lo, hi) = if axis == Axis::Row {
                (m.start.row, m.end.row)
            } else {
                (m.start.col, m.end.col)
            };
            if let Some((a, b)) = shift_span(lo, hi, at, delta) {
                let (s, e) = if axis == Axis::Row {
                    (CellRef::new(a, m.start.col), CellRef::new(b, m.end.col))
                } else {
                    (CellRef::new(m.start.row, a), CellRef::new(m.end.row, b))
                };
                if s != e {
                    self.merges.push(Range::new(s, e));
                }
            }
        }
        let span_of = |r: Range| {
            let (lo, hi) = if axis == Axis::Row {
                (r.start.row, r.end.row)
            } else {
                (r.start.col, r.end.col)
            };
            shift_span(lo, hi, at, delta).map(|(a, b)| {
                if axis == Axis::Row {
                    Range::new(CellRef::new(a, r.start.col), CellRef::new(b, r.end.col))
                } else {
                    Range::new(CellRef::new(r.start.row, a), CellRef::new(r.end.row, b))
                }
            })
        };
        self.validations = std::mem::take(&mut self.validations)
            .into_iter()
            .filter_map(|mut v| {
                v.range = span_of(v.range)?;
                Some(v)
            })
            .collect();
        self.cond_rules = std::mem::take(&mut self.cond_rules)
            .into_iter()
            .filter_map(|mut v| {
                v.range = span_of(v.range)?;
                Some(v)
            })
            .collect();
        self.page.print_area = self.page.print_area.and_then(span_of);
        self.sparklines = std::mem::take(&mut self.sparklines)
            .into_iter()
            .filter_map(|mut sp| {
                sp.data = span_of(sp.data)?;
                let i = if axis == Axis::Row {
                    sp.cell.row
                } else {
                    sp.cell.col
                };
                let j = shift_point(i, at, delta)?;
                sp.cell = if axis == Axis::Row {
                    CellRef::new(j, sp.cell.col)
                } else {
                    CellRef::new(sp.cell.row, j)
                };
                Some(sp)
            })
            .collect();
        // A chart follows the cells it draws; one whose cells are gone is gone, and one that sits at a moved cell moves.
        self.charts = std::mem::take(&mut self.charts)
            .into_iter()
            .filter_map(|mut c| {
                c.data = span_of(c.data)?;
                let i = if axis == Axis::Row {
                    c.at.row
                } else {
                    c.at.col
                };
                if let Some(j) = shift_point(i, at, delta) {
                    c.at = if axis == Axis::Row {
                        CellRef::new(j, c.at.col)
                    } else {
                        CellRef::new(c.at.row, j)
                    };
                }
                Some(c)
            })
            .collect();
        if axis == Axis::Row {
            self.page.repeat_rows = self
                .page
                .repeat_rows
                .and_then(|(a, b)| shift_span(a, b, at, delta));
        }
        moved(&mut self.comments, axis, at, delta);
        self.autofilter = self
            .autofilter
            .take()
            .and_then(|f| shift_filter(f, axis, at, delta));
        self.tables = std::mem::take(&mut self.tables)
            .into_iter()
            .filter_map(|t| {
                Some(Table {
                    name: t.name,
                    banded: t.banded,
                    style: t.style,
                    filter: shift_filter(t.filter, axis, at, delta)?,
                })
            })
            .collect();
        let styles = if axis == Axis::Row {
            &mut self.row_styles
        } else {
            &mut self.col_styles
        };
        *styles = std::mem::take(styles)
            .into_iter()
            .filter_map(|(i, s)| Some((shift_point(i, at, delta)?, s)))
            .collect();
        let levels = if axis == Axis::Row {
            &mut self.row_levels
        } else {
            &mut self.col_levels
        };
        *levels = std::mem::take(levels)
            .into_iter()
            .filter_map(|(i, l)| Some((shift_point(i, at, delta)?, l)))
            .collect();
        let hidden = if axis == Axis::Row {
            &mut self.hidden_rows
        } else {
            &mut self.hidden_cols
        };
        *hidden = std::mem::take(hidden)
            .into_iter()
            .filter_map(|i| shift_point(i, at, delta))
            .collect();
        let sizes = if axis == Axis::Row {
            &mut self.row_heights
        } else {
            &mut self.col_widths
        };
        let old = std::mem::take(sizes);
        for (i, v) in old {
            if let Some(j) = shift_point(i, at, delta) {
                sizes.insert(j, v);
            }
        }
    }

    /// Remember the value a file gave a formula cell (see the field's note). Call it after [`Sheet::set`], which forgets it.
    pub fn set_cached(&mut self, at: CellRef, value: Value) {
        self.cached.insert(at, value);
    }

    #[must_use]
    pub fn cached(&self, at: CellRef) -> Option<&Value> {
        self.cached.get(&at)
    }

    pub fn clear(&mut self, at: CellRef) {
        self.cached.remove(&at);
        self.cells.remove(&at);
    }

    #[must_use]
    pub fn get(&self, at: CellRef) -> Option<&Content> {
        self.cells.get(&at)
    }

    /// The cells that are not empty, by row then column.
    pub fn iter(&self) -> impl Iterator<Item = (CellRef, &Content)> {
        self.cells.iter().map(|(k, v)| (*k, v))
    }

    /// The cells inside a rectangle, by row then column.
    pub fn cells_in(&self, r: Range) -> impl Iterator<Item = (CellRef, &Content)> {
        self.cells
            .range(CellRef::new(r.start.row, 0)..=CellRef::new(r.end.row, u32::MAX))
            .filter(move |(k, _)| k.col >= r.start.col && k.col <= r.end.col)
            .map(|(k, v)| (*k, v))
    }

    /// The cells after `at` in row-major order (`at` itself excluded).
    pub fn iter_after(&self, at: CellRef) -> impl Iterator<Item = (CellRef, &Content)> {
        self.cells
            .range((std::ops::Bound::Excluded(at), std::ops::Bound::Unbounded))
            .map(|(k, v)| (*k, v))
    }

    /// The cells before `at`, nearest first (`at` itself excluded).
    pub fn iter_before(&self, at: CellRef) -> impl Iterator<Item = (CellRef, &Content)> {
        self.cells.range(..at).rev().map(|(k, v)| (*k, v))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The smallest rectangle holding every cell, `None` for an empty sheet.
    #[must_use]
    pub fn used_range(&self) -> Option<Range> {
        let mut it = self.cells.keys();
        let first = *it.next()?;
        let (mut min, mut max) = (first, first);
        for c in it {
            min = CellRef::new(min.row.min(c.row), min.col.min(c.col));
            max = CellRef::new(max.row.max(c.row), max.col.max(c.col));
        }
        Some(Range::new(min, max))
    }
}

/// Sheets in tab order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    /// Cell styles; index 0 is the default style.
    pub styles: Vec<Style>,
    /// Defined names and what they refer to, as the formula text (`Sheet1!$A$1:$A$9`, `0.2`), without the leading `=`.
    pub names: Vec<(String, String)>,
}

impl Workbook {
    /// What a defined name refers to (names are compared without regard to case).
    #[must_use]
    pub fn name(&self, name: &str) -> Option<&str> {
        self.names
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, r)| r.as_str())
    }

    /// Define (or redefine) a name; empty text removes it.
    pub fn set_name(&mut self, name: &str, refers_to: &str) {
        self.names.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        if !refers_to.is_empty() {
            self.names.push((name.to_string(), refers_to.to_string()));
            self.names.sort_by_key(|(n, _)| n.to_lowercase());
        }
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            sheets: vec![Sheet::new("Sheet1")],
            styles: vec![Style::default()],
            names: Vec::new(),
        }
    }

    /// The index of a style, adding it when the workbook does not have it yet.
    pub fn style_id(&mut self, style: &Style) -> u32 {
        let at = self
            .styles
            .iter()
            .position(|s| s == style)
            .unwrap_or_else(|| {
                self.styles.push(style.clone());
                self.styles.len() - 1
            });
        u32::try_from(at).unwrap_or(0)
    }

    /// The number format of a cell.
    #[must_use]
    pub fn number_format(&self, sheet: usize, at: CellRef) -> &str {
        let id = self.sheets.get(sheet).map_or(0, |s| s.style(at));
        self.styles
            .get(id as usize)
            .map_or("General", |s| s.number_format.as_str())
    }

    /// A sheet by name; names compare without regard to case, as in Excel.
    #[must_use]
    pub fn sheet(&self, name: &str) -> Option<&Sheet> {
        self.sheets
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn sheet_mut(&mut self, name: &str) -> Option<&mut Sheet> {
        self.sheets
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

const fn overlaps(a: &Range, b: &Range) -> bool {
    a.start.row <= b.end.row
        && b.start.row <= a.end.row
        && a.start.col <= b.end.col
        && b.start.col <= a.end.col
}
