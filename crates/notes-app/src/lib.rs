//! lore-notes: markdown notes with a live preview, links between notes, and any note presented as a deck.
//!
//! Host-independent: the app draws with quench-ui and asks its host only for what the platform gives every app (files,
//! downloads, timers). The host keeps the vault ([`NotesState::encoded`] and [`Vault::decode`]).

#![allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::type_complexity,
    clippy::semicolon_if_nothing_returned,
    clippy::missing_const_for_fn,
    clippy::format_push_string,
    clippy::needless_pass_by_value,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::option_if_let_else
)]

mod demo;
mod export;
pub mod i18n;
pub mod math;
mod render;
mod state;
mod vault;
mod view;

/// The style sheet: how a slide looks (also in the pages [`export`] makes), then the screen's own.
pub const STYLE: &str = concat!(
    include_str!("../assets/deck.css"),
    "\n",
    include_str!("style.css")
);

pub use export::{deck_html, note_html, slides_of};
pub use state::{
    Deck, Mode, NotesState, Prompt, PromptKind, THEMES, fragment_total, fragments_shown, stage_of,
};
pub use vault::{Hit, Note, Vault, clean_name};
pub use view::notes_view;
