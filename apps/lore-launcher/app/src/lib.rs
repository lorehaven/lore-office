//! lore-launcher: the start page of lore-office. Tiles that open the apps; nothing else, and nothing leaves the machine.

use quench_ui::prelude::*;

pub mod i18n;
use i18n::{text, tr};

/// Where each app is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Links {
    pub docs: String,
    pub sheets: String,
    pub notes: String,
    pub pdf: String,
}

impl Default for Links {
    fn default() -> Self {
        Self {
            docs: "/lore-docs/".into(),
            sheets: "/lore-sheets/".into(),
            notes: "/lore-notes/".into(),
            pdf: "/lore-pdf/".into(),
        }
    }
}

impl Links {
    /// The links as the JSON the page carries to the browser.
    #[must_use]
    pub fn to_json(&self) -> String {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "{{\"docs\":\"{}\",\"sheets\":\"{}\",\"notes\":\"{}\",\"pdf\":\"{}\"}}",
            esc(&self.docs),
            esc(&self.sheets),
            esc(&self.notes),
            esc(&self.pdf)
        )
    }

    /// The links from [`Links::to_json`]'s text; what is missing or unreadable is the default.
    #[must_use]
    pub fn from_json(text: &str) -> Self {
        let mut links = Self::default();
        if let Ok(v) = quench_serde::json::from_str::<quench_serde::Value>(text) {
            let get = |k: &str, into: &mut String| {
                if let Some(s) = v.get(k).and_then(quench_serde::Value::as_str) {
                    *into = s.to_string();
                }
            };
            get("docs", &mut links.docs);
            get("sheets", &mut links.sheets);
            get("notes", &mut links.notes);
            get("pdf", &mut links.pdf);
        }
        links
    }
}

fn tile(name: &str, what: &str, icon: &str, href: &str) -> Element {
    a().class("tile")
        .attr("href", href.to_string())
        .child(
            span().class("tile-glyph").child(
                img()
                    .attr("src", office_ui::svg_address(icon))
                    .attr("alt", "")
                    .attr("width", "56")
                    .attr("height", "56"),
            ),
        )
        .child(span().class("tile-name").text(tr(name)))
        .child(span().class("tile-what").text(tr(what)))
}

/// The page, with the apps at `links`. It is drawn again when the language is changed.
#[must_use]
pub fn launcher(links: &Links) -> View {
    i18n::reset();
    let language = create_signal(i18n::language().to_string());
    let links = links.clone();
    View::dynamic(move || {
        let _ = language.get();
        page(&links, &language)
    })
}

fn page(links: &Links, language: &Signal<String>) -> View {
    let current = {
        let language = language.clone();
        std::rc::Rc::new(move || language.get()) as std::rc::Rc<dyn Fn() -> String>
    };
    let pick = language.clone();
    let picker = office_theme::language_picker(
        &text("language_label"),
        i18n::CODES,
        &current,
        std::rc::Rc::new(move |code| {
            let now = i18n::use_language(code);
            pick.set_if_changed(now.to_string());
            i18n::announce(now);
        }),
    );
    let theme = office_theme::theme_menu(&office_theme::ThemeLabels {
        toggle: text("theme_toggle"),
        mode: text("theme_mode"),
        light: text("theme_light"),
        dark: text("theme_dark"),
        system: text("theme_system"),
        palette: text("theme_palette"),
        palettes: ["blue", "green", "teal", "purple", "rose", "orange"]
            .map(|n| text(&format!("palette_{n}"))),
    });
    div()
        .class("launcher")
        .child(office_ui::topbar("lore-office", div().child(picker).child(theme)))
        .child(
            main()
                .class("home")
                .child(h1().text(tr("What would you like to work on?")))
                .child(p().class("lede").text(tr("Documents, spreadsheets, notes and PDFs, all in your browser. Nothing leaves this machine.")))
                .child(
                    div()
                        .class("tiles")
                        .child(tile("Documents", "Write letters and reports. Opens and saves Word files.", include_str!("../../../lore-docs/icon.svg"), &links.docs))
                        .child(tile("Spreadsheets", "Tables, formulas and charts. Opens and saves Excel files.", include_str!("../../../lore-sheets/icon.svg"), &links.sheets))
                        .child(tile("Notes", "Markdown notes with links, and slides to present.", include_str!("../../../lore-notes/icon.svg"), &links.notes))
                        .child(tile("PDF reader", "Read the text of a PDF and search it.", include_str!("../../../lore-pdf/icon.svg"), &links.pdf)),
                ),
        )
        .into_view()
}

/// The page's own style.
pub const STYLE: &str = include_str!("style.css");

/// The page the browser runs: the links come from the server's page state.
#[must_use]
pub fn app() -> View {
    #[cfg(target_arch = "wasm32")]
    let links =
        quench_ui_web::initial_state().map_or_else(Links::default, |t| Links::from_json(&t));
    #[cfg(not(target_arch = "wasm32"))]
    let links = Links::default();
    #[cfg(target_arch = "wasm32")]
    if let Some(code) = quench_ui_web::browser::storage_get("language") {
        i18n::reset();
        i18n::use_language(&code);
    }
    // The page's context (who is signed in, where gatehouse is) gives the top bar its dashboard and account menu.
    let _ = office_cloud_ui::setup();
    launcher(&links)
}

quench_ui_web::app!(app);
