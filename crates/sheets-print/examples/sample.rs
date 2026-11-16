//! Writes a sample PDF: `cargo run -p sheets-print --example sample -- out.pdf`
#![allow(clippy::unwrap_used)]
use sheets_calc::Engine;
use sheets_model::{Align, CellRef, Content, Style, Value, Workbook};
use sheets_print::{Which, to_pdf};

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample.pdf".to_string());
    let mut e = Engine::new(Workbook::new());
    let at = |a: &str| CellRef::parse(a).unwrap();
    let head = e.style_id(&Style {
        bold: true,
        fill: Some(0x00dd_e6f5),
        ..Style::default()
    });
    let money = e.style_id(&Style {
        number_format: "#,##0.00".into(),
        ..Style::default()
    });
    let red = e.style_id(&Style {
        color: Some(0x00c0_0000),
        italic: true,
        align: Align::Center,
        ..Style::default()
    });
    for (i, h) in ["Item", "Qty", "Price", "Total", "Note"].iter().enumerate() {
        let c = CellRef::new(0, u32::try_from(i).unwrap());
        e.set(0, c, Content::Value(Value::Text((*h).into())));
        e.set_style(0, c, head);
    }
    for r in 1..=40_u32 {
        e.set(
            0,
            CellRef::new(r, 0),
            Content::Value(Value::Text(format!("Widget {r} \u{2013} caf\u{e9}"))),
        );
        e.set(
            0,
            CellRef::new(r, 1),
            Content::Value(Value::Number(f64::from(r))),
        );
        e.set(
            0,
            CellRef::new(r, 2),
            Content::Value(Value::Number(f64::from(r) * 12.345)),
        );
        e.set_style(0, CellRef::new(r, 2), money);
        e.set(
            0,
            CellRef::new(r, 3),
            Content::Formula(format!("=B{}*C{}", r + 1, r + 1)),
        );
        e.set_style(0, CellRef::new(r, 3), money);
    }
    let boxed = e.style_id(&Style {
        border: sheets_model::Border {
            top: Some(sheets_model::Line::default()),
            bottom: Some(sheets_model::Line {
                kind: sheets_model::LineKind::Double,
                color: Some(0x00c0_0000),
            }),
            left: Some(sheets_model::Line::default()),
            right: Some(sheets_model::Line::default()),
        },
        font_size: Some(26),
        font_name: Some("Times New Roman".into()),
        ..Style::default()
    });
    e.set(0, at("D3"), Content::Value(Value::Text("boxed".into())));
    e.set_style(0, at("D3"), boxed);
    e.set(0, at("E3"), Content::Value(Value::Text("check".into())));
    e.set_style(0, at("E3"), red);
    std::fs::write(&out, to_pdf(&e, &Which::All, "Sample")).unwrap();
}
