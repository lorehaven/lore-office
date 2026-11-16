#![allow(clippy::unwrap_used, clippy::pedantic, clippy::nursery)]
use sheets_calc::Engine;
use sheets_model::{Align, CellRef, Content, Style, Value, Workbook};
use sheets_ods::{from_odf, read, to_odf, write};

const LIBREOFFICE: &[u8] = include_bytes!("../../../corpus/libreoffice_basic.ods");

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

#[quench_test::test]
fn formulas_change_between_the_two_syntaxes() {
    let pairs = [
        ("=SUM(A1:B2)", "of:=SUM([.A1:.B2])"),
        ("=A1+$B$2*2", "of:=[.A1]+[.$B$2]*2"),
        (
            "=IF(A1>1,\"a,b;c\",\"[x]\")",
            "of:=IF([.A1]>1;\"a,b;c\";\"[x]\")",
        ),
        ("=Other!B2+1", "of:=[$Other.B2]+1"),
        ("='My sheet'!A1:B3", "of:=[$'My sheet'.A1:.B3]"),
        ("=LOG10(100)+A1", "of:=LOG10(100)+[.A1]"),
        (
            "=VLOOKUP(\"x\",A2:D3,4,FALSE)",
            "of:=VLOOKUP(\"x\";[.A2:.D3];4;FALSE)",
        ),
        ("=-A1%", "of:=-[.A1]%"),
    ];
    for (ours, theirs) in pairs {
        assert_eq!(to_odf(ours.trim_start_matches('=')), theirs, "{ours}");
        assert_eq!(from_odf(theirs), ours.trim_start_matches('='), "{theirs}");
    }
    assert_eq!(
        from_odf("of:=COM.MICROSOFT.XLOOKUP([.A1];[.B1:.B3];[.C1:.C3])"),
        "XLOOKUP(A1,B1:B3,C1:C3)"
    );
    assert_eq!(from_odf("of:=SUM([$'It''s'.A1:.A3])"), "SUM('It''s'!A1:A3)");
    assert_eq!(from_odf("=[.A1]"), "A1", "without the prefix too");
}

#[quench_test::test]
fn a_file_from_libreoffice_is_read_with_values_formulas_and_looks() {
    let wb = read(LIBREOFFICE).unwrap();
    assert_eq!(
        wb.sheets
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Data", "Other"]
    );
    let s = &wb.sheets[0];
    let style = |c: &str| &wb.styles[s.style(at(c)) as usize];
    assert_eq!(
        s.get(at("A2")),
        Some(&Content::Value(Value::Text("bolt".into())))
    );
    assert_eq!(s.get(at("B2")), Some(&Content::Value(Value::Number(4.0))));
    assert_eq!(s.get(at("D2")), Some(&Content::Formula("B2*C2".into())));
    assert_eq!(
        s.get(at("A5")),
        Some(&Content::Value(Value::Text("multi\nline".into())))
    );
    assert_eq!(
        s.get(at("E2")),
        Some(&Content::Value(Value::Number(45351.0))),
        "a date is its serial number"
    );
    assert_eq!(
        s.get(at("F6")),
        Some(&Content::Formula("Other!B2+1".into()))
    );
    assert_eq!(
        s.get(at("A6")),
        Some(&Content::Formula(
            "XLOOKUP(\"nut, hex\",A2:A3,B2:B3)".into()
        ))
    );
    assert!(style("A1").bold && style("B1").bold && !style("A2").bold);
    assert_eq!(style("C1").fill, Some(0x00FF_FF00));
    assert_eq!(
        (
            style("C2").number_format.as_str(),
            style("E2").number_format.as_str(),
            style("A2").number_format.as_str()
        ),
        ("#,##0.00", "yyyy-mm-dd", "General")
    );
    assert_eq!(s.merges().len(), 1);
    assert!(s.merge_at(at("C8")).is_some());
    assert!((s.col_width(0).unwrap() - 168.0).abs() < 1.0, "5 cm wide");
    // And it calculates.
    let e = Engine::new(wb);
    assert_eq!(e.value(0, at("D2")), Value::Number(6.0));
    assert_eq!(e.value(0, at("B4")), Value::Number(13.0));
    assert_eq!(
        e.value(0, at("F6")),
        Value::Number(27.0),
        "a formula reading the other sheet"
    );
}

