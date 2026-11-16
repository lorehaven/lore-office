#![allow(clippy::literal_string_with_formatting_args)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_possible_truncation)]
#![allow(
    clippy::unwrap_used,
    clippy::unreadable_literal,
    clippy::format_push_string
)]
use docs_print::{A4, LETTER, Setup, layout, to_pdf};
use docs_text::{Align, Doc, Family, Kind, Mark, Pos, Style};

fn pages(bytes: &[u8]) -> Vec<String> {
    quench_pdf::extract_pages(bytes).unwrap()
}

#[quench_test::test]
fn a_short_document_is_one_page_with_its_text_and_a_number() {
    let mut d =
        Doc::from_text("Title\nA paragraph with some words.\nitem one\nitem two\nfirst\nsecond");
    d.set_kind(0, Kind::Heading(1));
    d.set_kind(2, Kind::Bullet);
    d.set_kind(3, Kind::Bullet);
    d.set_kind(4, Kind::Numbered);
    d.set_kind(5, Kind::Numbered);
    d.toggle(Pos::new(1, 2), Pos::new(1, 11), Mark::Bold);
    let out = pages(&to_pdf(&d, &Setup::default(), "Doc"));
    assert_eq!(out.len(), 1);
    for want in [
        "Title",
        "A paragraph with some words.",
        "\u{2022}",
        "item one",
        "1.",
        "2.",
        "second",
        "1 / 1",
    ] {
        assert!(out[0].contains(want), "{want} in {:?}", out[0]);
    }
}

#[quench_test::test]
fn long_text_wraps_and_flows_onto_more_pages_and_numbers_restart() {
    let words = "lorem ipsum dolor sit amet ".repeat(60);
    let mut text = String::new();
    for i in 0..40 {
        text.push_str(&format!("{i} {words}\n"));
    }
    let d = Doc::from_text(text.trim_end());
    let laid = layout(&d, &Setup::default());
    assert!(laid.pages.len() > 3, "{} pages", laid.pages.len());
    let (top, bottom) = (72.0, A4.height - 72.0);
    for items in &laid.pages {
        for i in items.iter().filter(|i| i.size > 9.5) {
            assert!(i.y >= top && i.y <= bottom + 1.0, "{} at {}", i.text, i.y);
            assert!(
                i.x >= 71.9 && i.x < A4.width - 72.0,
                "{} at x {}",
                i.text,
                i.x
            );
        }
    }
    let mut n = Doc::from_text("a\nb\nbreak\nc\nd");
    for i in [0, 1, 3, 4] {
        n.set_kind(i, Kind::Numbered);
    }
    let out = &pages(&to_pdf(&n, &Setup::default(), "n"))[0];
    assert_eq!(
        out.matches("1.").count(),
        2,
        "two lists, both from 1: {out:?}"
    );
}

