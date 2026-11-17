//! lore-notes in the browser: the notes screen over the vault left here last time.
//!
//! The vault is kept in the browser's storage a moment after each change, and read back when the page opens. Nothing leaves
//! the machine.

use notes_app::{NotesState, Vault, notes_view};
use quench_ui::prelude::*;

/// The page: the server renders it into the first response, the browser takes it over.
#[must_use]
pub fn app() -> View {
    #[cfg(target_arch = "wasm32")]
    let vault = browser::stored();
    #[cfg(not(target_arch = "wasm32"))]
    let vault = Vault::new();
    let state = NotesState::new(vault);
    #[cfg(target_arch = "wasm32")]
    browser::attach(&state);
    let cloud = office_cloud_ui::setup();
    {
        let (out, back, rev) = (state.clone(), state.clone(), state.clone());
        cloud.bind(office_cloud_ui::Binding {
            kind: "notes",
            export: std::rc::Rc::new(move || {
                Some((
                    format!("{}.md", out.active.get_untracked()),
                    out.buffer.get_untracked().into_bytes(),
                ))
            }),
            import: std::rc::Rc::new(move |name, bytes| {
                back.load_files(&[quench_ui::platform::PickedFile {
                    name: name.to_string(),
                    mime: "text/markdown".into(),
                    bytes: bytes.to_vec(),
                }]);
                Ok(())
            }),
            revision: std::rc::Rc::new(move || u64::from(rev.rev.get())),
            // The cloud copy is the note that was open when it was saved; which note is open changes, so a save is the person's.
            autosave: false,
            position: None,
            delta: None,
        });
    }
    #[cfg(target_arch = "wasm32")]
    if cloud.context.share.is_some() {
        cloud.open_share();
    }
    notes_view(&state)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{NotesState, Vault};
    use std::cell::Cell;
    use std::rc::Rc;

    /// Where the vault is kept, and the language.
    const KEY_VAULT: &str = "lore-notes.vault";
    const KEY_LANGUAGE: &str = "language";
    /// How long after the last change the vault is kept.
    const QUIET_MS: u32 = 800;

    pub fn stored() -> Vault {
        quench_ui_web::browser::storage_get(KEY_VAULT)
            .map(|t| Vault::decode(&t))
            .unwrap_or_default()
    }

    pub fn attach(state: &NotesState) {
        state.accept_drops();
        if let Some(code) = quench_ui_web::browser::storage_get(KEY_LANGUAGE) {
            state.set_language(&code);
        }
        notes_app::i18n::on_language(|code| {
            quench_ui_web::browser::storage_set(KEY_LANGUAGE, Some(code));
        });
        let generation = Rc::new(Cell::new(0_u32));
        let watched = state.clone();
        state.on_change(move || {
            generation.set(generation.get().wrapping_add(1));
            let (mine, current, st) = (generation.get(), generation.clone(), watched.clone());
            quench_ui_web::spawn(async move {
                quench_ui_web::sleep(QUIET_MS).await;
                if current.get() == mine {
                    quench_ui_web::browser::storage_set(KEY_VAULT, Some(&st.encoded()));
                }
            });
        });
    }
}

quench_ui_web::app!(app);