#[quench_test::test]
fn a_workbook_written_and_read_again_is_the_same_workbook() {
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Main & more".into();
    wb.sheets.push(sheets_model::Sheet::new("Second"));
    let bold = wb.style_id(&Style {
        bold: true,
        italic: true,
        underline: true,
        color: Some(0x00FF_0000),
        fill: Some(0x00CC_EEFF),
        align: Align::Center,
        wrap: true,
        ..Style::default()
    });
    let money = wb.style_id(&Style {
        number_format: "#,##0.00".into(),
        ..Style::default()
    });
    let pct = wb.style_id(&Style {
        number_format: "0.0%".into(),
        ..Style::default()
    });
    let date = wb.style_id(&Style {
        number_format: "yyyy-mm-dd".into(),
        ..Style::default()
    });
    let s = &mut wb.sheets[0];
    s.set(
        at("A1"),
        Content::Value(Value::Text("Title <&> \"q\"".into())),
    );
    s.set_style(at("A1"), bold);
    s.set(at("A2"), Content::Value(Value::Text("two\nlines".into())));
    s.set(at("B1"), Content::Value(Value::Number(1234.5)));
    s.set_style(at("B1"), money);
    s.set(at("B2"), Content::Value(Value::Number(0.256)));
    s.set_style(at("B2"), pct);
    s.set(at("B3"), Content::Value(Value::Number(45351.0)));
    s.set_style(at("B3"), date);
    s.set(at("C1"), Content::Value(Value::Bool(true)));
    s.set(at("C2"), Content::Formula("SUM(B1:B2)*2".into()));
    s.set(at("C3"), Content::Formula("Second!A1&\"!\"".into()));
    s.set(at("E5"), Content::Value(Value::Number(5.0)));
    s.merge(sheets_model::Range::parse("A7:C8").unwrap());
    s.set(at("A7"), Content::Value(Value::Text("merged".into())));
    s.set_col_width(1, Some(150.0));
    s.set_row_height(3, Some(48.0));
    s.set_row_hidden(4, true);
    s.set_col_hidden(3, true);
    wb.sheets[1].set(at("A1"), Content::Value(Value::Text("hello".into())));
    let e = Engine::new(wb);
    let bytes = write(&e).unwrap();
    let back = read(&bytes).unwrap();
    let t = &back.sheets[0];
    assert_eq!(t.name, "Main & more");
    let look = |c: &str| &back.styles[t.style(at(c)) as usize];
    assert_eq!(
        t.get(at("A1")),
        Some(&Content::Value(Value::Text("Title <&> \"q\"".into())))
    );
    assert_eq!(
        t.get(at("A2")),
        Some(&Content::Value(Value::Text("two\nlines".into())))
    );
    assert_eq!(
        t.get(at("B1")),
        Some(&Content::Value(Value::Number(1234.5)))
    );
    assert_eq!(
        t.get(at("B3")),
        Some(&Content::Value(Value::Number(45351.0)))
    );
    assert_eq!(t.get(at("C1")), Some(&Content::Value(Value::Bool(true))));
    assert_eq!(
        t.get(at("C2")),
        Some(&Content::Formula("SUM(B1:B2)*2".into()))
    );
    assert_eq!(
        t.get(at("C3")),
        Some(&Content::Formula("Second!A1&\"!\"".into()))
    );
    assert_eq!(
        t.get(at("E5")),
        Some(&Content::Value(Value::Number(5.0))),
        "after empty cells"
    );
    let a1 = look("A1");
    assert_eq!(
        (
            a1.bold,
            a1.italic,
            a1.underline,
            a1.color,
            a1.fill,
            a1.align,
            a1.wrap
        ),
        (
            true,
            true,
            true,
            Some(0x00FF_0000),
            Some(0x00CC_EEFF),
            Align::Center,
            true
        )
    );
    assert_eq!(
        (
            look("B1").number_format.as_str(),
            look("B2").number_format.as_str(),
            look("B3").number_format.as_str()
        ),
        ("#,##0.00", "0.0%", "yyyy-mm-dd")
    );
    assert!(t.merge_at(at("C8")).is_some() && t.merge_at(at("A7")).is_some());
    assert!((t.col_width(1).unwrap() - 150.0).abs() < 1.0);
    assert!((t.row_height(3).unwrap() - 48.0).abs() < 1.0);
    assert!(t.row_hidden(4) && t.col_hidden(3) && !t.row_hidden(3));
    assert_eq!(
        back.sheets[1].get(at("A1")),
        Some(&Content::Value(Value::Text("hello".into())))
    );
    assert_eq!(
        Engine::new(back).value(0, at("C2")),
        Value::Number(2469.512)
    );
}

#[quench_test::test]
fn files_that_are_not_spreadsheets_are_errors() {
    assert!(read(b"not a zip").is_err());
    let mut pkg = office_fmt::Package::new();
    pkg.set("content.xml", b"<office:document-content xmlns:office=\"o\"><office:body><office:text/></office:body></office:document-content>".to_vec());
    assert!(read(&pkg.write().unwrap()).is_err(), "a text document");
    assert!(
        read(&office_fmt::Package::new().write().unwrap()).is_err(),
        "no content"
    );
}

quench_test::main!();