#[quench_test::test]
fn alignment_sizes_colours_and_other_paper_are_honoured() {
    let mut d = Doc::from_text(
        "left\ncentred\nright\nsome justified text that wraps over more than one line because it is long enough to do so for sure yes",
    );
    d.set_align(1, Align::Center);
    d.set_align(2, Align::Right);
    d.set_align(3, Align::Justify);
    d.restyle(Pos::new(0, 0), Pos::new(0, 4), |s| Style {
        size: Some(48),
        color: Some(0xFF0000),
        family: Some(Family::Mono),
        ..s
    });
    let setup = Setup {
        paper: LETTER,
        margins: (50.0, 60.0, 50.0, 80.0),
        page_numbers: false,
        ..Setup::default()
    };
    let laid = layout(&d, &setup);
    let find = |t: &str| {
        laid.pages[0]
            .iter()
            .find(|i| i.text.starts_with(t))
            .unwrap()
    };
    assert!((find("left").x - 80.0).abs() < 0.01);
    assert_eq!((find("left").size, find("left").color), (24.0, 0x00FF_0000));
    let centred = find("centred");
    let w = pdf_write::text_width(&centred.text, centred.font, centred.size);
    assert!((centred.x - (80.0 + (LETTER.width - 140.0 - w) / 2.0)).abs() < 0.01);
    let right = find("right");
    let w = pdf_write::text_width(&right.text, right.font, right.size);
    assert!((right.x + w - (LETTER.width - 60.0)).abs() < 0.01);
    // A justified line is stretched to the margins; its last line is not.
    let just: Vec<_> = laid.pages[0]
        .iter()
        .filter(|i| i.y > find("right").y + 1.0)
        .collect();
    assert!(just.len() > 3, "pieces drawn apart: {}", just.len());
    let mut by_line: std::collections::BTreeMap<i64, Vec<&docs_print::Item>> =
        std::collections::BTreeMap::new();
    for i in just {
        by_line.entry(i.y as i64).or_default().push(i);
    }
    let first = by_line.values().next().unwrap();
    let end = first
        .iter()
        .map(|i| i.x + pdf_write::text_width(&i.text, i.font, i.size))
        .fold(0.0, f64::max);
    assert!(
        (end - (LETTER.width - 60.0)).abs() < 0.5,
        "ends at the right margin: {end}"
    );
    // The PDF's media box is the paper.
    let pdf = String::from_utf8_lossy(&to_pdf(&d, &setup, "t")).to_string();
    assert!(pdf.contains("/MediaBox [0 0 612 792]"));
    assert_eq!(
        pages(&to_pdf(&Doc::new(), &Setup::default(), "empty")).len(),
        1
    );
}

quench_test::main!();

#[quench_test::test]
fn a_table_is_laid_in_equal_columns_with_borders_and_rows_that_do_not_split() {
    let mut d = Doc::from_text("Above");
    d.insert_table(0, 3, 2);
    for (i, w) in ["one", "two", "three", "four", "five", "six"]
        .iter()
        .enumerate()
    {
        d.replace(Pos::new(1 + i, 0), Pos::new(1 + i, 0), w);
    }
    let setup = Setup::default();
    let laid = layout(&d, &setup);
    assert_eq!(laid.pages.len(), 1);
    let at = |t: &str| laid.pages[0].iter().find(|i| i.text == t).unwrap();
    // Two columns across the measure; rows go down; the table starts below the paragraph above.
    assert!((at("two").x - at("one").x - (595.28 - 144.0) / 2.0).abs() < 0.01);
    assert!(at("three").y > at("one").y && at("five").y > at("three").y);
    assert!(at("one").y > at("Above").y);
    // Borders: 4 horizontal lines, 3 vertical lines for each of the 3 rows.
    let (h, v): (Vec<&docs_print::Rule>, Vec<&docs_print::Rule>) =
        laid.rules[0].iter().partition(|r| r.y1 == r.y2);
    assert_eq!((h.len(), v.len()), (4, 9));
    // The text is in the PDF in reading order.
    let text = pages(&to_pdf(&d, &setup, "T")).join("\n");
    assert!(
        text.find("one").unwrap() < text.find("two").unwrap()
            && text.find("two").unwrap() < text.find("three").unwrap()
    );
    // A long table goes on to the next page by whole rows, and each page has its own borders.
    let mut long = Doc::new();
    long.insert_table(0, 80, 2);
    let laid = layout(&long, &setup);
    assert!(laid.pages.len() >= 2);
    assert!(laid.rules.iter().all(|r| !r.is_empty()));
    assert_eq!(laid.rules.len(), laid.pages.len());
}

fn png(w: u32, h: u32) -> docs_text::Picture {
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|i| [(i % 256) as u8, 40, 200, 255])
        .collect();
    docs_text::Picture {
        mime: "image/png",
        data: std::rc::Rc::new(quench_image::encode_png(w, h, &rgba)),
        width: 20000,
        height: 10000,
    }
}

