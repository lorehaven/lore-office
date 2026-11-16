use sheets_model::{CellRef, Content, Style, Value, Workbook};

/// A workbook to try the screen on: `rows` rows of numbers, text, formulas and a running total.
#[must_use]
pub fn demo_workbook(rows: u32) -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Data".into();
    let money = wb.style_id(&Style {
        number_format: "#,##0.00".into(),
        ..Default::default()
    });
    let head = ["Id", "Item", "Price", "Parity", "Running total", "Label"];
    for (c, h) in head.iter().enumerate() {
        wb.sheets[0].set(
            CellRef::new(0, u32::try_from(c).unwrap_or(0)),
            Content::Value(Value::Text((*h).into())),
        );
    }
    for r in 1..=rows {
        let n = r + 1; // the row number as shown
        let s = &mut wb.sheets[0];
        s.set(
            CellRef::new(r, 0),
            Content::Value(Value::Number(f64::from(r))),
        );
        s.set(
            CellRef::new(r, 1),
            Content::Value(Value::Text(format!("item {r}"))),
        );
        s.set(CellRef::new(r, 2), Content::Formula(format!("A{n}*1.5")));
        s.set_style(CellRef::new(r, 2), money);
        s.set(
            CellRef::new(r, 3),
            Content::Formula(format!("IF(MOD(A{n},2)=0,\"even\",\"odd\")")),
        );
        let prev = if r == 1 {
            "0".to_string()
        } else {
            format!("E{}", n - 1)
        };
        s.set(CellRef::new(r, 4), Content::Formula(format!("{prev}+C{n}")));
        s.set_style(CellRef::new(r, 4), money);
        s.set(
            CellRef::new(r, 5),
            Content::Formula(format!("B{n}&\" (\"&D{n}&\")\"")),
        );
    }
    wb
}
