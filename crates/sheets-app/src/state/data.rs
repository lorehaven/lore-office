//! Working with the data in a sheet: sorting, filtering.

use super::{CellState, Content, SheetState, Value, moved, state_of};
use quench_ui::prelude::*;
use sheets_model::{AutoFilter, CellRef, Range};
use std::cmp::Ordering;
use std::collections::BTreeSet;

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Position of a name in a cycle (full or three letter form), and which form it was.
fn cycle_pos(text: &str, names: &[&str]) -> Option<(usize, bool)> {
    let lower = text.to_lowercase();
    names
        .iter()
        .position(|n| n.to_lowercase() == lower)
        .map(|i| (i, true))
        .or_else(|| {
            names
                .iter()
                .position(|n| n.len() >= 3 && n[..3].to_lowercase() == lower)
                .map(|i| (i, false))
        })
}

/// The text of cycle item `i` in the same form and capitalisation as `like`.
fn cycle_text(names: &[&str], i: usize, full: bool, like: &str) -> String {
    let name = names[i.rem_euclid(names.len())];
    let word = if full {
        name.to_string()
    } else {
        name[..3].to_string()
    };
    if like.chars().all(|c| !c.is_lowercase())
        && like.chars().any(char::is_uppercase)
        && like.len() > 1
    {
        word.to_uppercase()
    } else if like.chars().next().is_some_and(char::is_lowercase) {
        word.to_lowercase()
    } else {
        word
    }
}

/// Text ending in a number (`Item 7`, `Q3`): the text before, the number, and the digits' width.
fn split_number(text: &str) -> Option<(&str, i64, usize)> {
    let digits = text.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 15 {
        return None;
    }
    let at = text.len() - digits;
    Some((
        &text[..at],
        text[at..].parse().ok()?,
        if text[at..].starts_with('0') {
            digits
        } else {
            0
        },
    ))
}

/// Most distinct values a filter list offers.
const MAX_FILTER_VALUES: usize = 1000;

fn rank(v: &Value) -> u8 {
    match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        Value::Empty => 4,
    }
}

/// Spreadsheet order: numbers, then text (without regard to case), then logical values, then errors; empty cells last.
fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

impl SheetState {
    /// The block of cells around `at` that touch each other (the area Ctrl+A would select in a spreadsheet).
    #[must_use]
    pub fn current_region(&self, at: CellRef) -> Range {
        let has = |r: i64, c: i64| {
            r >= 0
                && c >= 0
                && self
                    .engine()
                    .content(
                        self.sheet(),
                        CellRef::new(u32::try_from(r).unwrap_or(0), u32::try_from(c).unwrap_or(0)),
                    )
                    .is_some_and(|c| !matches!(c, Content::Value(Value::Empty)))
        };
        let (mut top, mut bottom, mut left, mut right) = (
            i64::from(at.row),
            i64::from(at.row),
            i64::from(at.col),
            i64::from(at.col),
        );
        loop {
            let before = (top, bottom, left, right);
            if (left..=right).any(|c| has(top - 1, c) || has(top - 1, c - 1) || has(top - 1, c + 1))
            {
                top -= 1;
            }
            if (left..=right)
                .any(|c| has(bottom + 1, c) || has(bottom + 1, c - 1) || has(bottom + 1, c + 1))
            {
                bottom += 1;
            }
            if (top..=bottom)
                .any(|r| has(r, left - 1) || has(r - 1, left - 1) || has(r + 1, left - 1))
            {
                left -= 1;
            }
            if (top..=bottom)
                .any(|r| has(r, right + 1) || has(r - 1, right + 1) || has(r + 1, right + 1))
            {
                right += 1;
            }
            if (top, bottom, left, right) == before || bottom - top > 200_000 {
                break;
            }
        }
        let to = |v: i64| u32::try_from(v.max(0)).unwrap_or(0);
        Range::new(
            CellRef::new(to(top), to(left)),
            CellRef::new(to(bottom), to(right)),
        )
    }

    /// The first row is a heading when every cell in it is text and some cell below is not.
    fn has_heading(&self, range: Range) -> bool {
        if range.rows() < 2 {
            return false;
        }
        let e = self.engine();
        let sheet = self.sheet();
        let first: Vec<Value> = (range.start.col..=range.end.col)
            .map(|c| e.value(sheet, CellRef::new(range.start.row, c)))
            .collect();
        let first_all_text = first.iter().any(|v| matches!(v, Value::Text(_)))
            && first
                .iter()
                .all(|v| matches!(v, Value::Text(_) | Value::Empty));
        first_all_text
            && (range.start.row + 1..=range.end.row.min(range.start.row + 50)).any(|r| {
                (range.start.col..=range.end.col).any(|c| {
                    matches!(
                        e.value(sheet, CellRef::new(r, c)),
                        Value::Number(_) | Value::Bool(_)
                    )
                })
            })
    }

    /// Sort the rows of the selected cells (or of the block around the cursor, when it is one cell) by the cursor's column.
    /// A heading row stays on top. One undo step.
    pub fn sort_selection(&self, ascending: bool) {
        let at = self.selected.get_untracked();
        let mut region = self.selection();
        if region.start == region.end {
            region = self.current_region(at);
        }
        let key_col = if (region.start.col..=region.end.col).contains(&at.col) {
            at.col
        } else {
            region.start.col
        };
        let first = region.start.row + u32::from(self.has_heading(region));
        if first >= region.end.row
            || u64::from(region.rows()) * u64::from(region.cols()) > 2_000_000
        {
            self.status
                .set(crate::i18n::tr("Select two or more rows of data to sort"));
            return;
        }
        self.sort_rows(region, first, key_col, ascending);
    }

