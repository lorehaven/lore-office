//! lore-sheets in the browser: the spreadsheet screen over an empty workbook, or the one left here last time.
//!
//! The workbook is kept in the browser's storage a moment after each change, as an `.xlsx`, and read back when the page
//! opens. Nothing leaves the machine.

use quench_ui::prelude::*;
use sheets_app::{SheetState, sheet_view};
use sheets_model::Workbook;

/// The most that is kept (characters of base64); browsers allow about 5 million per site.
const MAX_KEPT: usize = 3_500_000;

/// The page: the server renders it into the first response, the browser takes it over.
#[must_use]
pub fn app() -> View {
    let state = SheetState::new(Workbook::new());
    #[cfg(target_arch = "wasm32")]
    browser::attach(&state);
    let cloud = office_cloud_ui::setup();
    {
        let (out, back, rev) = (state.clone(), state.clone(), state.clone());
        cloud.bind(office_cloud_ui::Binding {
            kind: "sheets",
            export: std::rc::Rc::new(move || {
                let name = out.name.get_untracked();
                out.xlsx_bytes().ok().map(|b| (name, b))
            }),
            import: std::rc::Rc::new(move |name, bytes| back.load_bytes(name, bytes)),
            revision: std::rc::Rc::new(move || rev.rev.get()),
            autosave: true,
        });
    }
    #[cfg(target_arch = "wasm32")]
    if cloud.context.share.is_some() {
        cloud.open_share();
    }
    sheet_view(&state)
}

/// Days since 1899-12-30 for a moment given as milliseconds since 1970 UTC and the zone's offset in seconds.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn serial_at(unix_ms: i64, offset_seconds: i32) -> f64 {
    let local = unix_ms + i64::from(offset_seconds) * 1000;
    // 25569 days from 1899-12-30 to 1970-01-01.
    25_569.0 + local as f64 / 86_400_000.0
}

/// What is kept for a workbook: the text for storage, or `None` when it is too big to keep.
#[must_use]
pub fn keep(bytes: &[u8]) -> Option<String> {
    let text = quench_core::base64::STANDARD.encode(bytes);
    (text.len() <= MAX_KEPT).then_some(text)
}

/// The bytes of what was kept.
#[must_use]
pub fn unkeep(text: &str) -> Option<Vec<u8>> {
    quench_core::base64::STANDARD.decode(text).ok()
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{SheetState, keep, serial_at, unkeep};
    use std::cell::Cell;

    /// Where the workbook is kept, and what it was called.
    const KEY_BOOK: &str = "lore-sheets.book";
    const KEY_NAME: &str = "lore-sheets.name";
    /// How long after the last change the workbook is kept.
    const QUIET_MS: u32 = 1500;

    use std::rc::Rc;

    /// Where the language is kept: the key forge's script fills from its `qlocale` cookie.
    const KEY_LANGUAGE: &str = "language";

    pub fn attach(state: &SheetState) {
        state.accept_drops();
        if let Some(code) = quench_ui_web::browser::storage_get(KEY_LANGUAGE) {
            state.set_language(&code);
        }
        sheets_app::i18n::on_language(|code| {
            quench_ui_web::browser::storage_set(KEY_LANGUAGE, Some(code));
        });
        // Local time for NOW() and TODAY().
        let zone = quench_tz::Zone::named(&quench_ui_web::browser::time_zone()).ok();
        state.set_clock(move || {
            let ms = quench_ui_web::browser::wall_ms();
            let offset = zone.as_ref().map_or(0, |z| {
                z.offset_at(quench_core::time::Time::from_unix_millis(ms))
                    .utc_offset
            });
            serial_at(ms, offset)
        });
        // What was left last time.
        if let Some(bytes) = quench_ui_web::browser::storage_get(KEY_BOOK)
            .as_deref()
            .and_then(unkeep)
        {
            let name = quench_ui_web::browser::storage_get(KEY_NAME)
                .unwrap_or_else(|| "Untitled.xlsx".to_string());
            match state.load_bytes(&name, &bytes) {
                Ok(()) => state.status.set("Restored your last workbook".to_string()),
                Err(e) => state
                    .status
                    .set(format!("Could not restore the last workbook: {e}")),
            }
        }
        // Keep it after changes settle (and tells it to the state after the restore, so restoring does not save again at once).
        let generation = Rc::new(Cell::new(0_u32));
        let watched = state.clone();
        state.on_change(move || {
            generation.set(generation.get().wrapping_add(1));
            let (mine, current, st) = (generation.get(), generation.clone(), watched.clone());
            quench_ui_web::spawn(async move {
                quench_ui_web::sleep(QUIET_MS).await;
                if current.get() == mine {
                    save(&st);
                }
            });
        });
    }

    fn save(state: &SheetState) {
        match state.xlsx_bytes().ok().as_deref().and_then(keep) {
            Some(text) => {
                quench_ui_web::browser::storage_set(KEY_BOOK, Some(&text));
                quench_ui_web::browser::storage_set(KEY_NAME, Some(&state.name.get_untracked()));
            }
            None => state
                .status
                .set("Too large to keep in the browser: save it as a file".to_string()),
        }
    }
}

quench_ui_web::app!(app);
