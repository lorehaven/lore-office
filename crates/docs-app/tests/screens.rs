#![allow(clippy::literal_string_with_formatting_args)]
#![allow(clippy::cast_possible_truncation)]
#![allow(
    clippy::unwrap_used,
    clippy::missing_const_for_fn,
    clippy::assert_is_empty,
    clippy::manual_assert_eq
)]
use docs_app::{DocState, Range, docs_view};
use docs_text::{Doc, Kind, Mark, Pos};
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;
use quench_ui::{BeforeInputEvent, BlockPoint, EditSelectionEvent};
use std::rc::Rc;

struct Screen {
    r: Rc<TestRenderer>,
    state: DocState,
    page: NodeId,
    _scope: Scope,
}

fn open(text: &str) -> Screen {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let mut state = None;
    let (_, scope) = create_root(|| {
        let s = DocState::new(Doc::from_text(text));
        state = Some(s.clone());
        mount(&r, &root, docs_view(&s))
    });
    let page = r.find_by_attr("role", "textbox")[0];
    Screen {
        r,
        state: state.unwrap(),
        page,
        _scope: scope,
    }
}

/// The elements directly under the page (the browser counts only these as blocks).
fn elements_of(s: &Screen) -> Vec<NodeId> {
    s.r.children(s.page)
        .into_iter()
        .filter(|n| s.r.tag(*n).is_some())
        .collect()
}

fn elements(s: &Screen) -> Vec<NodeId> {
    elements_of(s)
}

fn at(block: usize, offset: usize) -> BlockPoint {
    BlockPoint { block, offset }
}

impl Screen {
    fn input(&self, kind: &str, data: &str, from: BlockPoint, to: BlockPoint) {
        self.r.fire(
            self.page,
            &Event::BeforeInput(BeforeInputEvent {
                input_type: kind.into(),
                data: data.into(),
                start: from,
                end: to,
            }),
        );
    }

    fn select(&self, a: BlockPoint, f: BlockPoint) {
        self.r.fire(
            self.page,
            &Event::EditSelection(EditSelectionEvent {
                anchor: a,
                focus: f,
            }),
        );
    }

    fn blocks(&self) -> Vec<String> {
        elements_of(self)
            .into_iter()
            .map(|n| self.r.text_of(n))
            .collect()
    }
}

#[quench_test::test]
fn the_page_is_an_editable_element_with_a_block_for_each_paragraph() {
    let s = open("one\ntwo");
    assert_eq!(s.r.attr(s.page, "contenteditable").as_deref(), Some("true"));
    assert_eq!(s.blocks(), ["one", "two"]);
}

#[quench_test::test]
fn typing_enter_deleting_and_pasting_change_the_model_and_only_the_paragraphs_that_changed() {
    let s = open("one\ntwo");
    let before = elements(&s);
    s.input("insertText", "X", at(0, 3), at(0, 3));
    assert_eq!(s.blocks(), ["oneX", "two"]);
    let after = elements(&s);
    assert_ne!(before[0], after[0], "the changed paragraph is built again");
    assert_eq!(before[1], after[1], "the other one stays");
    // Where the caret goes after.
    assert!(
        s.r.attr(s.page, "edit-selection")
            .unwrap()
            .starts_with("0,4,0,4")
    );
    s.input("insertParagraph", "", at(0, 2), at(0, 2));
    assert_eq!(s.blocks(), ["on", "eX", "two"]);
    // Backspace: the browser's target range is what is deleted; at the start of a paragraph it joins it to the one before.
    s.input("deleteContentBackward", "", at(1, 0), at(1, 0));
    assert_eq!(
        s.blocks(),
        ["on", "eX", "two"],
        "an empty range deletes nothing"
    );
    s.input("deleteContentBackward", "", at(0, 2), at(1, 0));
    assert_eq!(s.blocks(), ["oneX", "two"]);
    s.input("deleteWordBackward", "", at(1, 0), at(1, 3));
    assert_eq!(s.blocks(), ["oneX", ""]);
    s.input("insertFromPaste", "a\nb", at(1, 0), at(1, 0));
    assert_eq!(s.blocks(), ["oneX", "a", "b"]);
    assert_eq!(s.state.selection.get_untracked().focus, Pos::new(2, 1));
}