    /// Sort the rows `first..=region.end.row` of a block by one column. One undo step.
    fn sort_rows(&self, region: Range, first: u32, key_col: u32, ascending: bool) {
        let sheet = self.sheet();
        let edits: Vec<(CellRef, CellState)> = {
            let doc = self.doc.borrow();
            let e = &doc.engine;
            let mut rows: Vec<(Value, u32)> = (first..=region.end.row)
                .map(|r| (e.value(sheet, CellRef::new(r, key_col)), r))
                .collect();
            rows.sort_by(|(a, ra), (b, rb)| {
                let empty = |v: &Value| matches!(v, Value::Empty);
                match (empty(a), empty(b)) {
                    (true, true) => ra.cmp(rb),
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    _ => {
                        let o = compare(a, b);
                        (if ascending { o } else { o.reverse() }).then(ra.cmp(rb))
                    }
                }
            });
            let mut out = Vec::new();
            for (target, (_, source)) in (first..=region.end.row).zip(rows) {
                if target == source {
                    continue;
                }
                let delta = i64::from(target) - i64::from(source);
                for c in region.start.col..=region.end.col {
                    let mut state = state_of(e, sheet, CellRef::new(source, c));
                    state.content = state.content.map(|content| moved(&content, delta, 0));
                    out.push((CellRef::new(target, c), state));
                }
            }
            out
        };
        if edits.is_empty() {
            self.status.set(crate::i18n::tr("Already in order"));
            return;
        }
        self.edit(sheet, edits);
        self.status.set(if ascending {
            "Sorted A to Z".to_string()
        } else {
            "Sorted Z to A".to_string()
        });
    }

    // ---- filters ----

