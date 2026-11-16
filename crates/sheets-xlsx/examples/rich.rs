//! Writes a workbook that uses every feature the suite saves, for checking in other spreadsheets:
//! `cargo run -p sheets-xlsx --example rich -- out.xlsx`
#![allow(clippy::unwrap_used, clippy::unreadable_literal)]
use sheets_model::{
    Allow, Border, CellRef, Compare, Cond, CondRule, Content, Dxf, Line, LineKind, Range, Severity,
    Style, Validation, Value, Workbook,
};
use sheets_xlsx::Xlsx;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rich.xlsx".into());
    let mut wb = Workbook::new();
    let r = |t: &str| Range::parse(t).unwrap();
    for (i, v) in [5.0, 12.0, 7.0, 30.0, 18.0].into_iter().enumerate() {
        wb.sheets[0].set(
            CellRef::new(u32::try_from(i).unwrap(), 0),
            Content::Value(Value::Number(v)),
        );
        wb.sheets[0].set(
            CellRef::new(u32::try_from(i).unwrap(), 1),
            Content::Value(Value::Text(format!("item {i}"))),
        );
    }
    wb.sheets[0].set(CellRef::new(0, 3), Content::Formula("SUM(Data)*Tax".into()));
    wb.set_name("Data", "Sheet1!$A$1:$A$5");
    wb.set_name("Tax", "0.25");
    let boxed = wb.style_id(&Style {
        border: Border {
            top: Some(Line {
                kind: LineKind::Thick,
                color: Some(0xFF0000),
            }),
            bottom: Some(Line {
                kind: LineKind::Double,
                color: None,
            }),
            left: None,
            right: None,
        },
        font_size: Some(28),
        font_name: Some("Courier New".into()),
        bold: true,
        ..Style::default()
    });
    wb.sheets[0].set_style(CellRef::new(0, 1), boxed);
    wb.sheets[0].set_freeze(1, 1);
    wb.sheets[0].merge(r("F1:G2"));
    wb.sheets[0].set_row_hidden(2, true);
    wb.sheets[0].set_validations(vec![Validation {
        range: r("C1:C5"),
        allow: Allow::List(vec!["x".into(), "y".into()]),
        allow_blank: true,
        dropdown: true,
        severity: Severity::Stop,
        title: String::new(),
        message: "x or y".into(),
    }]);
    wb.sheets[0].set_cond_rules(vec![
        CondRule {
            range: r("A1:A5"),
            cond: Cond::CellIs {
                op: Compare::Greater,
                a: "10".into(),
                b: String::new(),
            },
            dxf: Dxf {
                bold: true,
                italic: false,
                color: Some(0x9C0006),
                fill: Some(0xFFC7CE),
            },
            priority: 1,
            stop_if_true: false,
        },
        CondRule {
            range: r("A1:A5"),
            cond: Cond::DataBar { color: 0x638EC6 },
            dxf: Dxf::default(),
            priority: 2,
            stop_if_true: false,
        },
    ]);
    let mut chart = sheets_model::Chart::new(sheets_model::ChartKind::Column, r("A1:B5"));
    chart.title = "Items".into();
    chart.at = CellRef::new(7, 0);
    wb.sheets[0].set_charts(vec![chart]);
    wb.sheets[0].set_comment(CellRef::new(1, 0), "a note");
    std::fs::write(out, Xlsx::new(wb).write().unwrap()).unwrap();
}
