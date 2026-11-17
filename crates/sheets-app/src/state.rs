//! The state of the sheet on screen: no view, no events.

// Sizes on screen are estimated in floating point and rounded to whole pixels and characters.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]

mod charts;
mod data;
mod dialog;

pub use data::PasteMode;
pub use dialog::{Dialog, DialogId, Field, FieldKind};

use crate::axis::{Axes, Axis};
use crate::clipboard::{self, Clip};
use crate::find;
use crate::history::{CellState, Delta, History, Snapshot, Step};
use crate::input::{edit_text, parse_input};
use quench_ui::platform::{self, PickedFile};
use quench_ui::prelude::*;
use sheets_calc::Engine;
use sheets_formula::{parse, print};
use sheets_model::{
    Align, Axis as Dim, Border, CellRef, Content, Line, LineKind, Range, Style, Value, Workbook,
};
use sheets_xlsx::Xlsx;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Files bigger than this many bytes are opened after the screen has said so.
const BIG_FILE: usize = 400_000;

/// Most cells a format may be put on one by one; a bigger selection (a whole column) formats the cells that exist.
const MAX_FORMAT_CELLS: u64 = 100_000;

/// How a cell is drawn: what it shows and how it looks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent switches that the cell's class names
pub struct Look {
    pub text: String,
    pub number: bool,
    pub error: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<u32>,
    pub fill: Option<u32>,
    pub align: Align,
    pub wrap: bool,
    pub border: Border,
    /// The note on the cell, if it has one.
    pub note: Option<String>,
    /// A sparkline drawn in the cell: an image (an SVG, percent-encoded).
    pub spark: Option<String>,
    /// A data bar: how much of the cell it fills in percent, and its colour.
    pub bar: Option<(u8, u32)>,
    /// Pixels beyond the cell's right edge that its text may use (empty neighbours); 0 when it stays inside.
    pub spill: u32,
    /// Half points; `None` for the sheet's default size.
    pub size: Option<u16>,
    pub font: Option<String>,
    /// The text being looked for is in this cell.
    pub found: bool,
    /// The class a plain table look gives the cell (a heading, a tinted row).
    pub table: Option<&'static str>,
}

impl Look {
    /// The `class` attribute of the cell.
    #[must_use]
    pub fn class(&self) -> String {
        let mut c = String::from("cell");
        for (on, name) in [
            (self.number, "num"),
            (self.error, "err"),
            (self.bold, "b"),
            (self.italic, "i"),
            (self.underline, "u"),
            (self.wrap, "wrap"),
            (self.found, "found"),
            (self.spill > 0, "spill"),
            (self.note.is_some(), "note"),
            (self.spark.is_some(), "spark"),
        ] {
            if on {
                c.push(' ');
                c.push_str(name);
            }
        }
        if let Some(t) = self.table {
            c.push(' ');
            c.push_str(t);
        }
        match self.align {
            Align::General => {}
            Align::Left => c.push_str(" al"),
            Align::Center => c.push_str(" ac"),
            Align::Right => c.push_str(" ar"),
        }
        c
    }
}

/// Most cells one paste may write.
const MAX_PASTE_CELLS: usize = 1_000_000;

/// What the document keeps apart from the cells: undo steps and the last copy.
#[derive(Debug, Default)]
struct Inner {
    history: History,
    clip: Option<Clip>,
    /// The selection as it was before the last press, for a secondary press that must not take it away.
    before_press: Option<Range>,
    /// The cell the pointer was last over, and whether a cell has seen the move being handled (a move that no cell sees
    /// is outside the grid).
    over: Option<CellRef>,
    hit: bool,
    /// Which way a drag that left the grid pulls (rows, columns), whether it is being followed, and for how many steps.
    pull: (i64, i64),
    pulling: bool,
    steps: u32,
}

fn range_has(r: &Range, at: CellRef) -> bool {
    (r.start.row..=r.end.row).contains(&at.row) && (r.start.col..=r.end.col).contains(&at.col)
}

/// The looks a table can have: what the menu calls them and how a `.xlsx` file does.
pub const TABLE_STYLES: [(&str, &str); 8] = [
    ("Plain (follows the theme)", ""),
    ("Blue", "TableStyleMedium2"),
    ("Orange", "TableStyleMedium3"),
    ("Grey", "TableStyleMedium4"),
    ("Gold", "TableStyleMedium5"),
    ("Light blue", "TableStyleMedium6"),
    ("Green", "TableStyleMedium7"),
    ("No look", "None"),
];

/// The colours of a coloured look: heading band, its text, and the tint of every other row. `None` for the plain look.
pub fn table_palette(style: &str) -> Option<(u32, u32, u32)> {
    if let Some(colors) = sheets_model::parse_custom_table_style(style) {
        return Some(colors);
    }
    Some(match style {
        "TableStyleMedium2" => (0x0044_72C4, 0x00FF_FFFF, 0x00D9_E1F2),
        "TableStyleMedium3" => (0x00ED_7D31, 0x00FF_FFFF, 0x00FB_E5D6),
        "TableStyleMedium4" => (0x00A5_A5A5, 0x00FF_FFFF, 0x00ED_EDED),
        "TableStyleMedium5" => (0x00FF_C000, 0x0020_2020, 0x00FF_F2CC),
        "TableStyleMedium6" => (0x005B_9BD5, 0x00FF_FFFF, 0x00DD_EBF7),
        "TableStyleMedium7" => (0x0070_AD47, 0x00FF_FFFF, 0x00E2_EFDA),
        _ => return None,
    })
}

/// Rows and columns of a sheet, as in a `.xlsx` file.
pub const ROWS: u32 = 1_048_576;
pub const COLS: u32 = 16_384;
pub const ROW_H: f64 = 24.0;
pub const COL_W: f64 = 96.0;
pub const HEADER_W: f64 = 52.0;
pub const HEADER_H: f64 = 24.0;
/// Limits for resizing, in pixels.
pub const MIN_COL_W: f64 = 20.0;
pub const MAX_COL_W: f64 = 1200.0;
pub const MIN_ROW_H: f64 = 12.0;
pub const MAX_ROW_H: f64 = 600.0;

/// A column or row border being dragged.
#[derive(Clone, Copy, Debug)]
struct Resize {
    row: bool,
    index: u32,
    /// Where the pointer was when it went down, and how big the item was then.
    start: f64,
    size: f64,
}

const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// The cells written since the changes were last taken, and whether anything else changed (rows inserted, a sheet added, a
/// width set): then only the whole file says what the document is.
#[derive(Debug, Default)]
struct Track {
    cells: Vec<crate::sync::Written>,
    opaque: bool,
}

/// A workbook being edited. Cloning shares it.
#[derive(Clone)]
pub struct SheetState {
    doc: Rc<RefCell<Xlsx>>,
    /// What changed since a document in the cloud was last sent (see [`SheetState::take_changes`]).
    track: Rc<RefCell<Track>>,
    inner: Rc<RefCell<Inner>>,
    /// Set when a text field handled a key that would also reach the grid (the field is inside it): the grid skips it.
    swallow: Rc<Cell<bool>>,
    resizing: Rc<Cell<Option<Resize>>>,
    /// Whether the pointer has moved since a header border was pressed (a click that ends a drag is not a click).
    resized: Rc<Cell<bool>>,
    /// Whether shift is held (a click then extends the selection). Tracked from the keys the grid sees.
    shift: Rc<Cell<bool>>,
    /// Where the columns and rows are, for the sheet on screen.
    pub axes: Signal<Rc<Axes>>,
    /// The zoom of the grid, in percent.
    pub zoom: Signal<u32>,
    /// Whether the text being edited is in the cell itself (typing in the grid) rather than in the formula bar.
    pub edit_in_cell: Signal<bool>,
    /// The sheet on screen.
    pub active: Signal<usize>,
    /// The file's name, shown and used when saving.
    pub name: Signal<String>,
    /// The last thing worth telling the person (a file opened, a file that would not open).
    pub status: Signal<String>,
    /// Counts changes to the workbook; cells read it to know when to look again.
    pub rev: Signal<u64>,
    /// The active cell: where typing goes. The selection is the rectangle between it and `range_end`.
    pub selected: Signal<CellRef>,
    /// The corner of the selection opposite the active cell.
    pub range_end: Signal<CellRef>,
    /// Where keyboard focus (and so the scrolling that follows it) goes: the cell the selection last moved to. A whole
    /// column or row selected from its header keeps it at the top, so the view stays where it is.
    pub focus_at: Signal<CellRef>,
    /// Whether a pointer is down on the grid, extending the selection as it moves.
    pub dragging: Signal<bool>,
    pub can_undo: Signal<bool>,
    pub can_redo: Signal<bool>,
    /// The text in the formula bar.
    pub draft: Signal<String>,
    /// Whether the formula bar has been typed in and not yet committed.
    pub editing: Signal<bool>,
    /// Bumped to bring the cursor back (and keyboard focus with it) after the formula bar.
    pub cursor_epoch: Signal<u32>,
    /// Whether the cursor should take keyboard focus (once the selection has moved; never at start, see `view`).
    pub focus_cursor: Signal<bool>,
    /// Whether the find bar is open, and what it is looking for.
    pub find_open: Signal<bool>,
    /// Where the context menu is open (the viewport's coordinates), if it is.
    pub context: Signal<Option<(f64, f64)>>,
    /// Whether the open context menu is for a row heading (`Some(true)`), a column heading, or cells (`None`).
    pub context_header: Signal<Option<bool>>,
    /// The language of the screen (a locale code); the screen is drawn again when it changes.
    pub language: Signal<String>,
    pub find_query: Signal<String>,
    pub replace_with: Signal<String>,
    pub match_case: Signal<bool>,
    pub whole_cell: Signal<bool>,
    /// The query is a regular expression, and the search covers every sheet.
    pub find_regex: Signal<bool>,
    pub find_all_sheets: Signal<bool>,
    /// What is wrong with the pattern, if anything.
    pub find_problem: Signal<String>,
    find_cache: Rc<RefCell<FindCache>>,
    /// Bumped to take keyboard focus to the find field (or to the replace field when `replace_first`).
    pub find_epoch: Signal<u32>,
    pub replace_first: Signal<bool>,
    /// The tab being renamed and the text typed so far, and the tab asked to be deleted.
    pub renaming: Signal<Option<usize>>,
    pub rename_draft: Signal<String>,
    pub pending_delete: Signal<Option<usize>>,
    /// Which of the suggested names is picked.
    pub suggest_index: Signal<usize>,
    /// The chart picked on the sheet, and where it would land if dropped now.
    pub chart_selected: Signal<Option<usize>>,
    pub chart_drop: Signal<Option<CellRef>>,
    chart_grab: Rc<Cell<Option<(usize, f64, f64)>>>,
    /// The form being filled in, if any.
    pub dialog: Signal<Option<Dialog>>,
    /// The list under the active cell's arrow is open.
    pub dropdown_open: Signal<bool>,
    /// The column whose filter list is open, and the values ticked in it.
    pub filter_open: Signal<Option<u32>>,
    /// Which filter the open list belongs to: `None` the sheet's, `Some(i)` table `i`'s.
    pub filter_on: Signal<Option<usize>>,
    /// The fill handle is being dragged, and the cell the pointer is over.
    pub fill_drag: Signal<Option<CellRef>>,
    fill_from: Rc<Cell<Option<Range>>>,
    pub filter_pick: Signal<std::collections::BTreeSet<String>>,
    /// What the conditional formats of the sheet on screen know about their ranges, kept until the workbook changes.
    cf: Rc<RefCell<CfCache>>,
    /// Where the time of day comes from (re-applied when a file is opened).
    clock: Rc<RefCell<Option<Clock>>>,
    /// Told when the workbook or its name changes.
    on_change: Rc<RefCell<Option<Hook>>>,
    /// The height of the grid on screen, in rows, for Page Up and Page Down.
    pub page_rows: Signal<u32>,
}

