#![allow(clippy::unwrap_used)]
use sheets_calc::Engine;
use sheets_model::{
    Align, Border, CellRef, Content, Line, LineKind, Orientation, PageSetup, Paper, Style, Value,
    Workbook,
};
use sheets_print::{Which, to_pdf};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn text(e: &mut Engine, sheet: usize, cell: &str, t: &str) {
    e.set(sheet, at(cell), Content::Value(Value::Text(t.into())));
}

fn pages(bytes: &[u8]) -> Vec<String> {
    quench_pdf::extract_pages(bytes).unwrap()
}

/// The file as text with the (compressed) page drawings read out, so what is drawn can be searched.
fn inflated(bytes: &[u8]) -> String {
    let mut out = String::from_utf8_lossy(bytes).to_string();
    if let Ok(doc) = quench_pdf::Document::load(bytes) {
        for p in doc.pages() {
            out.push('\n');
            out.push_str(&String::from_utf8_lossy(&doc.page_contents(&p)));
        }
    }
    out
}

#[quench_test::test]
fn a_small_sheet_prints_on_one_page_with_values_formats_and_a_footer() {
    let mut e = Engine::new(Workbook::new());
    text(&mut e, 0, "A1", "Item");
    text(&mut e, 0, "B1", "Price");
    text(&mut e, 0, "A2", "Caf\u{e9} table");
    e.set(0, at("B2"), Content::Value(Value::Number(1234.5)));
    e.set(0, at("B3"), Content::Formula("=B2*2".into()));
    let money = e.style_id(&Style {
        number_format: "#,##0.00".into(),
        bold: true,
        ..Style::default()
    });
    e.set_style(0, at("B2"), money);
    let out = pages(&to_pdf(&e, &Which::One(0), "Budget"));
    assert_eq!(out.len(), 1);
    for want in [
        "Item",
        "Price",
        "Caf\u{e9} table",
        "1,234.50",
        "2469",
        "Sheet1  -  page 1 of 1",
    ] {
        assert!(out[0].contains(want), "{want:?} in {:?}", out[0]);
    }
}

#[quench_test::test]
fn long_sheets_break_into_pages_in_order_and_wide_ones_turn_landscape() {
    let mut e = Engine::new(Workbook::new());
    for r in 1..=120 {
        text(&mut e, 0, &format!("A{r}"), &format!("row {r}"));
    }
    let out = pages(&to_pdf(&e, &Which::One(0), "Long"));
    assert!(out.len() >= 3, "{} pages", out.len());
    assert!(out[0].contains("row 1") && !out[0].contains("row 120"));
    assert!(out.last().unwrap().contains("row 120"));
    assert!(out[1].contains(&format!("page 2 of {}", out.len())));
    // Rows come in order across the pages.
    let joined = out.join("\n");
    let first = joined
        .find("row 2\n")
        .or_else(|| joined.find("row 2 "))
        .unwrap_or(0);
    assert!(joined.find("row 119").unwrap() > first);

    let mut wide = Engine::new(Workbook::new());
    for c in 0..12 {
        wide.set(
            0,
            CellRef::new(0, c),
            Content::Value(Value::Text(format!("col{c}"))),
        );
    }
    let landscape = to_pdf(&wide, &Which::One(0), "Wide");
    let n = pages(&landscape).len();
    assert!(
        n <= 2,
        "twelve 72 pt columns need at most two landscape pages, got {n}"
    );
    let header = String::from_utf8_lossy(&landscape);
    assert!(
        header.contains("/MediaBox [0 0 841.89 595.28]"),
        "landscape A4"
    );
}

#[quench_test::test]
fn all_sheets_in_order_skip_empty_ones_and_an_empty_workbook_says_so() {
    let mut e = Engine::new(Workbook::new());
    e.add_sheet("Empty");
    e.add_sheet("Second");
    text(&mut e, 0, "A1", "first sheet");
    text(&mut e, 2, "A1", "second sheet");
    let out = pages(&to_pdf(&e, &Which::All, "Both"));
    assert_eq!(out.len(), 2);
    assert!(out[0].contains("first sheet") && out[0].contains("Sheet1  -  page 1 of 2"));
    assert!(out[1].contains("second sheet") && out[1].contains("Second  -  page 2 of 2"));
    let none = pages(&to_pdf(
        &Engine::new(Workbook::new()),
        &Which::All,
        "Nothing",
    ));
    assert_eq!(none.len(), 1);
    assert!(none[0].contains("Nothing to print"));
}

