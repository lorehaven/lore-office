//! The data of a spreadsheet: cell addresses, values, errors, sheets and workbooks. Pure, no UI, no I/O; the formula
//! language lives in `sheets-formula` and the file formats in their own crates.

pub mod charts;
pub mod date;
pub mod refs;
pub mod rules;
pub mod sheet;
pub mod value;

pub use charts::{Chart, ChartKind, SparkKind, Sparkline};
pub use refs::{
    Axis, CellRef, MAX_COLS, MAX_ROWS, Range, col_index, col_name, shift_point, shift_span,
};
pub use rules::{
    Allow, Compare, Cond, CondRule, Dxf, NumberKind, Orientation, PageSetup, Paper, Severity,
    Validation,
};
pub use sheet::{
    Align, AutoFilter, Border, Content, Line, LineKind, Protection, Sheet, Style, Table, Workbook,
    custom_table_style, parse_custom_table_style,
};
pub use value::{CellError, Value};