impl std::fmt::Debug for SheetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetState").finish_non_exhaustive()
    }
}

impl SheetState {
    /// A new document holding `workbook`, called "Untitled.xlsx".
    #[must_use]
    pub fn new(workbook: Workbook) -> Self {
        crate::i18n::reset();
        let state = Self {
            doc: Rc::new(RefCell::new(Xlsx::new(workbook))),
            track: Rc::default(),
            inner: Rc::default(),
            swallow: Rc::default(),
            resizing: Rc::default(),
            resized: Rc::default(),
            shift: Rc::default(),
            zoom: create_signal(100),
            axes: create_signal(Rc::new(Axes {
                cols: Axis::new(COL_W, COLS, []),
                rows: Axis::new(ROW_H, ROWS, []),
                frozen: (0, 0),
            })),
            edit_in_cell: create_signal(false),
            active: create_signal(0),
            name: create_signal("Untitled.xlsx".to_string()),
            status: create_signal(String::new()),
            rev: create_signal(0),
            selected: create_signal(CellRef::new(0, 0)),
            range_end: create_signal(CellRef::new(0, 0)),
            focus_at: create_signal(CellRef::new(0, 0)),
            dragging: create_signal(false),
            can_undo: create_signal(false),
            can_redo: create_signal(false),
            draft: create_signal(String::new()),
            editing: create_signal(false),
            cursor_epoch: create_signal(0),
            focus_cursor: create_signal(false),
            find_open: create_signal(false),
            context: create_signal(None),
            context_header: create_signal(None),
            language: create_signal(crate::i18n::language().to_string()),
            find_query: create_signal(String::new()),
            replace_with: create_signal(String::new()),
            match_case: create_signal(false),
            whole_cell: create_signal(false),
            find_regex: create_signal(false),
            find_all_sheets: create_signal(false),
            find_problem: create_signal(String::new()),
            find_cache: Rc::default(),
            find_epoch: create_signal(0),
            replace_first: create_signal(false),
            renaming: create_signal(None),
            rename_draft: create_signal(String::new()),
            pending_delete: create_signal(None),
            chart_selected: create_signal(None),
            chart_drop: create_signal(None),
            chart_grab: Rc::default(),
            suggest_index: create_signal(0),
            dialog: create_signal(None),
            dropdown_open: create_signal(false),
            filter_open: create_signal(None),
            filter_on: create_signal(None),
            fill_drag: create_signal(None),
            fill_from: Rc::default(),
            filter_pick: create_signal(std::collections::BTreeSet::new()),
            cf: Rc::default(),
            clock: Rc::default(),
            on_change: Rc::default(),
            page_rows: create_signal(20),
        };
        state.load_draft();
        state.refresh_axes();
        state
    }

