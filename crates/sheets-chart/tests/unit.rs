#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::missing_const_for_fn,
    clippy::assert_is_empty
)]
use sheets_chart::{Anchor, Shape, compact, dataset, layout, nice_scale, to_svg};
use sheets_model::{CellRef, Chart, ChartKind, Range, Value};

fn t(s: &str) -> Value {
    Value::Text(s.into())
}
fn n(x: f64) -> Value {
    Value::Number(x)
}

fn sales() -> Vec<Vec<Value>> {
    vec![
        vec![t("Quarter"), t("North"), t("South")],
        vec![t("Q1"), n(10.0), n(30.0)],
        vec![t("Q2"), n(20.0), n(25.0)],
        vec![t("Q3"), n(15.0), Value::Empty],
        vec![t("Q4"), n(40.0), n(5.0)],
    ]
}

fn chart(kind: ChartKind) -> Chart {
    Chart::new(kind, Range::parse("A1:C5").unwrap())
}

#[quench_test::test]
fn the_dataset_finds_the_names_the_categories_and_the_numbers() {
    let d = dataset(&chart(ChartKind::Column), &sales());
    assert_eq!(d.categories, ["Q1", "Q2", "Q3", "Q4"]);
    assert_eq!(
        d.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["North", "South"]
    );
    assert_eq!(d.series[0].values, [10.0, 20.0, 15.0, 40.0]);
    assert!(d.series[1].values[2].is_nan(), "an empty cell is a gap");
    // Without headers or labels: counted from 1, series numbered.
    let plain = vec![vec![n(3.0), n(4.0)], vec![n(5.0), n(6.0)]];
    let d = dataset(&chart(ChartKind::Line), &plain);
    assert_eq!(d.categories, ["1", "2"]);
    assert_eq!(
        d.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Series 1", "Series 2"]
    );
    // Rows as series.
    let mut by_row = chart(ChartKind::Column);
    by_row.rows_as_series = true;
    let d = dataset(&by_row, &plain);
    assert_eq!(
        d.series[0].values,
        [3.0, 4.0],
        "the first row is the first series"
    );
    assert_eq!(d.series.len(), 2);
    // A scatter plot uses the first column of numbers across.
    let xy = vec![
        vec![t("x"), t("y")],
        vec![n(1.0), n(10.0)],
        vec![n(2.5), n(20.0)],
    ];
    let d = dataset(&chart(ChartKind::Scatter), &xy);
    assert_eq!(
        (d.x, d.series[0].values.clone()),
        (vec![1.0, 2.5], vec![10.0, 20.0])
    );
    assert!(dataset(&chart(ChartKind::Pie), &[]).series.is_empty());
}

#[quench_test::test]
fn scales_end_on_round_numbers_and_numbers_are_short() {
    assert_eq!(nice_scale(0.0, 40.0, 5), (0.0, 40.0, 10.0));
    assert_eq!(nice_scale(3.0, 97.0, 5), (0.0, 100.0, 20.0));
    assert_eq!(nice_scale(0.0, 1.0, 5), (0.0, 1.0, 0.2));
    assert_eq!(nice_scale(5.0, 5.0, 5).2, 0.5, "no range: still a scale");
    let (lo, hi, _) = nice_scale(-7.0, 12.0, 5);
    assert!(lo <= -7.0 && hi >= 12.0);
    assert_eq!(compact(1500.0), "1500");
    assert_eq!(compact(25_000.0), "25K");
    assert_eq!(compact(2_250_000.0), "2.25M");
    assert_eq!(compact(0.5), "0.5");
    assert_eq!(compact(-3.0), "-3");
}

fn count(shapes: &[Shape], f: impl Fn(&Shape) -> bool) -> usize {
    shapes.iter().filter(|s| f(s)).count()
}