#[quench_test::test]
fn a_picture_is_placed_by_its_alignment_scaled_to_the_measure_and_drawn_in_the_pdf() {
    let mut d = Doc::from_text("Above");
    d.insert_picture(0, png(4, 2));
    d.set_align(1, Align::Center);
    let setup = Setup::default();
    let laid = layout(&d, &setup);
    let im = &laid.images[0][0];
    assert_eq!((im.width, im.height), (200.0, 100.0));
    assert!((im.x - (72.0 + (451.28 - 200.0) / 2.0)).abs() < 0.01);
    assert!(im.y > laid.pages[0].iter().find(|i| i.text == "Above").unwrap().y);
    let bytes = to_pdf(&d, &setup, "T");
    assert!(String::from_utf8_lossy(&bytes).contains("/Subtype /Image"));
    // One wider than the page is scaled to fit, and a tall one goes to the next page.
    let mut wide = Doc::new();
    let mut p = png(4, 2);
    (p.width, p.height) = (100000, 50000);
    wide.insert_picture(0, p.clone());
    wide.insert_picture(1, p);
    let laid = layout(&wide, &setup);
    assert_eq!(laid.images[0][0].width, 451.28);
    assert_eq!(laid.images.len(), laid.pages.len());
    assert_eq!(laid.images.iter().map(Vec::len).sum::<usize>(), 2);
}

#[quench_test::test]
fn raised_text_is_smaller_and_higher_and_a_highlight_is_a_box_behind_the_text() {
    let mut d = Doc::from_text("x2 mark");
    d.toggle(Pos::new(0, 1), Pos::new(0, 2), Mark::Superscript);
    d.toggle(Pos::new(0, 3), Pos::new(0, 7), Mark::Highlight);
    d.toggle(Pos::new(0, 3), Pos::new(0, 7), Mark::Strike);
    let laid = layout(&d, &Setup::default());
    let at = |t: &str| laid.pages[0].iter().find(|i| i.text == t).unwrap();
    assert!(at("2").size < at("x").size && at("2").y < at("x").y);
    assert!(at("mark").strike && at("mark").highlight == Some(0x00FF_FF00));
    let pdf = to_pdf(&d, &Setup::default(), "T");
    assert!(
        quench_pdf::Document::load(&pdf)
            .unwrap()
            .render_page(0)
            .unwrap()
            .svg
            .contains("#ffff00"),
        "the box is filled yellow"
    );
}

#[quench_test::test]
fn a_link_is_a_clickable_box_in_the_pdf_over_the_text_it_is_on() {
    let mut d = Doc::from_text("see the site now");
    d.set_link(
        Pos::new(0, 4),
        Pos::new(0, 12),
        Some("https://example.org/a(b)"),
    );
    let pdf = to_pdf(&d, &Setup::default(), "T");
    let doc = quench_pdf::Document::load(&pdf).unwrap();
    let links = doc.render_page(0).unwrap().links;
    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].uri.as_deref(), Some("https://example.org/a(b)"));
    // Over the words, at the left margin plus what comes before.
    assert!(links[0].x > 72.0 && links[0].y < 100.0 && links[0].width > 20.0);
}

#[quench_test::test]
fn indents_line_spacing_and_page_breaks_change_the_layout() {
    let setup = Setup::default();
    let mut d = Doc::from_text("alpha\nbeta\ngamma");
    d.set_fmt(0, |f| docs_text::ParaFmt {
        indent: 1440,
        first: 720,
        ..f
    });
    d.set_fmt(1, |f| docs_text::ParaFmt { line: 480, ..f });
    let laid = layout(&d, &setup);
    let at = |t: &str| laid.pages[0].iter().find(|i| i.text == t).unwrap();
    assert!((at("alpha").x - (72.0 + 72.0 + 36.0)).abs() < 0.01);
    // Double line spacing doubles the line's height: the gap to the next paragraph grows.
    let plain = layout(&Doc::from_text("alpha\nbeta\ngamma"), &setup);
    let gap = |l: &docs_print::Layout| {
        let f = |t: &str| l.pages[0].iter().find(|i| i.text == t).unwrap().y;
        f("gamma") - f("beta")
    };
    assert!(gap(&laid) > gap(&plain) + 10.0);
    d.set_fmt(2, |f| docs_text::ParaFmt {
        page_break: true,
        ..f
    });
    let broken = layout(&d, &setup);
    assert_eq!(broken.pages.len(), 2);
    assert!(broken.pages[1].iter().any(|i| i.text == "gamma"));
}

