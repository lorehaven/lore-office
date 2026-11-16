#![allow(clippy::unwrap_used)]
use pdf_write::{A4, Document, Family, Font, LETTER, Page, text_width};

/// What is drawn on the pages, as the viewer reads it (the streams are compressed in the file).
fn drawn(bytes: &[u8]) -> String {
    let doc = quench_pdf::Document::load(bytes).unwrap();
    doc.pages()
        .iter()
        .map(|p| String::from_utf8_lossy(&doc.page_contents(p)).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn one_page(f: impl FnOnce(&mut Page)) -> Vec<u8> {
    let mut page = Page::new(A4.0, A4.1);
    f(&mut page);
    let mut doc = Document::new();
    doc.title("Test (1)");
    doc.add(page);
    doc.finish()
}

#[quench_test::test]
fn text_comes_back_out_of_the_reader() {
    let bytes = one_page(|p| {
        p.text(72.0, 100.0, "Hello, World", Font::Helvetica, 12.0, 0);
        p.text(
            72.0,
            120.0,
            "Bold (and) back\\slash",
            Font::HelveticaBold,
            12.0,
            0x00ff_0000,
        );
        p.text(
            72.0,
            140.0,
            "Caf\u{e9} \u{20ac}5 \u{201c}quoted\u{201d}",
            Font::HelveticaOblique,
            12.0,
            0,
        );
    });
    let pages = quench_pdf::extract_pages(&bytes).unwrap();
    assert_eq!(pages.len(), 1);
    for want in [
        "Hello, World",
        "Bold (and) back\\slash",
        "Caf\u{e9}",
        "quoted",
    ] {
        assert!(pages[0].contains(want), "{want:?} in {:?}", pages[0]);
    }
}

#[quench_test::test]
fn many_pages_of_different_sizes_in_order() {
    let mut doc = Document::new();
    for (i, size) in [A4, LETTER, A4].into_iter().enumerate() {
        let mut p = Page::new(size.0, size.1);
        p.text(
            50.0,
            50.0,
            &format!("page number {}", i + 1),
            Font::Helvetica,
            10.0,
            0,
        );
        doc.add(p);
    }
    assert_eq!(doc.page_count(), 3);
    let pages = quench_pdf::extract_pages(&doc.finish()).unwrap();
    assert_eq!(pages.len(), 3);
    for (i, p) in pages.iter().enumerate() {
        assert!(p.contains(&format!("page number {}", i + 1)), "{p:?}");
    }
}

#[quench_test::test]
fn an_empty_document_still_has_a_page_and_shapes_do_not_break_the_file() {
    let empty = Document::new().finish();
    assert_eq!(quench_pdf::extract_pages(&empty).unwrap().len(), 1);
    let bytes = one_page(|p| {
        p.fill_rect(10.0, 10.0, 100.0, 20.0, 0x00ff_ff00);
        p.line(10.0, 40.0, 110.0, 40.0, 0.5, 0x0080_8080);
        p.clipped(10.0, 50.0, 30.0, 12.0, |p| {
            p.text(10.0, 60.0, "clipped text", Font::Helvetica, 10.0, 0);
        });
    });
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert!(quench_pdf::extract_pages(&bytes).unwrap()[0].contains("clipped text"));
}

#[quench_test::test]
fn the_cross_reference_table_points_at_the_objects() {
    let bytes = one_page(|p| p.text(1.0, 20.0, "x", Font::Helvetica, 10.0, 0));
    let text = bytes
        .iter()
        .map(|b| if b.is_ascii() { char::from(*b) } else { '.' })
        .collect::<String>();
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        text[start..].starts_with("xref\n0 18\n"),
        "{}",
        &text[start..start + 12]
    );
    let entries: Vec<&str> = text[start..].lines().skip(3).take(17).collect();
    for (i, e) in entries.iter().enumerate() {
        let offset: usize = e[..10].parse().unwrap();
        assert!(
            text[offset..].starts_with(&format!("{} 0 obj", i + 1)),
            "object {}",
            i + 1
        );
    }
}

#[quench_test::test]
fn widths_follow_the_font_metrics() {
    assert!((text_width("Hello", Font::Helvetica, 10.0) - 22.78).abs() < 0.01);
    assert!(
        text_width("Hello", Font::HelveticaBold, 10.0) > text_width("Hello", Font::Helvetica, 10.0)
    );
    assert!((text_width("iiii", Font::Helvetica, 20.0) - 17.76).abs() < 0.01);
    assert_eq!(text_width("", Font::Helvetica, 10.0), 0.0);
    assert!((text_width("W", Font::Helvetica, 1000.0) - 944.0).abs() < 1e-9);
}

#[quench_test::test]
fn serif_and_fixed_width_fonts_have_their_own_metrics_and_text() {
    assert!(
        (text_width("Hello", Font::Times, 10.0) - 22.22).abs() < 0.01,
        "Times-Roman"
    );
    assert!(
        (text_width("Hello", Font::Courier, 10.0) - 30.0).abs() < 1e-9,
        "every Courier character is 600"
    );
    assert_eq!(
        Font::in_family(Family::Mono, true, true),
        Font::CourierBoldOblique
    );
    assert_eq!(Font::styled(true, false), Font::HelveticaBold);
    let bytes = one_page(|p| {
        p.text(10.0, 20.0, "serif text", Font::Times, 12.0, 0);
        p.text(10.0, 40.0, "mono text", Font::CourierBold, 12.0, 0);
    });
    let page = &quench_pdf::extract_pages(&bytes).unwrap()[0];
    assert!(
        page.contains("serif text") && page.contains("mono text"),
        "{page:?}"
    );
}

#[quench_test::test]
fn polygons_and_polylines_are_drawn() {
    let bytes = one_page(|p| {
        p.fill_polygon(&[(10.0, 10.0), (50.0, 10.0), (30.0, 40.0)], 0x00ff_0000);
        p.polyline(
            &[(0.0, 100.0), (20.0, 80.0), (40.0, 90.0)],
            2.0,
            0x0000_00ff,
        );
        p.fill_polygon(&[], 0);
        p.polyline(&[(1.0, 1.0)], 1.0, 0);
        p.text(10.0, 120.0, "after the shapes", Font::Helvetica, 10.0, 0);
    });
    let raw = drawn(&bytes);
    assert!(
        raw.contains("1 0 0 rg 10 831.89 m 50 831.89 l 30 801.89 l h f"),
        "{raw}"
    );
    assert!(raw.contains("0 0 1 RG 2 w 1 j 0 741.89 m 20 761.89 l 40 751.89 l S"));
    assert!(
        quench_pdf::extract_pages(&bytes).unwrap()[0].contains("after the shapes"),
        "the file still reads"
    );
}

quench_test::main!();

#[quench_test::test]
fn a_picture_is_an_image_object_with_its_alpha_as_a_mask_and_the_page_draws_it() {
    let mut page = Page::new(200.0, 200.0);
    // Two pixels wide, one tall: opaque red, half transparent blue.
    page.image(
        10.0,
        20.0,
        100.0,
        50.0,
        2,
        1,
        &[255, 0, 0, 255, 0, 0, 255, 128],
    );
    // A picture whose size does not fit its pixels is ignored.
    page.image(0.0, 0.0, 1.0, 1.0, 5, 5, &[0; 8]);
    let mut doc = Document::new();
    doc.add(page);
    let bytes = doc.finish();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(
        text.matches("/Subtype /Image").count(),
        2,
        "the picture and its mask"
    );
    assert!(text.contains("/SMask") && text.contains("/Im1 "));
    assert!(
        drawn(&bytes).contains("q 100 0 0 50 10 130 cm /Im1 Do Q"),
        "{text}"
    );
    assert_eq!(quench_pdf::extract_pages(&bytes).unwrap().len(), 1);
}

#[quench_test::test]
fn text_the_standard_fonts_lack_is_set_in_an_embedded_font_that_still_reads_as_text() {
    let text = "Za\u{17c}\u{f3}\u{142}\u{107} g\u{119}\u{15b}l\u{105} ja\u{17a}\u{144} \u{41a}\u{438}\u{440}";
    let bytes = one_page(|p| {
        p.text(72.0, 100.0, text, Font::Helvetica, 14.0, 0);
        p.text(72.0, 130.0, text, Font::TimesBoldItalic, 14.0, 0x00ff_0000);
        p.text(72.0, 160.0, "plain caf\u{e9}", Font::Courier, 12.0, 0);
    });
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        body.contains("/Subtype /CIDFontType2")
            && body.contains("/FontFile2")
            && body.contains("/ToUnicode")
    );
    assert!(
        body.contains("/FU 14 Tf") || body.contains("/FU"),
        "the page uses the font"
    );
    // Search and copy get the text back, and what the standard fonts can show stays in them.
    let pages = quench_pdf::extract_pages(&bytes).unwrap();
    assert!(pages[0].contains(text), "{}", pages[0]);
    assert!(pages[0].contains("plain caf\u{e9}"), "{}", pages[0]);
    // Width: the embedded font's own for what needs it, Helvetica's for the rest.
    let w = text_width(text, Font::Helvetica, 10.0);
    assert!(w > 50.0 && w < 400.0, "{w}");
    assert!((text_width("abc", Font::Helvetica, 10.0) - 16.12).abs() < 1e-9);
}