    fn engine(&self) -> Ref<'_, Engine> {
        Ref::map(self.doc.borrow(), |d| &d.engine)
    }

    fn sheet(&self) -> usize {
        self.active.get_untracked()
    }

    /// What a cell shows: the value under its number format.
    #[must_use]
    pub fn display(&self, at: CellRef) -> String {
        let e = self.engine();
        let sheet = self.sheet();
        match e.value(sheet, at) {
            Value::Empty => String::new(),
            v @ Value::Number(_) => {
                sheets_format::format_value(e.workbook().number_format(sheet, at), &v)
            }
            other => other.to_string(),
        }
    }

    /// What a cell shows and how: one look at the engine for all of it.
    #[must_use]
    pub fn look(&self, at: CellRef) -> Look {
        let e = self.engine();
        let sheet = self.sheet();
        let value = e.value(sheet, at);
        let style = e
            .workbook()
            .styles
            .get(e.style(sheet, at) as usize)
            .cloned()
            .unwrap_or_default();
        // Cells with the text being looked for are marked; this reads the find signals, so cells look again when they change.
        let found = self.find_open.get() && {
            let _ = (
                self.find_query.get(),
                self.match_case.get(),
                self.whole_cell.get(),
                self.find_regex.get(),
            );
            self.matcher().is_some_and(|m| {
                m.matches(&edit_text(
                    e.workbook().sheets.get(sheet).and_then(|s| s.get(at)),
                ))
            })
        };
        let effect = self.cf_effect(&e, sheet, at);
        let mut text = match &value {
            Value::Empty => String::new(),
            v @ Value::Number(_) => sheets_format::format_value(&style.number_format, v),
            other => other.to_string(),
        };
        // Too wide a number shows as `###`; too wide text borrows the empty cells to its right.
        let axes = self.axes.get();
        let merged = e.workbook().sheets.get(sheet).and_then(|s| s.merge_at(at));
        let width = merged.map_or_else(
            || axes.cols.size(at.col),
            |m| axes.cols.offset(m.end.col + 1) - axes.cols.offset(m.start.col),
        );
        let needs = text_px(&text, style.font_size) + 12.0;
        let mut spill = 0;
        if needs > width && !text.is_empty() {
            match &value {
                Value::Number(_) => text = "#".repeat(((width - 6.0) / CHAR_PX).max(1.0) as usize),
                Value::Error(_) => {}
                _ if style.wrap || matches!(style.align, Align::Center | Align::Right) => {}
                _ => {
                    let mut room = width;
                    let mut col = at.col + 1;
                    let sheet_ref = e.workbook().sheets.get(sheet);
                    while room < needs
                        && col < COLS
                        && col - at.col <= 40
                        && sheet_ref.is_some_and(|s| s.get(CellRef::new(at.row, col)).is_none())
                    {
                        room += axes.cols.size(col);
                        col += 1;
                    }
                    if col > at.col + 1 {
                        spill = (room - width).ceil() as u32;
                    }
                }
            }
        }
        // A table shows its headings on a band and, when it is banded, every other row tinted.
        let table = e
            .workbook()
            .sheets
            .get(sheet)
            .and_then(|sh| sh.table_at(at).map(|i| &sh.tables()[i]))
            .filter(|t| t.style != "None")
            .map(|t| {
                let heading = at.row == t.range().start.row;
                let tinted =
                    t.banded && !heading && (at.row - t.range().start.row).is_multiple_of(2);
                (heading, tinted, table_palette(&t.style))
            });
        let (mut table_fill, mut table_color, table_bold) =
            (None, None, table.is_some_and(|t| t.0));
        let mut table_class = None;
        match table {
            Some((heading, tinted, Some((head, head_text, band)))) => {
                if heading {
                    (table_fill, table_color) = (Some(head), Some(head_text));
                } else if tinted {
                    (table_fill, table_color) = (Some(band), Some(0x0020_2020));
                }
            }
            Some((true, _, None)) => table_class = Some("tbl-head"),
            Some((false, true, None)) => table_class = Some("tbl-band"),
            _ => {}
        }
        Look {
            text,
            number: matches!(value, Value::Number(_)),
            error: matches!(value, Value::Error(_)),
            bold: style.bold || table_bold || effect.as_ref().is_some_and(|f| f.bold),
            italic: style.italic || effect.as_ref().is_some_and(|f| f.italic),
            underline: style.underline,
            color: effect
                .as_ref()
                .and_then(|f| f.color)
                .or(style.color)
                .or(table_color),
            fill: effect
                .as_ref()
                .and_then(|f| f.fill)
                .or(style.fill)
                .or(table_fill),
            align: style.align,
            wrap: style.wrap,
            border: style.border,
            spark: self.spark_image(&e, sheet, at, (width, axes.rows.size(at.row))),
            note: e
                .workbook()
                .sheets
                .get(sheet)
                .and_then(|s| s.comment(at))
                .map(str::to_string),
            bar: effect
                .as_ref()
                .and_then(|f| f.bar)
                .map(|(part, color)| ((part * 100.0).round() as u8, color)),
            spill,
            size: style.font_size,
            font: style.font_name,
            found,
            table: table_class,
        }
    }

    /// What the sheet's conditional formats do to a cell.
    fn cf_effect(&self, e: &Engine, sheet: usize, at: CellRef) -> Option<crate::rules::Effect> {
        let s = e.workbook().sheets.get(sheet)?;
        if s.cond_rules().is_empty() {
            return None;
        }
        let key = Some((self.rev.get_untracked(), sheet));
        let mut cache = self.cf.borrow_mut();
        if cache.key != key {
            let mut rules = s.cond_rules().to_vec();
            rules.sort_by_key(|r| r.priority);
            cache.stats = rules
                .iter()
                .map(|r| crate::rules::stats(e, sheet, r))
                .collect();
            cache.rules = rules;
            cache.key = key;
        }
        crate::rules::effect(e, sheet, at, &cache.rules, &cache.stats)
    }

    /// The format of the active cell, for the toolbar to show.
    #[must_use]
    pub fn active_style(&self) -> Style {
        let at = self.selected.get_untracked();
        let e = self.engine();
        e.workbook()
            .styles
            .get(e.style(self.sheet(), at) as usize)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether a cell holds a number (shown to the right).
    #[must_use]
    pub fn is_number(&self, at: CellRef) -> bool {
        matches!(self.engine().value(self.sheet(), at), Value::Number(_))
    }

    /// Whether a cell holds an error value.
    #[must_use]
    pub fn is_error(&self, at: CellRef) -> bool {
        matches!(self.engine().value(self.sheet(), at), Value::Error(_))
    }

    /// The text to edit for a cell.
    #[must_use]
    pub fn edit_text(&self, at: CellRef) -> String {
        edit_text(
            self.engine()
                .workbook()
                .sheets
                .get(self.sheet())
                .and_then(|s| s.get(at)),
        )
    }

    /// The value at a cell, for tests and the host.
    #[must_use]
    pub fn value(&self, at: CellRef) -> Value {
        self.engine().value(self.sheet(), at)
    }

    /// Run something with the engine (to save the workbook, say).
    pub fn with_engine<T>(&self, f: impl FnOnce(&Engine) -> T) -> T {
        f(&self.engine())
    }

    /// The names of the sheets, in tab order.
    #[must_use]
    pub fn sheet_names(&self) -> Vec<String> {
        self.engine()
            .workbook()
            .sheets
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// Select a cell, dropping what was being typed.
    pub fn select(&self, at: CellRef) {
        let at = CellRef::new(at.row.min(ROWS - 1), at.col.min(COLS - 1));
        // Selecting inside a merged range selects the whole of it, at its top left cell.
        let (at, end) = self.merge_span(at).map_or((at, at), |m| (m.start, m.end));
        batch(|| {
            self.editing.set(false);
            self.focus_cursor.set(true);
            self.selected.set(at);
            self.range_end.set(end);
            self.focus_at.set(end);
            self.load_draft();
            self.cursor_epoch.update(|e| *e = e.wrapping_add(1));
        });
    }

    /// Move the selection by rows and columns, staying on the sheet.
    pub fn move_by(&self, rows: i64, cols: i64) {
        let at = self.selected.get_untracked();
        // From a merged range the move starts at its edge in the direction of travel.
        let m = self.merge_span(at).unwrap_or_else(|| Range::new(at, at));
        let base = CellRef::new(
            if rows > 0 { m.end.row } else { at.row },
            if cols > 0 { m.end.col } else { at.col },
        );
        let clamp = |v: u32, d: i64, max: u32| {
            u32::try_from((i64::from(v) + d).clamp(0, i64::from(max) - 1)).unwrap_or(v)
        };
        let mut target = CellRef::new(clamp(base.row, rows, ROWS), clamp(base.col, cols, COLS));
        // Hidden rows and columns are stepped over.
        let (hidden_rows, hidden_cols) = {
            let doc = self.doc.borrow();
            doc.engine
                .workbook()
                .sheets
                .get(self.sheet())
                .map_or((Vec::new(), Vec::new()), |s| {
                    (
                        s.hidden_rows().collect::<Vec<_>>(),
                        s.hidden_cols().collect::<Vec<_>>(),
                    )
                })
        };
        let step = |v: u32, d: i64, max: u32| clamp(v, if d == 0 { 0 } else { d.signum() }, max);
        while rows != 0
            && hidden_rows.contains(&target.row)
            && step(target.row, rows, ROWS) != target.row
        {
            target.row = step(target.row, rows, ROWS);
        }
        while cols != 0
            && hidden_cols.contains(&target.col)
            && step(target.col, cols, COLS) != target.col
        {
            target.col = step(target.col, cols, COLS);
        }
        if hidden_rows.contains(&target.row) {
            target.row = at.row;
        }
        if hidden_cols.contains(&target.col) {
            target.col = at.col;
        }
        self.select(target);
    }

    /// The merged ranges of the sheet on screen.
    #[must_use]
    pub fn merges(&self) -> Vec<Range> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .map(|s| s.merges().to_vec())
            .unwrap_or_default()
    }

    /// The merged range that holds a cell of the sheet on screen.
    #[must_use]
    pub fn merge_span(&self, at: CellRef) -> Option<Range> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(|s| s.merge_at(at))
    }

    /// The formula bar shows the selected cell again.
    pub fn load_draft(&self) {
        let at = self.selected.get_untracked();
        self.draft.set(self.edit_text(at));
    }

    /// Start editing with this text (a typed character replaces the cell, F2 keeps what is there).
    pub fn begin_edit(&self, text: Option<&str>) {
        batch(|| {
            self.edit_in_cell.set(true);
            if let Some(t) = text {
                self.draft.set(t.to_string());
            }
            self.editing.set(true);
        });
    }

    /// Put text in a cell and recalculate what depends on it. One undo step.
    pub fn set_text(&self, at: CellRef, text: &str) {
        let content = match parse_input(text) {
            Content::Value(Value::Empty) => None,
            c => Some(c),
        };
        let sheet = self.sheet();
        let style = self.engine().style(sheet, at);
        let edited = self.edit(sheet, vec![(at, CellState { content, style })]);
        if edited {
            self.grow_tables(at);
        }
        if edited && let Some((severity, message)) = self.enforce_validation(at) {
            if severity == sheets_model::Severity::Stop {
                // The entry is refused: it is taken back as if it had not been made.
                let step = self.inner.borrow_mut().history.pop_undo();
                if let Some(step) = step {
                    self.replay(step, false);
                    self.history_changed();
                }
                self.status.set(format!("Not entered. {message}"));
            } else {
                self.status.set(message);
            }
        }
    }

    /// Commit what is in the formula bar to the selected cell and move down, as Enter does.
    pub fn commit(&self, rows: i64, cols: i64) {
        let at = self.selected.get_untracked();
        let text = self.draft.get_untracked();
        batch(|| {
            self.set_text(at, &text);
            self.move_by(rows, cols);
        });
    }

    /// Drop what was typed and show the cell as it is.
    pub fn cancel_edit(&self) {
        batch(|| {
            self.editing.set(false);
            self.load_draft();
            self.cursor_epoch.update(|e| *e = e.wrapping_add(1));
        });
    }

    /// Empty every cell of the selection (formats stay). One undo step.
    pub fn clear_selected(&self) {
        let sel = self.selection();
        let sheet = self.sheet();
        let edits: Vec<(CellRef, CellState)> = {
            let doc = self.doc.borrow();
            let Some(s) = doc.engine.workbook().sheets.get(sheet) else {
                return;
            };
            s.iter()
                .filter(|(c, _)| sel.contains(*c))
                .map(|(c, _)| {
                    (
                        c,
                        CellState {
                            content: None,
                            style: s.style(c),
                        },
                    )
                })
                .collect()
        };
        self.edit(sheet, edits);
        self.load_draft();
    }

    /// A click on a cell: what was being typed is kept (as Enter would), and the cell is selected.
    pub fn click_cell(&self, at: CellRef) {
        if self.editing.get_untracked() {
            let current = self.selected.get_untracked();
            let text = self.draft.get_untracked();
            self.set_text(current, &text);
        }
        self.select(at);
    }

    // ---- selection ----

    /// The selected rectangle.
    #[must_use]
    pub fn selection(&self) -> Range {
        Range::new(
            self.selected.get_untracked(),
            self.range_end.get_untracked(),
        )
    }

    /// Where this page is, for the others who have the document open: `sheet:row:col:row:col` (the selected rectangle).
    #[must_use]
    pub fn place_text(&self) -> String {
        let r = self.selection();
        format!(
            "{}:{}:{}:{}:{}",
            self.active.get_untracked(),
            r.start.row,
            r.start.col,
            r.end.row,
            r.end.col
        )
    }

    /// Another page's place (see [`Self::place_text`]): its sheet and rectangle.
    #[must_use]
    pub fn parse_place(text: &str) -> Option<(usize, Range)> {
        let n: Vec<u32> = text
            .split(':')
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [sheet, r0, c0, r1, c1] = n[..] else {
            return None;
        };
        Some((
            usize::try_from(sheet).ok()?,
            Range::new(CellRef::new(r0, c0), CellRef::new(r1, c1)),
        ))
    }

    /// Select exactly this rectangle, the active cell at its top left.
    pub fn select_range(&self, range: Range) {
        batch(|| {
            self.select(range.start);
            self.range_end.set(range.end);
        });
    }

    pub fn select_all(&self) {
        self.select_range(Range::new(
            CellRef::new(0, 0),
            CellRef::new(ROWS - 1, COLS - 1),
        ));
    }

    /// Stretch the selection to a cell, the active cell staying where it is.
    pub fn extend_to(&self, at: CellRef) {
        let at = CellRef::new(at.row.min(ROWS - 1), at.col.min(COLS - 1));
        batch(|| {
            self.editing.set(false);
            self.focus_cursor.set(true);
            self.range_end.set(at);
            self.focus_at.set(at);
            self.cursor_epoch.update(|e| *e = e.wrapping_add(1));
        });
    }

    /// Stretch the selection by rows and columns (shift and an arrow key).
    pub fn extend_by(&self, rows: i64, cols: i64) {
        let at = self.range_end.get_untracked();
        let clamp = |v: u32, d: i64, max: u32| {
            u32::try_from((i64::from(v) + d).clamp(0, i64::from(max) - 1)).unwrap_or(v)
        };
        self.extend_to(CellRef::new(
            clamp(at.row, rows, ROWS),
            clamp(at.col, cols, COLS),
        ));
    }

    /// The pointer went down on a cell: select it (or extend to it with shift held) and start following the pointer.
    pub fn pointer_down(&self, at: CellRef) {
        self.inner.borrow_mut().before_press = Some(self.selection());
        self.chart_selected.set(None);
        if self.shift.get() {
            self.extend_to(at);
        } else {
            self.click_cell(at);
        }
        self.dragging.set(true);
    }

    /// Switch the language of the screen and tell the app, which keeps it.
    pub fn set_language(&self, code: &str) {
        crate::i18n::use_language(code);
        let now = crate::i18n::language();
        self.language.set_if_changed(now.to_string());
        crate::i18n::announce(now);
    }

    /// The secondary button went down on a cell: the selection stays when the cell was inside it (the press has just
    /// narrowed it to the cell), otherwise the cell is the selection; and the context menu opens at the pointer.
    pub fn open_context(&self, at: CellRef, x: f64, y: f64) {
        self.dragging.set(false);
        let before = self.inner.borrow_mut().before_press.take();
        if let Some(range) = before.filter(|r| range_has(r, at)) {
            self.select_range(range);
        }
        self.context_header.set(None);
        self.context.set(Some((x, y)));
    }

    /// The secondary button went down on a row or column heading: that row or column is selected unless it already is
    /// inside the selection, and the context menu opens at the pointer.
    pub fn open_header_context(&self, row: bool, index: u32, x: f64, y: f64) {
        let sel = self.selection();
        let inside = if row {
            sel.start.row <= index
                && index <= sel.end.row
                && sel.start.col == 0
                && sel.end.col == COLS - 1
        } else {
            sel.start.col <= index
                && index <= sel.end.col
                && sel.start.row == 0
                && sel.end.row == ROWS - 1
        };
        if !inside {
            self.select_range(if row {
                Range::new(CellRef::new(index, 0), CellRef::new(index, COLS - 1))
            } else {
                Range::new(CellRef::new(0, index), CellRef::new(ROWS - 1, index))
            });
        }
        self.context_header.set(Some(row));
        self.context.set(Some((x, y)));
    }

    /// What was last copied here, as text.
    #[must_use]
    pub fn clip_text(&self) -> Option<String> {
        self.inner.borrow().clip.as_ref().map(|c| c.text.clone())
    }

    /// The pointer moved over a cell: extends the selection while it is down.
    pub fn pointer_over(&self, at: CellRef) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.over = Some(at);
            inner.hit = true;
        }
        if self.fill_from.get().is_some() {
            self.fill_drag.set(Some(at));
            return;
        }
        if self.dragging.get_untracked() && self.range_end.get_untracked() != at {
            self.extend_to(at);
        }
    }

    /// The fill handle at the corner of the selection was pressed.
    pub fn begin_fill(&self) {
        self.fill_from.set(Some(self.selection()));
        self.fill_drag.set(None);
    }

    /// The pointer moved anywhere on the screen. While a selection is being dragged and the pointer is off the cells, the
    /// selection keeps growing the way the pointer went, and the grid scrolls after it. `toward` says, for the cell the
    /// pointer was last over, which way (rows, columns) lies outside the view from it: `(0, 0)` when none.
    pub fn drag_moved(&self, toward: impl Fn(CellRef) -> (i64, i64)) {
        let (hit, over) = {
            let mut inner = self.inner.borrow_mut();
            (std::mem::take(&mut inner.hit), inner.over)
        };
        let pull = match over {
            Some(at) if !hit && self.dragging.get_untracked() && self.fill_from.get().is_none() => {
                toward(at)
            }
            _ => (0, 0),
        };
        let start = {
            let mut inner = self.inner.borrow_mut();
            inner.pull = pull;
            pull != (0, 0) && !std::mem::replace(&mut inner.pulling, true)
        };
        if start {
            self.inner.borrow_mut().steps = 0;
            self.follow_drag();
        }
    }

    /// One step of following a drag that is outside the grid, and the next one a moment later.
    fn follow_drag(&self) {
        let (pull, steps) = {
            let mut inner = self.inner.borrow_mut();
            if inner.pull == (0, 0) || !self.dragging.get_untracked() {
                inner.pulling = false;
                inner.pull = (0, 0);
                return;
            }
            inner.steps += 1;
            (inner.pull, inner.steps)
        };
        // Slowly at first, then faster the longer the pointer stays out.
        let step = i64::from((steps / 15).min(5)) + 1;
        self.extend_by(pull.0 * step, pull.1 * step);
        let me = self.clone();
        quench_ui::platform::after(45, move || me.follow_drag());
    }

    pub fn pointer_up(&self) {
        self.inner.borrow_mut().pull = (0, 0);
        self.end_chart_drag();
        if let Some(source) = self.fill_from.take() {
            let to = self.fill_drag.get_untracked();
            self.fill_drag.set(None);
            if let Some(to) = to {
                self.autofill(source, to);
            }
            self.dragging.set(false);
            return;
        }
        if self.dragging.get_untracked() {
            self.dragging.set(false);
        }
    }

    /// Tell the state whether shift is held, from the key events the grid receives.
    pub fn set_shift(&self, held: bool) {
        self.shift.set(held);
    }

    // ---- editing: every change goes through here so that it can be undone ----

    /// Write cells (content and style) and remember the step. Returns whether anything changed.
    #[allow(clippy::must_use_candidate)]
    pub fn edit(&self, sheet: usize, edits: Vec<(CellRef, CellState)>) -> bool {
        if self.protected_cells(sheet, edits.iter().map(|(at, _)| *at)) {
            self.status.set(crate::i18n::tr(
                "The sheet is protected: locked cells cannot be changed",
            ));
            return false;
        }
        let step = self.apply(sheet, edits);
        if step.is_empty() {
            return false;
        }
        self.inner.borrow_mut().history.push(step);
        self.history_changed();
        true
    }

    /// The workbook, to be changed in a way no cell list describes: the whole file is what says what it is now.
    fn doc_mut(&self) -> std::cell::RefMut<'_, Xlsx> {
        self.track.borrow_mut().opaque = true;
        self.doc.borrow_mut()
    }

    /// What changed since this was last asked: nothing, the cells that were written, or (for anything else) the whole file.
    pub fn take_changes(&self) -> Option<office_cloud_ui::Change> {
        let mut track = self.track.borrow_mut();
        if track.opaque {
            *track = Track::default();
            return Some(office_cloud_ui::Change::Whole);
        }
        if track.cells.is_empty() {
            return None;
        }
        // The last write to a cell is the one that counts.
        let mut seen = std::collections::HashSet::new();
        let mut last: Vec<crate::sync::Written> = Vec::new();
        for w in track.cells.drain(..).rev() {
            if seen.insert((w.sheet, w.at)) {
                last.push(w);
            }
        }
        last.reverse();
        Some(office_cloud_ui::Change::Ops(crate::sync::encode(&last)))
    }

    /// Write the cells of a change someone else made. Cells this page has changed and not yet sent keep what they have: its
    /// change comes after theirs.
    ///
    /// # Errors
    ///
    /// A message when the change cannot be read.
    pub fn apply_remote(&self, op: &[u8]) -> Result<(), String> {
        let cells = crate::sync::decode(op)?;
        let waiting: std::collections::HashSet<(usize, CellRef)> = self
            .track
            .borrow()
            .cells
            .iter()
            .map(|w| (w.sheet, w.at))
            .collect();
        let sheets = self.sheet_names().len();
        {
            let mut doc = self.doc.borrow_mut();
            for w in cells {
                if w.sheet >= sheets || waiting.contains(&(w.sheet, w.at)) {
                    continue;
                }
                let style = doc.engine.style_id(&w.style);
                write(
                    &mut doc.engine,
                    w.sheet,
                    w.at,
                    &CellState {
                        content: w.content,
                        style,
                    },
                );
            }
        }
        batch(|| {
            self.refresh_axes();
            self.bump();
        });
        Ok(())
    }

    fn apply(&self, sheet: usize, edits: Vec<(CellRef, CellState)>) -> Vec<Delta> {
        let mut doc = self.doc.borrow_mut();
        let mut step = Vec::new();
        for (at, after) in edits {
            let before = state_of(&doc.engine, sheet, at);
            if before == after {
                continue;
            }
            write(&mut doc.engine, sheet, at, &after);
            self.track.borrow_mut().cells.push(crate::sync::Written {
                sheet,
                at,
                content: after.content.clone(),
                style: doc
                    .engine
                    .workbook()
                    .styles
                    .get(after.style as usize)
                    .cloned()
                    .unwrap_or_default(),
            });
            step.push(Delta {
                sheet,
                at,
                before,
                after,
            });
        }
        let wrapped = step.iter().any(|d| {
            let wrap = |state: &CellState| {
                doc.engine
                    .workbook()
                    .styles
                    .get(state.style as usize)
                    .is_some_and(|st| st.wrap)
            };
            wrap(&d.before) || wrap(&d.after)
        });
        drop(doc);
        if wrapped {
            self.refresh_axes();
        }
        if !step.is_empty() {
            self.bump();
        }
        step
    }

    fn history_changed(&self) {
        let (undo, redo) = {
            let i = self.inner.borrow();
            (i.history.can_undo(), i.history.can_redo())
        };
        batch(|| {
            self.can_undo.set(undo);
            self.can_redo.set(redo);
        });
    }

    /// Take back the last change.
    pub fn undo(&self) {
        let step = self.inner.borrow_mut().history.pop_undo();
        let Some(step) = step else { return };
        let opposite = self.replay(step, false);
        self.inner.borrow_mut().history.push_redo(opposite);
        self.history_changed();
    }

    /// Do again what was taken back.
    pub fn redo(&self) {
        let step = self.inner.borrow_mut().history.pop_redo();
        let Some(step) = step else { return };
        let opposite = self.replay(step, true);
        self.inner.borrow_mut().history.push_undo(opposite);
        self.history_changed();
    }

    /// Carry out a step one way and return the step that carries it out the other way.
    fn replay(&self, step: Step, forward: bool) -> Step {
        match step {
            Step::Cells(step) => {
                {
                    let mut doc = self.doc.borrow_mut();
                    let ordered: Vec<&Delta> = if forward {
                        step.iter().collect()
                    } else {
                        step.iter().rev().collect()
                    };
                    for d in ordered {
                        let state = if forward { &d.after } else { &d.before };
                        write(&mut doc.engine, d.sheet, d.at, state);
                        self.track.borrow_mut().cells.push(crate::sync::Written {
                            sheet: d.sheet,
                            at: d.at,
                            content: state.content.clone(),
                            style: doc
                                .engine
                                .workbook()
                                .styles
                                .get(state.style as usize)
                                .cloned()
                                .unwrap_or_default(),
                        });
                    }
                }
                if let Some(first) = step.first() {
                    let (sheet, at) = (first.sheet, first.at);
                    batch(|| {
                        self.active.set(sheet);
                        self.bump();
                        self.select(at);
                    });
                }
                Step::Cells(step)
            }
            Step::Book(saved) => {
                let now = Snapshot {
                    workbook: self.doc.borrow().engine.workbook().clone(),
                    sheet: self.sheet(),
                };
                self.doc_mut().engine.restore(saved.workbook);
                let names = self.sheet_names().len();
                batch(|| {
                    self.active.set(saved.sheet.min(names.saturating_sub(1)));
                    self.refresh_axes();
                    self.bump();
                    self.load_draft();
                });
                Step::Book(Box::new(now))
            }
        }
    }

    /// Run a change to the whole workbook as one undo step, by keeping the workbook as it was. `change` returns whether it
    /// did anything.
    pub fn whole_book_step(&self, change: impl FnOnce(&mut Engine) -> bool) -> bool {
        if self.protection().is_some() {
            self.status.set(crate::i18n::tr(
                "The sheet is protected: unprotect it to do this",
            ));
            return false;
        }
        self.whole_book_step_raw(change)
    }

    /// As `whole_book_step`, also on a protected sheet (for protecting and unprotecting themselves).
    pub fn whole_book_step_raw(&self, change: impl FnOnce(&mut Engine) -> bool) -> bool {
        let before = Snapshot {
            workbook: self.doc.borrow().engine.workbook().clone(),
            sheet: self.sheet(),
        };
        if !change(&mut self.doc_mut().engine) {
            return false;
        }
        self.inner.borrow_mut().history.push_book(before);
        self.history_changed();
        batch(|| {
            self.refresh_axes();
            self.bump();
            self.load_draft();
        });
        true
    }

    // ---- merged cells, hidden rows and columns, frozen panes ----

    /// Merge the selected cells into one, which shows the top left cell. What the other cells held is dropped.
    pub fn merge_selection(&self) {
        let sel = self.selection();
        let sheet = self.sheet();
        if sel.start == sel.end {
            return;
        }
        let done = self.whole_book_step(|e| {
            let others: Vec<CellRef> = e
                .workbook()
                .sheets
                .get(sheet)
                .map(|s| {
                    s.iter()
                        .map(|(c, _)| c)
                        .filter(|c| sel.contains(*c) && *c != sel.start)
                        .collect()
                })
                .unwrap_or_default();
            for c in others {
                e.set(sheet, c, Content::Value(Value::Empty));
            }
            e.update_sheet(sheet, |s| s.merge(sel)).is_some()
        });
        if done {
            self.select(sel.start);
            self.status.set(crate::i18n::tr("Merged cells"));
        }
    }

    /// Split the merged range under the cursor back into cells.
    pub fn unmerge_selection(&self) {
        let at = self.selected.get_untracked();
        let sheet = self.sheet();
        if self.merge_span(at).is_none() {
            return;
        }
        if self.whole_book_step(|e| e.update_sheet(sheet, |s| s.unmerge(at)).unwrap_or(false)) {
            self.select(at);
            self.status.set(crate::i18n::tr("Unmerged cells"));
        }
    }

    /// Hide the selected rows or columns.
    pub fn hide_selected(&self, dim: Dim) {
        let (at, count) = self.selected_span(dim);
        let sheet = self.sheet();
        let all = {
            let doc = self.doc.borrow();
            let total = if dim == Dim::Row { ROWS } else { COLS };
            // The last visible row or column cannot be hidden.
            let s = doc.engine.workbook().sheets.get(sheet);
            count >= total
                || s.is_some_and(|s| {
                    let hidden = if dim == Dim::Row {
                        s.hidden_rows().count()
                    } else {
                        s.hidden_cols().count()
                    };
                    hidden as u64 + u64::from(count) >= u64::from(total)
                })
        };
        if all {
            return;
        }
        let done = self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                for i in at..at + count {
                    if dim == Dim::Row {
                        s.set_row_hidden(i, true);
                    } else {
                        s.set_col_hidden(i, true);
                    }
                }
            })
            .is_some()
        });
        if done {
            let noun = if dim == Dim::Row { "row" } else { "column" };
            self.status.set(format!(
                "Hid {count} {noun}{}",
                if count == 1 { "" } else { "s" }
            ));
            // The cursor moves off what is hidden.
            let from = self.selected.get_untracked();
            self.move_by(i64::from(dim == Dim::Row), i64::from(dim == Dim::Col));
            if self.selected.get_untracked() == from {
                self.move_by(-i64::from(dim == Dim::Row), -i64::from(dim == Dim::Col));
            }
        }
    }

    /// Show the hidden rows or columns in the selection and the one on each side of it (so a hidden row between two selected
    /// ones, or at the edge of the selection, comes back).
    pub fn unhide_around(&self, dim: Dim) {
        let (at, count) = self.selected_span(dim);
        let (from, to) = (at.saturating_sub(1), at + count);
        let sheet = self.sheet();
        let done = self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut any = false;
                for i in from..=to {
                    let was = if dim == Dim::Row {
                        s.row_hidden(i)
                    } else {
                        s.col_hidden(i)
                    };
                    any |= was;
                    if dim == Dim::Row {
                        s.set_row_hidden(i, false);
                    } else {
                        s.set_col_hidden(i, false);
                    }
                }
                any
            })
            .unwrap_or(false)
        });
        if done {
            self.status.set(crate::i18n::tr("Showed hidden cells"));
        }
    }

    /// Keep the first `rows` rows and `cols` columns in view when the sheet scrolls.
    pub fn freeze(&self, rows: u32, cols: u32) {
        let sheet = self.sheet();
        let had = self.axes.get_untracked().frozen;
        if had == (rows, cols) {
            return;
        }
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| s.set_freeze(rows, cols))
                .is_some()
        }) {
            self.status.set(if (rows, cols) == (0, 0) {
                "Unfroze panes".to_string()
            } else {
                "Froze panes".to_string()
            });
        }
    }

    /// Freeze everything above and left of the cursor.
    pub fn freeze_at_cursor(&self) {
        let at = self.selected.get_untracked();
        self.freeze(at.row, at.col);
    }

    // ---- rows and columns ----

    /// The rows (or columns) the selection covers: where it starts and how many.
    fn selected_span(&self, dim: Dim) -> (u32, u32) {
        let sel = self.selection();
        match dim {
            Dim::Row => (sel.start.row, sel.end.row - sel.start.row + 1),
            Dim::Col => (sel.start.col, sel.end.col - sel.start.col + 1),
        }
    }

    /// Rows or columns, for a shortcut that does not say: columns when whole columns are selected, otherwise rows.
    #[must_use]
    pub fn structure_axis(&self) -> Dim {
        let sel = self.selection();
        if sel.start.row == 0 && sel.end.row == ROWS - 1 {
            Dim::Col
        } else {
            Dim::Row
        }
    }

    /// Insert as many rows above (or columns left of) the selection as it covers.
    pub fn insert_selected(&self, dim: Dim) {
        let (at, count) = self.selected_span(dim);
        let sheet = self.sheet();
        if self.whole_book_step(|e| e.insert(sheet, dim, at, count)) {
            let noun = if dim == Dim::Row { "row" } else { "column" };
            self.status.set(format!(
                "Inserted {count} {noun}{}",
                if count == 1 { "" } else { "s" }
            ));
        }
    }

    /// Delete the rows (or columns) the selection covers.
    pub fn delete_selected(&self, dim: Dim) {
        let (at, count) = self.selected_span(dim);
        let sheet = self.sheet();
        if self.whole_book_step(|e| e.delete(sheet, dim, at, count)) {
            let noun = if dim == Dim::Row { "row" } else { "column" };
            self.status.set(format!(
                "Deleted {count} {noun}{}",
                if count == 1 { "" } else { "s" }
            ));
        }
    }

    // ---- formats ----

    /// Change the format of every cell of the selection (one undo step): `change` is given each cell's current format and
    /// edits it. A selection too big to do cell by cell (a whole column) changes the cells that exist.
    pub fn format_selection(&self, change: impl Fn(&mut Style)) {
        self.format_edges(|s, _| change(s));
    }

    /// Like `format_selection`, but `change` also learns which edges of the selection the cell is on.
    pub fn format_edges(&self, change: impl Fn(&mut Style, Edges)) {
        let sel = self.selection();
        let sheet = self.sheet();
        let cells: Vec<CellRef> = {
            let doc = self.doc.borrow();
            let Some(s) = doc.engine.workbook().sheets.get(sheet) else {
                return;
            };
            if u64::from(sel.rows()) * u64::from(sel.cols()) <= MAX_FORMAT_CELLS {
                sel.cells().collect()
            } else {
                let mut at: Vec<CellRef> = s
                    .iter()
                    .map(|(c, _)| c)
                    .chain(s.styled().map(|(c, _)| c))
                    .filter(|c| sel.contains(*c))
                    .collect();
                at.sort_unstable();
                at.dedup();
                at
            }
        };
        let mut made: HashMap<(u32, Edges), u32> = HashMap::new();
        let edits: Vec<(CellRef, CellState)> = {
            let mut doc = self.doc_mut();
            cells
                .into_iter()
                .map(|at| {
                    let mut state = state_of(&doc.engine, sheet, at);
                    let edges = Edges {
                        top: at.row == sel.start.row,
                        bottom: at.row == sel.end.row,
                        left: at.col == sel.start.col,
                        right: at.col == sel.end.col,
                    };
                    let new_id = *made.entry((state.style, edges)).or_insert_with(|| {
                        let mut style = doc
                            .engine
                            .workbook()
                            .styles
                            .get(state.style as usize)
                            .cloned()
                            .unwrap_or_default();
                        change(&mut style, edges);
                        doc.engine.style_id(&style)
                    });
                    state.style = new_id;
                    (at, state)
                })
                .collect()
        };
        // Whole columns (or rows) are given the format too, so that empty cells in them look the same and a cell typed there
        // starts with it. That is one step, kept as a whole workbook.
        let whole_cols = sel.start.row == 0 && sel.end.row == ROWS - 1;
        let whole_rows = sel.start.col == 0 && sel.end.col == COLS - 1;
        if whole_cols || whole_rows {
            self.whole_book_step(|e| {
                for (at, state) in &edits {
                    write(e, sheet, *at, state);
                }
                let (first, last) = if whole_cols {
                    (sel.start.col, sel.end.col)
                } else {
                    (sel.start.row, sel.end.row)
                };
                for i in first..=last {
                    let current = e.workbook().sheets.get(sheet).map_or(0, |s| {
                        if whole_cols {
                            s.col_style(i)
                        } else {
                            s.row_style(i)
                        }
                    });
                    let mut style = e
                        .workbook()
                        .styles
                        .get(current as usize)
                        .cloned()
                        .unwrap_or_default();
                    let edges = Edges {
                        top: true,
                        bottom: true,
                        left: i == first,
                        right: i == last,
                    };
                    change(&mut style, edges);
                    let id = e.style_id(&style);
                    e.update_sheet(sheet, |s| {
                        if whole_cols {
                            s.set_col_style(i, id)
                        } else {
                            s.set_row_style(i, id)
                        }
                    });
                }
                true
            });
        } else {
            self.edit(sheet, edits);
        }
        self.refocus();
    }

    #[doc(hidden)]
    pub fn press(&self, at: CellRef) {
        self.pointer_down(at);
    }

    #[doc(hidden)]
    pub fn set_bold_for_test(&self) {
        self.toggle_bold();
    }

    /// Bring keyboard focus back to the grid after a toolbar action.
    pub fn refocus(&self) {
        batch(|| {
            self.focus_cursor.set(true);
            self.cursor_epoch.update(|e| *e = e.wrapping_add(1));
        });
    }

    pub fn toggle_bold(&self) {
        let on = !self.active_style().bold;
        self.format_selection(move |s| s.bold = on);
    }

    pub fn toggle_italic(&self) {
        let on = !self.active_style().italic;
        self.format_selection(move |s| s.italic = on);
    }

    pub fn toggle_underline(&self) {
        let on = !self.active_style().underline;
        self.format_selection(move |s| s.underline = on);
    }

    pub fn toggle_wrap(&self) {
        let on = !self.active_style().wrap;
        self.format_selection(move |s| s.wrap = on);
    }

    /// Align the selection; asking for the alignment it already has turns it off.
    pub fn set_align(&self, align: Align) {
        let now = self.active_style().align;
        let target = if now == align { Align::General } else { align };
        self.format_selection(move |s| s.align = target);
    }

    pub fn set_color(&self, color: Option<u32>) {
        self.format_selection(move |s| s.color = color);
    }

    pub fn set_fill(&self, fill: Option<u32>) {
        self.format_selection(move |s| s.fill = fill);
    }

    pub fn set_number_format(&self, format: &str) {
        let format = format.to_string();
        self.format_selection(move |s| s.number_format.clone_from(&format));
    }

    /// One more (or fewer) decimal place in every selected cell, each going by its own format.
    pub fn more_decimals(&self, delta: i32) {
        self.format_selection(move |s| {
            s.number_format = crate::format::more_decimals(&s.number_format, delta);
        });
    }

    /// Back to the plain format (colours, fonts, borders, alignment and number format).
    pub fn clear_formatting(&self) {
        self.format_selection(|s| {
            *s = Style::default();
        });
    }

    /// Text size in half points for the selection (`None` for the default); rows without a height of their own grow to fit.
    pub fn set_font_size(&self, half_points: Option<u16>) {
        self.format_selection(move |s| s.font_size = half_points);
        let Some(half) = half_points else { return };
        let want = (f64::from(half) / 2.0 * 4.0 / 3.0 * 1.45).ceil();
        let sel = self.selection();
        let sheet = self.sheet();
        if want > ROW_H && sel.rows() <= 10_000 {
            let mut doc = self.doc_mut();
            for row in sel.start.row..=sel.end.row {
                let own = doc
                    .engine
                    .workbook()
                    .sheets
                    .get(sheet)
                    .and_then(|s| s.row_height(row));
                if own.is_none_or(|h| h < want) {
                    doc.engine.set_row_height(sheet, row, Some(want));
                }
            }
            drop(doc);
            self.refresh_axes();
            self.bump();
        }
    }

    /// Font family for the selection (`None` for the default).
    pub fn set_font_name(&self, name: Option<&str>) {
        let name = name.map(str::to_string);
        self.format_selection(move |s| s.font_name.clone_from(&name));
    }

    /// Draw lines around or inside the selection.
    pub fn set_borders(&self, mode: BorderMode) {
        let line = |kind| Some(Line { kind, color: None });
        self.format_edges(move |s, e| match mode {
            BorderMode::None => s.border = Border::default(),
            BorderMode::All => {
                let l = line(LineKind::Thin);
                s.border = Border {
                    top: l,
                    bottom: l,
                    left: l,
                    right: l,
                };
            }
            BorderMode::Outside | BorderMode::ThickOutside => {
                let l = line(if mode == BorderMode::Outside {
                    LineKind::Thin
                } else {
                    LineKind::Thick
                });
                if e.top {
                    s.border.top = l;
                }
                if e.bottom {
                    s.border.bottom = l;
                }
                if e.left {
                    s.border.left = l;
                }
                if e.right {
                    s.border.right = l;
                }
            }
            BorderMode::Top => {
                if e.top {
                    s.border.top = line(LineKind::Thin);
                }
            }
            BorderMode::Bottom => {
                if e.bottom {
                    s.border.bottom = line(LineKind::Thin);
                }
            }
            BorderMode::Left => {
                if e.left {
                    s.border.left = line(LineKind::Thin);
                }
            }
            BorderMode::Right => {
                if e.right {
                    s.border.right = line(LineKind::Thin);
                }
            }
            BorderMode::DoubleBottom => {
                if e.bottom {
                    s.border.bottom = line(LineKind::Double);
                }
            }
        });
    }

    // ---- the clipboard ----

    /// What a copy of the selection puts on the clipboard: `(text, html)`, remembering the cells for pasting here.
    /// `None` when there is nothing there to copy.
    #[must_use]
    pub fn copy(&self, cut: bool) -> Option<(String, String)> {
        let sel = self.selection();
        let sheet = self.sheet();
        let doc = self.doc.borrow();
        let used = doc.engine.workbook().sheets.get(sheet)?.used_range()?;
        if sel.start.row > used.end.row || sel.start.col > used.end.col {
            return None;
        }
        let end = CellRef::new(sel.end.row.min(used.end.row), sel.end.col.min(used.end.col));
        let range = Range::new(sel.start, end);
        let (mut text_rows, mut cells) = (Vec::new(), Vec::new());
        for r in range.start.row..=range.end.row {
            let (mut texts, mut states) = (Vec::new(), Vec::new());
            for c in range.start.col..=range.end.col {
                let at = CellRef::new(r, c);
                texts.push(display_in(&doc.engine, sheet, at));
                states.push(state_of(&doc.engine, sheet, at));
            }
            text_rows.push(texts);
            cells.push(states);
        }
        drop(doc);
        let (text, html) = (
            clipboard::to_text(&text_rows),
            clipboard::to_html(&text_rows),
        );
        self.inner.borrow_mut().clip = Some(Clip {
            text: text.clone(),
            sheet,
            origin: range.start,
            cells,
            cut: cut.then_some((sheet, range)),
        });
        Some((text, html))
    }

    /// Paste text at the selection. Text that was copied here keeps its formulas (moved) and formats; other text is
    /// read as tab-separated values. A single copied cell fills a larger selection. One undo step.
    pub fn paste(&self, text: &str) -> bool {
        let sel = self.selection();
        let sheet = self.sheet();
        if self.protection().is_some() {
            // Anywhere the paste could land must be unlocked.
            let reach = Range::new(
                sel.start,
                CellRef::new(
                    (sel.start.row + 1000).min(ROWS - 1),
                    (sel.start.col + 100).min(COLS - 1),
                ),
            );
            if self.protected_cells(sheet, reach.cells().take(20_000))
                && self.protected_cells(sheet, std::iter::once(sel.start))
            {
                self.status.set(crate::i18n::tr(
                    "The sheet is protected: locked cells cannot be changed",
                ));
                return false;
            }
        }
        let internal = self.inner.borrow().clip.clone().filter(|c| c.text == text);
        let cells: Vec<Vec<CellState>> = internal.as_ref().map_or_else(
            || {
                clipboard::from_text(text)
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|f| CellState {
                                content: content_of(f),
                                style: u32::MAX,
                            })
                            .collect()
                    })
                    .collect()
            },
            |c| c.cells.clone(),
        );
        let height = cells.len();
        let width = cells.iter().map(Vec::len).max().unwrap_or(0);
        if height == 0 || width == 0 || height.saturating_mul(width) > MAX_PASTE_CELLS {
            return false;
        }
        // One copied cell is repeated over a bigger selection.
        let tile = height == 1 && width == 1 && (sel.rows() > 1 || sel.cols() > 1);
        let (rows, cols) = if tile {
            (sel.rows() as usize, sel.cols() as usize)
        } else {
            (height, width)
        };
        let origin = internal.as_ref().map_or(CellRef::new(0, 0), |c| c.origin);
        let mut edits = Vec::new();
        let source_cleared = internal.as_ref().and_then(|c| c.cut);
        if let Some((from_sheet, range)) = source_cleared {
            for at in range.cells() {
                edits.push((from_sheet, at, CellState::default()));
            }
        }
        for i in 0..rows {
            for j in 0..cols {
                let (si, sj) = if tile { (0, 0) } else { (i, j) };
                let Some(src) = cells.get(si).and_then(|r| r.get(sj)) else {
                    continue;
                };
                let (Ok(di), Ok(dj)) = (u32::try_from(i), u32::try_from(j)) else {
                    continue;
                };
                let (row, col) = (sel.start.row + di, sel.start.col + dj);
                if row >= ROWS || col >= COLS {
                    continue;
                }
                let at = CellRef::new(row, col);
                let mut cell = src.clone();
                match &internal {
                    Some(_) => {
                        let from = CellRef::new(
                            origin.row + u32::try_from(si).unwrap_or(0),
                            origin.col + u32::try_from(sj).unwrap_or(0),
                        );
                        cell.content = cell.content.map(|c| {
                            moved(
                                &c,
                                i64::from(at.row) - i64::from(from.row),
                                i64::from(at.col) - i64::from(from.col),
                            )
                        });
                    }
                    None => cell.style = self.engine().style(sheet, at), // typed text keeps the format of the cell it lands on
                }
                edits.push((sheet, at, cell));
            }
        }
        // One step for the whole paste, even when a cut also empties cells on another sheet.
        let mut step = Vec::new();
        for (s, at, after) in edits {
            step.extend(self.apply(s, vec![(at, after)]));
        }
        if step.is_empty() {
            return false;
        }
        {
            let mut inner = self.inner.borrow_mut();
            inner.history.push(step);
            if source_cleared.is_some() {
                inner.clip = None; // a cut is pasted once
            }
        }
        self.history_changed();
        let last = CellRef::new(
            (sel.start.row + u32::try_from(rows).unwrap_or(1) - 1).min(ROWS - 1),
            (sel.start.col + u32::try_from(cols).unwrap_or(1) - 1).min(COLS - 1),
        );
        self.select_range(Range::new(sel.start, last));
        true
    }

    /// A text field used a key; the grid must not use it again when it bubbles up.
    pub fn swallow_key(&self) {
        self.swallow.set(true);
    }

    /// Whether the grid should skip the key it is seeing: a field used it, or a field is being typed in.
    #[must_use]
    pub fn skip_key(&self) -> bool {
        self.swallow.replace(false) || self.editing.get_untracked()
    }

    // ---- sizes ----

    /// Rebuild the positions of columns and rows from the sheet on screen.
    pub fn refresh_axes(&self) {
        let sheet = self.sheet();
        let axes = {
            let doc = self.doc.borrow();
            let e = &doc.engine;
            let s = e.workbook().sheets.get(sheet);
            let cols = Axis::new(
                COL_W,
                COLS,
                s.map(|s| {
                    s.col_widths()
                        .chain(s.hidden_cols().map(|c| (c, 0.0)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            );
            // Rows with wrapped text and no height of their own grow to hold it.
            let mut heights: Vec<(u32, f64)> = Vec::new();
            if let Some(s) = s {
                let mut wanted: HashMap<u32, f64> = HashMap::new();
                for (at, id) in s.styled() {
                    let style = e.workbook().styles.get(id as usize);
                    let Some(style) = style.filter(|st| st.wrap) else {
                        continue;
                    };
                    if s.row_height(at.row).is_some() || s.row_hidden(at.row) {
                        continue;
                    }
                    let value = e.value(sheet, at);
                    let text = match &value {
                        Value::Empty => continue,
                        v @ Value::Number(_) => {
                            sheets_format::format_value(&style.number_format, v)
                        }
                        other => other.to_string(),
                    };
                    let width = (cols.size(at.col) - 12.0).max(CHAR_PX);
                    let scale = style
                        .font_size
                        .map_or(1.0, |h| f64::from(h) / 2.0 * 4.0 / 3.0 / 14.0);
                    let per_line = (width / (CHAR_PX * scale)).floor().max(1.0);
                    let lines: f64 = text
                        .lines()
                        .map(|l| (l.chars().count() as f64 / per_line).ceil().max(1.0))
                        .sum::<f64>()
                        .max(1.0);
                    let want = (lines * 17.5 * scale + 6.0).ceil();
                    let entry = wanted.entry(at.row).or_insert(ROW_H);
                    *entry = entry.max(want);
                }
                heights.extend(wanted.into_iter().filter(|(_, h)| *h > ROW_H));
                heights.extend(s.row_heights());
                heights.extend(s.hidden_rows().map(|r| (r, 0.0)));
            }
            Axes {
                rows: Axis::new(ROW_H, ROWS, heights),
                cols,
                frozen: s.map_or((0, 0), sheets_model::Sheet::freeze),
            }
        };
        self.axes.set(Rc::new(axes));
    }

    /// A header border was pressed: `row` for a row border, `index` the item above or left of it, `pointer` the pointer's
    /// position along the axis.
    /// Zoom in (`1`) or out (`-1`) one step; `0` goes back to 100 percent.
    pub fn zoom_by(&self, step: i32) {
        const STEPS: [u32; 10] = [50, 67, 80, 90, 100, 110, 125, 150, 200, 300];
        let now = self.zoom.get_untracked();
        let at = STEPS.iter().position(|z| *z >= now).unwrap_or(4);
        let to = if step == 0 {
            4
        } else {
            usize::try_from((i32::try_from(at).unwrap_or(4) + step).clamp(0, 9)).unwrap_or(4)
        };
        self.zoom.set(STEPS[to]);
    }

    /// How much bigger than the sheet's own pixels the grid is drawn (1.0 at 100 percent).
    #[must_use]
    pub fn zoom_factor(&self) -> f64 {
        f64::from(self.zoom.get_untracked()) / 100.0
    }

    pub fn begin_resize(&self, row: bool, index: u32, pointer: f64) {
        let axes = self.axes.get_untracked();
        let size = if row {
            axes.rows.size(index)
        } else {
            axes.cols.size(index)
        };
        self.resizing.set(Some(Resize {
            row,
            index,
            start: pointer,
            size,
        }));
        self.resized.set(false);
    }

    /// The pointer moved while a border is held: the item takes the size that follows it.
    pub fn resize_to(&self, pointer: f64) {
        let Some(r) = self.resizing.get() else { return };
        let (min, max) = if r.row {
            (MIN_ROW_H, MAX_ROW_H)
        } else {
            (MIN_COL_W, MAX_COL_W)
        };
        let size = (r.size + (pointer - r.start) / self.zoom_factor())
            .clamp(min, max)
            .round();
        let sheet = self.sheet();
        let current = {
            let a = self.axes.get_untracked();
            if r.row {
                a.rows.size(r.index)
            } else {
                a.cols.size(r.index)
            }
        };
        if (size - current).abs() < f64::EPSILON {
            return;
        }
        self.resized.set(true);
        self.set_size(sheet, r.row, r.index, Some(size));
    }

    /// The pointer moved anywhere over the screen: follows a border that is held (`x` for a column, `y` for a row).
    pub fn resize_pointer(&self, x: f64, y: f64) {
        if let Some(r) = self.resizing.get() {
            self.resize_to(if r.row { y } else { x });
        }
    }

    pub fn end_resize(&self) {
        self.resizing.set(None);
    }

    /// Whether a click that arrives now is the end of a resize drag and should be ignored. Asking forgets it.
    #[must_use]
    pub fn take_resized(&self) -> bool {
        self.resized.replace(false)
    }

    /// Forget a pending resize drag (a new press has started).
    pub fn clear_resized(&self) {
        self.resized.set(false);
    }

    fn set_size(&self, sheet: usize, row: bool, index: u32, size: Option<f64>) {
        {
            let mut doc = self.doc_mut();
            if row {
                doc.engine.set_row_height(sheet, index, size);
            } else {
                doc.engine.set_col_width(sheet, index, size);
            }
        }
        self.refresh_axes();
    }

    /// Make a column as wide as what is in it (a double click on its border); a row goes back to the default height.
    pub fn autofit(&self, row: bool, index: u32) {
        let sheet = self.sheet();
        if row {
            self.set_size(sheet, true, index, None);
            return;
        }
        let longest = {
            let doc = self.doc.borrow();
            let Some(s) = doc.engine.workbook().sheets.get(sheet) else {
                return;
            };
            s.iter()
                .filter(|(c, _)| c.col == index)
                .map(|(c, _)| display_in(&doc.engine, sheet, c).chars().count())
                .max()
                .unwrap_or(0)
        };
        #[allow(clippy::cast_precision_loss)]
        let width = (longest as f64)
            .mul_add(7.5, 16.0)
            .clamp(MIN_COL_W, MAX_COL_W)
            .round();
        self.set_size(
            sheet,
            false,
            index,
            if longest == 0 { None } else { Some(width) },
        );
    }

    // ---- sheets ----

    /// Show another sheet.
    pub fn switch_sheet(&self, index: usize) {
        if index >= self.sheet_names().len() {
            return;
        }
        batch(|| {
            self.active.set(index);
            self.refresh_axes();
            self.bump();
            self.select(CellRef::new(0, 0));
        });
    }

    /// Add an empty sheet called "Sheet<n>" and show it.
    pub fn add_sheet(&self) {
        let names = self.sheet_names();
        let name = (names.len() + 1..=names.len() + 10_000)
            .map(|n| format!("Sheet{n}"))
            .find(|n| !names.iter().any(|e| e.eq_ignore_ascii_case(n)))
            .unwrap_or_default();
        let added = self.doc_mut().add_sheet(&name);
        if let Some(i) = added {
            self.switch_sheet(i);
        }
    }

    // ---- files ----

    /// Start a new empty workbook.
    pub fn new_file(&self) {
        self.replace(Xlsx::new(Workbook::new()), "Untitled.xlsx");
        self.status.set(String::new());
    }

    /// Open the first file the person drops on the page.
    pub fn accept_drops(&self) {
        let state = self.clone();
        platform::on_drop(move |files| {
            if let Some(file) = files.into_iter().next() {
                state.load_file(&file);
            }
        });
    }

    /// Ask the person for a file to open (`.xlsx`, `.csv`, `.tsv`). Call it from a click.
    pub fn open_file(&self) {
        let state = self.clone();
        platform::pick_files(".xlsx,.ods,.xls,.csv,.tsv,.txt", false, move |files| {
            if let Some(file) = files.into_iter().next() {
                state.load_file(&file);
            }
        });
    }

    /// Read a picked file into the sheet; a file that cannot be read is reported and the workbook stays as it was. A big file
    /// is read a moment later, so that the screen can first say it is being opened.
    pub fn load_file(&self, file: &PickedFile) {
        if file.bytes.len() > BIG_FILE {
            self.status.set(format!(
                "Opening {} ({} MB)\u{2026}",
                file.name,
                file.bytes.len().div_ceil(1_000_000)
            ));
            let (state, file) = (self.clone(), file.clone());
            // The timer is kept running by the platform; it need not be held.
            let _ = platform::after(40, move || state.load_file_now(&file));
            return;
        }
        self.load_file_now(file);
    }

    fn load_file_now(&self, file: &PickedFile) {
        let extension = std::path::Path::new(&file.name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_lowercase);
        let read = match extension.as_deref() {
            Some("xlsx") => sheets_xlsx::read(&file.bytes).map_err(|e| e.to_string()),
            Some("ods") => sheets_ods::read(&file.bytes)
                .map(Xlsx::new)
                .map_err(|e| e.to_string()),
            Some("xls") => sheets_xls::read(&file.bytes)
                .map(Xlsx::new)
                .map_err(|e| e.to_string()),
            Some(kind @ ("csv" | "tsv" | "txt")) => {
                let delimiter = if kind == "tsv" { b'\t' } else { b',' };
                let options = sheets_csv::ReadOptions {
                    delimiter,
                    ..sheets_csv::ReadOptions::default()
                };
                let stem = file
                    .name
                    .rsplit_once('.')
                    .map_or(file.name.as_str(), |(s, _)| s);
                sheets_csv::read(&String::from_utf8_lossy(&file.bytes), stem, &options)
                    .map(Xlsx::new)
                    .map_err(|e| e.to_string())
            }
            _ => Err(crate::i18n::tr(
                "only .xlsx, .ods, .xls, .csv and .tsv files can be opened",
            )),
        };
        match read {
            Ok(xlsx) => {
                self.replace(xlsx, &file.name);
                self.status.set(format!("Opened {}", file.name));
            }
            Err(e) => self
                .status
                .set(format!("Could not open {}: {e}", file.name)),
        }
    }

    fn replace(&self, xlsx: Xlsx, name: &str) {
        *self.doc.borrow_mut() = xlsx;
        // A file loaded is the baseline: nothing is waiting to be sent.
        *self.track.borrow_mut() = Track::default();
        self.apply_clock();
        *self.inner.borrow_mut() = Inner::default();
        self.history_changed();
        batch(|| {
            self.name.set(name.to_string());
            self.active.set(0);
            self.refresh_axes();
            self.bump();
            self.select(CellRef::new(0, 0));
        });
    }

    fn stem(&self) -> String {
        let name = self.name.get_untracked();
        name.rsplit_once('.')
            .map_or_else(|| name.clone(), |(s, _)| s.to_string())
    }

    /// Offer the workbook as a `.xlsx` to save.
    pub fn save_xlsx(&self) {
        match self.xlsx_bytes() {
            Ok(bytes) => {
                let name = format!("{}.xlsx", self.stem());
                platform::download(&name, XLSX_MIME, &bytes);
                self.name.set(name.clone());
                self.changed();
                self.status.set(format!("Saved {name}"));
            }
            Err(e) => self.status.set(format!("Could not save: {e}")),
        }
    }

    /// Offer the workbook as an `OpenDocument` `.ods` to save.
    pub fn save_ods(&self) {
        match sheets_ods::write(&self.engine()) {
            Ok(bytes) => {
                let name = format!("{}.ods", self.stem());
                platform::download(
                    &name,
                    "application/vnd.oasis.opendocument.spreadsheet",
                    &bytes,
                );
                self.name.set(name.clone());
                self.status.set(format!("Saved {name}"));
            }
            Err(e) => self.status.set(format!("Could not save: {e}")),
        }
    }

    /// Offer the sheet on screen as a `.csv` to save, as it shows.
    pub fn save_csv(&self) {
        self.save_text(b',', "csv", "text/csv;charset=utf-8");
    }

    /// Offer the sheet on screen as a `.tsv` to save.
    pub fn save_tsv(&self) {
        self.save_text(b'\t', "tsv", "text/tab-separated-values;charset=utf-8");
    }

    fn save_text(&self, delimiter: u8, extension: &str, mime: &str) {
        let text = sheets_csv::write(
            &self.engine(),
            self.sheet(),
            &sheets_csv::WriteOptions {
                delimiter,
                ..sheets_csv::WriteOptions::default()
            },
        );
        let name = format!("{}.{extension}", self.stem());
        platform::download(&name, mime, text.as_bytes());
        self.status.set(format!("Saved {name}"));
    }

    /// Offer every sheet as a `.pdf` to save.
    pub fn export_pdf(&self) {
        let bytes = sheets_print::to_pdf(&self.engine(), &sheets_print::Which::All, &self.stem());
        let name = format!("{}.pdf", self.stem());
        platform::download(&name, "application/pdf", &bytes);
        self.status.set(format!("Exported {name}"));
    }

    /// Show the sheet on screen as pages in a new tab, where the viewer's print button prints it. Call it from a click.
    pub fn print(&self) {
        let bytes = sheets_print::to_pdf(
            &self.engine(),
            &sheets_print::Which::One(self.sheet()),
            &self.stem(),
        );
        platform::open_in_tab("application/pdf", &bytes);
        self.status.set(crate::i18n::tr(
            "Opened the pages in a new tab: print them from there",
        ));
    }
}

/// What a cell shows: the value under its number format.
fn display_in(e: &Engine, sheet: usize, at: CellRef) -> String {
    match e.value(sheet, at) {
        Value::Empty => String::new(),
        v @ Value::Number(_) => {
            sheets_format::format_value(e.workbook().number_format(sheet, at), &v)
        }
        other => other.to_string(),
    }
}

fn state_of(e: &Engine, sheet: usize, at: CellRef) -> CellState {
    CellState {
        content: e.content(sheet, at).cloned(),
        style: e.style(sheet, at),
    }
}

fn write(e: &mut Engine, sheet: usize, at: CellRef, state: &CellState) {
    match &state.content {
        Some(c) => {
            e.set(sheet, at, c.clone());
        }
        None => {
            e.clear(sheet, at);
        }
    }
    e.set_style(sheet, at, state.style);
}

/// What typed text becomes in a cell; nothing for an empty field.
fn content_of(text: &str) -> Option<Content> {
    match parse_input(text) {
        Content::Value(Value::Empty) => None,
        c => Some(c),
    }
}

/// A formula as it reads when copied `rows` down and `cols` right; other content as it is.
fn moved(content: &Content, rows: i64, cols: i64) -> Content {
    match content {
        Content::Formula(f) => {
            Content::Formula(parse(f).map_or_else(|_| f.clone(), |e| print(&e.shifted(rows, cols))))
        }
        Content::Value(_) => content.clone(),
    }
}

/// About how wide a character is on screen, in pixels, at the default size.
pub const CHAR_PX: f64 = 7.5;

/// About how wide a text is on screen: its longest line.
#[must_use]
pub fn text_px(text: &str, size_half: Option<u16>) -> f64 {
    let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    let factor = size_half.map_or(1.0, |h| f64::from(h) / 2.0 * 4.0 / 3.0 / 14.0);
    longest as f64 * CHAR_PX * factor
}

type FindKey = (String, bool, bool, bool);
type FindCache = Option<(FindKey, Option<Rc<find::Matcher>>)>;

#[derive(Default)]
struct CfCache {
    key: Option<(u64, usize)>,
    rules: Vec<sheets_model::CondRule>,
    stats: Vec<crate::rules::Stats>,
}

/// Which edges of the selected rectangle a cell is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Edges {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// What the borders menu can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderMode {
    All,
    Outside,
    ThickOutside,
    Top,
    Bottom,
    Left,
    Right,
    DoubleBottom,
    None,
}

/// A source of the time of day, as a serial number.
type Clock = Rc<dyn Fn() -> f64>;

/// A callback told of changes.
type Hook = Rc<dyn Fn()>;

/// The least a sheet name must be to be saved in a workbook, as a message when it is not.
fn check_sheet_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(crate::i18n::tr("A sheet needs a name"));
    }
    if name.chars().count() > 31 {
        return Err(crate::i18n::tr("A sheet name has at most 31 characters"));
    }
    if name.contains(['[', ']', ':', '*', '?', '/', '\\']) {
        return Err(crate::i18n::tr(
            "A sheet name cannot contain [ ] : * ? / \\",
        ));
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return Err(crate::i18n::tr(
            "A sheet name cannot start or end with an apostrophe",
        ));
    }
    Ok(())
}

