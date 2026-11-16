#![allow(clippy::unwrap_used)]
//! `cargo run -p sheets-xlsx --example table_only -- out.xlsx`
use sheets_model::{AutoFilter, CellRef, Content, Range, Table, Value, Workbook};
use sheets_xlsx::Xlsx;

fn main() {
    let mut wb = Workbook::new();
    let s = &mut wb.sheets[0];
    for (a, v) in [("A1", "Name"), ("B1", "Qty"), ("A2", "bolt"), ("A3", "nut")] {
        s.set(
            CellRef::parse(a).unwrap(),
            Content::Value(Value::Text(v.into())),
        );
    }
    s.set(
        CellRef::parse("B2").unwrap(),
        Content::Value(Value::Number(4.0)),
    );
    s.set(
        CellRef::parse("B3").unwrap(),
        Content::Value(Value::Number(9.0)),
    );
    s.set_tables(vec![Table {
        name: "Parts".into(),
        filter: AutoFilter {
            range: Range::parse("A1:B3").unwrap(),
            allowed: std::collections::BTreeMap::new(),
        },
        banded: true,
        style: sheets_model::custom_table_style(0x00FF_0000, 0, 0x0000_FF00),
    }]);
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        Xlsx::new(wb).write().unwrap(),
    )
    .unwrap();
}
