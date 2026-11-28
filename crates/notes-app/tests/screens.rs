#![allow(clippy::unwrap_used, clippy::assert_is_empty, clippy::manual_assert_eq)]
use notes_app::{Mode, NotesState, PromptKind, Vault, notes_view};
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;
use std::rc::Rc;

struct Screen {
    r: Rc<TestRenderer>,
    state: NotesState,
    _scope: Scope,
}

fn open(vault: Vault) -> Screen {
    // Timers (the presentation clock) run on the test's clock.
    quench_ui::platform::install(Rc::new(quench_ui::platform::ManualPlatform::new()));
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let mut state = None;
    let (_, scope) = create_root(|| {
        let s = NotesState::new(vault);
        state = Some(s.clone());
        mount(&r, &root, notes_view(&s))
    });
    Screen {
        r,
        state: state.unwrap(),
        _scope: scope,
    }
}

impl Screen {
    fn with_class(&self, tag: &str, class: &str) -> Vec<NodeId> {
        self.r
            .find_by_tag(tag)
            .into_iter()
            .filter(|n| {
                self.r
                    .attr(*n, "class")
                    .is_some_and(|c| c.split(' ').any(|x| x == class))
            })
            .collect()
    }

    fn button(&self, label: &str) -> NodeId {
        let all: Vec<NodeId> = self
            .r
            .find_by_tag("button")
            .into_iter()
            .filter(|b| self.r.text_of(*b) == label)
            .collect();
        assert!(!all.is_empty(), "{label}: no button");
        all[0]
    }
}

#[quench_test::test]
fn a_new_vault_starts_with_a_welcome_note_shown_in_the_editor_and_the_preview() {
    let s = open(Vault::new());
    assert_eq!(s.state.active.get_untracked(), "Welcome");
    assert!(
        s.state
            .buffer
            .get_untracked()
            .contains("# Welcome to lore-notes")
    );
    let h1 = s.r.find_by_tag("h1");
    assert_eq!(h1.len(), 1);
    assert_eq!(s.r.text_of(h1[0]), "Welcome to lore-notes");
    // The editor holds the text, front matter and all; the preview leaves the front matter out.
    let editor = s.r.find_by_tag("textarea")[0];
    assert!(
        s.r.attr(editor, "value")
            .unwrap()
            .starts_with("---\ntheme: default")
    );
    assert!(
        !s.r.text_of(s.with_class("div", "preview")[0])
            .contains("theme: default")
    );
}

#[quench_test::test]
fn typing_updates_the_preview_and_the_stored_vault() {
    let s = open(Vault::new());
    let saved = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&saved);
    s.state.on_change(move || counter.set(counter.get() + 1));
    let editor = s.r.find_by_tag("textarea")[0];
    s.r.fire(
        editor,
        &Event::Input("# Changed\n\nSome *words* here.".into()),
    );
    assert_eq!(saved.get(), 1);
    assert_eq!(s.r.text_of(s.r.find_by_tag("h1")[0]), "Changed");
    assert_eq!(s.r.find_by_tag("em").len(), 1);
    assert!(s.state.encoded().contains("Some *words* here."));
    assert_eq!(
        Vault::decode(&s.state.encoded()).text("Welcome"),
        Some("# Changed\n\nSome *words* here.")
    );
}

#[quench_test::test]
fn notes_are_made_found_linked_renamed_and_deleted() {
    let s = open(Vault::new());
    let name = s.state.new_note("Ideas");
    assert_eq!(name, "Ideas");
    s.state
        .edit("# Ideas\nBack to [[Welcome]] and on to [[Plans]]. #brainstorm");
    // Links on the screen: clicking one opens that note; a missing one is made.
    let links = s.with_class("span", "wikilink");
    assert_eq!(links.len(), 2);
    s.r.fire(links[0], &Event::Click);
    assert_eq!(s.state.active.get_untracked(), "Welcome");
    assert_eq!(s.state.backlinks(), ["Ideas"]);
    s.state.select("Ideas");
    s.r.fire(s.with_class("span", "wikilink")[1], &Event::Click);
    assert_eq!(s.state.active.get_untracked(), "Plans");
    assert!(s.state.exists("Plans"));
    // The list, search and tags.
    assert_eq!(s.state.titles().len(), 3);
    assert_eq!(s.state.search("brain")[0].name, "Ideas");
    assert_eq!(
        s.state
            .tags()
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>(),
        ["brainstorm", "welcome"]
    );
    // Renaming points the links of the others at the new name.
    s.state.select("Welcome");
    assert!(s.state.rename_active("Ideas").is_err(), "taken");
    s.state.rename_active("Start here").unwrap();
    assert_eq!(s.state.active.get_untracked(), "Start here");
    s.state.select("Ideas");
    assert!(s.state.buffer.get_untracked().contains("[[Start here]]"));
    // Deleting opens another; the last one is replaced by an empty note.
    s.state.delete_active();
    assert!(!s.state.exists("Ideas"));
    for name in ["Plans", "Start here"] {
        s.state.select(name);
        s.state.delete_active();
    }
    assert_eq!(s.state.titles().len(), 1);
    assert_eq!(s.state.active.get_untracked(), "Untitled");
}

