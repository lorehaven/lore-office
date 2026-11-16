//! lore-docs: the editor spike.
//!
//! The page is an editable element (`contenteditable`) whose children are the document's paragraphs. The browser is not allowed
//! to change it: every typed character, deletion, paste, formatting command and undo arrives as a `BeforeInput` event, is
//! applied to the model ([`docs_text::Doc`]), and the changed paragraphs are drawn again; the caret is put back where the model
//! says. An input method (IME) composes inside the browser and is read back when it ends.

#![allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::type_complexity,
    clippy::missing_const_for_fn,
    clippy::semicolon_if_nothing_returned
)]

pub mod i18n;
mod state;
mod view;

/// How the editor looks.
pub const STYLE: &str = include_str!("style.css");

pub use state::{DocState, Range};
pub use view::docs_view;
