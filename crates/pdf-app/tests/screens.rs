#![allow(clippy::assert_is_empty)]
#![allow(clippy::unwrap_used)]
use pdf_app::{PdfState, pdf_view};
use pdf_write::{Document, Font, Page};
use quench_ui::prelude::*;
use std::rc::Rc;

fn sample() -> Vec<u8> {
    let mut doc = Document::new();
    doc.title("T");
    for (n, text) in [
        "First page about apples",
        "Second page about pears and apples",
        "",
    ]
    .into_iter()
    .enumerate()
    {
        let mut page = Page::new(595.0, 842.0);
        page.text(72.0, 100.0, text, Font::Helvetica, 14.0, 0);
        let _ = n;
        doc.add(page);
    }
    doc.finish()
}

/// Two pages, an outline entry for the second, and on the first a web link and a link to the second.
fn linked() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Annots [5 0 R 8 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
        "<< /Type /Annot /Subtype /Link /Rect [10 20 110 40] /A << /S /URI /URI (https://example.org/) >> >>",
        "<< /Type /Outlines /First 7 0 R /Last 7 0 R /Count 1 >>",
        "<< /Title (Chapter two) /Parent 6 0 R /Dest [4 0 R /Fit] >>",
        "<< /Type /Annot /Subtype /Link /Rect [10 100 60 120] /Dest [4 0 R /Fit] >>",
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

struct Screen {
    r: Rc<TestRenderer>,
    state: PdfState,
    _scope: Scope,
}

fn open() -> Screen {
    let s = open_drawn();
    // The other tests look at the text; the drawn page has its own test.
    s.state.as_picture.set(false);
    s
}

fn open_drawn() -> Screen {
    quench_ui::platform::install(Rc::new(quench_ui::platform::ManualPlatform::new()));
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let mut state = None;
    let (_, scope) = create_root(|| {
        let s = PdfState::new();
        state = Some(s.clone());
        mount(&r, &root, pdf_view(&s))
    });
    Screen {
        r,
        state: state.unwrap(),
        _scope: scope,
    }
}

#[quench_test::test]
fn a_pdf_is_read_page_by_page_and_the_pages_are_listed() {
    let s = open();
    assert!(s.state.load_bytes("a.pdf", b"not a pdf").is_err());
    s.state.load_bytes("a.pdf", &sample()).unwrap();
    assert_eq!(s.state.page_count(), 3);
    assert_eq!(
        s.r.text_of(s.r.find_by_tag("pre")[0]),
        "First page about apples"
    );
    let thumbs: Vec<String> =
        s.r.find_by_tag("button")
            .into_iter()
            .filter(|b| s.r.attr(*b, "class").is_some_and(|c| c.contains("thumb")))
            .map(|b| s.r.text_of(b))
            .collect();
    assert_eq!(thumbs.len(), 3);
    assert!(thumbs[1].contains("Second page"));
    s.state.step(1);
    assert!(s.r.text_of(s.r.find_by_tag("pre")[0]).contains("pears"));
    s.state.go_to(99);
    assert_eq!(s.state.page.get_untracked(), 2, "clamped to the last page");
    assert!(
        s.r.text_of(s.r.find_by_attr("role", "status")[0])
            .contains("3 pages")
    );
    // A page with no text says so.
    assert!(s.r.find_by_tag("pre").is_empty());
}

#[quench_test::test]
fn search_finds_matches_across_pages_highlights_and_steps_round() {
    let s = open();
    s.state.load_bytes("a.pdf", &sample()).unwrap();
    s.state.search("APPLES");
    assert_eq!(s.state.hit_count.get_untracked(), 2);
    assert_eq!(s.state.page.get_untracked(), 0);
    assert_eq!(s.r.find_by_tag("mark").len(), 1);
    assert_eq!(s.r.text_of(s.r.find_by_tag("mark")[0]), "apples");
    s.state.next_hit(false);
    assert_eq!(s.state.page.get_untracked(), 1);
    s.state.next_hit(false);
    assert_eq!(s.state.page.get_untracked(), 0, "it wraps");
    s.state.next_hit(true);
    assert_eq!(s.state.page.get_untracked(), 1);
    s.state.search("zzz");
    assert_eq!(s.state.hit_count.get_untracked(), 0);
    assert!(s.r.find_by_tag("mark").is_empty());
}

#[quench_test::test]
fn zoom_steps_stay_inside_their_range_and_the_keys_move_pages() {
    let s = open();
    s.state.load_bytes("a.pdf", &sample()).unwrap();
    for _ in 0..20 {
        s.state.zoom_by(1);
    }
    let paper =
        s.r.find_by_tag("div")
            .into_iter()
            .find(|d| s.r.attr(*d, "class").as_deref() == Some("paper"))
            .unwrap();
    assert_eq!(s.r.attr(paper, "data-zoom").as_deref(), Some("250"));
    let root =
        s.r.find_by_tag("div")
            .into_iter()
            .find(|d| s.r.attr(*d, "class").as_deref() == Some("pdf"))
            .unwrap();
    s.r.fire(
        root,
        &Event::KeyDown(KeyEvent {
            key: "ArrowRight".into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }),
    );
    assert_eq!(s.state.page.get_untracked(), 1);
    s.r.fire(
        root,
        &Event::KeyDown(KeyEvent {
            key: "End".into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }),
    );
    assert_eq!(s.state.page.get_untracked(), 2);
}

#[quench_test::test]
fn a_page_is_shown_as_it_is_drawn_at_the_zoom_or_as_its_text() {
    let s = open_drawn();
    assert!(pictures(&s.r).is_empty(), "nothing is open yet");
    s.state.load_bytes("t.pdf", &sample()).unwrap();
    let shown = pictures(&s.r);
    assert_eq!(shown.len(), 1);
    let src = s.r.attr(shown[0], "src").unwrap();
    assert!(
        src.starts_with("data:image/svg+xml;utf8,%3Csvg"),
        "{}",
        &src[..60]
    );
    assert!(
        src.contains("First page about apples") || src.contains("tspan"),
        "the text is in the drawing"
    );
    assert!(!src.contains('<') && !src.contains('#') && !src.contains('"'));
    // 595 points are 793.33 pixels at 100%; zoom in and the picture grows.
    assert_eq!(s.r.style_of(shown[0], "width").as_deref(), Some("793px"));
    s.state.zoom_by(1);
    assert_eq!(
        s.r.style_of(pictures(&s.r)[0], "width").as_deref(),
        Some("912px")
    );
    // The text lies over the picture, where the file puts it; a search marks the runs that hold it.
    let runs = s.r.find_by_attr("class", "run");
    assert_eq!(runs.len(), 1);
    assert_eq!(s.r.text_of(runs[0]), "First page about apples");
    assert_eq!(s.r.style_of(runs[0], "left").as_deref(), Some("110.4px"));
    s.state.search("apples");
    assert_eq!(s.r.find_by_attr("class", "run hit current").len(), 1);
    s.state.search("zzz");
    assert!(s.r.find_by_attr("class", "run hit current").is_empty());
    // Another page is another picture; the text view has the text and no picture.
    s.state.go_to(1);
    assert_ne!(s.r.attr(pictures(&s.r)[0], "src").unwrap(), src);
    s.state.as_picture.set(false);
    assert!(pictures(&s.r).is_empty());
    assert!(!s.r.find_by_tag("pre").is_empty());
}

#[quench_test::test]
fn pages_are_turned_moved_deleted_and_added_and_the_file_can_be_saved_with_the_changes() {
    let s = open();
    s.state.load_bytes("t.pdf", &sample()).unwrap();
    assert!(!s.state.edited.get_untracked());
    s.state.go_to(1);
    s.state.move_page(-1);
    assert_eq!(s.state.page.get_untracked(), 0);
    assert!(
        s.state.page_text(0).starts_with("Second") && s.state.page_text(1).starts_with("First")
    );
    assert!(s.state.edited.get_untracked());
    s.state.rotate_page(90);
    let page = s.state.picture(0).unwrap();
    assert_eq!((page.width, page.height), (842.0, 595.0));
    s.state.go_to(2);
    s.state.delete_page();
    assert_eq!(s.state.page_count(), 2);
    // The last page cannot go.
    s.state.delete_page();
    s.state.go_to(0);
    s.state.delete_page();
    assert_eq!(s.state.page_count(), 1);
    s.state.delete_page();
    assert_eq!(s.state.page_count(), 1);
    assert!(s.state.status.get_untracked().contains("at least one"));
    // Pages of another file go on the end; saving offers the file.
    let manual = quench_ui::platform::ManualPlatform::new();
    let manual = Rc::new(manual);
    quench_ui::platform::install(manual.clone());
    s.state.append_file();
    assert!(manual.answer_pick(vec![quench_ui::platform::PickedFile {
        name: "b.pdf".into(),
        mime: "application/pdf".into(),
        bytes: sample()
    }]));
    assert_eq!(s.state.page_count(), 4);
    s.state.save_copy();
    let saved = manual.downloads();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "t.pdf");
    assert_eq!(quench_pdf::extract_pages(&saved[0].2).unwrap().len(), 4);
    s.state.go_to(2);
    s.state.save_page();
    assert_eq!(manual.downloads()[1].0, "t-page-3.pdf");
}

