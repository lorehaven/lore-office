//! lore-docs in the browser: the editor spike over an empty document.

use docs_app::{DocState, docs_view};
use docs_text::Doc;
use quench_ui::prelude::*;

/// The page: the server renders it into the first response, the browser takes it over.
#[must_use]
pub fn app() -> View {
    let state = DocState::new(Doc::from_text(
        "Type here.\n\nThe browser changes nothing by itself: every key becomes an event for the model, and the paragraphs are drawn again.",
    ));
    #[cfg(target_arch = "wasm32")]
    browser::attach(&state);
    let cloud = office_cloud_ui::setup();
    {
        let (out, back) = (state.clone(), state.clone());
        cloud.bind(office_cloud_ui::Binding {
            kind: "docs",
            export: std::rc::Rc::new(move || {
                let name = out.name.get_untracked();
                out.docx_bytes().ok().map(|b| (name, b))
            }),
            import: std::rc::Rc::new(move |name, bytes| back.load_bytes(name, bytes)),
        });
    }
    #[cfg(target_arch = "wasm32")]
    if cloud.context.share.is_some() {
        cloud.open_share();
    }
    docs_view(&state)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::DocState;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Where the document is kept (a `.docx`, as base64), and what it was called.
    const KEY_DOC: &str = "lore-docs.doc";
    const KEY_NAME: &str = "lore-docs.name";
    /// The most that is kept (characters of base64).
    const MAX_KEPT: usize = 3_500_000;
    /// How long after the last change the document is kept.
    const QUIET_MS: u32 = 1500;

    pub fn attach(state: &DocState) {
        state.accept_drops();
        if let Some(code) = quench_ui_web::browser::storage_get("language") {
            state.set_language(&code);
        }
        docs_app::i18n::on_language(|code| {
            quench_ui_web::browser::storage_set("language", Some(code));
        });
        if let Some(bytes) = quench_ui_web::browser::storage_get(KEY_DOC)
            .and_then(|t| quench_core::base64::STANDARD.decode(&t).ok())
        {
            let name = quench_ui_web::browser::storage_get(KEY_NAME)
                .unwrap_or_else(|| "Untitled.docx".to_string());
            if state.load_bytes(&name, &bytes).is_ok() {
                state
                    .status
                    .set(docs_app::i18n::tr("Restored your last document"));
            }
        }
        let generation = Rc::new(Cell::new(0_u32));
        let watched = state.clone();
        state.on_change(move || {
            generation.set(generation.get().wrapping_add(1));
            let (mine, current, st) = (generation.get(), generation.clone(), watched.clone());
            quench_ui_web::spawn(async move {
                quench_ui_web::sleep(QUIET_MS).await;
                if current.get() != mine {
                    return;
                }
                match st.docx_bytes() {
                    Ok(bytes) => {
                        let text = quench_core::base64::STANDARD.encode(&bytes);
                        if text.len() <= MAX_KEPT {
                            quench_ui_web::browser::storage_set(KEY_DOC, Some(&text));
                            quench_ui_web::browser::storage_set(
                                KEY_NAME,
                                Some(&st.name.get_untracked()),
                            );
                        } else {
                            st.status.set(docs_app::i18n::tr(
                                "Too large to keep in the browser: save it as a file",
                            ));
                        }
                    }
                    Err(e) => st.status.set(format!(
                        "{}: {e}",
                        docs_app::i18n::tr("Could not keep the document")
                    )),
                }
            });
        });
    }
}

quench_ui_web::app!(app);