impl SheetState {
    // ---- find and replace ----

    /// Open the find bar (with the replace field first for Ctrl+H). A single selected cell's text becomes the search.
    pub fn open_find(&self, replace: bool) {
        batch(|| {
            let sel = self.selection();
            if sel.start == sel.end && self.find_query.get_untracked().is_empty() {
                let text = self.edit_text(sel.start);
                if !text.is_empty() && text.chars().count() <= 60 && !text.contains('\n') {
                    self.find_query.set(text);
                }
            }
            self.replace_first.set(replace);
            self.find_open.set(true);
            self.find_epoch.update(|e| *e = e.wrapping_add(1));
        });
    }

    pub fn close_find(&self) {
        self.find_open.set(false);
        self.refocus();
    }

    /// The matcher for what the find bar holds now (kept until the query or an option changes); `None` for an empty query or
    /// a pattern that does not parse (the reason is in `find_problem`).
    fn matcher(&self) -> Option<Rc<find::Matcher>> {
        let key = (
            self.find_query.get_untracked(),
            self.match_case.get_untracked(),
            self.whole_cell.get_untracked(),
            self.find_regex.get_untracked(),
        );
        if key.0.is_empty() {
            return None;
        }
        let mut cache = self.find_cache.borrow_mut();
        if cache.as_ref().is_none_or(|(k, _)| *k != key) {
            let built = find::Matcher::new(&key.0, key.1, key.2, key.3);
            let problem = built.as_ref().err().cloned().unwrap_or_default();
            self.find_problem.set(problem);
            *cache = Some((key, built.ok().map(Rc::new)));
        }
        cache.as_ref().and_then(|(_, m)| m.clone())
    }

