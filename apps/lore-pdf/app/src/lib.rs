//! lore-pdf in the browser: the text reader over the file you open or drop on the page.

use pdf_app::{PdfState, pdf_view};
use quench_ui::prelude::*;

/// The page: the server renders it into the first response, the browser takes it over.
#[must_use]
pub fn app() -> View {
    let state = PdfState::new();
    #[cfg(target_arch = "wasm32")]
    {
        state.accept_drops();
        if let Some(code) = quench_ui_web::browser::storage_get("language") {
            state.set_language(&code);
        }
        pdf_app::i18n::on_language(|code| {
            quench_ui_web::browser::storage_set("language", Some(code));
        });
    }
    let cloud = office_cloud_ui::setup();
    {
        let (out, back, rev) = (state.clone(), state.clone(), state.clone());
        cloud.bind(office_cloud_ui::Binding {
            kind: "pdf",
            export: std::rc::Rc::new(move || {
                let bytes = out.save_bytes();
                (!bytes.is_empty()).then(|| (out.name.get_untracked(), bytes))
            }),
            import: std::rc::Rc::new(move |name, bytes| back.load_bytes(name, bytes)),
            revision: std::rc::Rc::new(move || u64::from(rev.rev.get())),
            // Changes to a PDF (marks, moved pages) are sent when the person saves, not by themselves.
            autosave: false,
            position: None,
            delta: None,
        });
    }
    #[cfg(target_arch = "wasm32")]
    if cloud.context.share.is_some() {
        cloud.open_share();
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(id) = cloud.context.open.clone() {
        cloud.open_doc(&id);
    }
    office_cloud_ui::with_bar(&cloud, pdf_view(&state))
}

quench_ui_web::app!(app);