    /// The filter of the sheet on screen.
    #[must_use]
    pub fn autofilter(&self) -> Option<AutoFilter> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(|s| s.autofilter().cloned())
    }

    /// Turn the filter on, over the block around the cursor (its first row is the heading), or off, showing every row again.
    pub fn toggle_filter(&self) {
        let sheet = self.sheet();
        if let Some(f) = self.autofilter() {
            self.filter_open.set(None);
            let done = self.whole_book_step(|e| {
                e.update_sheet(sheet, |s| {
                    for r in f.range.start.row + 1..=f.range.end.row {
                        s.set_row_hidden(r, false);
                    }
                    s.set_autofilter(None);
                })
                .is_some()
            });
            if done {
                self.status.set(crate::i18n::tr("Filter off"));
            }
            return;
        }
        let at = self.selected.get_untracked();
        let mut range = self.selection();
        if range.start == range.end {
            range = self.current_region(at);
        }
        if range.rows() < 2 {
            self.status.set(crate::i18n::tr(
                "Put the cursor in a table with a heading row to filter it",
            ));
            return;
        }
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                s.set_autofilter(Some(AutoFilter {
                    range,
                    allowed: Default::default(),
                }))
            })
            .is_some()
        }) {
            self.status
                .set(crate::i18n::tr("Filter on: use the arrows in the heading"));
        }
    }

    /// The filter a target names: `None` is the sheet's own, `Some(i)` the filter of table `i`.
    #[must_use]
    pub fn filter_for(&self, target: Option<usize>) -> Option<AutoFilter> {
        let doc = self.doc.borrow();
        let sheet = doc.engine.workbook().sheets.get(self.sheet())?;
        match target {
            None => sheet.autofilter().cloned(),
            Some(i) => sheet.tables().get(i).map(|t| t.filter.clone()),
        }
    }

    /// The filter whose list is open.
    #[must_use]
    pub fn open_target_filter(&self) -> Option<AutoFilter> {
        self.open_target()
    }

    fn open_target(&self) -> Option<AutoFilter> {
        self.filter_for(self.filter_on.get_untracked())
    }

    /// Put a filter back where it came from.
    fn store_filter(s: &mut sheets_model::Sheet, target: Option<usize>, filter: AutoFilter) {
        match target {
            None => s.set_autofilter(Some(filter)),
            Some(i) => {
                let mut tables = s.tables().to_vec();
                if let Some(t) = tables.get_mut(i) {
                    t.filter = filter;
                }
                s.set_tables(tables);
            }
        }
    }

    /// The text a cell shows, as a filter sees it.
    fn filter_text(&self, at: CellRef) -> String {
        self.display(at)
    }

    /// The distinct texts in a filtered column (below the heading), in order, and whether there were more than are offered.
    #[must_use]
    pub fn filter_values(&self, col: u32) -> (Vec<String>, bool) {
        let Some(f) = self.open_target() else {
            return (Vec::new(), false);
        };
        let mut seen: BTreeSet<(Vec<u8>, String)> = BTreeSet::new();
        let mut more = false;
        for r in f.range.start.row + 1..=f.range.end.row {
            let text = self.filter_text(CellRef::new(r, col));
            let key = text.to_lowercase().into_bytes();
            if seen.len() >= MAX_FILTER_VALUES && !seen.contains(&(key.clone(), text.clone())) {
                more = true;
                break;
            }
            seen.insert((key, text));
        }
        (seen.into_iter().map(|(_, t)| t).collect(), more)
    }

    /// Open the list for a column of the heading row.
    pub fn open_filter(&self, target: Option<usize>, col: u32) {
        self.filter_on.set(target);
        let (all, _) = self.filter_values(col);
        let checked = self
            .filter_for(target)
            .and_then(|f| f.allowed.get(&col).cloned())
            .unwrap_or_else(|| all.into_iter().collect());
        batch(|| {
            self.filter_pick.set(checked);
            self.filter_open.set(Some(col));
        });
    }

    pub fn close_filter(&self) {
        self.filter_open.set(None);
        self.refocus();
    }

    /// Tick or untick one value in the open list.
    pub fn toggle_filter_value(&self, value: &str) {
        self.filter_pick.update(|set| {
            if !set.remove(value) {
                set.insert(value.to_string());
            }
        });
    }

    /// Tick or untick them all.
    pub fn set_all_filter_values(&self, on: bool) {
        let Some(col) = self.filter_open.get_untracked() else {
            return;
        };
        let (all, _) = self.filter_values(col);
        self.filter_pick.set(if on {
            all.into_iter().collect()
        } else {
            BTreeSet::new()
        });
    }

    /// Keep the rows whose value in the open column is ticked (all ticked: no filter on that column).
    pub fn apply_filter(&self) {
        let Some(col) = self.filter_open.get_untracked() else {
            return;
        };
        let picked = self.filter_pick.get_untracked();
        let (all, _) = self.filter_values(col);
        let everything = all.iter().all(|v| picked.contains(v));
        self.set_filter_column(col, (!everything).then_some(picked));
        self.close_filter();
    }

    /// Set (or with `None` clear) what a column allows, and show and hide the rows to match.
    pub fn set_filter_column(&self, col: u32, allowed: Option<BTreeSet<String>>) {
        let target = self.filter_on.get_untracked();
        let Some(mut f) = self.open_target() else {
            return;
        };
        match allowed {
            Some(set) => {
                f.allowed.insert(col, set);
            }
            None => {
                f.allowed.remove(&col);
            }
        }
        let sheet = self.sheet();
        let hidden: Vec<(u32, bool)> = (f.range.start.row + 1..=f.range.end.row)
            .map(|r| {
                let visible = f
                    .allowed
                    .iter()
                    .all(|(c, set)| set.contains(&self.filter_text(CellRef::new(r, *c))));
                (r, !visible)
            })
            .collect();
        if self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                for (r, h) in &hidden {
                    s.set_row_hidden(*r, *h);
                }
                Self::store_filter(s, target, f);
            })
            .is_some()
        }) {
            // The rows speak for themselves: no note.
            self.status.set(String::new());
        }
    }

    /// Show every row again, keeping the filter on.
    pub fn clear_filters(&self) {
        let Some(f) = self.autofilter() else { return };
        let sheet = self.sheet();
        if self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                for r in f.range.start.row + 1..=f.range.end.row {
                    s.set_row_hidden(r, false);
                }
                s.set_autofilter(Some(AutoFilter {
                    range: f.range,
                    allowed: Default::default(),
                }));
            })
            .is_some()
        }) {
            self.status.set(crate::i18n::tr("Showing all rows"));
        }
    }

    /// Whether a filter has its arrow on a given heading cell.
    #[must_use]
    pub fn is_filter_heading(&self, at: CellRef) -> bool {
        self.filters().iter().any(|(_, f)| {
            at.row == f.range.start.row && (f.range.start.col..=f.range.end.col).contains(&at.col)
        })
    }

    /// Every filter of the sheet on screen: the sheet's own (target `None`) and those of its tables.
    #[must_use]
    pub fn filters(&self) -> Vec<(Option<usize>, AutoFilter)> {
        let doc = self.doc.borrow();
        let Some(sheet) = doc.engine.workbook().sheets.get(self.sheet()) else {
            return Vec::new();
        };
        sheet
            .autofilter()
            .map(|f| (None, f.clone()))
            .into_iter()
            .chain(
                sheet
                    .tables()
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (Some(i), t.filter.clone())),
            )
            .collect()
    }

    /// Sort the rows of the table (or filtered block) whose list is open by that column. Headings stay on top.
    pub fn sort_filter_column(&self, ascending: bool) {
        let (Some(col), Some(f)) = (self.filter_open.get_untracked(), self.open_target()) else {
            return;
        };
        self.close_filter();
        if f.range.rows() < 3 {
            self.status.set(crate::i18n::tr("Nothing to sort"));
            return;
        }
        self.sort_rows(f.range, f.range.start.row + 1, col, ascending);
    }

    // ---- tables ----

    /// The tables of the sheet on screen.
    #[must_use]
    pub fn engine_tables(&self) -> Vec<sheets_model::Table> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .map(|s| s.tables().to_vec())
            .unwrap_or_default()
    }

    /// The table the cursor is in.
    #[must_use]
    pub fn table_at_cursor(&self) -> Option<usize> {
        let at = self.selected.get_untracked();
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(|s| s.table_at(at))
    }

    /// Make a table of the selected cells (or of the block around the cursor, when it is one cell): its first row holds the
    /// headings. Empty or repeated headings get names. One undo step.
    pub fn insert_table(&self) {
        let sheet = self.sheet();
        let at = self.selected.get_untracked();
        let mut range = self.selection();
        if range.start == range.end {
            range = self.current_region(at);
        }
        if range.rows() < 2 {
            self.status.set(crate::i18n::tr(
                "Put the cursor in data with a heading row to make a table",
            ));
            return;
        }
        if range.cols() > 1000 || u64::from(range.rows()) * u64::from(range.cols()) > 2_000_000 {
            self.status
                .set(crate::i18n::tr("That is too much to make a table of"));
            return;
        }
        let overlaps = {
            let e = self.engine();
            e.workbook().sheets.get(sheet).is_some_and(|s| {
                s.tables().iter().any(|t| {
                    let r = t.range();
                    r.start.row <= range.end.row
                        && range.start.row <= r.end.row
                        && r.start.col <= range.end.col
                        && range.start.col <= r.end.col
                })
            })
        };
        if overlaps {
            self.status
                .set(crate::i18n::tr("Part of that is a table already"));
            return;
        }
        // Headings are text, and different from each other.
        let mut names: Vec<String> = Vec::new();
        let mut fixes: Vec<(CellRef, String)> = Vec::new();
        for c in range.start.col..=range.end.col {
            let cell = CellRef::new(range.start.row, c);
            let mut name = self.display(cell).trim().to_string();
            if name.is_empty() {
                name = format!("Column{}", c - range.start.col + 1);
            }
            let base = name.clone();
            let mut n = 2;
            while names.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
                name = format!("{base}{n}");
                n += 1;
            }
            if name != self.display(cell) {
                fixes.push((cell, name.clone()));
            }
            names.push(name);
        }
        let table_name = {
            let e = self.engine();
            (1..=u32::MAX)
                .map(|n| format!("Table{n}"))
                .find(|n| {
                    e.workbook()
                        .sheets
                        .iter()
                        .all(|s| s.tables().iter().all(|t| !t.name.eq_ignore_ascii_case(n)))
                })
                .unwrap_or_default()
        };
        let done = self.whole_book_step(|e| {
            for (cell, text) in &fixes {
                e.set(sheet, *cell, Content::Value(Value::Text(text.clone())));
            }
            e.update_sheet(sheet, |s| {
                let mut tables = s.tables().to_vec();
                tables.push(sheets_model::Table {
                    name: table_name.clone(),
                    filter: AutoFilter {
                        range,
                        allowed: Default::default(),
                    },
                    banded: true,
                    style: String::new(),
                });
                s.set_tables(tables);
            })
            .is_some()
        });
        if done {
            self.status
                .set(format!("Made {table_name}: use the arrows in the heading"));
        }
    }

    /// Turn the table the cursor is in back into plain cells (what they hold stays; rows the filter hid are shown again).
    pub fn remove_table(&self) {
        let Some(i) = self.table_at_cursor() else {
            self.status
                .set(crate::i18n::tr("The cursor is not in a table"));
            return;
        };
        let sheet = self.sheet();
        let Some((_, f)) = self.filters().into_iter().find(|(t, _)| *t == Some(i)) else {
            return;
        };
        self.filter_open.set(None);
        if self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                for r in f.range.start.row + 1..=f.range.end.row {
                    s.set_row_hidden(r, false);
                }
                let mut tables = s.tables().to_vec();
                tables.remove(i);
                s.set_tables(tables);
            })
            .is_some()
        }) {
            self.status
                .set(crate::i18n::tr("Converted to a plain range"));
        }
    }

    /// Show or hide the tinted rows of the table the cursor is in.
    pub fn toggle_table_bands(&self) {
        let Some(i) = self.table_at_cursor() else {
            return;
        };
        let sheet = self.sheet();
        let _ = self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                let mut tables = s.tables().to_vec();
                tables[i].banded = !tables[i].banded;
                s.set_tables(tables);
            })
            .is_some()
        });
    }

    /// Give the table the cursor is in another look (a name from `TABLE_STYLES`).
    pub fn set_table_style(&self, style: &str) {
        let Some(i) = self.table_at_cursor() else {
            return;
        };
        let sheet = self.sheet();
        let style = style.to_string();
        let _ = self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                let mut tables = s.tables().to_vec();
                tables[i].style = style;
                s.set_tables(tables);
            })
            .is_some()
        });
    }

    /// Give the table the cursor is in another name (letters, digits and underscores, starting with a letter; not used by
    /// another table).
    pub fn rename_table(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        let ok = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && sheets_model::CellRef::parse(name).is_none();
        if !ok {
            return Err(crate::i18n::tr(
                "A name uses letters, digits and underscores, starts with a letter and is not a cell",
            ));
        }
        let sheet = self.sheet();
        let Some(i) = self.table_at_cursor() else {
            return Err(crate::i18n::tr("The cursor is not in a table"));
        };
        let taken =
            self.engine()
                .workbook()
                .sheets
                .iter()
                .enumerate()
                .any(|(si, s)| {
                    s.tables().iter().enumerate().any(|(ti, t)| {
                        t.name.eq_ignore_ascii_case(name) && !(si == sheet && ti == i)
                    })
                });
        if taken {
            return Err(crate::i18n::tr("Another table has that name"));
        }
        let name = name.to_string();
        let _ = self.whole_book_step(move |e| {
            e.update_sheet(sheet, |s| {
                let mut tables = s.tables().to_vec();
                tables[i].name = name;
                s.set_tables(tables);
            })
            .is_some()
        });
        Ok(())
    }

    /// The name of the table the cursor is in.
    #[must_use]
    pub fn table_name(&self) -> Option<String> {
        let i = self.table_at_cursor()?;
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())?
            .tables()
            .get(i)
            .map(|t| t.name.clone())
    }

    /// A value was typed in the row under a table, in one of its columns: the table grows to hold the row.
    pub fn grow_tables(&self, at: CellRef) {
        let sheet = self.sheet();
        let grow = {
            let e = self.engine();
            e.workbook().sheets.get(sheet).and_then(|s| {
                s.tables().iter().position(|t| {
                    let r = t.range();
                    at.row == r.end.row + 1 && (r.start.col..=r.end.col).contains(&at.col)
                })
            })
        };
        let Some(i) = grow else { return };
        if self.engine().workbook().sheets[sheet].get(at).is_none() {
            return;
        }
        let mut doc = self.doc.borrow_mut();
        let _ = doc.engine.update_sheet(sheet, |s| {
            let mut tables = s.tables().to_vec();
            let r = tables[i].filter.range;
            tables[i].filter.range = Range::new(r.start, CellRef::new(r.end.row + 1, r.end.col));
            s.set_tables(tables);
        });
    }

    // ---- autofill ----

    /// What the cell `k` steps beyond a run of source cells holds (`k` counts from 1 in the direction of the fill; `down` is
    /// false when filling up or left, so the series runs backwards). `source` is the run in fill order.
    fn series(&self, source: &[CellRef], k: i64, forward: bool, vertical: bool) -> CellState {
        let e = self.engine();
        let sheet = self.sheet();
        let n = i64::try_from(source.len()).unwrap_or(1);
        let cycle = |i: i64| usize::try_from(i.rem_euclid(n)).unwrap_or(0);
        let states: Vec<CellState> = source.iter().map(|at| state_of(&e, sheet, *at)).collect();
        let values: Vec<Value> = source.iter().map(|at| e.value(sheet, *at)).collect();
        let sign = if forward { 1 } else { -1 };
        let mut out = states[cycle(if forward { k - 1 } else { n - k })].clone();
        // The cells a step moves from, in the order the series runs.
        let (dr, dc) = if vertical { (1, 0) } else { (0, 1) };
        // Formulas move as in a copy: by the distance from the cell they came from.
        if let Some(Content::Formula(_)) = &out.content {
            let distance = if forward {
                n * ((k - 1) / n + 1)
            } else {
                -(n * ((k - 1) / n + 1))
            };
            out.content = out.content.map(|c| moved(&c, distance * dr, distance * dc));
            return out;
        }
        let all_numbers = values.iter().all(|v| matches!(v, Value::Number(_)));
        let numbers: Vec<f64> = values
            .iter()
            .filter_map(|v| {
                if let Value::Number(x) = v {
                    Some(*x)
                } else {
                    None
                }
            })
            .collect();
        let date = e.workbook().sheets.get(sheet).is_some_and(|_| {
            let format = e.workbook().number_format(sheet, source[0]);
            sheets_format::Format::parse(format).is_ok_and(|f| f.is_date())
        });
        if all_numbers && (numbers.len() >= 2 || date) {
            let step = if numbers.len() >= 2 {
                (numbers[numbers.len() - 1] - numbers[0]) / (numbers.len() - 1) as f64
            } else {
                1.0
            };
            let value = if forward {
                numbers[numbers.len() - 1] + step * k as f64
            } else {
                numbers[0] - step * k as f64
            };
            let value = (value * 1e10).round() / 1e10;
            out.content = Some(Content::Value(Value::Number(value)));
            return out;
        }
        if source.len() == 1 || values.iter().all(|v| matches!(v, Value::Text(_))) {
            if let Value::Text(text) = &values[if forward { values.len() - 1 } else { 0 }] {
                for names in [&WEEKDAYS[..], &MONTHS[..]] {
                    if let Some((pos, full)) = cycle_pos(text, names) {
                        let step = if values.len() >= 2 {
                            let first = cycle_pos(
                                &match &values[0] {
                                    Value::Text(t) => t.clone(),
                                    _ => String::new(),
                                },
                                names,
                            )
                            .map_or(0, |p| p.0 as i64);
                            let second = cycle_pos(
                                &match &values[1] {
                                    Value::Text(t) => t.clone(),
                                    _ => String::new(),
                                },
                                names,
                            )
                            .map_or(0, |p| p.0 as i64);
                            (second - first).rem_euclid(names.len() as i64).max(1)
                        } else {
                            1
                        };
                        let i = pos as i64 + sign * step * k;
                        out.content = Some(Content::Value(Value::Text(cycle_text(
                            names,
                            usize::try_from(i.rem_euclid(names.len() as i64)).unwrap_or(0),
                            full,
                            text,
                        ))));
                        return out;
                    }
                }
                if let Some((head, number, width)) = split_number(text) {
                    let step = if values.len() >= 2 {
                        match (&values[0], &values[1]) {
                            (Value::Text(a), Value::Text(b)) => {
                                match (split_number(a), split_number(b)) {
                                    (Some((_, x, _)), Some((_, y, _))) => y - x,
                                    _ => 1,
                                }
                            }
                            _ => 1,
                        }
                    } else {
                        1
                    };
                    let value = number + sign * step * k;
                    out.content = Some(Content::Value(Value::Text(format!(
                        "{head}{value:0width$}"
                    ))));
                    return out;
                }
            }
        }
        // Anything else repeats.
        out
    }

    /// Fill from the selection `source` out to the cell `to` (below, above, left or right of it): a series where the cells
    /// suggest one (numbers, dates, names of days and months, text ending in a number), otherwise a copy; formulas move as
    /// in a copy. One undo step. Returns the filled rectangle, or `None` when `to` is inside the source.
    pub fn autofill(&self, source: Range, to: CellRef) -> Option<Range> {
        let (down, up) = (to.row > source.end.row, to.row < source.start.row);
        let (right, left) = (to.col > source.end.col, to.col < source.start.col);
        let vertical = if down || up {
            true
        } else if right || left {
            false
        } else {
            return None;
        };
        // The dominant direction when the pointer is off at a corner.
        let vertical = if (down || up) && (right || left) {
            let (dr, dc) = (
                if down {
                    to.row - source.end.row
                } else {
                    source.start.row - to.row
                },
                if right {
                    to.col - source.end.col
                } else {
                    source.start.col - to.col
                },
            );
            dr >= dc
        } else {
            vertical
        };
        let forward = if vertical {
            down || !up
        } else {
            right || !left
        };
        let sheet = self.sheet();
        let mut edits: Vec<(CellRef, CellState)> = Vec::new();
        let target;
        if vertical {
            let (from, count) = if forward {
                (source.end.row + 1, to.row.saturating_sub(source.end.row))
            } else {
                (to.row, source.start.row - to.row)
            };
            if count == 0 || count > 100_000 {
                return None;
            }
            for c in source.start.col..=source.end.col {
                let run: Vec<CellRef> = (source.start.row..=source.end.row)
                    .map(|r| CellRef::new(r, c))
                    .collect();
                for k in 1..=i64::from(count) {
                    let row = if forward {
                        source.end.row + u32::try_from(k).ok()?
                    } else {
                        source.start.row - u32::try_from(k).ok()?
                    };
                    edits.push((CellRef::new(row, c), self.series(&run, k, forward, true)));
                }
            }
            target = if forward {
                Range::new(
                    CellRef::new(source.start.row, source.start.col),
                    CellRef::new(from + count - 1, source.end.col),
                )
            } else {
                Range::new(
                    CellRef::new(from, source.start.col),
                    CellRef::new(source.end.row, source.end.col),
                )
            };
        } else {
            let count = if forward {
                to.col.saturating_sub(source.end.col)
            } else {
                source.start.col - to.col
            };
            if count == 0 || count > 16_384 {
                return None;
            }
            for r in source.start.row..=source.end.row {
                let run: Vec<CellRef> = (source.start.col..=source.end.col)
                    .map(|c| CellRef::new(r, c))
                    .collect();
                for k in 1..=i64::from(count) {
                    let col = if forward {
                        source.end.col + u32::try_from(k).ok()?
                    } else {
                        source.start.col - u32::try_from(k).ok()?
                    };
                    edits.push((CellRef::new(r, col), self.series(&run, k, forward, false)));
                }
            }
            target = if forward {
                Range::new(
                    source.start,
                    CellRef::new(source.end.row, source.end.col + count),
                )
            } else {
                Range::new(
                    CellRef::new(source.start.row, source.start.col - count),
                    source.end,
                )
            };
        }
        self.edit(sheet, edits);
        self.select_range(target);
        Some(target)
    }

    // ---- paste special ----

    /// Paste what was last copied in this app in a particular way, at the selection: only what the cells show (`Values`),
    /// only their formulas and values without formats, only their formats, or with rows and columns swapped. One undo step.
    pub fn paste_special(&self, mode: PasteMode) {
        let Some(clip) = self.inner.borrow().clip.clone() else {
            self.status
                .set(crate::i18n::tr("Copy some cells here first"));
            return;
        };
        let sel = self.selection();
        let sheet = self.sheet();
        let height = clip.cells.len();
        let width = clip.cells.iter().map(Vec::len).max().unwrap_or(0);
        if height == 0 || width == 0 || height * width > 1_000_000 {
            return;
        }
        let (rows, cols) = if mode == PasteMode::Transpose {
            (width, height)
        } else {
            (height, width)
        };
        let mut edits: Vec<(CellRef, CellState)> = Vec::new();
        {
            let e = self.engine();
            for i in 0..rows {
                for j in 0..cols {
                    let (si, sj) = if mode == PasteMode::Transpose {
                        (j, i)
                    } else {
                        (i, j)
                    };
                    let Some(src) = clip.cells.get(si).and_then(|r| r.get(sj)) else {
                        continue;
                    };
                    let (Ok(di), Ok(dj)) = (u32::try_from(i), u32::try_from(j)) else {
                        continue;
                    };
                    let at = CellRef::new(sel.start.row + di, sel.start.col + dj);
                    if at.row >= super::ROWS || at.col >= super::COLS {
                        continue;
                    }
                    let from = CellRef::new(
                        clip.origin.row + u32::try_from(si).unwrap_or(0),
                        clip.origin.col + u32::try_from(sj).unwrap_or(0),
                    );
                    let here = state_of(&e, sheet, at);
                    let moved_content = src.content.as_ref().map(|c| {
                        moved(
                            c,
                            i64::from(at.row) - i64::from(from.row),
                            i64::from(at.col) - i64::from(from.col),
                        )
                    });
                    let cell = match mode {
                        PasteMode::Values => {
                            let value = e.value(clip.sheet, from);
                            CellState {
                                content: (!matches!(value, Value::Empty))
                                    .then_some(Content::Value(value)),
                                style: here.style,
                            }
                        }
                        PasteMode::Formulas => CellState {
                            content: moved_content,
                            style: here.style,
                        },
                        PasteMode::Formats => CellState {
                            content: here.content,
                            style: src.style,
                        },
                        PasteMode::Transpose => CellState {
                            content: src.content.as_ref().map(|c| turned(c, from, at)),
                            style: src.style,
                        },
                    };
                    edits.push((at, cell));
                }
            }
        }
        let changed = self.edit(sheet, edits);
        self.status.set(if changed {
            "Pasted".to_string()
        } else {
            "Nothing changed".to_string()
        });
        let end = CellRef::new(
            (sel.start.row + u32::try_from(rows).unwrap_or(1) - 1).min(super::ROWS - 1),
            (sel.start.col + u32::try_from(cols).unwrap_or(1) - 1).min(super::COLS - 1),
        );
        self.select_range(Range::new(sel.start, end));
    }
}