    /// Builds the matcher so that a bad pattern is reported (for the find bar's count).
    pub fn matcher_ready(&self) -> bool {
        self.matcher().is_some()
    }

    fn cell_text(&self, at: CellRef) -> String {
        self.edit_text(at)
    }

    /// Whether the text of this cell (what was typed in it, formulas included) is what the find bar looks for.
    #[must_use]
    pub fn is_found(&self, at: CellRef) -> bool {
        self.matcher()
            .is_some_and(|m| m.matches(&self.cell_text(at)))
    }

    /// The sheets a search covers, the one on screen first.
    fn search_sheets(&self) -> Vec<usize> {
        let (here, n) = (self.sheet(), self.sheet_names().len());
        if self.find_all_sheets.get_untracked() {
            (0..n).map(|i| (here + i) % n).collect()
        } else {
            vec![here]
        }
    }

    /// How many cells match (on every sheet when the search covers them); `None` for a workbook too big to count on every
    /// keystroke.
    #[must_use]
    pub fn find_count(&self) -> Option<usize> {
        let Some(m) = self.matcher() else {
            return Some(0);
        };
        let doc = self.doc.borrow();
        let wb = doc.engine.workbook();
        let sheets = self.search_sheets();
        let total: usize = sheets
            .iter()
            .filter_map(|i| wb.sheets.get(*i))
            .map(sheets_model::Sheet::len)
            .sum();
        (total <= 200_000).then(|| {
            sheets
                .iter()
                .filter_map(|i| wb.sheets.get(*i))
                .map(|s| {
                    s.iter()
                        .filter(|(_, c)| m.matches(&edit_text(Some(c))))
                        .count()
                })
                .sum()
        })
    }