#[quench_test::test]
fn formatting_shows_in_the_paragraph_and_the_buttons_follow_the_selection() {
    let s = open("hello world");
    s.select(at(0, 6), at(0, 11));
    let bold =
        s.r.find_by_tag("button")
            .into_iter()
            .find(|b| s.r.text_of(*b) == "B")
            .unwrap();
    assert!(s.r.attr(bold, "class").is_none_or(|c| !c.contains("on")));
    s.r.fire(bold, &Event::Click);
    let spans = s.r.find_by_tag("span");
    assert!(
        spans
            .iter()
            .any(|n| s.r.text_of(*n) == "world" && s.r.attr(*n, "class").as_deref() == Some("b"))
    );
    assert!(s.state.marked(Mark::Bold));
    assert!(
        s.r.attr(bold, "class").unwrap().contains("on"),
        "the button is pressed while the selection is bold"
    );
    assert!(
        s.r.attr(s.page, "edit-selection")
            .unwrap()
            .starts_with("0,6,0,11"),
        "the selection is put back"
    );
    // The browser's own command (Ctrl+B) arrives as an input event.
    s.input("formatBold", "", at(0, 6), at(0, 11));
    assert!(!s.state.marked(Mark::Bold));
    s.select(at(0, 0), at(0, 0));
    s.r.fire(
        s.r.find_by_tag("button")
            .into_iter()
            .find(|b| s.r.attr(*b, "title").as_deref() == Some("Heading 1: a large heading"))
            .unwrap(),
        &Event::Click,
    );
    assert_eq!(s.r.find_by_tag("h1").len(), 1);
    assert_eq!(s.state.doc().paras()[0].kind, Kind::Heading(1));
}

#[quench_test::test]
fn undo_takes_back_typing_as_one_step_and_redo_does_it_again() {
    let s = open("ab");
    for (i, c) in ["c", "d", "e"].into_iter().enumerate() {
        s.input("insertText", c, at(0, 2 + i), at(0, 2 + i));
    }
    s.input("insertParagraph", "", at(0, 5), at(0, 5));
    assert_eq!(s.blocks(), ["abcde", ""]);
    s.input("historyUndo", "", at(0, 0), at(0, 0));
    assert_eq!(s.blocks(), ["abcde"], "the break is one step");
    s.input("historyUndo", "", at(0, 0), at(0, 0));
    assert_eq!(s.blocks(), ["ab"], "the typing is one step");
    s.input("historyRedo", "", at(0, 0), at(0, 0));
    assert_eq!(s.blocks(), ["abcde"]);
    assert!(s.state.can_undo() && s.state.can_redo());
    s.input("insertText", "!", at(0, 5), at(0, 5));
    assert!(!s.state.can_redo(), "a new change ends the redo line");
}

#[quench_test::test]
fn an_input_method_composes_in_the_browser_and_is_read_back_when_it_ends() {
    let s = open("ab\ncd");
    s.input("compositionEnd", "a\u{4e2d}\u{6587}b", at(0, 0), at(0, 4));
    assert_eq!(s.blocks(), ["a\u{4e2d}\u{6587}b", "cd"]);
    assert_eq!(s.state.selection.get_untracked().focus, Pos::new(0, 4));
    // Things the model does not know are reported, not lost silently.
    s.input("insertFromWhatever", "", at(0, 0), at(0, 0));
    assert!(
        s.r.text_of(s.r.find_by_attr("role", "status")[0])
            .contains("Not handled")
    );
}

#[quench_test::test]
fn the_selection_the_browser_reports_is_kept() {
    let s = open("one\ntwo");
    s.select(at(1, 1), at(0, 2));
    let sel = s.state.selection.get_untracked();
    assert_eq!(
        sel,
        Range {
            anchor: Pos::new(1, 1),
            focus: Pos::new(0, 2)
        }
    );
    assert_eq!(sel.ordered(), (Pos::new(0, 2), Pos::new(1, 1)));
}

#[quench_test::test]
fn the_undo_keys_are_the_apps_own() {
    let s = open("ab");
    s.input("insertText", "c", at(0, 2), at(0, 2));
    let key = |k: &str, shift: bool| {
        s.r.fire(
            s.page,
            &Event::KeyDown(KeyEvent {
                key: k.into(),
                ctrl: true,
                shift,
                alt: false,
                meta: false,
            }),
        );
    };
    key("z", false);
    assert_eq!(s.blocks(), ["ab"]);
    key("y", false);
    assert_eq!(s.blocks(), ["abc"]);
    key("z", false);
    key("z", true);
    assert_eq!(s.blocks(), ["abc"], "Ctrl+Shift+Z is redo too");
}

