#![allow(
    clippy::missing_const_for_fn,
    clippy::let_unit_value,
    clippy::match_single_binding
)]
#![allow(clippy::literal_string_with_formatting_args)]
#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]
use docs_docx::{read, write, write_new};
use docs_text::{Align, Doc, Family, Kind, Mark, Pos, Style};
use office_fmt::Package;

fn sample() -> Doc {
    let mut d = Doc::from_text(
        "Title\nSome bold and italic text\nitem one\nitem two\nTab\tseparated & <escaped>",
    );
    d.set_kind(0, Kind::Heading(1));
    d.set_kind(2, Kind::Bullet);
    d.set_kind(3, Kind::Bullet);
    d.toggle(Pos::new(1, 5), Pos::new(1, 9), Mark::Bold);
    d.toggle(Pos::new(1, 14), Pos::new(1, 20), Mark::Italic);
    d.toggle(Pos::new(1, 5), Pos::new(1, 9), Mark::Underline);
    d
}

#[quench_test::test]
fn a_document_written_and_read_again_is_the_same_document() {
    let doc = sample();
    let bytes = write_new(&doc).unwrap();
    let back = read(&bytes).unwrap();
    assert_eq!(back.doc.plain(), doc.plain());
    assert_eq!(back.doc.paras().len(), doc.paras().len());
    for (a, b) in back.doc.paras().iter().zip(doc.paras()) {
        assert_eq!((a.kind, &a.runs), (b.kind, &b.runs));
    }
    assert!(!back.report.is_lossy());
    // The package is a real one: the parts Word wants are there.
    let pkg = Package::read(&bytes).unwrap();
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/numbering.xml",
        "word/_rels/document.xml.rels",
    ] {
        assert!(pkg.get(part).is_some(), "{part}");
    }
    let xml = String::from_utf8(pkg.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("<w:pStyle w:val=\"Heading1\"/>")
            && xml.contains("<w:b/>")
            && xml.contains("<w:tab/>")
            && xml.contains("&amp; &lt;escaped&gt;"),
        "{xml}"
    );
}

#[quench_test::test]
fn a_file_that_was_read_keeps_its_other_parts_and_page_setup_when_written_back() {
    let first = write_new(&sample()).unwrap();
    let mut pkg = Package::read(&first).unwrap();
    pkg.set("word/theme/theme1.xml", b"<theme/>".to_vec());
    let doc_xml = String::from_utf8(pkg.get("word/document.xml").unwrap().to_vec())
        .unwrap()
        .replace(
            "</w:body>",
            "<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr></w:body>",
        );
    pkg.set("word/document.xml", doc_xml.into_bytes());
    let file = pkg.write().unwrap();
    let mut loaded = read(&file).unwrap();
    loaded.doc.replace(Pos::new(1, 0), Pos::new(1, 0), "NEW ");
    let out = write(&loaded.doc, &loaded.package).unwrap();
    let again = Package::read(&out).unwrap();
    assert_eq!(again.get("word/theme/theme1.xml"), Some(&b"<theme/>"[..]));
    let xml = String::from_utf8(again.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("w:pgSz") && xml.contains("NEW "),
        "the page setup stays"
    );
    assert_eq!(
        read(&out).unwrap().doc.text(1),
        "NEW Some bold and italic text"
    );
    // Writing twice does not pile up styles or lists.
    let twice = write(&read(&out).unwrap().doc, &again).unwrap();
    let styles = String::from_utf8(
        Package::read(&twice)
            .unwrap()
            .get("word/styles.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(styles.matches("w:styleId=\"Heading1\"").count(), 1);
}