#[quench_test::test]
fn the_questions_make_rename_delete_and_go_to() {
    let s = open(Vault::new());
    s.state.ask(PromptKind::New);
    assert_eq!(s.with_class("div", "prompt").len(), 1);
    let input = s.r.find_by_attr("aria-label", "Name")[0];
    s.r.fire(input, &Event::Input("Meeting: notes/1".into()));
    s.r.fire(
        input,
        &Event::KeyDown(KeyEvent {
            key: "Enter".into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }),
    );
    assert_eq!(
        s.state.active.get_untracked(),
        "Meeting- notes-1",
        "the name is made safe"
    );
    assert!(s.with_class("div", "prompt").is_empty(), "answered");
    s.state.ask(PromptKind::Rename);
    s.state.set_prompt_value("Welcome");
    s.state.answer();
    assert!(
        s.r.text_of(s.with_class("div", "prompt-error")[0])
            .contains("Another note")
    );
    s.state.cancel_prompt();
    s.state.ask(PromptKind::Switch);
    s.state.set_prompt_value("welc");
    s.state.answer();
    assert_eq!(s.state.active.get_untracked(), "Welcome");
    s.state.ask(PromptKind::Delete);
    s.r.fire(s.button("OK"), &Event::Click);
    assert!(!s.state.exists("Welcome"));
}

#[quench_test::test]
fn the_modes_show_the_editor_the_preview_or_both() {
    let s = open(Vault::new());
    let hidden = |class: &str| {
        s.r.attr(s.with_class("div", class)[0], "class")
            .unwrap()
            .contains("hidden")
    };
    assert!(!hidden("editor-pane") && !hidden("preview-pane"));
    s.r.fire(s.button("Edit"), &Event::Click);
    assert_eq!(s.state.mode.get_untracked(), Mode::Edit);
    assert!(hidden("preview-pane") && !hidden("editor-pane"));
    s.r.fire(s.button("Preview"), &Event::Click);
    assert!(hidden("editor-pane") && !hidden("preview-pane"));
}

#[quench_test::test]
fn the_note_is_a_deck_that_can_be_presented_with_notes_and_a_timer() {
    let s = open(Vault::new());
    let deck = s.state.deck();
    assert_eq!(deck.slides.len(), 3);
    assert_eq!((deck.theme.as_str(), deck.stage), ("default", (1280, 720)));
    s.r.fire(s.button("Present"), &Event::Click);
    assert!(s.state.presenting.get_untracked());
    let present = s.with_class("div", "present")[0];
    let key = |k: &str| {
        Event::KeyDown(KeyEvent {
            key: k.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        })
    };
    assert_eq!(s.r.text_of(s.with_class("span", "progress")[0]), "1 / 3");
    s.r.fire(present, &key("ArrowRight"));
    assert_eq!(s.state.slide.get_untracked(), 1);
    assert_eq!(s.r.text_of(s.with_class("span", "progress")[0]), "2 / 3");
    assert!(
        s.r.text_of(s.with_class("div", "slide")[0])
            .contains("A second slide")
    );
    s.r.fire(present, &key("ArrowRight"));
    s.r.fire(present, &key("ArrowRight"));
    assert_eq!(s.state.slide.get_untracked(), 2, "it stops at the end");
    // The notes are shown on request.
    assert!(s.with_class("div", "speaker").is_empty());
    s.r.fire(present, &key("n"));
    assert!(
        s.r.text_of(s.with_class("div", "speaker")[0])
            .contains("Only you see them.")
    );
    // The timer ticks with the platform's clock.
    s.r.fire(present, &key("Home"));
    assert_eq!(s.state.slide.get_untracked(), 0);
    s.r.fire(present, &key("Escape"));
    assert!(!s.state.presenting.get_untracked() && s.with_class("div", "present").is_empty());
}

