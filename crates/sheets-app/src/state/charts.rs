//! Charts on the sheet on screen: drawing them, picking, moving, and the form that makes and changes them.

use super::{Dialog, DialogId, Field, FieldKind, SheetState};
use sheets_model::{CellRef, Chart, ChartKind, Range, SparkKind, Sparkline};

/// An SVG as the text of a CSS `url(...)` or an image `src`.
fn svg_uri(svg: &str) -> String {
    let mut out = String::from("data:image/svg+xml;utf8,");
    for c in svg.chars() {
        match c {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '"' => out.push_str("%22"),
            '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

impl SheetState {
    /// The sparkline in a cell, drawn for a cell `size` pixels (width, height).
    #[allow(clippy::unused_self)]
    pub(super) fn spark_image(
        &self,
        e: &sheets_calc::Engine,
        sheet: usize,
        at: CellRef,
        size: (f64, f64),
    ) -> Option<String> {
        let sp = e.workbook().sheets.get(sheet)?.sparkline_at(at)?;
        if size.0 < 4.0 || size.1 < 4.0 {
            return None;
        }
        let values: Vec<f64> = sp
            .data
            .cells()
            .take(2000)
            .map(|c| match e.value(sheet, c) {
                sheets_model::Value::Number(n) => n,
                _ => f64::NAN,
            })
            .collect();
        Some(svg_uri(&sheets_chart::sparkline_svg(
            sp.kind, &values, size.0, size.1, sp.color,
        )))
    }

    /// The charts of the sheet on screen.
    #[must_use]
    pub fn charts(&self) -> Vec<Chart> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .map(|s| s.charts().to_vec())
            .unwrap_or_default()
    }

    /// A chart drawn from the cells as they are now, as the `src` of an image (an SVG, percent-encoded).
    #[must_use]
    pub fn chart_image(&self, index: usize) -> String {
        let Some(chart) = self.charts().get(index).cloned() else {
            return String::new();
        };
        let e = self.engine();
        let sheet = self.sheet();
        let r = chart.data;
        let cells: Vec<Vec<sheets_model::Value>> = (r.start.row
            ..=r.end.row.min(r.start.row + 5000))
            .map(|row| {
                (r.start.col..=r.end.col)
                    .map(|col| e.value(sheet, CellRef::new(row, col)))
                    .collect()
            })
            .collect();
        let data = sheets_chart::dataset(&chart, &cells);
        let (w, h) = (f64::from(chart.width), f64::from(chart.height));
        let svg = sheets_chart::to_svg(&sheets_chart::layout(&chart, &data, w, h), w, h);
        svg_uri(&svg)
    }

    /// Pick a chart (and clear the cell selection's focus from it).
    pub fn select_chart(&self, index: usize) {
        self.chart_selected.set(Some(index));
    }

    /// The pointer went down on a chart: pick it and start carrying it.
    pub fn begin_chart_drag(&self, index: usize, x: f64, y: f64) {
        self.chart_selected.set(Some(index));
        self.chart_grab.set(Some((index, x, y)));
    }

    /// The pointer moved while a chart is carried: it would land at the cell under its new corner.
    pub fn chart_pointer(&self, x: f64, y: f64) {
        let Some((index, x0, y0)) = self.chart_grab.get() else {
            return;
        };
        let Some(chart) = self.charts().get(index).cloned() else {
            return;
        };
        let axes = self.axes.get_untracked();
        let z = self.zoom_factor();
        let left = axes.cols.offset(chart.at.col) + (x - x0) / z;
        let top = axes.rows.offset(chart.at.row) + (y - y0) / z;
        let at = CellRef::new(
            axes.rows.index_at(top.max(0.0)),
            axes.cols.index_at(left.max(0.0)),
        );
        if at != chart.at {
            self.chart_drop.set(Some(at));
        }
    }

    /// The pointer came up: the chart stays where it was dropped.
    pub fn end_chart_drag(&self) {
        let Some((index, ..)) = self.chart_grab.take() else {
            return;
        };
        let Some(at) = self.chart_drop.get_untracked() else {
            return;
        };
        self.chart_drop.set(None);
        let sheet = self.sheet();
        self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut charts = s.charts().to_vec();
                if let Some(c) = charts.get_mut(index) {
                    c.at = at;
                }
                s.set_charts(charts);
            })
            .is_some()
        });
    }

    /// Remove the chart that is picked.
    pub fn delete_chart(&self) {
        let Some(index) = self.chart_selected.get_untracked() else {
            self.status.set(crate::i18n::tr("Click a chart first"));
            return;
        };
        let sheet = self.sheet();
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut charts = s.charts().to_vec();
                if index < charts.len() {
                    charts.remove(index);
                }
                s.set_charts(charts);
            })
            .is_some()
        }) {
            self.chart_selected.set(None);
            self.status.set(crate::i18n::tr("Removed the chart"));
        }
    }

    /// The form for a new chart of this kind over the selection.
    pub fn new_chart_dialog(&self, kind: ChartKind) {
        let sel = self.selection();
        let range = if sel.start == sel.end {
            self.current_region(sel.start)
        } else {
            sel
        };
        let chart = Chart::new(kind, range);
        self.chart_dialog(&chart, false);
    }

    /// The form for changing the chart that is picked.
    pub fn edit_chart_dialog(&self) {
        let Some(index) = self.chart_selected.get_untracked() else {
            self.status.set(crate::i18n::tr("Click a chart first"));
            return;
        };
        if let Some(chart) = self.charts().get(index).cloned() {
            self.chart_dialog(&chart, true);
        }
    }

    fn chart_dialog(&self, chart: &Chart, editing: bool) {
        let text = |label: &str, value: String, hint: &str| Field {
            label: label.into(),
            kind: FieldKind::Text,
            value,
            hint: hint.into(),
        };
        let kinds: Vec<(String, String)> = [
            ("column", "Columns"),
            ("bar", "Bars"),
            ("line", "Lines"),
            ("area", "Areas"),
            ("pie", "Pie"),
            ("scatter", "Scatter plot"),
        ]
        .iter()
        .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
        .collect();
        let range = if chart.data.start == chart.data.end {
            chart.data.start.to_string()
        } else {
            chart.data.to_string()
        };
        self.dialog.set(Some(Dialog {
            id: DialogId::Chart,
            title: if editing {
                "Change the chart".into()
            } else {
                "New chart".into()
            },
            intro: crate::i18n::tr(
                "Names in the first row and labels in the first column are found by themselves.",
            ),
            fields: vec![
                Field {
                    label: "Kind".into(),
                    kind: FieldKind::Choice(kinds),
                    value: chart.kind.name().into(),
                    hint: String::new(),
                },
                text("Title", chart.title.clone(), ""),
                text("Cells", range, "A1:C5"),
                Field {
                    label: "A series is a".into(),
                    kind: FieldKind::Choice(vec![
                        ("column".into(), "column".into()),
                        ("row".into(), "row".into()),
                    ]),
                    value: if chart.rows_as_series {
                        "row".into()
                    } else {
                        "column".into()
                    },
                    hint: String::new(),
                },
                Field {
                    label: "Show the legend".into(),
                    kind: FieldKind::Check,
                    value: if chart.legend {
                        "1".into()
                    } else {
                        String::new()
                    },
                    hint: String::new(),
                },
                text("Width (pixels)", chart.width.to_string(), "480"),
                text("Height (pixels)", chart.height.to_string(), "300"),
                text("Top left corner", chart.at.to_string(), "F2"),
            ],
            error: String::new(),
        }));
    }

    /// Apply the chart form: a new chart, or the picked one changed.
    pub(super) fn submit_chart(&self, fields: &[String]) -> Result<String, String> {
        let f = |i: usize| {
            fields
                .get(i)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let kind = ChartKind::from_name(&f(0)).unwrap_or(ChartKind::Column);
        let t = f(2).replace('$', "");
        let Some(data) = Range::parse(&t).or_else(|| CellRef::parse(&t).map(|c| Range::new(c, c)))
        else {
            return Err(crate::i18n::tr("Write the cells like A1:C5"));
        };
        let (Ok(width), Ok(height)) = (f(5).parse::<u32>(), f(6).parse::<u32>()) else {
            return Err(crate::i18n::tr("The size is a number of pixels"));
        };
        if !(80..=4000).contains(&width) || !(60..=4000).contains(&height) {
            return Err(crate::i18n::tr(
                "A chart is from 80 by 60 to 4000 by 4000 pixels",
            ));
        }
        let Some(at) = CellRef::parse(&f(7).replace('$', "")) else {
            return Err(crate::i18n::tr("Write the corner as a cell, like F2"));
        };
        let chart = Chart {
            kind,
            title: f(1),
            data,
            rows_as_series: f(3) == "row",
            legend: !f(4).is_empty(),
            at,
            width,
            height,
        };
        let editing = self
            .dialog
            .get_untracked()
            .is_some_and(|d| d.title.starts_with("Change"));
        let picked = self.chart_selected.get_untracked();
        let sheet = self.sheet();
        let mut chosen = None;
        self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut charts = s.charts().to_vec();
                match picked.filter(|_| editing) {
                    Some(i) if i < charts.len() => {
                        charts[i] = chart;
                        chosen = Some(i);
                    }
                    _ => {
                        charts.push(chart);
                        chosen = Some(charts.len() - 1);
                    }
                }
                s.set_charts(charts);
            })
            .is_some()
        });
        self.chart_selected.set(chosen);
        Ok(if editing {
            "Chart changed".to_string()
        } else {
            "Chart added".to_string()
        })
    }
}