#[quench_test::test]
fn documents_are_opened_and_saved_as_docx_or_text() {
    let s = open("old");
    s.input("insertText", "!", at(0, 3), at(0, 3));
    let bytes = s.state.docx_bytes().unwrap();
    // A new document, then the saved one opened again.
    s.state.new_document();
    assert_eq!(s.blocks(), [""]);
    s.state.load_bytes("saved.docx", &bytes).unwrap();
    assert_eq!(s.blocks(), ["old!"]);
    assert_eq!(s.state.name.get_untracked(), "saved.docx");
    assert!(!s.state.can_undo(), "a document opened has no history");
    s.state.load_bytes("notes.txt", b"one\ntwo").unwrap();
    assert_eq!(s.blocks(), ["one", "two"]);
    assert_eq!(s.state.name.get_untracked(), "notes.docx");
    assert!(s.state.load_bytes("x.pdf", b"").is_err());
    assert!(s.state.load_bytes("broken.docx", b"nope").is_err());
    // Changes are announced so the host can keep the document.
    let n = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&n);
    s.state.on_change(move || counter.set(counter.get() + 1));
    s.input("insertText", "x", at(0, 0), at(0, 0));
    assert_eq!(n.get(), 1);
    // The File menu offers it.
    assert!(
        s.r.find_by_tag("button")
            .iter()
            .any(|b| s.r.text_of(*b) == "File")
    );
}

#[quench_test::test]
fn size_colour_typeface_alignment_and_numbered_lists_show_on_the_page() {
    let s = open("big red words\nsecond");
    s.select(at(0, 0), at(0, 3));
    s.state.set_size(Some(48));
    s.state.set_color(Some(0x00FF_0000));
    s.state.set_family(Some(docs_text::Family::Mono));
    let span =
        s.r.find_by_tag("span")
            .into_iter()
            .find(|n| s.r.text_of(*n) == "big")
            .unwrap();
    assert_eq!(s.r.style_of(span, "font-size").as_deref(), Some("32px"));
    assert_eq!(s.r.style_of(span, "color").as_deref(), Some("#ff0000"));
    assert!(s.r.style_of(span, "font-family").unwrap().contains("Mono"));
    assert_eq!(s.state.common_style().unwrap().size, Some(48));
    s.select(at(1, 0), at(1, 0));
    s.state.set_align(docs_text::Align::Center);
    s.state.set_kind(Kind::Numbered);
    let paras: Vec<_> = elements(&s);
    let class = s.r.attr(paras[1], "class").unwrap();
    assert!(class.contains("ac") && class.contains("li"), "{class}");
    assert_eq!(s.state.align_here(), docs_text::Align::Center);
    // Undo takes the alignment and the list back.
    s.state.undo();
    s.state.undo();
    assert_eq!(s.state.doc().paras()[1].kind, Kind::Paragraph);
}

#[quench_test::test]
fn find_selects_matches_in_turn_and_replace_changes_one_or_all_with_one_undo() {
    let s = open("The cat sat. the CAT ran.\nA cat.");
    s.state.find_query.set("cat".into());
    assert_eq!(s.state.matches().len(), 3);
    s.state.match_case.set(true);
    assert_eq!(s.state.matches().len(), 2);
    s.state.match_case.set(false);
    s.state.find_next(false);
    assert_eq!(
        s.state.selection.get_untracked().ordered().0,
        Pos::new(0, 4)
    );
    s.state.find_next(false);
    assert_eq!(
        s.state.selection.get_untracked().ordered().0,
        Pos::new(0, 17)
    );
    s.state.find_next(true);
    assert_eq!(
        s.state.selection.get_untracked().ordered().0,
        Pos::new(0, 4)
    );
    s.state.replace_with.set("dog".into());
    s.state.replace_current();
    assert_eq!(s.state.doc().text(0), "The dog sat. the CAT ran.");
    s.state.replace_all();
    assert_eq!(s.state.doc().text(0), "The dog sat. the dog ran.");
    assert_eq!(s.state.doc().text(1), "A dog.");
    s.state.undo();
    assert_eq!(s.state.doc().text(1), "A cat.");
    assert_eq!(s.state.doc().text(0), "The dog sat. the CAT ran.");
}