    /// Select the next (or previous) cell that matches, wrapping round the sheet (or all the sheets). Returns whether there
    /// was one.
    #[allow(clippy::must_use_candidate)]
    pub fn find_next(&self, forward: bool) -> bool {
        let Some(m) = self.matcher() else {
            return false;
        };
        let here = self.selected.get_untracked();
        let sheets = self.search_sheets();
        let first = self.sheet();
        let hit: Option<(usize, CellRef)> = {
            let doc = self.doc.borrow();
            let wb = doc.engine.workbook();
            let test = |c: &Content| m.matches(&edit_text(Some(c)));
            // The rest of this sheet, then each of the others, then this sheet from its start again (which finds the match the
            // search began at when it is the only one).
            let order: Vec<usize> = if forward {
                sheets.clone()
            } else {
                sheets.iter().rev().copied().collect()
            };
            let mut found = None;
            for (step, sheet) in order.iter().copied().enumerate() {
                let Some(s) = wb.sheets.get(sheet) else {
                    continue;
                };
                let on_first = sheet == first;
                let candidate = if forward {
                    if on_first && step == 0 {
                        s.iter_after(here).find(|(_, c)| test(c)).map(|(at, _)| at)
                    } else {
                        s.iter().find(|(_, c)| test(c)).map(|(at, _)| at)
                    }
                } else if on_first && step == 0 {
                    s.iter_before(here).find(|(_, c)| test(c)).map(|(at, _)| at)
                } else {
                    s.iter().filter(|(_, c)| test(c)).last().map(|(at, _)| at)
                };
                if let Some(at) = candidate {
                    found = Some((sheet, at));
                    break;
                }
            }
            // Round to where the search began: this sheet from its start (or, backwards, from its end).
            if found.is_none()
                && let Some(s) = wb.sheets.get(first)
            {
                found = if forward {
                    s.iter().find(|(_, c)| test(c)).map(|(at, _)| (first, at))
                } else {
                    s.iter()
                        .filter(|(_, c)| test(c))
                        .last()
                        .map(|(at, _)| (first, at))
                };
            }
            found
        };
        hit.is_some_and(|(sheet, at)| {
            if sheet != self.sheet() {
                self.switch_sheet(sheet);
            }
            self.select(at);
            true
        })
    }