#[quench_test::test]
fn the_deck_asks_for_its_theme_and_shape_in_the_front_matter() {
    let s = open(Vault::new());
    s.state.set_theme("night");
    s.state.set_aspect("4:3");
    assert!(
        s.state
            .buffer
            .get_untracked()
            .starts_with("---\ntheme: night\naspect: 4:3\n---\n")
    );
    let deck = s.state.deck();
    assert_eq!((deck.theme.as_str(), deck.stage), ("night", (1024, 768)));
    s.state.set_theme("nonsense");
    assert_eq!(
        s.state.deck().theme,
        "default",
        "an unknown theme is the default"
    );
    // A slide may ask for a layout.
    s.state
        .edit("# One\n---\n<!-- layout: two-col -->\n## Two\n");
    s.state.present(1);
    assert!(
        s.r.attr(s.with_class("div", "slide")[0], "class")
            .unwrap()
            .contains("layout-two-col")
    );
}

#[quench_test::test]
fn markdown_blocks_are_drawn_and_html_is_not() {
    let s = open(Vault::new());
    s.state.edit("# T\n\n> [!warning] Careful\n> text of it\n\n- [x] done\n- [ ] todo\n\n| A | B |\n|:-:|--:|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n<script>alert(1)</script>\n\nText <b>raw</b> [x](javascript:alert(1)) [ok](https://example.com)\n\nFootnote[^a].\n\n[^a]: The note.\n");
    assert_eq!(s.with_class("div", "callout-warning").len(), 1);
    assert!(s.r.text_of(s.with_class("div", "callout-title")[0]) == "Careful");
    assert_eq!(
        s.r.find_by_tag("input")
            .iter()
            .filter(|i| s.r.attr(**i, "type").as_deref() == Some("checkbox"))
            .count(),
        2
    );
    assert_eq!(s.r.find_by_tag("th").len(), 2);
    assert_eq!(s.r.find_by_tag("code").len(), 1);
    assert!(
        s.r.find_by_tag("script").is_empty(),
        "raw html blocks are left out"
    );
    let anchors = s.r.find_by_tag("a");
    assert!(
        anchors
            .iter()
            .any(|a| s.r.attr(*a, "href").as_deref() == Some("https://example.com"))
    );
    assert!(
        !anchors.iter().any(|a| s
            .r
            .attr(*a, "href")
            .is_some_and(|h| h.starts_with("javascript"))),
        "unsafe links are text"
    );
    assert_eq!(s.with_class("section", "footnotes").len(), 1);
}

#[quench_test::test]
fn files_are_opened_saved_and_exported() {
    let s = open(Vault::new());
    let files = vec![
        quench_ui::platform::PickedFile {
            name: "Todo list.md".into(),
            mime: String::new(),
            bytes: b"# Todo\n- a".to_vec(),
        },
        quench_ui::platform::PickedFile {
            name: "pic.png".into(),
            mime: String::new(),
            bytes: vec![1, 2, 3],
        },
    ];
    s.state.load_files(&files);
    assert_eq!(s.state.active.get_untracked(), "Todo list");
    assert_eq!(s.state.buffer.get_untracked(), "# Todo\n- a");
    s.state.load_files(&files[1..]);
    assert!(
        s.r.text_of(s.r.find_by_attr("role", "status")[0])
            .contains("Only .md")
    );
    // The pages it makes stand alone and carry no raw html.
    s.state.edit("---\ntitle: My page\n---\n# Heading\n\nSee [[Welcome|the start]] <b>x</b>\n\n---\n\nSecond\n\n???\nsecret\n");
    let page = notes_app::note_html("My page", &s.state.buffer.get_untracked());
    assert!(
        page.contains("<title>My page</title>")
            && page.contains("<h1>Heading</h1>")
            && page.contains("the start")
            && !page.contains("[[")
    );
    assert!(
        !page.contains("<b>x</b>") && page.contains("&lt;b&gt;x&lt;/b&gt;"),
        "{page}"
    );
    assert!(!page.contains("title: My page"));
    let deck_pdf = notes_pdf::deck_pdf("T", "default", (1280, 720), &s.state.deck().slides);
    assert!(deck_pdf.starts_with(b"%PDF"));
    s.state.export_pdf();
    assert!(
        s.r.text_of(s.r.find_by_attr("role", "status")[0])
            .contains(".pdf")
    );
    let deck = notes_app::deck_html(&s.state.deck());
    assert_eq!(deck.matches("<section class=\"slide").count(), 2);
    assert!(
        deck.contains("<aside hidden>secret</aside>")
            && deck.contains("keydown")
            && deck.contains("1280px")
    );
}