/// A cell's content as if it were turned over the diagonal when moved from one place to another (formulas read the cells
/// that are in the same relation to the new place).
fn turned(content: &Content, from: CellRef, to: CellRef) -> Content {
    match content {
        Content::Formula(f) => Content::Formula(sheets_formula::parse(f).map_or_else(
            |_| f.clone(),
            |e| sheets_formula::print(&e.transposed(from, to)),
        )),
        Content::Value(_) => content.clone(),
    }
}

/// The ways of pasting what was copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMode {
    /// What the cells show, as values: no formulas, no formats.
    Values,
    /// Formulas and values, but the formats stay as they are.
    Formulas,
    /// Only the formats.
    Formats,
    /// Everything, with rows and columns swapped.
    Transpose,
}

impl SheetState {
    // ---- outline: groups of rows and columns ----

    /// The group level of a row or column of the sheet on screen (0: in no group).
    #[must_use]
    pub fn level_of(&self, rows: bool, index: u32) -> u8 {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .map_or(0, |s| {
                if rows {
                    s.row_level(index)
                } else {
                    s.col_level(index)
                }
            })
    }

    fn span_of(&self, rows: bool) -> (u32, u32) {
        let sel = self.selection();
        if rows {
            (sel.start.row, sel.end.row)
        } else {
            (sel.start.col, sel.end.col)
        }
    }