    /// Replace in the selected cell if it matches, then go on to the next match.
    pub fn replace_current(&self) {
        let at = self.selected.get_untracked();
        if let Some(m) = self.matcher()
            && let Some(text) = m.replace(&self.cell_text(at), &self.replace_with.get_untracked())
        {
            self.set_text(at, &text);
        }
        self.find_next(true);
    }

    /// Replace in every matching cell (one undo step). Returns how many cells changed.
    #[allow(clippy::must_use_candidate)]
    pub fn replace_all(&self) -> usize {
        let query = self.find_query.get_untracked();
        let Some(m) = self.matcher() else {
            self.status
                .set(if self.find_problem.get_untracked().is_empty() {
                    "Nothing to replace".to_string()
                } else {
                    format!("Not a valid pattern: {}", self.find_problem.get_untracked())
                });
            return 0;
        };
        let with = self.replace_with.get_untracked();
        let sheets = self.search_sheets();
        let plan: Vec<(usize, Vec<(CellRef, CellState)>)> = {
            let doc = self.doc.borrow();
            let wb = doc.engine.workbook();
            sheets
                .iter()
                .filter_map(|i| wb.sheets.get(*i).map(|s| (*i, s)))
                .map(|(i, s)| {
                    let edits = s
                        .iter()
                        .filter_map(|(at, c)| {
                            let text = m.replace(&edit_text(Some(c)), &with)?;
                            Some((
                                at,
                                CellState {
                                    content: content_of(&text),
                                    style: s.style(at),
                                },
                            ))
                        })
                        .collect();
                    (i, edits)
                })
                .collect()
        };
        let count: usize = plan.iter().map(|(_, e)| e.len()).sum();
        if count > 0 {
            if plan.iter().filter(|(_, e)| !e.is_empty()).count() == 1 {
                if let Some((sheet, edits)) = plan.into_iter().find(|(_, e)| !e.is_empty()) {
                    self.edit(sheet, edits);
                }
            } else {
                // Cells on several sheets are one step by keeping the workbook as it was.
                self.whole_book_step(|e| {
                    for (sheet, edits) in &plan {
                        for (at, state) in edits {
                            write(e, *sheet, *at, state);
                        }
                    }
                    true
                });
            }
        }
        self.status.set(match count {
            0 => format!("Nothing to replace for \"{query}\""),
            1 => "Replaced in 1 cell".to_string(),
            n => format!("Replaced in {n} cells"),
        });
        self.load_draft();
        count
    }

