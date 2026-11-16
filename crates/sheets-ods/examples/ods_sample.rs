#![allow(clippy::unwrap_used)]
//! `cargo run -p sheets-ods --example ods_sample -- out.ods`
use sheets_calc::Engine;
use sheets_model::{CellRef, Content, Style, Value, Workbook};

fn main() {
    let mut wb = Workbook::new();
    let money = wb.style_id(&Style {
        number_format: "#,##0.00".into(),
        bold: true,
        ..Style::default()
    });
    let date = wb.style_id(&Style {
        number_format: "yyyy-mm-dd".into(),
        ..Style::default()
    });
    let s = &mut wb.sheets[0];
    let at = |a: &str| CellRef::parse(a).unwrap();
    s.set(at("A1"), Content::Value(Value::Text("Item".into())));
    s.set(at("B1"), Content::Value(Value::Number(1234.5)));
    s.set_style(at("B1"), money);
    s.set(at("B2"), Content::Value(Value::Number(45351.0)));
    s.set_style(at("B2"), date);
    s.set(at("B3"), Content::Formula("B1*2+SUM(B1:B1)".into()));
    s.set(
        at("A4"),
        Content::Formula("IF(B1>1000,\"big\",\"small\")&\"!\"".into()),
    );
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        sheets_ods::write(&Engine::new(wb)).unwrap(),
    )
    .unwrap();
}