    /// Put the selected rows (or columns) one group level deeper.
    pub fn group_selected(&self, rows: bool) {
        let (from, to) = self.span_of(rows);
        let sheet = self.sheet();
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                for i in from..=to {
                    let level = if rows { s.row_level(i) } else { s.col_level(i) };
                    if rows {
                        s.set_row_level(i, (level + 1).min(7))
                    } else {
                        s.set_col_level(i, (level + 1).min(7))
                    }
                }
            })
            .is_some()
        }) {
            self.status.set(if rows {
                "Grouped rows".to_string()
            } else {
                "Grouped columns".to_string()
            });
        }
    }

    /// Take the selected rows (or columns) out of their innermost group.
    pub fn ungroup_selected(&self, rows: bool) {
        let (from, to) = self.span_of(rows);
        let sheet = self.sheet();
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut any = false;
                for i in from..=to {
                    let level = if rows { s.row_level(i) } else { s.col_level(i) };
                    if level == 0 {
                        continue;
                    }
                    any = true;
                    if rows {
                        s.set_row_level(i, level - 1);
                        if level == 1 {
                            s.set_row_hidden(i, false);
                        }
                    } else {
                        s.set_col_level(i, level - 1);
                        if level == 1 {
                            s.set_col_hidden(i, false);
                        }
                    }
                }
                any
            })
            .unwrap_or(false)
        }) {
            self.status.set(crate::i18n::tr("Ungrouped"));
        }
    }

    /// Hide every row (or column) that is in a group, in the selection or, with one cell selected, on the whole sheet.
    pub fn collapse_groups(&self, rows: bool) {
        self.set_groups_hidden(rows, true);
    }

    /// Show the rows (or columns) of every group.
    pub fn expand_groups(&self, rows: bool) {
        self.set_groups_hidden(rows, false);
    }

    fn set_groups_hidden(&self, rows: bool, hide: bool) {
        let sel = self.selection();
        let whole = sel.start == sel.end;
        let (from, to) = self.span_of(rows);
        let sheet = self.sheet();
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let items: Vec<u32> = if rows {
                    s.row_levels().map(|(i, _)| i).collect()
                } else {
                    s.col_levels().map(|(i, _)| i).collect()
                };
                let mut any = false;
                for i in items
                    .into_iter()
                    .filter(|i| whole || (from..=to).contains(i))
                {
                    any = true;
                    if rows {
                        s.set_row_hidden(i, hide)
                    } else {
                        s.set_col_hidden(i, hide)
                    }
                }
                any
            })
            .unwrap_or(false)
        }) {
            self.status.set(if hide {
                "Collapsed the groups".to_string()
            } else {
                "Expanded the groups".to_string()
            });
        }
    }
}

