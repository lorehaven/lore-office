#![allow(clippy::unwrap_used)]
use sheets_model::{CellRef, Chart, ChartKind, Content, Range, Value, Workbook};
use sheets_xlsx::Xlsx;

fn main() {
    let mut wb = Workbook::new();
    for (i, v) in [5.0, 12.0, 7.0, 30.0, 18.0].into_iter().enumerate() {
        wb.sheets[0].set(
            CellRef::new(u32::try_from(i).unwrap(), 0),
            Content::Value(Value::Number(v)),
        );
        wb.sheets[0].set(
            CellRef::new(u32::try_from(i).unwrap(), 1),
            Content::Value(Value::Number(v * 2.0)),
        );
    }
    let mut c = Chart::new(ChartKind::Column, Range::parse("A1:B5").unwrap());
    c.title = "Items".into();
    c.at = CellRef::new(1, 3);
    wb.sheets[0].set_charts(vec![c]);
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        Xlsx::new(wb).write().unwrap(),
    )
    .unwrap();
}