#[quench_test::test]
fn alignment_fills_and_errors_do_not_break_the_page() {
    let mut e = Engine::new(Workbook::new());
    text(&mut e, 0, "A1", "left");
    text(&mut e, 0, "B1", "centre");
    text(&mut e, 0, "C1", "right");
    e.set(0, at("D1"), Content::Formula("=1/0".into()));
    e.set(0, at("E5"), Content::Value(Value::Empty)); // nothing there
    for (cell, align) in [
        ("A1", Align::Left),
        ("B1", Align::Center),
        ("C1", Align::Right),
    ] {
        let id = e.style_id(&Style {
            align,
            fill: Some(0x00ff_ff00),
            underline: true,
            color: Some(0x00ff_0000),
            ..Style::default()
        });
        e.set_style(0, at(cell), id);
    }
    // A coloured but empty cell far away still extends the page.
    let fill = e.style_id(&Style {
        fill: Some(0x0000_ff00),
        ..Style::default()
    });
    e.set_style(0, at("B3"), fill);
    let out = pages(&to_pdf(&e, &Which::One(0), "Styles"));
    assert_eq!(out.len(), 1);
    for want in ["left", "centre", "right", "#DIV/0!"] {
        assert!(out[0].contains(want), "{want:?} in {:?}", out[0]);
    }
}

#[quench_test::test]
fn borders_sizes_and_families_are_drawn() {
    let mut e = Engine::new(Workbook::new());
    text(&mut e, 0, "A1", "boxed");
    text(&mut e, 0, "B1", "serif");
    text(&mut e, 0, "C1", "mono");
    let l = |kind| {
        Some(Line {
            kind,
            color: Some(0x00ff_0000),
        })
    };
    let boxed = e.style_id(&Style {
        border: Border {
            top: l(LineKind::Thick),
            bottom: l(LineKind::Double),
            left: l(LineKind::Thin),
            right: None,
        },
        font_size: Some(28),
        ..Style::default()
    });
    let serif = e.style_id(&Style {
        font_name: Some("Times New Roman".into()),
        ..Style::default()
    });
    let mono = e.style_id(&Style {
        font_name: Some("Consolas".into()),
        bold: true,
        ..Style::default()
    });
    e.set_style(0, at("A1"), boxed);
    e.set_style(0, at("B1"), serif);
    e.set_style(0, at("C1"), mono);
    let bytes = to_pdf(&e, &Which::One(0), "Look");
    let raw = inflated(&bytes);
    assert!(raw.contains("1 0 0 RG 2 w"), "thick red line");
    assert!(raw.contains("/F5 10 Tf"), "Times-Roman for the serif cell");
    assert!(
        raw.contains("/F10 10 Tf"),
        "Courier-Bold for the mono bold cell"
    );
    assert!(raw.contains("/F1 14 Tf"), "Helvetica at 14 points");
    let out = pages(&bytes);
    assert!(out[0].contains("boxed") && out[0].contains("serif") && out[0].contains("mono"));
}

