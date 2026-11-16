#![allow(clippy::unwrap_used, clippy::pedantic, clippy::nursery)]
use sheets_model::{CellRef, Content, Value};

const LIBREOFFICE: &[u8] = include_bytes!("../../../corpus/libreoffice_basic.xls");

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

#[quench_test::test]
fn an_xls_file_is_read_with_values_dates_formats_and_merges() {
    let wb = sheets_xls::read(LIBREOFFICE).unwrap();
    assert_eq!(
        wb.sheets
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Data", "Other"]
    );
    let s = &wb.sheets[0];
    assert_eq!(
        s.get(at("A1")),
        Some(&Content::Value(Value::Text("Name".into())))
    );
    assert_eq!(
        s.get(at("A2")),
        Some(&Content::Value(Value::Text("bolt".into())))
    );
    assert_eq!(s.get(at("B2")), Some(&Content::Value(Value::Number(4.0))));
    assert_eq!(
        s.get(at("D2")),
        Some(&Content::Value(Value::Number(6.0))),
        "a formula is its value"
    );
    assert_eq!(
        s.get(at("E2")),
        Some(&Content::Value(Value::Number(45351.0)))
    );
    assert_eq!(
        wb.styles[s.style(at("C2")) as usize].number_format,
        "#,##0.00"
    );
    assert!(
        wb.styles[s.style(at("E2")) as usize]
            .number_format
            .contains('y')
    );
    assert!(
        wb.styles[s.style(at("A1")) as usize].bold && !wb.styles[s.style(at("A2")) as usize].bold
    );
    assert!(s.merge_at(at("B8")).is_some());
    assert_eq!(
        wb.sheets[1].get(at("A1")),
        Some(&Content::Value(Value::Text("It's".into())))
    );
}

#[quench_test::test]
fn things_that_are_not_xls_are_errors() {
    assert!(sheets_xls::read(b"hello").is_err());
    assert!(
        sheets_xls::read(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0, 0, 0]).is_err()
    );
}

quench_test::main!();
