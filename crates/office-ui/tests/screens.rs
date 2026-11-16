#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use office_ui::*;
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;
use std::cell::Cell;
use std::rc::Rc;

struct Screen {
    r: Rc<TestRenderer>,
    done: Rc<Cell<u32>>,
    _scope: Scope,
}

fn open() -> Screen {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let done = Rc::new(Cell::new(0));
    let (a, b) = (done.clone(), done.clone());
    let (_, scope) = create_root(|| {
        let (name, status) = (
            create_signal("Book.xlsx".to_string()),
            create_signal("Ready".to_string()),
        );
        let view = ribbon(
            "lore-test",
            name,
            status,
            "Home",
            div(),
            div(),
            vec![(
                "Home",
                vec![group(
                    "File",
                    vec![
                        menu(
                            "File",
                            "File actions",
                            vec![
                                MenuItem::action("New", "", move || a.set(a.get() + 1)),
                                MenuItem::Separator,
                                MenuItem::action("Save", "Ctrl+S", move || b.set(b.get() + 10)),
                            ],
                        )
                        .into_view(),
                    ],
                )],
            )],
        );
        mount(&r, &root, view.into_view())
    });
    Screen {
        r,
        done,
        _scope: scope,
    }
}

impl Screen {
    fn items(&self) -> Vec<NodeId> {
        self.r.find_by_attr("role", "menuitem")
    }

    fn toggle(&self) {
        let b = self.r.find_by_attr("aria-haspopup", "menu");
        assert_eq!(b.len(), 1);
        self.r.fire(b[0], &Event::Click);
    }
}

#[quench_test::test]
fn the_ribbon_names_the_app_the_document_and_the_status() {
    let s = open();
    assert_eq!(s.r.text_of(s.r.find_by_attr("role", "status")[0]), "Ready");
    assert_eq!(s.r.find_by_attr("role", "toolbar").len(), 1);
    assert_eq!(s.r.find_by_attr("role", "group").len(), 1);
}

#[quench_test::test]
fn a_menu_opens_lists_its_choices_and_closes_on_a_choice() {
    let s = open();
    assert!(s.items().is_empty(), "closed at first");
    let button = s.r.find_by_attr("aria-haspopup", "menu")[0];
    assert_eq!(s.r.attr(button, "aria-expanded").as_deref(), Some("false"));
    s.toggle();
    assert_eq!(s.r.attr(button, "aria-expanded").as_deref(), Some("true"));
    let items = s.items();
    assert_eq!(items.len(), 2);
    assert_eq!(s.r.find_by_attr("role", "separator").len(), 1);
    assert_eq!(s.r.text_of(items[1]), "SaveCtrl+S", "label and shortcut");
    s.r.fire(items[1], &Event::Click);
    assert_eq!(s.done.get(), 10);
    assert!(s.items().is_empty(), "a choice closes it");
    s.toggle();
    s.toggle();
    assert!(s.items().is_empty(), "the button closes it again");
}

#[quench_test::test]
fn escape_and_a_click_elsewhere_close_a_menu_without_choosing() {
    let s = open();
    s.toggle();
    s.r.fire(
        s.r.find_by_attr("role", "menu")[0],
        &Event::KeyDown(KeyEvent::new("Escape")),
    );
    assert!(s.items().is_empty(), "escape closes it");
    s.toggle();
    let backdrop = s.r.find_by_attr("class", "menu-backdrop");
    assert_eq!(backdrop.len(), 1);
    s.r.fire(backdrop[0], &Event::Click);
    assert!(s.items().is_empty(), "so does a click elsewhere");
    assert_eq!(s.done.get(), 0, "nothing was chosen");
}

#[quench_test::test]
fn the_style_sheet_uses_the_theme_variables() {
    for needle in [
        "--office-bg",
        "var(--office-bg)",
        "var(--office-fg)",
        ".menu-list",
        ".ribbon",
        "::-webkit-scrollbar",
    ] {
        assert!(STYLE.contains(needle), "{needle}");
    }
}

quench_test::main!();

#[quench_test::test]
fn a_typed_name_keeps_the_extension_and_has_no_path_characters() {
    assert_eq!(office_ui::renamed("Untitled.docx", "Report"), "Report.docx");
    assert_eq!(office_ui::renamed("Untitled.docx", "  Plan.md "), "Plan.md");
    assert_eq!(office_ui::renamed("Untitled.docx", ""), "Untitled.docx");
    assert_eq!(office_ui::renamed("Untitled.docx", "a/b:c"), "a_b_c.docx");
    assert_eq!(office_ui::renamed("Untitled", "Notes"), "Notes");
}