#[quench_test::test]
fn a_files_own_outline_fills_the_sidebar_and_its_links_lie_over_the_page() {
    let s = open_drawn();
    s.state.load_bytes("t.pdf", &linked()).unwrap();
    assert_eq!(s.state.bookmarks().len(), 1);
    let thumbs = s.r.find_by_attr("class", "thumb level-0");
    assert_eq!(thumbs.len(), 1);
    assert_eq!(s.r.text_of(thumbs[0]), "Chapter two");
    s.r.fire(thumbs[0], &Event::Click);
    assert_eq!(s.state.page.get_untracked(), 1);
    s.state.go_to(0);
    let links = s.r.find_by_attr("class", "link");
    assert_eq!(links.len(), 2);
    assert_eq!(
        s.r.attr(links[0], "href").as_deref(),
        Some("https://example.org/")
    );
    assert_eq!(
        s.r.attr(links[0], "rel").as_deref(),
        Some("noopener noreferrer")
    );
    s.r.fire(links[1], &Event::Click);
    assert_eq!(s.state.page.get_untracked(), 1);
}

#[quench_test::test]
fn a_click_on_a_line_marks_it_in_the_file_and_the_change_can_be_taken_back() {
    let s = open_drawn();
    s.state.load_bytes("t.pdf", &sample()).unwrap();
    // Nothing is chosen: a click does nothing.
    s.state.mark_run(0);
    assert!(!s.state.edited.get_untracked());
    s.state.mark.set("Highlight".into());
    s.r.fire(s.r.find_by_attr("class", "run")[0], &Event::Click);
    assert!(s.state.edited.get_untracked());
    assert_eq!(
        s.state.mark.get_untracked(),
        "Highlight",
        "the tool stays chosen"
    );
    let svg = s.state.picture(0).unwrap().address.clone();
    assert!(
        svg.contains("mix-blend-mode:multiply"),
        "the highlight is drawn"
    );
    s.state.mark.set("Text".into());
    s.state.note.set("Check this".into());
    s.state.go_to(1);
    s.state.mark_run(0);
    let saved = quench_pdf::Document::load(&s.state.save_bytes()).unwrap();
    assert_eq!(saved.markup_of(0)[0].kind, "Highlight");
    assert_eq!(saved.markup_of(1)[0].contents, "Check this");
    assert_eq!(saved.page_count(), 3);
    s.state.undo_edit();
    assert!(
        quench_pdf::Document::load(&s.state.save_bytes())
            .unwrap()
            .markup_of(1)
            .is_empty()
    );
    s.state.undo_edit();
    assert!(
        quench_pdf::Document::load(&s.state.save_bytes())
            .unwrap()
            .markup_of(0)
            .is_empty()
    );
    s.state.undo_edit();
    assert!(s.state.status.get_untracked().contains("Nothing"));
}