#[quench_test::test]
fn merged_cells_are_split_in_the_grid_and_pictures_that_cannot_be_shown_are_reported() {
    let body = "<w:p><w:r><w:t>before</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>d</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:drawing/></w:r><w:hyperlink><w:r><w:rPr><w:u w:val=\"single\"/></w:rPr><w:t>link</w:t></w:r></w:hyperlink></w:p>";
    let mut pkg = Package::new();
    pkg.set("word/document.xml", format!("<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>").into_bytes());
    let loaded = read(&pkg.write().unwrap()).unwrap();
    assert_eq!(loaded.doc.plain(), "before\na\n\nb\nc\nd\n\nlink");
    assert_eq!(
        loaded.doc.table_of(1).map(|t| (t.2, t.3)),
        Some((2, 3)),
        "the merged cell is two columns, the first holding the text; a short row is filled out"
    );
    assert_eq!((loaded.report.tables, loaded.report.pictures), (1, 1));
    assert!(loaded.report.is_lossy());
    assert!(
        loaded
            .doc
            .all_marked(Pos::new(7, 0), Pos::new(7, 4), Mark::Underline),
        "runs inside a hyperlink keep their style"
    );
}

#[quench_test::test]
fn a_plain_grid_is_kept_as_a_table_with_the_paragraphs_of_a_cell_joined() {
    let body = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t>a2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>after</w:t></w:r></w:p>";
    let mut pkg = Package::new();
    pkg.set("word/document.xml", format!("<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>").into_bytes());
    let loaded = read(&pkg.write().unwrap()).unwrap();
    assert_eq!(loaded.doc.plain(), "a a2\nb\nafter");
    assert!(!loaded.report.is_lossy());
    assert_eq!(loaded.doc.table_of(0).map(|t| (t.2, t.3)), Some((1, 2)));
    assert_eq!(loaded.doc.cell_at(2), None);
}

#[quench_test::test]
fn what_is_not_a_document_is_an_error() {
    assert!(read(b"not a zip").is_err());
    assert!(
        read(&Package::new().write().unwrap()).is_err(),
        "no word/document.xml"
    );
}

#[quench_test::test]
fn sizes_colours_typefaces_alignment_and_numbered_lists_round_trip() {
    let mut d = Doc::from_text("Centred title\nbig red serif text\none\ntwo\nbreak\nuno\ndos");
    d.set_align(0, Align::Center);
    d.set_align(1, Align::Justify);
    d.restyle(Pos::new(1, 4), Pos::new(1, 7), |s| Style {
        size: Some(36),
        color: Some(0x00C0_0000),
        family: Some(Family::Serif),
        ..s
    });
    for i in [2, 3, 5, 6] {
        d.set_kind(i, Kind::Numbered);
    }
    let bytes = write_new(&d).unwrap();
    let back = read(&bytes).unwrap();
    for (a, b) in back.doc.paras().iter().zip(d.paras()) {
        assert_eq!(
            (a.kind, a.align, &a.runs),
            (b.kind, b.align, &b.runs),
            "{}",
            b.runs.iter().map(|r| r.text.as_str()).collect::<String>()
        );
    }
    // Two runs of numbered paragraphs are two lists that both start at 1.
    let pkg = Package::read(&bytes).unwrap();
    let numbering = String::from_utf8(pkg.get("word/numbering.xml").unwrap().to_vec()).unwrap();
    assert_eq!(
        numbering.matches("<w:startOverride w:val=\"1\"/>").count(),
        2,
        "{numbering}"
    );
    let xml = String::from_utf8(pkg.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("<w:jc w:val=\"center\"/>")
            && xml.contains("<w:jc w:val=\"both\"/>")
            && xml.contains("<w:sz w:val=\"36\"/>")
            && xml.contains("<w:color w:val=\"C00000\"/>"),
        "{xml}"
    );
    assert!(xml.contains("Times New Roman"));
}

quench_test::main!();