#[quench_test::test]
fn a_link_is_made_from_the_selection_shows_its_address_and_comes_off() {
    let s = open("see the site now");
    s.state.selection.set(Range {
        anchor: Pos::new(0, 4),
        focus: Pos::new(0, 12),
    });
    s.state.open_link();
    assert!(s.state.link_open.get_untracked());
    assert!(
        !s.r.find_by_attr("class", "find-field link-field")
            .is_empty()
    );
    s.state.link_url.set("example.org".into());
    s.state.apply_link();
    assert!(!s.state.link_open.get_untracked());
    assert_eq!(
        s.state.doc().link_at(Pos::new(0, 6)),
        Some("https://example.org")
    );
    let linked = s.r.find_by_attr("title", "https://example.org");
    assert_eq!(linked.len(), 1);
    assert_eq!(s.r.text_of(linked[0]), "the site");
    s.state.remove_link();
    assert_eq!(s.state.doc().link_at(Pos::new(0, 6)), None);
    s.state.undo();
    assert_eq!(
        s.state.doc().link_at(Pos::new(0, 6)),
        Some("https://example.org")
    );
    // A caret alone has nothing to link.
    s.state.selection.set(Range {
        anchor: Pos::new(0, 1),
        focus: Pos::new(0, 1),
    });
    s.state.link_url.set("a@b.c".into());
    s.state.apply_link();
    assert_eq!(s.state.doc().link_at(Pos::new(0, 1)), None);
}

#[quench_test::test]
fn a_table_is_cells_on_the_page_that_tab_and_enter_move_between_and_that_grows_with_rows() {
    let s = open("above");
    s.state.insert_table(2, 3);
    assert_eq!(s.state.doc().paras().len(), 8);
    let cells: Vec<_> = elements(&s)
        .into_iter()
        .filter(|n| s.r.attr(*n, "class").is_some_and(|c| c.contains("cell")))
        .collect();
    assert_eq!(cells.len(), 6);
    // Each cell takes a third of the grid.
    assert!(
        s.r.style_of(cells[0], "grid-column")
            .is_some_and(|v| v == "span 280"),
        "{:?}",
        s.r.style_of(cells[0], "grid-column")
    );
    // The caret is in the first cell; typing goes in it, Tab and Enter go on.
    assert_eq!(s.state.selection.get_untracked().focus.para, 1);
    s.input("insertText", "x", at(1, 0), at(1, 0));
    assert_eq!(s.state.doc().text(1), "x");
    s.input("insertParagraph", "", at(1, 1), at(1, 1));
    assert_eq!(
        s.state.doc().paras().len(),
        8,
        "Enter does not split a cell"
    );
    assert_eq!(s.state.selection.get_untracked().focus.para, 2);
    assert!(s.state.cell_move(true));
    assert_eq!(s.state.selection.get_untracked().focus.para, 1);
    // After the last cell, Tab adds a row.
    s.state.selection.set(Range {
        anchor: Pos::new(6, 0),
        focus: Pos::new(6, 0),
    });
    assert!(s.state.cell_move(false));
    assert_eq!(s.state.doc().table_of(1).map(|t| (t.2, t.3)), Some((3, 3)));
    // Outside a table those keys are the browser's; the table edits say so.
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(0, 0),
    });
    assert!(!s.state.cell_move(false));
    s.state.add_col();
    assert_eq!(s.state.doc().table_of(1).map(|t| t.3), Some(3));
    s.state.selection.set(Range {
        anchor: Pos::new(1, 0),
        focus: Pos::new(1, 0),
    });
    s.state.delete_table();
    assert_eq!(s.state.doc().plain(), "above\n");
    s.state.undo();
    assert_eq!(s.state.doc().table_of(1).map(|t| (t.2, t.3)), Some((3, 3)));
}

fn png_file(w: u32, h: u32) -> Vec<u8> {
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|i| [(i % 256) as u8, 80, 160, 255])
        .collect();
    quench_image::encode_png(w, h, &rgba)
}