impl SheetState {
    // ---- pivot tables: a summary built from formulas ----

    /// The form for a summary table of the block around the cursor (its first row holds the headings).
    pub fn open_pivot_dialog(&self) {
        use super::{Dialog, DialogId, Field, FieldKind};
        let sel = self.selection();
        let range = if sel.start == sel.end {
            self.current_region(sel.start)
        } else {
            sel
        };
        let headings: Vec<String> = (range.start.col..=range.end.col)
            .map(|c| {
                let t = self.display(CellRef::new(range.start.row, c));
                if t.is_empty() {
                    format!("Column {}", sheets_model::col_name(c))
                } else {
                    t
                }
            })
            .collect();
        if range.rows() < 2 || range.cols() < 2 {
            self.status.set(
                "Put the cursor in a table with headings and at least one row of data".to_string(),
            );
            return;
        }
        let pick = |label: &str, value: usize, none: bool| {
            let mut options: Vec<(String, String)> = Vec::new();
            if none {
                options.push((String::new(), "(none)".to_string()));
            }
            options.extend(
                headings
                    .iter()
                    .enumerate()
                    .map(|(i, h)| (i.to_string(), h.clone())),
            );
            Field {
                label: label.into(),
                kind: FieldKind::Choice(options),
                value: if none && value == usize::MAX {
                    String::new()
                } else {
                    value.to_string()
                },
                hint: String::new(),
            }
        };
        let sum = headings.len().saturating_sub(1);
        let anchor = CellRef::new(range.start.row, range.end.col + 2);
        self.dialog.set(Some(Dialog {
            id: DialogId::Pivot,
            title: crate::i18n::tr("Summary table"),
            intro: crate::i18n::tr("A table of formulas that adds up (or counts, averages...) one column for each value of another. It stays up to date with the data; to pick up new values, make the table again."),
            fields: vec![
                Field { label: "Data (with headings)".into(), kind: FieldKind::Text, value: range.to_string(), hint: "A1:D100".into() },
                pick("Down the side", 0, false),
                pick("Across the top", usize::MAX, true),
                pick("Values", sum, false),
                Field {
                    label: "Summarise by".into(),
                    kind: FieldKind::Choice(vec![("sum".into(), "Sum".into()), ("count".into(), "Count".into()), ("average".into(), "Average".into()), ("min".into(), "Smallest".into()), ("max".into(), "Largest".into())]),
                    value: "sum".into(),
                    hint: String::new(),
                },
                Field { label: "Put it at".into(), kind: FieldKind::Text, value: anchor.to_string(), hint: "H1".into() },
            ],
            error: String::new(),
        }));
    }