#[quench_test::test]
fn the_screen_speaks_the_language_picked() {
    let s = open(Vault::new());
    s.state.set_language("de-DE");
    assert!(
        s.r.find_by_tag("button")
            .iter()
            .any(|b| s.r.text_of(*b) == "Start")
    );
    s.state.set_language("en-US");
    assert!(
        s.r.find_by_tag("button")
            .iter()
            .any(|b| s.r.text_of(*b) == "Home")
    );
}

#[quench_test::test]
fn every_catalogue_has_the_same_texts() {
    let english = office_i18n::ids(include_str!("../i18n/en-US.ftl"));
    assert!(english.len() > 100);
    for (name, file) in [
        ("de", include_str!("../i18n/de-DE.ftl")),
        ("es", include_str!("../i18n/es-ES.ftl")),
        ("fr", include_str!("../i18n/fr-FR.ftl")),
        ("pl", include_str!("../i18n/pl-PL.ftl")),
    ] {
        assert_eq!(office_i18n::ids(file), english, "{name}");
    }
}

#[quench_test::test]
fn the_text_tools_work_at_the_caret_and_on_the_selection() {
    let s = open(Vault::new());
    let st = &s.state;
    st.edit("héllo wörld\nsecond");
    // The editor reports its selection (UTF-16 units: the accents count one each).
    let editor = s.r.find_by_tag("textarea")[0];
    s.r.fire(
        editor,
        &Event::Selection(quench_ui::SelectionEvent { start: 6, end: 11 }),
    );
    assert_eq!(st.caret.get_untracked(), (6, 11));
    st.wrap_selection("**", "**", "bold");
    assert_eq!(st.buffer.get_untracked(), "héllo **wörld**\nsecond");
    assert_eq!(
        st.caret.get_untracked(),
        (8, 13),
        "the same words stay selected"
    );
    assert_eq!(
        s.r.attr(s.r.find_by_tag("textarea")[0], "selection")
            .as_deref(),
        Some("8,13")
    );
    st.wrap_selection("**", "**", "bold");
    assert_eq!(
        st.buffer.get_untracked(),
        "héllo wörld\nsecond",
        "wrapped again takes it off"
    );
    // Nothing selected: the markers go in with a word between them, selected.
    st.set_caret(0, 0);
    st.wrap_selection("*", "*", "italic");
    assert!(st.buffer.get_untracked().starts_with("*italic*héllo"));
    assert_eq!(st.caret.get_untracked(), (1, 7));
    // Lines.
    st.edit("one\ntwo\nthree");
    st.set_caret(1, 6);
    st.toggle_line_prefix("- ");
    assert_eq!(st.buffer.get_untracked(), "- one\n- two\nthree");
    st.toggle_line_prefix("- ");
    assert_eq!(st.buffer.get_untracked(), "one\ntwo\nthree");
    st.set_caret(0, 0);
    st.toggle_line_prefix("## ");
    st.toggle_line_prefix("# ");
    assert_eq!(
        st.buffer.get_untracked(),
        "# one\ntwo\nthree",
        "a heading takes the place of a heading"
    );
    st.indent(false);
    assert!(st.buffer.get_untracked().starts_with("  # one"));
    st.indent(true);
    assert!(st.buffer.get_untracked().starts_with("# one"));
    // Tab types two spaces; a block starts on a line of its own.
    st.edit("text");
    st.set_caret(4, 4);
    st.insert_block("---\n");
    assert_eq!(st.buffer.get_untracked(), "text\n\n---\n\n");
    st.insert_text("[[X]]");
    assert!(st.buffer.get_untracked().ends_with("[[X]]"));
    // The ribbon's buttons act on the caret and build the editor again for the focus.
    st.edit("pick");
    st.set_caret(0, 4);
    let before = st.editor_epoch.get_untracked();
    s.r.fire(s.button("Bold"), &Event::Click);
    assert_eq!(st.buffer.get_untracked(), "**pick**");
    assert_eq!(st.editor_epoch.get_untracked(), before + 1);
    assert!(s.r.find_by_tag("textarea").len() == 1, "one editor still");
}

