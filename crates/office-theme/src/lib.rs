//! The look of the lore apps: light, dark or the system's, six palettes, and the menus that pick them.
//!
//! Serve [`STYLE`] first, then the apps' own style sheets (which use the `--office-*` variables), and load [`SCRIPT`]
//! before the app starts; it applies the stored choice to the page and answers clicks on [`theme_menu`]. Nothing here
//! depends on a document or on another suite: the labels come from the app, in whatever language it speaks.

use quench_ui::prelude::*;
use std::rc::Rc;

/// Variables for both schemes and the palettes, and the rules of the menu and the language picker.
pub const STYLE: &str = include_str!("../assets/theme.css");

/// Keeps `html[data-theme]` and `html[data-palette]` in step with the browser's storage (`q:theme`, `q:palette`).
pub const SCRIPT: &str = include_str!("../assets/theme.js");

/// A script that gives the page the tab icon `svg` (a whole SVG file).
///
/// It is meant to go in front of the loader, like [`SCRIPT`]: the page's head has no place for a link of our own, so the
/// script adds it. A page that already has one keeps it.
#[must_use]
pub fn icon_script(svg: &str) -> String {
    let mut text = String::new();
    for c in svg.trim().chars() {
        match c {
            '\\' => text.push_str("\\\\"),
            '\'' => text.push_str("\\'"),
            '\n' | '\r' => {}
            c => text.push(c),
        }
    }
    format!(
        "(function(){{if(document.querySelector('link[rel~=\"icon\"]'))return;var l=document.createElement('link');l.rel='icon';l.type='image/svg+xml';l.href='data:image/svg+xml,'+encodeURIComponent('{text}');document.head.appendChild(l);}})();"
    )
}

/// The palettes, in menu order, each with the colour of its swatch (the style sheet draws them: a page allows no inline styles). The first is the one used until another is chosen.
pub const PALETTES: [(&str, &str); 6] = [
    ("blue", "#1a73e8"),
    ("green", "#1a7f4b"),
    ("teal", "#0e8a7d"),
    ("purple", "#7048c9"),
    ("rose", "#d6275d"),
    ("orange", "#d9560d"),
];

/// The texts of the theme menu, in the app's language.
#[derive(Debug, Clone)]
pub struct ThemeLabels {
    /// The button that opens the menu (its tooltip).
    pub toggle: String,
    pub mode: String,
    pub light: String,
    pub dark: String,
    /// Follow the system's scheme.
    pub system: String,
    pub palette: String,
    /// The names of the palettes, in the order of [`PALETTES`].
    pub palettes: [String; 6],
}

/// The menu: a button that opens light, dark or system, and the palette swatches. The script does the choosing.
#[must_use]
pub fn theme_menu(labels: &ThemeLabels) -> Element {
    let mode = |value: &'static str, text: &str| {
        button()
            .attr("type", "button")
            .class("theme-mode")
            .attr("data-theme-mode", value)
            .text(text.to_string())
    };
    let mut swatches = div().class("theme-palettes");
    for ((name, _), label) in PALETTES.iter().zip(&labels.palettes) {
        swatches = swatches.child(
            button()
                .attr("type", "button")
                .class("theme-swatch")
                .attr("data-theme-palette", *name)
                .attr("title", label.clone())
                .attr("aria-label", label.clone()),
        );
    }
    element("details")
        .class("theme-menu")
        .child(
            element("summary")
                .attr("title", labels.toggle.clone())
                .attr("aria-label", labels.toggle.clone())
                .text("\u{25D0}"),
        )
        .child(
            div()
                .class("theme-panel")
                .child(
                    div()
                        .child(div().class("theme-label").text(labels.mode.clone()))
                        .child(
                            div()
                                .class("theme-modes")
                                .child(mode("light", &labels.light))
                                .child(mode("dark", &labels.dark))
                                .child(mode("system", &labels.system)),
                        ),
                )
                .child(
                    div()
                        .child(div().class("theme-label").text(labels.palette.clone()))
                        .child(swatches),
                ),
        )
}

/// `pl-PL` as `🇵🇱`: the region's two letters as regional indicators. Nothing for a locale with no region.
#[must_use]
pub fn flag(locale: &str) -> String {
    let region = locale.split(['-', '_']).nth(1).unwrap_or_default();
    if region.len() != 2 || !region.chars().all(|c| c.is_ascii_alphabetic()) {
        return String::new();
    }
    region
        .to_ascii_uppercase()
        .chars()
        .filter_map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)))
        .collect()
}

/// A select of locales, each with its flag.
///
/// `current` says which is in use now (it is read in each option, so build the picker where the language is read);
/// `on_pick` gets the locale the person chose.
#[must_use]
pub fn language_picker(
    label: &str,
    codes: &'static [&'static str],
    current: &Rc<dyn Fn() -> String>,
    on_pick: Rc<dyn Fn(&str)>,
) -> Element {
    let options = codes.iter().map(|code| {
        let current = Rc::clone(current);
        option()
            .attr("value", *code)
            .attr_bool("selected", move || current() == *code)
            .text(format!("{} {code}", flag(code)).trim().to_string())
            .into_view()
    });
    select()
        .class("language-picker")
        .attr("id", "language")
        .attr("aria-label", label.to_string())
        .on_input(move |code| on_pick(code))
        .children(options)
}