impl SheetState {
    /// The form for sparklines over the selection.
    pub fn new_sparkline_dialog(&self, kind: SparkKind) {
        let sel = self.selection();
        let range = if sel.start == sel.end {
            self.current_region(sel.start)
        } else {
            sel
        };
        let name = match kind {
            SparkKind::Line => "line",
            SparkKind::Column => "column",
            SparkKind::WinLoss => "winloss",
        };
        // The cells just right of the data, one for each row.
        let place = Range::new(
            CellRef::new(range.start.row, range.end.col + 1),
            CellRef::new(range.end.row, range.end.col + 1),
        );
        let text = |label: &str, value: String, hint: &str| Field {
            label: label.into(),
            kind: FieldKind::Text,
            value,
            hint: hint.into(),
        };
        self.dialog.set(Some(Dialog {
            id: DialogId::Sparkline,
            title: crate::i18n::tr("Sparklines"),
            intro: crate::i18n::tr("A tiny chart in a cell. Each row of data (or, for cells in a row, each column) gets its own."),
            fields: vec![
                Field {
                    label: "Kind".into(),
                    kind: FieldKind::Choice(vec![("line".into(), "Line".into()), ("column".into(), "Columns".into()), ("winloss".into(), "Win / loss".into())]),
                    value: name.into(),
                    hint: String::new(),
                },
                text("Data", range.to_string(), "B2:F5"),
                text("Put them in", if place.start == place.end { place.start.to_string() } else { place.to_string() }, "G2:G5"),
                Field { label: "Colour".into(), kind: FieldKind::Color, value: "#376092".into(), hint: String::new() },
            ],
            error: String::new(),
        }));
    }