#[quench_test::test]
fn a_picture_is_an_image_paragraph_that_scales_prints_and_takes_no_typing() {
    let s = open("above");
    s.state.insert_picture(&png_file(40, 20)).unwrap();
    assert_eq!(s.state.doc().paras().len(), 3);
    assert_eq!(s.state.selection.get_untracked().focus.para, 1);
    let shown = s.r.find_by_tag("img");
    assert_eq!(shown.len(), 1);
    let src = s.r.attr(shown[0], "src").unwrap();
    assert!(src.starts_with("data:image/png;base64,"), "{src}");
    // 40 pixels at 96 to the inch are 30 points, 40 CSS pixels.
    assert_eq!(s.r.style_of(shown[0], "width").as_deref(), Some("40px"));
    s.input("insertText", "x", at(1, 0), at(1, 0));
    assert_eq!(s.state.doc().plain(), "above\n\n");
    s.state.scale_picture(2.0);
    assert_eq!(
        s.r.style_of(s.r.find_by_tag("img")[0], "width").as_deref(),
        Some("80px")
    );
    s.state.undo();
    assert_eq!(
        s.r.style_of(s.r.find_by_tag("img")[0], "width").as_deref(),
        Some("40px")
    );
    // Not an image: the picture is refused and the document is as it was.
    assert!(s.state.insert_picture(b"not a picture").is_err());
    assert_eq!(s.state.doc().paras().len(), 3);
    // The pages view draws it too, and Word's file keeps it.
    s.state.pages_view.set(true);
    assert_eq!(s.r.find_by_tag("img").len(), 1);
    let docx = s.state.docx_bytes().unwrap();
    assert!(
        docs_docx::read(&docx).unwrap().doc.paras()[1]
            .picture
            .is_some()
    );
    // On a paragraph with no picture the size buttons say so.
    s.state.pages_view.set(false);
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(0, 0),
    });
    s.state.scale_picture(2.0);
    assert!(s.state.status.get_untracked().contains("picture"));
}

#[quench_test::test]
fn markdown_opens_as_a_formatted_document_and_the_word_count_counts_the_selection() {
    let s = open("");
    s.state
        .load_bytes("n.md", b"# Head\n\nSome **bold** words here.\n")
        .unwrap();
    assert_eq!(s.state.doc().plain(), "Head\nSome bold words here.");
    assert_eq!(s.state.name.get_untracked(), "n.docx");
    s.state.count_words();
    assert!(
        s.state.status.get_untracked().starts_with("5 words, "),
        "{}",
        s.state.status.get_untracked()
    );
    s.state.selection.set(Range {
        anchor: Pos::new(1, 0),
        focus: Pos::new(1, 9),
    });
    s.state.count_words();
    assert!(
        s.state.status.get_untracked().contains("2 words"),
        "{}",
        s.state.status.get_untracked()
    );
}

#[quench_test::test]
fn the_pages_view_shows_sheets_with_the_text_in_place_of_the_editable_page() {
    let s = open("Hello pages");
    assert!(s.r.find_by_attr("role", "textbox").len() == 1);
    s.state.pages_view.set(true);
    assert!(s.r.find_by_attr("role", "textbox").is_empty());
    assert!(!s.r.find_by_attr("class", "sheet").is_empty());
    assert!(
        s.r.find_by_attr("class", "piece")
            .iter()
            .any(|n| s.r.text_of(*n).contains("Hello"))
    );
    s.state.pages_view.set(false);
    assert!(s.r.find_by_attr("role", "textbox").len() == 1);
}

#[quench_test::test]
fn the_screen_speaks_the_language_picked_and_the_catalogues_agree() {
    let s = open("x");
    let labels = |s: &Screen| -> Vec<String> {
        s.r.find_by_tag("button")
            .into_iter()
            .map(|b| s.r.text_of(b))
            .collect()
    };
    assert!(labels(&s).iter().any(|l| l == "Home"));
    s.state.set_language("de-DE");
    assert!(labels(&s).iter().any(|l| l == "Start"), "{:?}", labels(&s));
    s.state.set_language("pl-PL");
    assert!(labels(&s).iter().all(|l| l != "Home"));
    s.state.set_language("en-US");
    let english = office_i18n::ids(include_str!("../i18n/en-US.ftl"));
    assert!(english.len() > 30);
    for (name, file) in [
        ("de", include_str!("../i18n/de-DE.ftl")),
        ("es", include_str!("../i18n/es-ES.ftl")),
        ("fr", include_str!("../i18n/fr-FR.ftl")),
        ("pl", include_str!("../i18n/pl-PL.ftl")),
    ] {
        assert_eq!(office_i18n::ids(file), english, "{name}");
    }
}

quench_test::main!();

