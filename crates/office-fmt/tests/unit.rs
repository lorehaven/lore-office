#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use office_fmt::*;

#[quench_test::test]
fn a_package_round_trips_in_order() {
    let mut p = Package::new();
    p.set("xl/workbook.xml", b"<w/>".to_vec());
    p.set("[Content_Types].xml", b"<t/>".to_vec());
    p.set("docProps/blob.bin", (0..=255u8).collect());
    p.set("xl/workbook.xml", b"<w2/>".to_vec()); // replaced where it is
    assert_eq!(p.len(), 3);
    let bytes = p.write().unwrap();
    let back = Package::read(&bytes).unwrap();
    assert_eq!(
        back.names().collect::<Vec<_>>(),
        [
            "[Content_Types].xml",
            "xl/workbook.xml",
            "docProps/blob.bin"
        ]
    );
    assert_eq!(back.get("xl/workbook.xml"), Some(&b"<w2/>"[..]));
    assert_eq!(back.get("docProps/blob.bin").unwrap().len(), 256);
    assert_eq!(back.get("nope"), None);
    let mut back = back;
    assert!(back.remove("xl/workbook.xml"));
    assert!(!back.remove("xl/workbook.xml"));
    assert!(!back.is_empty());
}

#[quench_test::test]
fn damaged_input_is_an_error_not_a_panic() {
    assert!(matches!(Package::read(b"not a zip"), Err(Error::Zip(_))));
    assert!(matches!(Package::read(&[]), Err(Error::Zip(_))));
    let mut bytes = {
        let mut p = Package::new();
        p.set("a.txt", vec![7; 1000]);
        p.write().unwrap()
    };
    let n = bytes.len();
    bytes.truncate(n / 2);
    assert!(Package::read(&bytes).is_err());
    assert!(Error::Limit("x".into()).to_string().contains("too large"));
}

#[quench_test::test]
fn part_names_resolve_like_a_package() {
    assert_eq!(
        rels::rels_part("xl/workbook.xml"),
        "xl/_rels/workbook.xml.rels"
    );
    assert_eq!(rels::rels_part(""), "_rels/.rels");
    assert_eq!(rels::rels_part("a.xml"), "_rels/a.xml.rels");
    assert_eq!(
        rels::resolve("xl/workbook.xml", "worksheets/sheet1.xml"),
        "xl/worksheets/sheet1.xml"
    );
    assert_eq!(
        rels::resolve("xl/worksheets/sheet1.xml", "../sharedStrings.xml"),
        "xl/sharedStrings.xml"
    );
    assert_eq!(
        rels::resolve("xl/workbook.xml", "/xl/styles.xml"),
        "xl/styles.xml"
    );
    assert_eq!(rels::resolve("", "xl/workbook.xml"), "xl/workbook.xml");
    assert_eq!(
        rels::resolve("xl/workbook.xml", "./a/./b.xml"),
        "xl/a/b.xml"
    );
    assert_eq!(rels::resolve("a.xml", "../../x.xml"), "x.xml");
}

#[quench_test::test]
fn relationships_round_trip() {
    let list = vec![
        Rel {
            id: "rId1".into(),
            kind: rels::WORKSHEET.into(),
            target: "worksheets/sheet1.xml".into(),
            external: false,
        },
        Rel {
            id: "rId2".into(),
            kind: "http://x/hyperlink".into(),
            target: "https://a.b/?q=1&r=2".into(),
            external: true,
        },
    ];
    let bytes = rels::to_bytes(&list);
    assert!(std::str::from_utf8(&bytes).unwrap().starts_with("<?xml"));
    assert_eq!(rels::parse(&bytes).unwrap(), list);
    assert!(rels::parse(b"<broken").is_err());
}

#[quench_test::test]
fn content_types_find_by_name_then_extension() {
    let mut ct = ContentTypes::default();
    ct.set_default("XML", "application/xml");
    ct.set_default("xml", "ignored: already there");
    ct.set_default("png", "image/png");
    ct.set_override("xl/workbook.xml", "application/vnd.ms-excel.sheet.main+xml");
    assert_eq!(
        ct.of("xl/workbook.xml"),
        Some("application/vnd.ms-excel.sheet.main+xml")
    );
    assert_eq!(ct.of("xl/other.xml"), Some("application/xml"));
    assert_eq!(ct.of("media/a.PNG"), Some("image/png"));
    assert_eq!(ct.of("x.unknown"), None);
    assert_eq!(ct.of("noext"), None);
    ct.set_override("xl/workbook.xml", "changed");
    assert_eq!(ct.overrides.len(), 1);
    assert_eq!(ContentTypes::parse(&ct.to_bytes()).unwrap(), ct);
    ct.remove_override("xl/workbook.xml");
    assert_eq!(ct.of("xl/workbook.xml"), Some("application/xml"));
}

#[quench_test::test]
fn xml_parts_keep_unknown_content() {
    let src = r#"<?xml version="1.0"?><!-- c --><ws xmlns="urn:a" xmlns:x14="urn:b" x14:k="1"><cols><col min="1" max="2" width="12.5"/></cols><x14:ext a="&amp;"/><t> </t></ws>"#;
    let tree = xml::parse(src.as_bytes(), true).unwrap();
    let out = xml::write(&tree);
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<ws"));
    assert!(text.contains(r#"xmlns:x14="urn:b" x14:k="1""#));
    assert!(text.contains("<x14:ext a=\"&amp;\"/>"));
    assert!(text.contains("<t> </t>"));
    assert_eq!(xml::parse(&out, true).unwrap(), tree);
    assert!(xml::parse(&[0xFF, 0xFE], false).is_err());
    assert!(xml::parse(b"<a><b></a>", false).is_err());
}

#[quench_test::test]
fn xml_trees_can_be_edited() {
    let mut root = xml::el(
        "r",
        &[("a", "1")],
        vec![xml::el("c", &[], vec![xml::txt("x")]), xml::txt("tail")],
    );
    xml::set_attr(&mut root, "a", "2");
    xml::set_attr(&mut root, "b", "3");
    xml::child_mut(&mut root, "c")
        .unwrap()
        .clone_from(&xml::el("d", &[], vec![]));
    xml::children_mut(&mut root)
        .unwrap()
        .push(xml::el("e", &[], vec![]));
    assert_eq!(xml::attr(&root, "a"), Some("2"));
    assert_eq!(xml::attr(&root, "b"), Some("3"));
    assert!(xml::child(&root, "d").is_some());
    assert!(xml::child_mut(&mut root, "zzz").is_none());
    assert_eq!(xml::children(&root).len(), 3);
    let mut not_element = Value::String("s".into());
    assert!(xml::children_mut(&mut not_element).is_none());
    xml::set_attr(&mut not_element, "a", "b");
}

quench_test::main!();
