//! The spreadsheet screen.
//!
//! [`SheetState`] holds the workbook (an engine that keeps it calculated), the selection and the text being edited; it has
//! no view in it and is tested on its own. [`sheet_view`] draws it: column and row headers, a formula bar, and a grid that
//! builds only the cells in view however large the sheet is.

// The screen works in estimated pixels and small counts, and its commands are long lists of similar steps.
#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::missing_const_for_fn,
    clippy::collapsible_if,
    clippy::needless_late_init,
    clippy::useless_let_if_seq,
    clippy::must_use_candidate,
    clippy::default_trait_access,
    clippy::semicolon_if_nothing_returned,
    clippy::option_if_let_else,
    clippy::needless_pass_by_value,
    clippy::type_complexity,
    clippy::redundant_clone
)]

mod axis;
mod clipboard;
mod completion;
mod demo;
mod find;
mod format;
mod history;
pub mod i18n;
mod input;
pub mod rules;
mod state;
pub mod sync;
mod view;

pub use axis::{Axes, Axis};
pub use clipboard::{from_text, to_html, to_text};
pub use completion::{
    accept as accept_suggestion, hint as formula_hint, suggestions as formula_suggestions,
};
pub use demo::demo_workbook;
pub use find::{matches, replace};
pub use format::{PRESETS, color_css, more_decimals, parse_color};
pub use history::{CellState, MAX_STEPS};
pub use input::{edit_text, parse_input};
pub use state::{
    BorderMode, COL_W, COLS, Dialog, DialogId, Field, FieldKind, HEADER_H, HEADER_W, MAX_COL_W,
    MAX_ROW_H, MIN_COL_W, MIN_ROW_H, PasteMode, ROW_H, ROWS, SheetState,
};
pub use view::sheet_view;

/// The screen's own style, to be served beside the framework's.
pub const STYLE: &str = include_str!("style.css");