#[quench_test::test]
fn every_kind_of_chart_draws_what_the_numbers_say() {
    let data = |k| dataset(&chart(k), &sales());
    let is_rect = |s: &Shape| matches!(s, Shape::Rect { .. });
    // Columns: one bar per number (7), and a legend swatch per series (2); the bars are in proportion.
    let shapes = layout(
        &chart(ChartKind::Column),
        &data(ChartKind::Column),
        480.0,
        300.0,
    );
    assert_eq!(count(&shapes, is_rect), 7 + 2);
    let bar = |v: f64| {
        shapes.iter().find_map(|s| {
            if let Shape::Rect { h, w, .. } = s {
                (*w > 10.0 && (*h - v).abs() < 80.0 * 2.0).then_some(*h)
            } else {
                None
            }
        })
    };
    assert!(bar(0.0).is_some());
    let heights: Vec<f64> = shapes
        .iter()
        .filter_map(|s| {
            if let Shape::Rect { h, w, .. } = s {
                (*w > 12.0).then_some(*h)
            } else {
                None
            }
        })
        .collect();
    assert!(
        heights[3] > heights[0] * 3.9 && heights[3] < heights[0] * 4.1,
        "40 is four times 10: {heights:?}"
    );
    // The title and the axis labels are text.
    let mut titled = chart(ChartKind::Column);
    titled.title = "Sales & costs".into();
    let shapes = layout(&titled, &data(ChartKind::Column), 480.0, 300.0);
    assert!(shapes.iter().any(|s| matches!(s, Shape::Text { text, bold: true, anchor: Anchor::Middle, .. } if text == "Sales & costs")));
    assert!(
        shapes
            .iter()
            .any(|s| matches!(s, Shape::Text { text, .. } if text == "Q3"))
    );
    assert!(
        shapes
            .iter()
            .any(|s| matches!(s, Shape::Text { text, .. } if text == "40")),
        "the top of the scale"
    );
    // Horizontal bars, lines, areas and scatter plots.
    let bars = layout(&chart(ChartKind::Bar), &data(ChartKind::Bar), 480.0, 300.0);
    assert!(
        bars.iter()
            .any(|s| matches!(s, Shape::Rect { w, h, .. } if *w > *h * 2.0)),
        "bars lie down"
    );
    let lines = layout(
        &chart(ChartKind::Line),
        &data(ChartKind::Line),
        480.0,
        300.0,
    );
    assert_eq!(
        count(
            &lines,
            |s| matches!(s, Shape::Line { width, .. } if *width > 1.5)
        ),
        2,
        "one line per series"
    );
    let area = layout(
        &chart(ChartKind::Area),
        &data(ChartKind::Area),
        480.0,
        300.0,
    );
    assert_eq!(count(&area, |s| matches!(s, Shape::Polygon { .. })), 2);
    let xy = vec![
        vec![t("x"), t("y")],
        vec![n(1.0), n(10.0)],
        vec![n(2.0), n(20.0)],
        vec![n(3.0), n(15.0)],
    ];
    let sc = layout(
        &chart(ChartKind::Scatter),
        &dataset(&chart(ChartKind::Scatter), &xy),
        480.0,
        300.0,
    );
    assert_eq!(
        count(&sc, |s| matches!(s, Shape::Rect { w, .. } if *w == 6.0)),
        3
    );
}

#[quench_test::test]
fn pies_are_slices_that_add_up_and_empty_charts_say_so() {
    let mut pie = chart(ChartKind::Pie);
    pie.legend = true;
    let d = dataset(&pie, &sales());
    let shapes = layout(&pie, &d, 400.0, 300.0);
    assert_eq!(
        count(&shapes, |s| matches!(s, Shape::Polygon { .. })),
        4,
        "one slice per category"
    );
    let labels: Vec<String> = shapes
        .iter()
        .filter_map(|s| {
            if let Shape::Text {
                text, bold: true, ..
            } = s
            {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(labels, ["12%", "24%", "18%", "47%"], "percent of 85");
    assert_eq!(
        count(&shapes, |s| matches!(s, Shape::Rect { .. })),
        4,
        "a legend entry for each"
    );
    let none = layout(
        &chart(ChartKind::Pie),
        &dataset(&chart(ChartKind::Pie), &[]),
        300.0,
        200.0,
    );
    assert!(matches!(&none[0], Shape::Text { text, .. } if text == "Nothing to show"));
    let zeros = vec![vec![t("a"), n(0.0)], vec![t("b"), n(-1.0)]];
    let flat = layout(&pie, &dataset(&pie, &zeros), 300.0, 200.0);
    assert!(
        flat.iter()
            .any(|s| matches!(s, Shape::Text { text, .. } if text == "Nothing to show"))
    );
}

#[quench_test::test]
fn the_svg_is_a_whole_document_with_the_text_escaped() {
    let mut c = chart(ChartKind::Column);
    c.title = "A <b> & \"c\"".into();
    let svg = to_svg(
        &layout(&c, &dataset(&c, &sales()), 480.0, 300.0),
        480.0,
        300.0,
    );
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"300\"")
    );
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("A &lt;b&gt; &amp; &quot;c&quot;"));
    assert!(svg.contains("fill=\"#4e79a7\""), "the first series colour");
    assert_eq!(svg.matches('<').count(), svg.matches('>').count());
    let _ = CellRef::new(0, 0);
}

#[quench_test::test]
fn sparklines_are_tiny_drawings_of_one_row_of_numbers() {
    use sheets_chart::sparkline_svg;
    use sheets_model::SparkKind;
    let values = [3.0, 7.0, 1.0, 9.0, f64::NAN, 5.0];
    let line = sparkline_svg(SparkKind::Line, &values, 100.0, 24.0, 0x0037_6092);
    assert!(
        line.starts_with("<svg")
            && line.ends_with("</svg>")
            && line.contains("<polyline")
            && line.contains("<circle")
            && line.contains("#376092"),
        "{line}"
    );
    assert_eq!(line.matches(',').count(), 5, "five points, the gap skipped");
    let columns = sparkline_svg(SparkKind::Column, &values, 100.0, 24.0, 0x00ff_0000);
    assert_eq!(columns.matches("<rect").count(), 5, "a bar for each number");
    let win = sparkline_svg(
        SparkKind::WinLoss,
        &[1.0, -2.0, 0.0, 5.0],
        100.0,
        24.0,
        0x0000_8000,
    );
    assert_eq!(win.matches("<rect").count(), 3, "no bar for zero");
    assert!(
        win.contains("#e15759") && win.contains("#008000"),
        "losses and wins have their own colours"
    );
    assert!(!sparkline_svg(SparkKind::Line, &[], 50.0, 20.0, 0).contains("<polyline"));
    assert!(sparkline_svg(SparkKind::Column, &[f64::NAN], 50.0, 20.0, 0).ends_with("</svg>"));
}

quench_test::main!();