    pub(super) fn submit_sparkline(&self, fields: &[String]) -> Result<String, String> {
        let f = |i: usize| {
            fields
                .get(i)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let range = |t: String| {
            let t = t.replace('$', "");
            Range::parse(&t).or_else(|| CellRef::parse(&t).map(|c| Range::new(c, c)))
        };
        let (Some(data), Some(place)) = (range(f(1)), range(f(2))) else {
            return Err(crate::i18n::tr(
                "Write the data and the place like B2:F5 and G2:G5",
            ));
        };
        let kind = match f(0).as_str() {
            "column" => SparkKind::Column,
            "winloss" => SparkKind::WinLoss,
            _ => SparkKind::Line,
        };
        let color = crate::format::parse_color(&f(3)).unwrap_or(0x0037_6092);
        let cells: Vec<CellRef> = place.cells().collect();
        // One sparkline per row of data when the places are a column; per column when they are a row; or all the data in one.
        let pieces: Vec<Range> = if cells.len() == 1 {
            vec![data]
        } else if place.cols() == 1 && place.rows() == data.rows() {
            (data.start.row..=data.end.row)
                .map(|r| {
                    Range::new(
                        CellRef::new(r, data.start.col),
                        CellRef::new(r, data.end.col),
                    )
                })
                .collect()
        } else if place.rows() == 1 && place.cols() == data.cols() {
            (data.start.col..=data.end.col)
                .map(|c| {
                    Range::new(
                        CellRef::new(data.start.row, c),
                        CellRef::new(data.end.row, c),
                    )
                })
                .collect()
        } else {
            return Err(crate::i18n::tr(
                "Put them in one cell, or in as many cells in a line as there are rows (or columns) of data",
            ));
        };
        let sheet = self.sheet();
        self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut all: Vec<Sparkline> = s
                    .sparklines()
                    .iter()
                    .filter(|sp| !cells.contains(&sp.cell))
                    .copied()
                    .collect();
                all.extend(cells.iter().zip(pieces).map(|(cell, data)| Sparkline {
                    cell: *cell,
                    data,
                    kind,
                    color,
                }));
                s.set_sparklines(all);
            })
            .is_some()
        });
        Ok(format!(
            "{} sparkline{}",
            cells.len(),
            if cells.len() == 1 { "" } else { "s" }
        ))
    }

    /// Remove the sparklines in the selected cells.
    pub fn clear_sparklines(&self) {
        let (sheet, range) = (self.sheet(), self.selection());
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let before = s.sparklines().len();
                let kept: Vec<Sparkline> = s
                    .sparklines()
                    .iter()
                    .filter(|sp| !range.contains(sp.cell))
                    .copied()
                    .collect();
                let changed = kept.len() != before;
                s.set_sparklines(kept);
                changed
            })
            .unwrap_or(false)
        }) {
            self.status.set(crate::i18n::tr("Removed the sparklines"));
        }
    }
}
