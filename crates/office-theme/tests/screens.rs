use office_theme::{
    PALETTES, SCRIPT, STYLE, ThemeLabels, flag, icon_script, language_picker, theme_menu,
};
use quench_ui::prelude::*;
use quench_ui::{Event, TestRenderer, mount};
use std::cell::RefCell;
use std::rc::Rc;

fn labels() -> ThemeLabels {
    ThemeLabels {
        toggle: "Theme".into(),
        mode: "Mode".into(),
        light: "Light".into(),
        dark: "Dark".into(),
        system: "System".into(),
        palette: "Colour".into(),
        palettes: ["Blue", "Green", "Teal", "Purple", "Rose", "Orange"].map(String::from),
    }
}

#[quench_test::test]
fn the_menu_offers_three_modes_and_six_palettes_for_the_script_to_answer() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let (_, _scope) = create_root(|| mount(&r, &root, theme_menu(&labels()).into_view()));
    let modes: Vec<String> = r
        .find_by_tag("button")
        .into_iter()
        .filter(|b| r.attr(*b, "data-theme-mode").is_some())
        .map(|b| r.text_of(b))
        .collect();
    assert_eq!(modes, ["Light", "Dark", "System"]);
    let swatches: Vec<String> = r
        .find_by_tag("button")
        .into_iter()
        .filter_map(|b| r.attr(b, "data-theme-palette"))
        .collect();
    assert_eq!(swatches, PALETTES.map(|(n, _)| n.to_string()));
}

#[quench_test::test]
fn the_picker_lists_locales_with_flags_and_reports_the_choice() {
    assert_eq!(flag("pl-PL"), "\u{1F1F5}\u{1F1F1}");
    assert_eq!(flag("pl"), "");
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let picked = Rc::new(RefCell::new(String::new()));
    let sink = Rc::clone(&picked);
    let (_, _scope) = create_root(|| {
        mount(
            &r,
            &root,
            language_picker(
                "Language",
                &["en-US", "de-DE"],
                &(Rc::new(|| "de-DE".to_string()) as Rc<dyn Fn() -> String>),
                Rc::new(move |c| *sink.borrow_mut() = c.to_string()),
            )
            .into_view(),
        )
    });
    let select = r.find_by_tag("select")[0];
    let options = r.find_by_tag("option");
    assert_eq!(options.len(), 2);
    assert!(r.attr(options[1], "selected").is_some() && r.attr(options[0], "selected").is_none());
    r.fire(select, &Event::Input("de-DE".into()));
    assert_eq!(*picked.borrow(), "de-DE");
}

#[quench_test::test]
fn the_style_sheet_and_script_agree_on_their_names() {
    for needle in [
        "--office-bg",
        "prefers-color-scheme: dark",
        "data-theme=\"dark\"",
        "data-palette=\"green\"",
        ".theme-panel",
        "::-webkit",
    ] {
        // The scrollbar rules live with the ribbon's sheet; the others are here.
        if needle != "::-webkit" {
            assert!(STYLE.contains(needle), "{needle}");
        }
    }
    for needle in [
        "q:' + key",
        "data-theme-mode",
        "data-theme-palette",
        "data-theme",
    ] {
        assert!(SCRIPT.contains(needle), "{needle}");
    }
}

#[quench_test::test]
fn the_icon_script_adds_the_svg_as_a_tab_icon_and_escapes_what_would_break_it() {
    let js = icon_script("<svg a='b'>\n<path d=\"M0 0\\\"/></svg>\n");
    assert!(
        js.contains("link[rel~=\"icon\"]")
            && js.contains("rel='icon'")
            && js.contains("image/svg+xml")
    );
    assert!(
        js.contains("encodeURIComponent('<svg a=\\'b\\'><path d=\"M0 0\\\\\"/></svg>')"),
        "{js}"
    );
    assert!(!js.contains('\n'));
}

quench_test::main!();
