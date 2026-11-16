#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use sheets_model::value::number_text;
use sheets_model::*;

#[quench_test::test]
fn columns_count_like_a_sheet() {
    assert_eq!(col_name(0), "A");
    assert_eq!(col_name(25), "Z");
    assert_eq!(col_name(26), "AA");
    assert_eq!(col_name(701), "ZZ");
    assert_eq!(col_name(702), "AAA");
    assert_eq!(col_name(MAX_COLS - 1), "XFD");
    for c in [0, 1, 25, 26, 700, 16_383] {
        assert_eq!(col_index(&col_name(c)), Some(c));
    }
    assert_eq!(col_index("XFE"), None);
    assert_eq!(col_index(""), None);
    assert_eq!(col_index("A1"), None);
    assert_eq!(col_index("abc"), Some(730));
}

#[quench_test::test]
fn a1_addresses_parse_and_print() {
    let c = CellRef::parse("B7").unwrap();
    assert_eq!(c, CellRef::new(6, 1));
    assert_eq!(c.to_string(), "B7");
    assert_eq!(CellRef::parse("$c$3"), Some(CellRef::new(2, 2)));
    assert_eq!(
        CellRef::parse_anchored("$C3"),
        Some((CellRef::new(2, 2), true, false))
    );
    assert_eq!(
        CellRef::parse_anchored("C$3"),
        Some((CellRef::new(2, 2), false, true))
    );
    assert_eq!(CellRef::parse("A0"), None);
    assert_eq!(CellRef::parse("A1048577"), None);
    assert_eq!(CellRef::parse("1A"), None);
    assert_eq!(CellRef::parse("A"), None);
    assert_eq!(CellRef::parse("A1B"), None);
    assert_eq!(
        CellRef::parse("XFD1048576"),
        Some(CellRef::new(MAX_ROWS - 1, MAX_COLS - 1))
    );
}

#[quench_test::test]
fn ranges_normalise_and_iterate() {
    let r = Range::parse("C3:A1").unwrap();
    assert_eq!(r.start, CellRef::new(0, 0));
    assert_eq!(r.end, CellRef::new(2, 2));
    assert_eq!((r.rows(), r.cols()), (3, 3));
    assert_eq!(r.to_string(), "A1:C3");
    assert!(r.contains(CellRef::new(1, 1)));
    assert!(!r.contains(CellRef::new(3, 0)));
    assert_eq!(r.cells().count(), 9);
    assert_eq!(r.cells().nth(3), Some(CellRef::new(1, 0)));
    assert_eq!(Range::parse("B2").unwrap().to_string(), "B2");
    assert_eq!(Range::parse("A1:"), None);
}

#[quench_test::test]
fn values_convert_like_arithmetic_reads_them() {
    assert_eq!(Value::Empty.to_number(), Ok(0.0));
    assert_eq!(Value::Bool(true).to_number(), Ok(1.0));
    assert_eq!(Value::Text(" 4.5 ".into()).to_number(), Ok(4.5));
    assert_eq!(Value::Text("abc".into()).to_number(), Err(CellError::Value));
    assert_eq!(Value::Text("inf".into()).to_number(), Err(CellError::Value));
    assert_eq!(
        Value::Error(CellError::Div0).to_number(),
        Err(CellError::Div0)
    );
    assert_eq!(Value::Number(3.0).to_text().unwrap(), "3");
    assert_eq!(Value::Bool(false).to_text().unwrap(), "FALSE");
    assert_eq!(Value::Text("TRUE".into()).to_bool(), Ok(true));
    assert_eq!(Value::Text("x".into()).to_bool(), Err(CellError::Value));
    assert_eq!(Value::Number(0.0).to_bool(), Ok(false));
    assert_eq!(Value::Empty.to_bool(), Ok(false));
    assert!(Value::Empty.is_empty());
    assert_eq!(Value::Error(CellError::NA).error(), Some(CellError::NA));
    assert_eq!(Value::Error(CellError::Name).to_string(), "#NAME?");
    assert_eq!(CellError::parse("#div/0!"), Some(CellError::Div0));
    assert_eq!(CellError::parse("nope"), None);
    assert_eq!(CellError::parse("#CIRC!"), Some(CellError::Circular));
}

#[quench_test::test]
fn numbers_print_with_fifteen_digits() {
    assert_eq!(number_text(0.0), "0");
    assert_eq!(number_text(-0.0), "0");
    assert_eq!(number_text(1.0), "1");
    assert_eq!(number_text(0.1 + 0.2), "0.3");
    assert_eq!(number_text(1.5), "1.5");
    assert_eq!(number_text(-12.25), "-12.25");
    assert_eq!(number_text(1e21), "1e21");
}

#[quench_test::test]
fn a_sheet_holds_only_what_is_there() {
    let mut s = Sheet::new("Data");
    assert!(s.is_empty());
    assert_eq!(s.used_range(), None);
    s.set(CellRef::new(2, 3), Content::Value(Value::Number(1.0)));
    s.set(CellRef::new(0, 1), Content::Formula("A1+1".into()));
    assert_eq!(s.len(), 2);
    assert_eq!(s.used_range().unwrap().to_string(), "B1:D3");
    assert_eq!(s.iter().next().unwrap().0, CellRef::new(0, 1));
    s.set(CellRef::new(2, 3), Content::Value(Value::Empty));
    assert_eq!(s.len(), 1);
    s.clear(CellRef::new(0, 1));
    assert!(s.get(CellRef::new(0, 1)).is_none());
}

#[quench_test::test]
fn workbook_finds_sheets_by_name() {
    let mut wb = Workbook::new();
    assert!(wb.sheet("sheet1").is_some());
    wb.sheet_mut("SHEET1").unwrap().name = "Totals".into();
    assert!(wb.sheet("Sheet1").is_none());
    assert!(wb.sheet("totals").is_some());
}

#[quench_test::test]
fn columns_and_rows_can_have_their_own_size() {
    let mut s = Sheet::new("S");
    assert_eq!((s.col_width(2), s.row_height(2)), (None, None));
    s.set_col_width(2, Some(140.0));
    s.set_col_width(0, Some(50.5));
    s.set_row_height(6, Some(40.0));
    assert_eq!(s.col_width(2), Some(140.0));
    assert_eq!(
        s.col_widths().collect::<Vec<_>>(),
        vec![(0, 50.5), (2, 140.0)],
        "in column order"
    );
    assert_eq!(s.row_heights().collect::<Vec<_>>(), vec![(6, 40.0)]);
    s.set_col_width(2, None);
    assert_eq!(s.col_width(2), None);
    // Nonsense sizes are the default.
    s.set_col_width(0, Some(f64::NAN));
    s.set_col_width(1, Some(-3.0));
    s.set_row_height(6, Some(0.0));
    assert_eq!(s.col_widths().count(), 0);
    assert_eq!(s.row_heights().count(), 0);
}

quench_test::main!();