#[quench_test::test]
fn the_screen_speaks_the_language_picked_and_the_catalogues_agree() {
    let s = open();
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
fn dark_pages_turn_the_colours_of_the_drawn_page() {
    let s = open_drawn();
    s.state.load_bytes("t.pdf", &sample()).unwrap();
    let img = |s: &Screen| pictures(&s.r)[0];
    assert!(!s.r.attr(img(&s), "class").unwrap().contains("dark"));
    s.state.dark_pages.set(true);
    assert!(s.r.attr(img(&s), "class").unwrap().contains("dark"));
}

#[quench_test::test]
fn words_typed_after_a_line_are_put_in_the_file_in_the_embedded_font_and_read_back() {
    let s = open_drawn();
    s.state.load_bytes("t.pdf", &sample()).unwrap();
    s.state.mark.set("Type".into());
    s.r.fire(s.r.find_by_attr("class", "run")[0], &Event::Click);
    assert_eq!(s.state.typing.get_untracked(), Some(0));
    let boxes = s.r.find_by_attr("class", "typebox");
    assert_eq!(boxes.len(), 1);
    s.r.fire(boxes[0], &Event::Input("Zażółć".into()));
    assert_eq!(s.state.typed.get_untracked(), "Zażółć");
    s.r.fire(
        boxes[0],
        &Event::KeyDown(quench_ui::KeyEvent {
            key: "Enter".into(),
            ..Default::default()
        }),
    );
    assert_eq!(s.state.typing.get_untracked(), None);
    assert!(s.state.edited.get_untracked());
    assert!(
        s.state.page_text(0).contains("First page about apples")
            && s.state.page_text(0).contains("Zażółć"),
        "{}",
        s.state.page_text(0)
    );
    s.state.undo_edit();
    assert!(!s.state.page_text(0).contains("Zażółć"));
    // Escape drops what was being typed.
    s.state.mark_run(0);
    let again = s.r.find_by_attr("class", "typebox")[0];
    s.r.fire(
        again,
        &Event::KeyDown(quench_ui::KeyEvent {
            key: "Escape".into(),
            ..Default::default()
        }),
    );
    assert!(s.r.find_by_attr("class", "typebox").is_empty());
}

/// The pictures of the document on screen (not the app's own, in the top bar).
fn pictures(r: &TestRenderer) -> Vec<quench_ui::test_renderer::NodeId> {
    r.find_by_tag("img")
        .into_iter()
        .filter(|n| r.attr(*n, "class").as_deref() != Some("fbar-mark"))
        .collect()
}