#[quench_test::test]
fn a_header_and_a_footer_with_page_numbers_are_on_every_page_and_letter_paper_is_used() {
    let mut d = Doc::from_text(&vec!["line of text"; 120].join("\n"));
    d.set_header("Report");
    d.set_footer("Page {page} of {pages}");
    let laid = layout(&d, &Setup::for_doc(&d));
    let n = laid.pages.len();
    assert!(n >= 2);
    for (i, page) in laid.pages.iter().enumerate() {
        assert!(page.iter().any(|t| t.text == "Report" && t.y < 72.0));
        let want = format!("Page {} of {n}", i + 1);
        assert!(page.iter().any(|t| t.text == want && t.y > 700.0), "{want}");
        assert!(
            !page.iter().any(|t| t.text.contains(" / ")),
            "the footer replaces the plain page number"
        );
    }
    d.set_paper(docs_text::PaperSize::Letter);
    assert_eq!(Setup::for_doc(&d).paper, docs_print::LETTER);
}

#[quench_test::test]
fn columns_take_their_widths_and_a_header_row_comes_again_on_the_next_page() {
    let mut d = Doc::new();
    d.insert_table(0, 90, 2);
    let t = d.cell_at(1).unwrap().table;
    d.set_table_widths(t, &[1, 3]);
    d.replace(Pos::new(1, 0), Pos::new(1, 0), "HEAD");
    d.replace(Pos::new(2, 0), Pos::new(2, 0), "second");
    d.set_table_header(t, true);
    let laid = layout(&d, &Setup::default());
    assert!(laid.pages.len() >= 2);
    let at = |page: usize, text: &str| {
        laid.pages[page]
            .iter()
            .find(|i| i.text == text)
            .map(|i| i.x)
    };
    // The second column starts a quarter of the way across (451.28 points of measure).
    assert!((at(0, "second").unwrap() - at(0, "HEAD").unwrap() - 451.28 / 4.0).abs() < 0.01);
    assert!(
        at(1, "HEAD").is_some(),
        "the header row is set again on the next page"
    );
}

#[quench_test::test]
fn nested_items_sit_further_in_with_their_own_markers_and_a_line_break_starts_a_new_line() {
    let mut d = Doc::from_text("outer\ninner\nafter\nfirst line\u{2028}second line");
    for i in 0..3 {
        d.set_kind(i, Kind::Numbered);
    }
    d.set_fmt(1, |f| docs_text::ParaFmt { level: 1, ..f });
    let laid = layout(&d, &Setup::default());
    let at = |t: &str| laid.pages[0].iter().find(|i| i.text == t).unwrap();
    assert!(at("inner").x > at("outer").x + 10.0);
    assert!(
        laid.pages[0].iter().any(|i| i.text == "a."),
        "the marker of level 2"
    );
    assert!(
        laid.pages[0].iter().any(|i| i.text == "2."),
        "numbering goes on at the first level"
    );
    assert!(at("second line").y > at("first line").y + 10.0);
    assert!((at("second line").x - at("first line").x).abs() < 0.01);
}

#[quench_test::test]
fn the_layout_says_which_paragraphs_start_a_page_and_paper_sizes_set_the_sheet() {
    let mut d = Doc::from_text(
        &(1..=120)
            .map(|i| format!("Paragraph {i} with a few words in it."))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let laid = layout(&d, &Setup::for_doc(&d));
    assert!(laid.pages.len() > 2);
    assert_eq!(laid.breaks.len(), laid.pages.len() - 1);
    assert!(laid.breaks.windows(2).all(|w| w[0] < w[1]) && laid.breaks[0] > 0);
    d.set_paper(docs_text::PaperSize::A5);
    let small = Setup::for_doc(&d);
    assert!(
        (small.paper.width - 419.55).abs() < 0.1,
        "{:?}",
        small.paper
    );
    assert!(layout(&d, &small).pages.len() > laid.pages.len());
}