#[quench_test::test]
fn math_in_a_note_is_a_picture_with_its_tex_as_the_alternative_text() {
    let s = open(Vault::new());
    let editor = s.r.find_by_tag("textarea")[0];
    s.r.fire(
        editor,
        &Event::Input("Inline $x^2$ and\n\n$$\\frac{a}{b}$$\n\nand $\\broken{$.".into()),
    );
    let pics = s.with_class("img", "math");
    assert_eq!(pics.len(), 1, "{:?}", pictures(&s.r).len());
    assert_eq!(s.r.attr(pics[0], "alt").as_deref(), Some("x^2"));
    assert!(
        s.r.attr(pics[0], "src")
            .unwrap()
            .starts_with("data:image/svg+xml")
    );
    assert_eq!(
        pictures(&s.r).len(),
        2,
        "the inline formula and the display one"
    );
    // What cannot be set stays as the TeX it is.
    assert!(!s.with_class("span", "math").is_empty());
}

#[quench_test::test]
fn a_slide_with_fragments_shows_its_list_items_one_at_a_time_before_the_next_slide() {
    let s = open(Vault::new());
    let editor = s.r.find_by_tag("textarea")[0];
    s.r.fire(
        editor,
        &Event::Input("# Deck\n\n- keep\n- keep too\n\n---\n\n<!-- fragments -->\n\n# Steps\n\n- one\n- two\n  - under two\n- three\n\n---\n\n# End".into()),
    );
    s.state.present(1);
    let shown = |s: &Screen| s.r.find_by_tag("li").len();
    assert_eq!(s.state.revealed.get_untracked(), 0);
    let before = shown(&s);
    s.state.step(1);
    s.state.step(1);
    assert_eq!(s.state.slide.get_untracked(), 1, "still on the slide");
    assert_eq!(s.state.revealed.get_untracked(), 2);
    assert!(shown(&s) > before);
    s.state.step(1);
    assert_eq!(s.state.revealed.get_untracked(), 3);
    s.state.step(1);
    assert_eq!(
        s.state.slide.get_untracked(),
        2,
        "all shown, the next step is the next slide"
    );
    s.state.step(-1);
    assert_eq!(s.state.slide.get_untracked(), 1);
    s.state.step(-1);
    s.state.step(-1);
    assert_eq!(s.state.slide.get_untracked(), 1);
    assert_eq!(
        notes_app::fragments_shown("- a\n- b\n  - c\n- d", 2),
        "- a\n- b\n  - c"
    );
    assert_eq!(notes_app::fragment_total("# T\n- a\n1. b\n  - c"), 2);
}

#[quench_test::test]
fn math_is_typeset_as_a_picture_with_a_size_and_unreadable_math_is_left_as_text() {
    let f = notes_app::math::typeset(r"\frac{a}{b} + x^2", false).unwrap();
    assert!(
        f.address.starts_with("data:image/svg+xml;utf8,%3Csvg"),
        "{}",
        &f.address[..80]
    );
    assert!(f.width > 0.5 && f.height > 0.5, "{f:?}");
    assert!(!f.address.contains('<') && !f.address.contains('#'));
    assert!(notes_app::math::typeset(r"\notacommand{", false).is_none());
    assert_eq!(
        notes_app::math::typeset(r"\frac{a}{b} + x^2", false),
        Some(f)
    );
}

#[quench_test::test]
fn exported_web_pages_carry_typeset_math_as_svg_and_leave_unreadable_math_as_tex() {
    let html = notes_app::note_html(
        "T",
        "Inline $x^2$ here\n\n$$\\frac{a}{b}$$\n\nand $\\broken{$ stays.",
    );
    assert_eq!(html.matches("<svg").count(), 2, "{html}");
    assert!(html.contains("currentColor") && html.contains("class=\"math display\""));
    assert!(
        html.contains("\\broken{"),
        "what cannot be set stays as TeX"
    );
    assert!(!html.contains("\\frac"));
}

quench_test::main!();

/// The pictures of the document on screen (not the app's own, in the top bar).
fn pictures(r: &TestRenderer) -> Vec<quench_ui::test_renderer::NodeId> {
    r.find_by_tag("img")
        .into_iter()
        .filter(|n| r.attr(*n, "class").as_deref() != Some("fbar-mark"))
        .collect()
}