#[quench_test::test]
fn indent_spacing_and_page_break_apply_to_the_paragraphs_selected_and_show_on_the_page() {
    let s = open("one\ntwo");
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(1, 1),
    });
    s.state.indent_by(720);
    s.state.set_line_spacing(1.5);
    assert!(
        s.state
            .doc()
            .paras()
            .iter()
            .all(|p| p.fmt.indent == 720 && p.fmt.line == 360)
    );
    let first = elements(&s)[0];
    assert_eq!(s.r.style_of(first, "margin-left").as_deref(), Some("48px"));
    s.state.indent_by(-5000);
    assert_eq!(s.state.doc().paras()[0].fmt.indent, 0);
    s.state.set_line_spacing(1.0);
    assert_eq!(s.state.doc().paras()[1].fmt.line, 0);
    s.state.selection.set(Range {
        anchor: Pos::new(1, 0),
        focus: Pos::new(1, 0),
    });
    s.state.toggle_page_break();
    assert!(s.state.doc().paras()[1].fmt.page_break && !s.state.doc().paras()[0].fmt.page_break);
    assert!(s.r.attr(elements(&s)[1], "class").unwrap().contains("pb"));
    s.state.undo();
    assert!(!s.state.doc().paras()[1].fmt.page_break);
}

#[quench_test::test]
fn zoom_steps_through_the_sizes_and_the_canvas_follows() {
    let s = open("x");
    let canvas = s.r.find_by_attr("class", "canvas")[0];
    assert_eq!(s.r.style_of(canvas, "zoom").as_deref(), Some("100%"));
    s.state.zoom_by(1);
    s.state.zoom_by(1);
    assert_eq!(s.state.zoom.get_untracked(), 125);
    assert_eq!(s.r.style_of(canvas, "zoom").as_deref(), Some("125%"));
    for _ in 0..10 {
        s.state.zoom_by(-1);
    }
    assert_eq!(s.state.zoom.get_untracked(), 50);
    s.state.zoom_by(0);
    assert_eq!(s.state.zoom.get_untracked(), 100);
}

#[quench_test::test]
fn tab_nests_a_list_item_shift_enter_breaks_the_line_and_markers_come_from_the_model() {
    let s = open("one\ntwo");
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(1, 0),
    });
    s.state.set_kind(Kind::Numbered);
    s.state.selection.set(Range {
        anchor: Pos::new(1, 0),
        focus: Pos::new(1, 0),
    });
    assert!(s.state.nest(1));
    assert_eq!(s.state.doc().paras()[1].fmt.level, 1);
    let second = elements(&s)[1];
    assert_eq!(s.r.attr(second, "data-marker").as_deref(), Some("a."));
    assert_eq!(
        s.r.attr(elements(&s)[0], "data-marker").as_deref(),
        Some("1.")
    );
    assert!(s.state.nest(-5));
    assert_eq!(s.state.doc().paras()[1].fmt.level, 0);
    // Not in a list: Tab is the browser's.
    s.state.set_kind(Kind::Numbered);
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(0, 0),
    });
    s.state.set_kind(Kind::Paragraph);
    s.state.selection.set(Range {
        anchor: Pos::new(0, 0),
        focus: Pos::new(0, 0),
    });
    assert!(!s.state.nest(1));
    // Shift+Enter is a character in the paragraph, drawn as a new line.
    s.input("insertLineBreak", "", at(0, 3), at(0, 3));
    assert_eq!(s.state.doc().paras().len(), 2);
    assert_eq!(s.state.doc().text(0), "one\u{2028}");
}

#[quench_test::test]
fn a_typeface_picked_by_name_is_drawn_by_name_and_keeps_its_kind() {
    let s = open("some words here");
    s.select(at(0, 0), at(0, 4));
    s.state.set_typeface("Georgia");
    assert_eq!(s.state.typeface_name().as_deref(), Some("Georgia"));
    assert_eq!(
        s.state.common_style().and_then(|c| c.family),
        Some(docs_text::Family::Serif)
    );
    let span =
        s.r.find_by_tag("span")
            .into_iter()
            .find(|n| s.r.text_of(*n) == "some")
            .unwrap();
    let css = s.r.style_of(span, "font-family").unwrap();
    assert!(css.starts_with("Georgia"), "{css}");
    // And back to the default.
    s.state.set_typeface("");
    assert_eq!(s.state.typeface_name(), None);
}