#[quench_test::test]
fn links_survive_a_round_trip_and_are_real_hyperlinks_in_the_file() {
    let mut d = Doc::from_text("See the site and the other one.");
    d.set_link(
        Pos::new(0, 8),
        Pos::new(0, 16),
        Some("https://example.org/a?x=1&y=2"),
    );
    d.set_link(
        Pos::new(0, 21),
        Pos::new(0, 30),
        Some("mailto:me@example.org"),
    );
    let bytes = write_new(&d).unwrap();
    let back = read(&bytes).unwrap().doc;
    assert_eq!(back.plain(), d.plain());
    assert_eq!(
        back.link_at(Pos::new(0, 10)),
        Some("https://example.org/a?x=1&y=2")
    );
    assert_eq!(back.link_at(Pos::new(0, 25)), Some("mailto:me@example.org"));
    assert_eq!(back.link_at(Pos::new(0, 4)), None);
    let package = Package::read(&bytes).unwrap();
    let doc_xml = String::from_utf8(package.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(doc_xml.contains("<w:hyperlink r:id="), "{doc_xml}");
    let rels = String::from_utf8(
        package
            .get("word/_rels/document.xml.rels")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        rels.contains("TargetMode=\"External\"") && rels.contains("x=1&amp;y=2"),
        "{rels}"
    );
    // Writing again over the package it came in leaves one relationship for each link.
    let again = write(&back, &read(&bytes).unwrap().package).unwrap();
    let rels = String::from_utf8(
        Package::read(&again)
            .unwrap()
            .get("word/_rels/document.xml.rels")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(rels.matches("/hyperlink\"").count(), 2, "{rels}");
}

#[quench_test::test]
fn a_table_is_a_real_table_in_the_file_and_comes_back_as_one() {
    let mut d = Doc::from_text("Intro\nOutro");
    d.insert_table(0, 2, 3);
    for (i, w) in ["a", "b", "c", "d", "e", "f"].iter().enumerate() {
        d.replace(Pos::new(1 + i, 0), Pos::new(1 + i, 0), w);
    }
    d.toggle(Pos::new(1, 0), Pos::new(1, 1), Mark::Bold);
    let bytes = write_new(&d).unwrap();
    let package = Package::read(&bytes).unwrap();
    let xml = String::from_utf8(package.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert_eq!(xml.matches("<w:tr>").count(), 2, "{xml}");
    assert_eq!(xml.matches("<w:tc>").count(), 6);
    assert!(xml.contains("<w:tblGrid>"));
    let back = read(&bytes).unwrap();
    assert!(!back.report.is_lossy());
    assert_eq!(back.doc.plain(), d.plain());
    assert_eq!(back.doc.table_of(1).map(|t| (t.2, t.3)), Some((2, 3)));
    assert_eq!(back.doc.paras().iter().filter_map(|p| p.cell).count(), 6);
    assert!(
        back.doc
            .all_marked(Pos::new(1, 0), Pos::new(1, 1), Mark::Bold)
    );
    // Written over the package it came in, the table stays one.
    let again = read(&write(&back.doc, &back.package).unwrap()).unwrap();
    assert_eq!(again.doc.plain(), d.plain());
}

#[quench_test::test]
fn a_table_that_ends_the_document_gets_a_paragraph_after_it() {
    let mut d = Doc::new();
    d.insert_table(0, 1, 2);
    let paras = d.paras().len();
    let mut only = d.clone();
    only.replace(Pos::new(paras - 1, 0), Pos::new(paras - 1, 0), "");
    let bytes = write_new(&d).unwrap();
    let xml = String::from_utf8(
        Package::read(&bytes)
            .unwrap()
            .get("word/document.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        xml.find("</w:tbl>").unwrap() < xml.rfind("<w:p").unwrap() || xml.contains("</w:tbl><w:p")
    );
}

fn png(w: u32, h: u32) -> docs_text::Picture {
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|i| [(i * 7 % 256) as u8, 90, 200, 255])
        .collect();
    docs_text::Picture {
        mime: "image/png",
        data: std::rc::Rc::new(quench_image::encode_png(w, h, &rgba)),
        width: 15000,
        height: 7500,
    }
}

#[quench_test::test]
fn a_picture_is_a_media_part_with_a_relationship_and_comes_back_at_its_size() {
    let mut d = Doc::from_text("Before\nAfter");
    d.insert_picture(0, png(8, 4));
    d.set_align(1, Align::Center);
    let bytes = write_new(&d).unwrap();
    let package = Package::read(&bytes).unwrap();
    assert!(package.get("word/media/lore0.png").is_some());
    let xml = String::from_utf8(package.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("<a:blip r:embed=") && xml.contains("cx=\"1905000\""),
        "{xml}"
    );
    let ct = String::from_utf8(package.get("[Content_Types].xml").unwrap().to_vec()).unwrap();
    assert!(ct.contains("image/png"), "{ct}");
    let back = read(&bytes).unwrap();
    assert!(!back.report.is_lossy());
    assert_eq!(back.doc.paras().len(), d.paras().len());
    assert_eq!(back.doc.paras()[1].picture, Some(0));
    assert_eq!(back.doc.paras()[1].align, Align::Center);
    let (a, b) = (back.doc.picture(0).unwrap(), d.picture(0).unwrap());
    assert_eq!(
        (a.size(), a.mime, a.data.as_slice()),
        (b.size(), b.mime, b.data.as_slice())
    );
    // Written again over the package it came in, there is still one media part for it and one relationship.
    let again = write(&back.doc, &back.package).unwrap();
    let rels = String::from_utf8(
        Package::read(&again)
            .unwrap()
            .get("word/_rels/document.xml.rels")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(rels.matches("media/lore0.png").count(), 1, "{rels}");
    assert_eq!(
        read(&again).unwrap().doc.picture(0).map(|p| p.size()),
        Some((150.0, 75.0))
    );
}

#[quench_test::test]
fn strike_raised_lowered_and_highlighted_text_round_trip_and_are_real_run_properties() {
    let mut d = Doc::from_text("a strike b sup c sub d lit");
    d.toggle(Pos::new(0, 2), Pos::new(0, 8), Mark::Strike);
    d.toggle(Pos::new(0, 11), Pos::new(0, 14), Mark::Superscript);
    d.toggle(Pos::new(0, 17), Pos::new(0, 20), Mark::Subscript);
    d.toggle(Pos::new(0, 23), Pos::new(0, 26), Mark::Highlight);
    let bytes = write_new(&d).unwrap();
    let xml = String::from_utf8(
        Package::read(&bytes)
            .unwrap()
            .get("word/document.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        xml.contains("<w:strike/>")
            && xml.contains("w:val=\"superscript\"")
            && xml.contains("w:val=\"subscript\"")
            && xml.contains("w:fill=\"FFFF00\""),
        "{xml}"
    );
    let back = read(&bytes).unwrap().doc;
    for (m, a, b) in [
        (Mark::Strike, 2, 8),
        (Mark::Superscript, 11, 14),
        (Mark::Subscript, 17, 20),
        (Mark::Highlight, 23, 26),
    ] {
        assert!(back.all_marked(Pos::new(0, a), Pos::new(0, b), m), "{m:?}");
    }
    assert!(!back.all_marked(Pos::new(0, 0), Pos::new(0, 2), Mark::Strike));
    // Raised and lowered exclude each other.
    let mut e = Doc::from_text("xx");
    e.toggle(Pos::new(0, 0), Pos::new(0, 2), Mark::Superscript);
    e.toggle(Pos::new(0, 0), Pos::new(0, 2), Mark::Subscript);
    assert!(
        e.all_marked(Pos::new(0, 0), Pos::new(0, 2), Mark::Subscript)
            && !e.all_marked(Pos::new(0, 0), Pos::new(0, 2), Mark::Superscript)
    );
}

#[quench_test::test]
fn a_file_made_by_libreoffice_reads_with_its_formatting_and_survives_being_written_back() {
    const FILE: &[u8] = include_bytes!("../../../corpus/libreoffice_rich.docx");
    let loaded = read(FILE).unwrap();
    let doc = &loaded.doc;
    assert_eq!(doc.text(0), "Raport kwartalny");
    assert_eq!(doc.paras()[0].kind, Kind::Heading(1));
    assert!(doc.text(1).starts_with("Za\u{17c}\u{f3}\u{142}\u{107} g\u{119}\u{15b}l\u{105} ja\u{17a}\u{144}: pogrubiony, pochylony"));
    let at = |word: &str| {
        let i = doc.text(1).find(word).unwrap();
        let from = doc.text(1)[..i].encode_utf16().count();
        (
            Pos::new(1, from),
            Pos::new(1, from + word.encode_utf16().count()),
        )
    };
    for (word, mark) in [
        ("pogrubiony", Mark::Bold),
        ("pochylony", Mark::Italic),
        ("podkre\u{15b}lony", Mark::Underline),
        ("przekre\u{15b}lony", Mark::Strike),
    ] {
        let (a, b) = at(word);
        assert!(doc.all_marked(a, b, mark), "{word}");
    }
    let (a, _) = at("link");
    assert_eq!(
        doc.link_at(Pos::new(1, a.offset + 2)),
        Some("https://example.org/raport")
    );
    let table = doc.paras().iter().position(|p| p.cell.is_some()).unwrap();
    assert_eq!(doc.table_of(table).map(|t| (t.2, t.3)), Some((3, 3)));
    assert_eq!(doc.text(table + 8), "0,50");
    // The linked (not embedded) picture is not shown, and that is reported.
    assert_eq!(loaded.report.pictures, 1);
    // Written back over its own package and read again, the text and the formatting are the same.
    let again = read(&write(&loaded.doc, &loaded.package).unwrap()).unwrap();
    assert_eq!(again.doc.plain(), doc.plain());
    for (a, b) in again.doc.paras().iter().zip(doc.paras()) {
        assert_eq!(
            (a.kind, a.align, a.cell.is_some()),
            (b.kind, b.align, b.cell.is_some())
        );
    }
    assert!(
        again
            .doc
            .all_marked(at("pogrubiony").0, at("pogrubiony").1, Mark::Bold)
    );
}

#[quench_test::test]
fn indents_spacing_and_page_breaks_round_trip_as_paragraph_properties() {
    let mut d = Doc::from_text("one\ntwo\nthree");
    d.set_fmt(0, |f| docs_text::ParaFmt {
        indent: 720,
        first: 360,
        ..f
    });
    d.set_fmt(1, |f| docs_text::ParaFmt {
        indent: 1440,
        first: -360,
        after: Some(240),
        line: 360,
        ..f
    });
    d.set_fmt(2, |f| docs_text::ParaFmt {
        page_break: true,
        ..f
    });
    let bytes = write_new(&d).unwrap();
    let xml = String::from_utf8(
        Package::read(&bytes)
            .unwrap()
            .get("word/document.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        xml.contains("w:firstLine=\"360\"")
            && xml.contains("w:hanging=\"360\"")
            && xml.contains("w:after=\"240\"")
            && xml.contains("<w:pageBreakBefore/>"),
        "{xml}"
    );
    let back = read(&bytes).unwrap().doc;
    for i in 0..3 {
        assert_eq!(back.paras()[i].fmt, d.paras()[i].fmt, "{i}");
    }
    // A page break inside a run (Word's way) marks its paragraph.
    let body = "<w:p><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:br w:type=\"page\"/></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>";
    let mut pkg = Package::new();
    pkg.set("word/document.xml", format!("<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>").into_bytes());
    let doc = read(&pkg.write().unwrap()).unwrap().doc;
    assert!(doc.paras()[1].fmt.page_break && !doc.paras()[0].fmt.page_break);
}

#[quench_test::test]
fn a_header_a_footer_with_page_fields_and_letter_paper_are_kept_in_the_file() {
    let mut d = Doc::from_text("body");
    d.set_header("Quarterly report");
    d.set_footer("Page {page} of {pages}");
    d.set_paper(docs_text::PaperSize::Letter);
    let bytes = write_new(&d).unwrap();
    let package = Package::read(&bytes).unwrap();
    let hdr = String::from_utf8(package.get("word/header1.xml").unwrap().to_vec()).unwrap();
    let ftr = String::from_utf8(package.get("word/footer1.xml").unwrap().to_vec()).unwrap();
    assert!(
        hdr.contains("Quarterly report") && ftr.contains("PAGE") && ftr.contains("NUMPAGES"),
        "{ftr}"
    );
    let xml = String::from_utf8(package.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("<w:headerReference")
            && xml.contains("<w:footerReference")
            && xml.contains("w:w=\"12240\""),
        "{xml}"
    );
    let back = read(&bytes).unwrap().doc;
    assert_eq!(
        (back.header(), back.footer(), back.paper()),
        (
            "Quarterly report",
            "Page {page} of {pages}",
            docs_text::PaperSize::Letter
        )
    );
    // Taken away, they are gone from the section.
    let mut e = back;
    e.set_header("");
    e.set_footer("");
    let again = read(&write_new(&e).unwrap()).unwrap().doc;
    assert_eq!((again.header(), again.footer()), ("", ""));
}

#[quench_test::test]
fn column_widths_and_a_repeating_header_row_survive_the_file() {
    let mut d = Doc::new();
    d.insert_table(0, 2, 3);
    let t = d.cell_at(1).unwrap().table;
    d.set_table_widths(t, &[1, 2, 7]);
    d.mark_table_header(t, true);
    let bytes = write_new(&d).unwrap();
    let xml = String::from_utf8(
        Package::read(&bytes)
            .unwrap()
            .get("word/document.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        xml.contains("<w:gridCol w:w=\"900\"/>") && xml.contains("<w:tblHeader/>"),
        "{xml}"
    );
    let back = read(&bytes).unwrap().doc;
    assert_eq!(
        back.table_widths(back.cell_at(1).unwrap().table),
        [100, 200, 700]
    );
    assert!(back.table_header(back.cell_at(1).unwrap().table));
}

#[quench_test::test]
fn nested_list_levels_and_line_breaks_round_trip() {
    let mut d = Doc::from_text("one\ntwo\ndeeper\nback\nline\u{2028}break");
    for i in 0..4 {
        d.set_kind(i, Kind::Numbered);
    }
    d.set_fmt(1, |f| docs_text::ParaFmt { level: 1, ..f });
    d.set_fmt(2, |f| docs_text::ParaFmt { level: 2, ..f });
    let bytes = write_new(&d).unwrap();
    let package = Package::read(&bytes).unwrap();
    let xml = String::from_utf8(package.get("word/document.xml").unwrap().to_vec()).unwrap();
    assert!(
        xml.contains("<w:ilvl w:val=\"2\"/>") && xml.contains("<w:br/>"),
        "{xml}"
    );
    let numbering = String::from_utf8(package.get("word/numbering.xml").unwrap().to_vec()).unwrap();
    assert!(
        numbering.contains("w:ilvl=\"8\"")
            && numbering.contains("lowerLetter")
            && numbering.contains("lowerRoman")
    );
    let back = read(&bytes).unwrap().doc;
    assert_eq!(back.plain(), d.plain());
    assert_eq!(
        back.paras().iter().map(|p| p.fmt.level).collect::<Vec<_>>(),
        [0, 1, 2, 0, 0]
    );
    assert_eq!(back.list_markers()[2].as_deref(), Some("i."));
}

#[quench_test::test]
fn typeface_names_are_kept_not_flattened_to_three_families() {
    let body = "<w:p><w:r><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/></w:rPr><w:t>sans</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii=\"Georgia\"/></w:rPr><w:t>serif</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii=\"Arial\"/></w:rPr><w:t>arial</w:t></w:r></w:p>";
    let mut pkg = Package::new();
    pkg.set("word/document.xml", format!("<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>").into_bytes());
    let loaded = read(&pkg.write().unwrap()).unwrap();
    let doc = &loaded.doc;
    let name_at = |off: usize| {
        doc.style_at(Pos::new(0, off))
            .font
            .and_then(|f| doc.font_name(f))
            .map(str::to_string)
    };
    assert_eq!(name_at(2), Some("Calibri".into()));
    assert_eq!(
        doc.style_at(Pos::new(0, 2)).family,
        Some(docs_text::Family::Sans)
    );
    assert_eq!(name_at(7), Some("Georgia".into()));
    assert_eq!(
        doc.style_at(Pos::new(0, 7)).family,
        Some(docs_text::Family::Serif)
    );
    assert_eq!(name_at(12), None, "Arial is the sans kind itself");
    let out = write(doc, &loaded.package).unwrap();
    let xml = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("word/document.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        xml.contains("w:ascii=\"Calibri\"")
            && xml.contains("w:ascii=\"Georgia\"")
            && xml.contains("w:ascii=\"Arial\""),
        "{xml}"
    );
    let again = read(&out).unwrap().doc;
    assert_eq!(
        again
            .style_at(Pos::new(0, 2))
            .font
            .and_then(|f| again.font_name(f)),
        Some("Calibri")
    );
}

/// A cheap deterministic generator for mutating files.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[quench_test::test]
fn damaged_files_are_errors_or_documents_but_never_a_panic() {
    let mut d = Doc::from_text("Title\nbody with a link\ncell");
    d.set_link(Pos::new(1, 12), Pos::new(1, 16), Some("https://e.org/"));
    d.insert_table(1, 2, 2);
    d.set_header("h");
    let good = write_new(&d).unwrap();
    let rich: &[u8] = include_bytes!("../../../corpus/libreoffice_rich.docx");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for source in [good.as_slice(), rich] {
        for round in 0..400 {
            let mut bytes = source.to_vec();
            for _ in 0..=(round % 6) {
                let at = (rng.next() as usize) % bytes.len();
                match rng.next() % 3 {
                    0 => bytes[at] = rng.next() as u8,
                    1 => bytes.truncate(at.max(1)),
                    _ => {
                        let end = (at + 1 + (rng.next() as usize) % 8).min(bytes.len());
                        bytes.drain(at..end);
                    }
                }
                if bytes.is_empty() {
                    break;
                }
            }
            std::panic::catch_unwind(|| {
                if let Ok(loaded) = read(&bytes) {
                    let _ = write(&loaded.doc, &loaded.package);
                }
            })
            .unwrap_or_else(|_| panic!("round {round} panicked"));
        }
    }
}

#[quench_test::test]
fn a_damaged_document_part_inside_a_good_package_is_never_a_panic() {
    let rich: &[u8] = include_bytes!("../../../corpus/libreoffice_rich.docx");
    let package = Package::read(rich).unwrap();
    let xml = package.get("word/document.xml").unwrap().to_vec();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for round in 0..600 {
        let mut bytes = xml.clone();
        for _ in 0..=(round % 5) {
            let at = (rng.next() as usize) % bytes.len();
            match rng.next() % 4 {
                0 => bytes[at] = b"<>\"&/= \n"[(rng.next() % 8) as usize],
                1 => bytes.truncate(at.max(1)),
                2 => {
                    let end = (at + 1 + (rng.next() as usize) % 30).min(bytes.len());
                    bytes.drain(at..end);
                }
                _ => bytes.insert(at, rng.next() as u8),
            }
            if bytes.is_empty() {
                break;
            }
        }
        let mut broken = package.clone();
        broken.set("word/document.xml", bytes);
        let packed = broken.write().unwrap();
        std::panic::catch_unwind(|| {
            if let Ok(loaded) = read(&packed) {
                let _ = write(&loaded.doc, &loaded.package);
            }
        })
        .unwrap_or_else(|_| panic!("round {round} panicked"));
    }
}

#[quench_test::test]
fn every_paper_size_is_written_and_read_back() {
    for size in docs_text::PaperSize::ALL {
        let mut d = Doc::from_text("body");
        d.set_paper(size);
        let back = read(&write_new(&d).unwrap()).unwrap().doc;
        assert_eq!(back.paper(), size);
    }
}