    pub(super) fn submit_pivot(&self, fields: &[String]) -> Result<String, String> {
        let f = |i: usize| {
            fields
                .get(i)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let text = f(0).replace('$', "");
        let Some(source) = Range::parse(&text) else {
            return Err(crate::i18n::tr("Write the data like A1:D100"));
        };
        let Some(at) = CellRef::parse(&f(5).replace('$', "")) else {
            return Err(crate::i18n::tr("Write where to put it as a cell, like H1"));
        };
        let col_of = |i: &str| {
            i.parse::<u32>()
                .ok()
                .filter(|n| *n < source.cols())
                .map(|n| source.start.col + n)
        };
        let (Some(row_col), Some(value_col)) = (col_of(&f(1)), col_of(&f(3))) else {
            return Err(crate::i18n::tr("Pick the columns"));
        };
        let across = col_of(&f(2));
        let how = f(4);
        let sheet = self.sheet();
        let first = source.start.row + 1;
        if first > source.end.row {
            return Err(crate::i18n::tr("There is no data under the headings"));
        }
        let sheet_name = self.sheet_names().get(sheet).cloned().unwrap_or_default();
        let quote = if sheet_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            sheet_name
        } else {
            format!("'{}'", sheet_name.replace('\'', "''"))
        };
        let column = |c: u32| {
            format!(
                "{quote}!${0}${1}:${0}${2}",
                sheets_model::col_name(c),
                first + 1,
                source.end.row + 1
            )
        };
        let distinct = |c: u32| -> Vec<String> {
            let mut seen: Vec<String> = Vec::new();
            for r in first..=source.end.row {
                let t = self.display(CellRef::new(r, c));
                if !t.is_empty() && !seen.iter().any(|s| s.eq_ignore_ascii_case(&t)) {
                    seen.push(t);
                }
            }
            seen.sort_by_key(|s| s.to_lowercase());
            seen
        };
        let (rows, cols) = (distinct(row_col), across.map(distinct).unwrap_or_default());
        if rows.is_empty() || rows.len() > 500 || cols.len() > 100 {
            return Err(
                "There are no values to list down the side, or too many (500 down, 100 across)"
                    .into(),
            );
        }
        let heading = |c: u32| {
            let t = self.display(CellRef::new(source.start.row, c));
            if t.is_empty() {
                format!("Column {}", sheets_model::col_name(c))
            } else {
                t
            }
        };
        let (verb, label) = match how.as_str() {
            "count" => ("COUNTIFS", "Count of"),
            "average" => ("AVERAGEIFS", "Average of"),
            "min" => ("MINIFS", "Smallest"),
            "max" => ("MAXIFS", "Largest"),
            _ => ("SUMIFS", "Sum of"),
        };
        let (values, row_range) = (column(value_col), column(row_col));
        let cell = |r: u32, c: u32| CellRef::new(at.row + r, at.col + c);
        let colname = |c: u32| sheets_model::col_name(at.col + c);
        let mut put: Vec<(CellRef, Content, bool)> = Vec::new();
        let text_cell = |t: &str| Content::Value(Value::Text(t.to_string()));
        put.push((
            cell(0, 0),
            text_cell(&format!("{label} {}", heading(value_col))),
            true,
        ));
        let formula = |extra: &str| {
            Content::Formula(match verb {
                "COUNTIFS" => format!("COUNTIFS({extra})"),
                _ => format!("IFERROR({verb}({values},{extra}),\"\")"),
            })
        };
        let width = cols.len().max(1);
        for (j, c) in cols.iter().enumerate() {
            put.push((
                cell(0, u32::try_from(j + 1).unwrap_or(0)),
                text_cell(c),
                true,
            ));
        }
        if across.is_some() {
            put.push((
                cell(0, u32::try_from(width + 1).unwrap_or(0)),
                text_cell("Total"),
                true,
            ));
        } else {
            put.push((cell(0, 1), text_cell("Total"), true));
        }
        for (i, r) in rows.iter().enumerate() {
            let ri = u32::try_from(i + 1).unwrap_or(0);
            put.push((cell(ri, 0), text_cell(r), true));
            let by_row = format!("{row_range},${}{}", colname(0), at.row + ri + 1);
            if let Some(ac) = across {
                let across_range = column(ac);
                for j in 0..cols.len() {
                    let cj = u32::try_from(j + 1).unwrap_or(0);
                    put.push((
                        cell(ri, cj),
                        formula(&format!(
                            "{by_row},{across_range},{}${}",
                            colname(cj),
                            at.row + 1
                        )),
                        false,
                    ));
                }
                put.push((
                    cell(ri, u32::try_from(width + 1).unwrap_or(0)),
                    formula(&by_row),
                    false,
                ));
            } else {
                put.push((cell(ri, 1), formula(&by_row), false));
            }
        }
        // The totals row.
        let tr = u32::try_from(rows.len() + 1).unwrap_or(0);
        put.push((cell(tr, 0), text_cell("Total"), true));
        let last_col = if across.is_some() {
            u32::try_from(width + 1).unwrap_or(0)
        } else {
            1
        };
        for cj in 1..=last_col {
            let all = if across.is_some() && cj <= u32::try_from(cols.len()).unwrap_or(0) {
                formula(&format!(
                    "{},{}${}",
                    column(across.unwrap_or(0)),
                    colname(cj),
                    at.row + 1
                ))
            } else {
                match verb {
                    "COUNTIFS" | "SUMIFS" => Content::Formula(format!(
                        "SUM({0}{1}:{0}{2})",
                        colname(cj),
                        at.row + 2,
                        at.row + tr
                    )),
                    "AVERAGEIFS" => Content::Formula(format!("IFERROR(AVERAGE({values}),\"\")")),
                    "MINIFS" => Content::Formula(format!("MIN({values})")),
                    _ => Content::Formula(format!("MAX({values})")),
                }
            };
            put.push((cell(tr, cj), all, false));
        }
        let bold = {
            let mut e = self.doc.borrow_mut();
            let base = e.engine.style(sheet, at);
            let mut style = e
                .engine
                .workbook()
                .styles
                .get(base as usize)
                .cloned()
                .unwrap_or_default();
            style.bold = true;
            e.engine.style_id(&style)
        };
        let edits: Vec<(CellRef, CellState)> = put
            .into_iter()
            .map(|(at, content, head)| {
                (
                    at,
                    CellState {
                        content: Some(content),
                        style: if head { bold } else { 0 },
                    },
                )
            })
            .collect();
        let count = edits.len();
        self.edit(sheet, edits);
        self.select(at);
        Ok(format!("Made a summary table of {count} cells"))
    }
}