    // ---- moving about ----

    fn has_content(&self, at: CellRef) -> bool {
        self.engine()
            .content(self.sheet(), at)
            .is_some_and(|c| !matches!(c, Content::Value(Value::Empty)))
    }

    /// Where Ctrl+arrow goes: to the end of a block of cells, or to the next block, or to the edge of the sheet.
    #[must_use]
    pub fn jump_target(&self, from: CellRef, rows: i64, cols: i64) -> CellRef {
        let at = |r: i64, c: i64| {
            (r >= 0 && c >= 0 && r < i64::from(ROWS) && c < i64::from(COLS))
                .then(|| CellRef::new(u32::try_from(r).unwrap_or(0), u32::try_from(c).unwrap_or(0)))
        };
        let (mut r, mut c) = (i64::from(from.row), i64::from(from.col));
        let Some(first) = at(r + rows, c + cols) else {
            return from;
        };
        if self.has_content(from) && self.has_content(first) {
            // Inside a block: to its last cell.
            while let Some(n) = at(r + rows, c + cols) {
                if !self.has_content(n) {
                    break;
                }
                (r, c) = (i64::from(n.row), i64::from(n.col));
            }
        } else {
            // On an edge or in space: to the next cell with something in it, else the edge of the sheet.
            while let Some(n) = at(r + rows, c + cols) {
                (r, c) = (i64::from(n.row), i64::from(n.col));
                if self.has_content(n) {
                    break;
                }
            }
        }
        CellRef::new(u32::try_from(r).unwrap_or(0), u32::try_from(c).unwrap_or(0))
    }

    /// Ctrl+arrow (and Ctrl+Shift+arrow to stretch the selection).
    pub fn jump(&self, rows: i64, cols: i64, extend: bool) {
        let from = if extend {
            self.range_end.get_untracked()
        } else {
            self.selected.get_untracked()
        };
        let target = self.jump_target(from, rows, cols);
        if extend {
            self.extend_to(target);
        } else {
            self.select(target);
        }
    }

    /// Ctrl+End: the last cell in use.
    pub fn select_last_used(&self) {
        let end = self
            .engine()
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(sheets_model::Sheet::used_range)
            .map(|r| r.end);
        self.select(end.unwrap_or(CellRef::new(0, 0)));
    }

    /// Ctrl+D / Ctrl+R: copy the first row (column) of the selection over the rest, formulas moved as in a copy.
    pub fn fill(&self, down: bool) {
        let sel = self.selection();
        let sheet = self.sheet();
        let span = if down {
            u64::from(sel.rows())
        } else {
            u64::from(sel.cols())
        };
        if span < 2 || u64::from(sel.rows()) * u64::from(sel.cols()) > 1_000_000 {
            return;
        }
        let mut edits = Vec::new();
        {
            let doc = self.doc.borrow();
            for other in 0..(if down { sel.cols() } else { sel.rows() }) {
                let source = if down {
                    CellRef::new(sel.start.row, sel.start.col + other)
                } else {
                    CellRef::new(sel.start.row + other, sel.start.col)
                };
                let state = state_of(&doc.engine, sheet, source);
                for step in 1..span {
                    let step = u32::try_from(step).unwrap_or(0);
                    let (dr, dc) = if down { (step, 0) } else { (0, step) };
                    let mut cell = state.clone();
                    cell.content = cell
                        .content
                        .map(|c| moved(&c, i64::from(dr), i64::from(dc)));
                    edits.push((CellRef::new(source.row + dr, source.col + dc), cell));
                }
            }
        }
        self.edit(sheet, edits);
        self.load_draft();
    }

    // ---- sheet tabs ----

    pub fn begin_rename(&self, index: usize) {
        let Some(name) = self.sheet_names().get(index).cloned() else {
            return;
        };
        batch(|| {
            self.pending_delete.set(None);
            self.rename_draft.set(name);
            self.renaming.set(Some(index));
        });
    }

    pub fn cancel_rename(&self) {
        self.renaming.set(None);
        self.refocus();
    }

    /// Rename the sheet being renamed to what was typed; a name that will not do is explained and the field stays open.
    pub fn commit_rename(&self) {
        let Some(index) = self.renaming.get_untracked() else {
            return;
        };
        let name = self.rename_draft.get_untracked().trim().to_string();
        if self.sheet_names().get(index).is_some_and(|n| *n == name) {
            self.cancel_rename();
            return;
        }
        let result = check_sheet_name(&name).and_then(|()| {
            if self.doc_mut().engine.rename_sheet(index, &name) {
                Ok(())
            } else {
                Err(format!("There is already a sheet called \"{name}\""))
            }
        });
        match result {
            Ok(()) => batch(|| {
                self.bump();
                self.renaming.set(None);
                self.status.set(format!("Renamed to {name}"));
            }),
            Err(message) => self.status.set(message),
        }
        if self.renaming.get_untracked().is_none() {
            self.refocus();
        }
    }

    pub fn ask_delete(&self, index: usize) {
        if self.sheet_names().len() > 1 {
            batch(|| {
                self.renaming.set(None);
                self.pending_delete.set(Some(index));
            });
        }
    }

    pub fn cancel_delete(&self) {
        self.pending_delete.set(None);
    }

    /// Delete the sheet that was asked about. This cannot be undone: the history is cleared, since it speaks of sheets by number.
    pub fn confirm_delete(&self) {
        let Some(index) = self.pending_delete.get_untracked() else {
            return;
        };
        let Some(name) = self.sheet_names().get(index).cloned() else {
            return;
        };
        if !self.doc_mut().remove_sheet(index) {
            return;
        }
        let last = self.sheet_names().len() - 1;
        let active = self.active.get_untracked();
        batch(|| {
            self.pending_delete.set(None);
            self.active.set(if active > index {
                active - 1
            } else {
                active.min(last)
            });
            self.refresh_axes();
            *self.inner.borrow_mut() = Inner::default();
            self.history_changed();
            self.bump();
            self.select(CellRef::new(0, 0));
            self.status.set(format!("Deleted sheet {name}"));
        });
    }

    // ---- the clock, and the workbook as a file ----

    /// Where `NOW()` and `TODAY()` get the time: a function giving the local date and time as a serial number.
    pub fn set_clock(&self, now: impl Fn() -> f64 + 'static) {
        *self.clock.borrow_mut() = Some(Rc::new(now));
        self.apply_clock();
    }

    fn apply_clock(&self) {
        let clock = self.clock.borrow().clone();
        if let Some(c) = clock {
            let mut doc = self.doc.borrow_mut();
            doc.engine.set_clock(move || c());
            doc.engine.recalc_volatile();
        }
        self.bump();
    }

    /// Call `f` whenever the workbook or its name changes (and once when the screen is built).
    pub fn on_change(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Rc::new(f));
    }

    /// The workbook changed: cells look again, and whoever asked is told.
    fn bump(&self) {
        self.rev.update(|r| *r += 1);
        self.changed();
    }

    pub(crate) fn changed(&self) {
        let hook = self.on_change.borrow().clone();
        if let Some(f) = hook {
            f();
        }
    }

    /// The workbook as `.xlsx` bytes.
    pub fn xlsx_bytes(&self) -> Result<Vec<u8>, String> {
        self.doc.borrow().write().map_err(|e| e.to_string())
    }

    /// Open `.xlsx` bytes as the workbook (a restored draft, say), under this name.
    pub fn load_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let xlsx = sheets_xlsx::read(bytes).map_err(|e| e.to_string())?;
        self.replace(xlsx, name);
        Ok(())
    }
}