#[quench_test::test]
fn page_setup_chooses_paper_orientation_area_margins_gridlines_and_repeated_rows() {
    let mut e = Engine::new(Workbook::new());
    for r in 1..=120 {
        text(&mut e, 0, &format!("A{r}"), &format!("row {r}"));
        text(&mut e, 0, &format!("B{r}"), &format!("beta {r}"));
    }
    let with = |e: &mut Engine, f: &dyn Fn(&mut PageSetup)| {
        let mut wb = e.workbook().clone();
        let mut page = wb.sheets[0].page().clone();
        f(&mut page);
        wb.sheets[0].set_page(page);
        *e = Engine::new(wb);
        to_pdf(e, &Which::One(0), "Setup")
    };
    let raw = inflated;
    // Paper and orientation.
    let letter = with(&mut e, &|p| p.paper = Paper::Letter);
    assert!(raw(&letter).contains("/MediaBox [0 0 612 792]"));
    with(&mut e, &|p| *p = PageSetup::default());
    let landscape = with(&mut e, &|p| p.orientation = Orientation::Landscape);
    assert!(raw(&landscape).contains("/MediaBox [0 0 841.89 595.28]"));
    with(&mut e, &|p| *p = PageSetup::default());
    // Gridlines can be left out.
    let plain = with(&mut e, &|p| p.gridlines = false);
    let lined = with(&mut e, &|p| p.gridlines = true);
    assert!(
        raw(&lined).matches(" 0.25 w ").count() > 10
            && raw(&plain).matches(" 0.25 w ").count() == 0
    );
    // Only the print area.
    let area = with(&mut e, &|p| {
        p.print_area = sheets_model::Range::parse("A3:A5");
    });
    let only = pages(&area);
    assert_eq!(only.len(), 1);
    assert!(
        only[0].contains("row 3")
            && only[0].contains("row 5")
            && !only[0].contains("row 6")
            && !only[0].contains("row 2")
            && !only[0].contains("beta"),
        "{:?}",
        only[0]
    );
    with(&mut e, &|p| *p = PageSetup::default());
    // Rows repeated at the top of every page.
    let titled = pages(&with(&mut e, &|p| p.repeat_rows = Some((0, 0))));
    assert!(titled.len() >= 3);
    assert!(
        titled.iter().all(|p| p.contains("row 1")),
        "the heading is on every page"
    );
    assert!(!titled[1].contains("row 2\n") || titled[1].matches("row 1\n").count() <= 1);
    assert!(titled.last().unwrap().contains("row 120"));
    with(&mut e, &|p| *p = PageSetup::default());
    // A wide sheet shrinks to fit the width, and its text with it.
    let mut wide = Engine::new(Workbook::new());
    for c in 0..14 {
        wide.set(
            0,
            CellRef::new(0, c),
            Content::Value(Value::Text(format!("col{c}"))),
        );
    }
    let big = to_pdf(&wide, &Which::One(0), "Wide");
    assert!(pages(&big).len() >= 2, "too wide for one page");
    let mut wb = wide.workbook().clone();
    let mut page = wb.sheets[0].page().clone();
    page.fit_width = true;
    page.orientation = Orientation::Portrait;
    wb.sheets[0].set_page(page);
    let fitted = to_pdf(&Engine::new(wb), &Which::One(0), "Wide");
    assert_eq!(pages(&fitted).len(), 1, "one page when fitted");
    assert!(raw(&fitted).contains("/MediaBox [0 0 595.28 841.89]"));
    // Margins.
    let wide_margin = with(&mut e, &|p| p.margin_mm = 40);
    assert!(
        raw(&wide_margin).contains(" 113.39 "),
        "40 mm is 113.39 points"
    );
}

#[quench_test::test]
fn charts_are_drawn_on_the_page_that_holds_their_corner() {
    use sheets_model::{Chart, ChartKind, Range};
    let mut e = Engine::new(Workbook::new());
    for (r, (label, a)) in [("Quarter", 0.0), ("Q1", 10.0), ("Q2", 20.0), ("Q3", 30.0)]
        .into_iter()
        .enumerate()
    {
        let r = u32::try_from(r).unwrap();
        e.set(
            0,
            CellRef::new(r, 0),
            Content::Value(Value::Text(label.into())),
        );
        e.set(
            0,
            CellRef::new(r, 1),
            if r == 0 {
                Content::Value(Value::Text("Sales".into()))
            } else {
                Content::Value(Value::Number(a))
            },
        );
    }
    let mut wb = e.workbook().clone();
    let mut chart = Chart::new(ChartKind::Column, Range::parse("A1:B4").unwrap());
    chart.title = "Sales by quarter".into();
    chart.at = CellRef::new(6, 0);
    let mut pie = Chart::new(ChartKind::Pie, Range::parse("A1:B4").unwrap());
    pie.at = CellRef::new(6, 4);
    wb.sheets[0].set_charts(vec![chart, pie]);
    let e = Engine::new(wb);
    let pdf = to_pdf(&e, &Which::One(0), "Charts");
    let raw = inflated(&pdf);
    assert!(
        raw.contains(" 4E79A7") || raw.contains("0.31 0.47 0.65 rg"),
        "the first series colour is used: {}",
        &raw[..raw.len().min(600)]
    );
    assert!(
        raw.matches(" f\n").count() > 8,
        "bars, slices and boxes are filled"
    );
    let out = pages(&pdf);
    assert!(
        out[0].contains("Sales by quarter"),
        "the title is text: {:?}",
        out[0]
    );
    assert!(
        out[0].contains("Q2") && out[0].contains("30"),
        "axis labels: {:?}",
        out[0]
    );
    // A sheet with nothing but a chart still prints it.
    let mut wb = Workbook::new();
    let mut c = Chart::new(ChartKind::Line, Range::parse("A1:A2").unwrap());
    c.title = "Alone".into();
    wb.sheets[0].set_charts(vec![c]);
    let alone = pages(&to_pdf(&Engine::new(wb), &Which::One(0), "Alone"));
    assert!(
        alone[0].contains("Alone") || alone[0].contains("Nothing to show"),
        "{alone:?}"
    );
}

quench_test::main!();
