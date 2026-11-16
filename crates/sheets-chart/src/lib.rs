//! Charts as shapes.
//!
//! [`dataset`] reads what a chart shows out of cells; [`layout`] turns it into [`Shape`]s in a box of a given size; [`to_svg`]
//! writes them as an SVG document.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::while_float
)]

use sheets_model::{Chart, ChartKind, Value};
use std::fmt::Write as _;

/// The colours series take in turn.
pub const PALETTE: [u32; 8] = [
    0x004E_79A7,
    0x00F2_8E2B,
    0x00E1_5759,
    0x0076_B7B2,
    0x0059_A14F,
    0x00ED_C948,
    0x00B0_7AA1,
    0x00FF_9DA7,
];

const INK: u32 = 0x0044_4444;
const GRID: u32 = 0x00DD_DDDD;

/// One row of numbers with its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    /// `NaN` where a cell held no number.
    pub values: Vec<f64>,
}

/// What a chart shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Dataset {
    /// The first row (or, for rows as series, the first column) of the cells holds the names of the series.
    pub header: bool,
    /// The first column (or row) holds the labels of the points: text, or the x values of a scatter plot.
    pub labels: bool,
    /// The label of each point.
    pub categories: Vec<String>,
    /// Where each point is across (scatter plots).
    pub x: Vec<f64>,
    pub series: Vec<Series>,
}

fn text_of(v: &Value) -> String {
    match v {
        Value::Empty => String::new(),
        Value::Number(n) => compact(*n),
        other => other.to_string(),
    }
}

fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => f64::NAN,
    }
}

/// The dataset for a chart from the values of its range, row by row. A first row of text over numbers names the series; a
/// first column of text (or, for a scatter plot, of numbers) is the categories.
#[must_use]
pub fn dataset(chart: &Chart, cells: &[Vec<Value>]) -> Dataset {
    let grid: Vec<Vec<Value>> = if chart.rows_as_series {
        let n = cells.iter().map(Vec::len).max().unwrap_or(0);
        (0..n)
            .map(|c| {
                cells
                    .iter()
                    .map(|r| r.get(c).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    } else {
        cells.to_vec()
    };
    let rows = grid.len();
    let cols = grid.iter().map(Vec::len).max().unwrap_or(0);
    if rows == 0 || cols == 0 {
        return Dataset {
            header: false,
            labels: false,
            categories: Vec::new(),
            x: Vec::new(),
            series: Vec::new(),
        };
    }
    let at = |r: usize, c: usize| grid[r].get(c).cloned().unwrap_or_default();
    let numeric = |v: &Value| matches!(v, Value::Number(_));
    // A header row: the first row is text and some cell below it is a number.
    let header = rows > 1
        && (0..cols).any(|c| matches!(at(0, c), Value::Text(_)))
        && (1..rows).any(|r| (0..cols).any(|c| numeric(&at(r, c))))
        && !(0..cols).any(|c| numeric(&at(0, c)));
    let first = usize::from(header);
    // A label column: the first column is text (or, for scatter plots, the x values).
    let label_col = cols > 1
        && (first..rows)
            .filter(|r| !matches!(at(*r, 0), Value::Empty))
            .all(|r| !numeric(&at(r, 0)))
        && (first..rows).any(|r| matches!(at(r, 0), Value::Text(_)));
    let scatter = chart.kind == ChartKind::Scatter;
    let x_col = scatter && !label_col && cols > 1;
    let start = usize::from(label_col || x_col);
    let categories = (first..rows)
        .map(|r| {
            if label_col || x_col {
                text_of(&at(r, 0))
            } else {
                (r - first + 1).to_string()
            }
        })
        .collect();
    let x = (first..rows)
        .map(|r| {
            if x_col {
                number(&at(r, 0))
            } else {
                (r - first + 1) as f64
            }
        })
        .collect();
    let series = (start..cols)
        .map(|c| Series {
            name: if header {
                text_of(&at(0, c))
            } else {
                format!("Series {}", c - start + 1)
            },
            values: (first..rows).map(|r| number(&at(r, c))).collect(),
        })
        .collect();
    Dataset {
        header,
        labels: label_col || x_col,
        categories,
        x,
        series,
    }
}

/// How text is anchored at its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// One thing a chart draws, in pixels from its top left corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: u32,
    },
    Line {
        points: Vec<(f64, f64)>,
        width: f64,
        color: u32,
    },
    Polygon {
        points: Vec<(f64, f64)>,
        fill: u32,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        size: f64,
        anchor: Anchor,
        color: u32,
        bold: bool,
    },
}

/// A number as short text: `1.5K`, `2.25M`, `0.5`.
#[must_use]
pub fn compact(n: f64) -> String {
    let a = n.abs();
    let trim = |x: f64| {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if a >= 1e9 {
        format!("{}B", trim(n / 1e9))
    } else if a >= 1e6 {
        format!("{}M", trim(n / 1e6))
    } else if a >= 1e4 {
        format!("{}K", trim(n / 1e3))
    } else {
        trim(n)
    }
}

/// A scale from `lo` to `hi` that ends on round numbers: (first, last, step).
#[must_use]
pub fn nice_scale(lo: f64, hi: f64, ticks: usize) -> (f64, f64, f64) {
    let (lo, hi) = if (hi - lo).abs() < f64::EPSILON {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    };
    let raw = (hi - lo) / ticks.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .map(|m| m * magnitude)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * magnitude);
    ((lo / step).floor() * step, (hi / step).ceil() * step, step)
}

fn lighter(color: u32, amount: f64) -> u32 {
    let mix = |shift: u32| {
        let c = f64::from((color >> shift) & 255);
        ((c + (255.0 - c) * amount).round() as u32) & 255
    };
    mix(16) << 16 | mix(8) << 8 | mix(0)
}

fn text(x: f64, y: f64, t: &str, size: f64, anchor: Anchor) -> Shape {
    Shape::Text {
        x,
        y,
        text: t.to_string(),
        size,
        anchor,
        color: INK,
        bold: false,
    }
}

fn clip_label(t: &str, room: f64, size: f64) -> String {
    let fits = (room / (size * 0.55)).floor().max(1.0) as usize;
    if t.chars().count() <= fits {
        t.to_string()
    } else {
        format!(
            "{}\u{2026}",
            t.chars()
                .take(fits.saturating_sub(1).max(1))
                .collect::<String>()
        )
    }
}

/// The shapes of a chart in a box `w` by `h` pixels.
#[must_use]
pub fn layout(chart: &Chart, data: &Dataset, w: f64, h: f64) -> Vec<Shape> {
    let mut out = Vec::new();
    if !chart.title.is_empty() {
        out.push(Shape::Text {
            x: w / 2.0,
            y: 20.0,
            text: chart.title.clone(),
            size: 14.0,
            anchor: Anchor::Middle,
            color: INK,
            bold: true,
        });
    }
    if data.series.is_empty() || data.categories.is_empty() {
        out.push(text(
            w / 2.0,
            h / 2.0,
            "Nothing to show",
            12.0,
            Anchor::Middle,
        ));
        return out;
    }
    let top = if chart.title.is_empty() { 12.0 } else { 34.0 };
    if chart.kind == ChartKind::Pie {
        pie(chart, data, w, h, top, &mut out);
        return out;
    }
    let legend_h = if chart.legend && data.series.len() > 1 {
        22.0
    } else {
        0.0
    };
    let (left, right, bottom) = (52.0, 14.0, 30.0 + legend_h);
    let (plot_w, plot_h) = ((w - left - right).max(10.0), (h - top - bottom).max(10.0));
    let horizontal = chart.kind == ChartKind::Bar;
    let scatter = chart.kind == ChartKind::Scatter;
    let finite = |v: &f64| v.is_finite();
    let all: Vec<f64> = data
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied().filter(finite))
        .collect();
    let (mut lo, mut hi) = (
        all.iter().copied().fold(f64::INFINITY, f64::min),
        all.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    if matches!(
        chart.kind,
        ChartKind::Column | ChartKind::Bar | ChartKind::Area
    ) {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if !lo.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    let (vlo, vhi, step) = nice_scale(lo, hi, 5);
    // Position along the value axis (0 at the bottom or left) and across it.
    let value_at = |v: f64| (v - vlo) / (vhi - vlo);
    let n = data.categories.len();
    let (px, py) = (left, top);
    // Axis lines and gridlines with their labels.
    let mut tick = vlo;
    while tick <= vhi + step * 1e-6 {
        let f = value_at(tick);
        if horizontal {
            let x = px + f * plot_w;
            out.push(Shape::Line {
                points: vec![(x, py), (x, py + plot_h)],
                width: 0.5,
                color: GRID,
            });
            out.push(text(
                x,
                py + plot_h + 14.0,
                &compact(tick),
                10.0,
                Anchor::Middle,
            ));
        } else {
            let y = py + plot_h - f * plot_h;
            out.push(Shape::Line {
                points: vec![(px, y), (px + plot_w, y)],
                width: 0.5,
                color: GRID,
            });
            out.push(text(px - 6.0, y + 3.5, &compact(tick), 10.0, Anchor::End));
        }
        tick += step;
    }
    out.push(Shape::Line {
        points: vec![(px, py), (px, py + plot_h), (px + plot_w, py + plot_h)],
        width: 1.0,
        color: INK,
    });
    // Scatter plots have a number axis across too.
    let (xlo, xhi, xstep) = if scatter {
        let xs: Vec<f64> = data.x.iter().copied().filter(finite).collect();
        nice_scale(
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            5,
        )
    } else {
        (0.0, 1.0, 1.0)
    };
    let band = if horizontal {
        plot_h / n as f64
    } else {
        plot_w / n as f64
    };
    let along = |i: usize| band * (i as f64 + 0.5);
    if scatter {
        let mut t = xlo;
        while t <= xhi + xstep * 1e-6 {
            let x = px + (t - xlo) / (xhi - xlo) * plot_w;
            out.push(text(
                x,
                py + plot_h + 14.0,
                &compact(t),
                10.0,
                Anchor::Middle,
            ));
            t += xstep;
        }
    } else {
        let every = (n as f64 / (plot_w / 48.0).max(1.0)).ceil().max(1.0) as usize;
        for (i, c) in data
            .categories
            .iter()
            .enumerate()
            .filter(|(i, _)| i % every == 0)
        {
            if horizontal {
                out.push(text(
                    px - 6.0,
                    py + along(i) + 3.5,
                    &clip_label(c, left - 8.0, 10.0),
                    10.0,
                    Anchor::End,
                ));
            } else {
                out.push(text(
                    px + along(i),
                    py + plot_h + 14.0,
                    &clip_label(c, band * every as f64, 10.0),
                    10.0,
                    Anchor::Middle,
                ));
            }
        }
    }
    let count = data.series.len();
    for (si, s) in data.series.iter().enumerate() {
        let color = PALETTE[si % PALETTE.len()];
        let zero = value_at(0.0).clamp(0.0, 1.0);
        match chart.kind {
            ChartKind::Column | ChartKind::Bar => {
                let group = band * 0.8;
                let each = group / count as f64;
                for (i, v) in s.values.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                    let offset = band * 0.1 + each * si as f64;
                    let (a, b) = (zero, value_at(*v));
                    let (lo_f, hi_f) = (a.min(b), a.max(b));
                    if horizontal {
                        out.push(Shape::Rect {
                            x: px + lo_f * plot_w,
                            y: py + band * i as f64 + offset,
                            w: (hi_f - lo_f) * plot_w,
                            h: each * 0.92,
                            fill: color,
                        });
                    } else {
                        out.push(Shape::Rect {
                            x: px + band * i as f64 + offset,
                            y: py + plot_h - hi_f * plot_h,
                            w: each * 0.92,
                            h: (hi_f - lo_f) * plot_h,
                            fill: color,
                        });
                    }
                }
            }
            ChartKind::Line | ChartKind::Area => {
                let pts: Vec<(f64, f64)> = s
                    .values
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_finite())
                    .map(|(i, v)| (px + along(i), py + plot_h - value_at(*v) * plot_h))
                    .collect();
                if chart.kind == ChartKind::Area && pts.len() > 1 {
                    let base = py + plot_h - zero * plot_h;
                    let mut poly = vec![(pts[0].0, base)];
                    poly.extend(&pts);
                    poly.push((pts[pts.len() - 1].0, base));
                    out.push(Shape::Polygon {
                        points: poly,
                        fill: lighter(color, 0.45),
                    });
                }
                if pts.len() > 1 {
                    out.push(Shape::Line {
                        points: pts.clone(),
                        width: 2.0,
                        color,
                    });
                }
                for (x, y) in pts {
                    out.push(Shape::Rect {
                        x: x - 2.5,
                        y: y - 2.5,
                        w: 5.0,
                        h: 5.0,
                        fill: color,
                    });
                }
            }
            ChartKind::Scatter => {
                for (i, v) in s.values.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                    let x = data.x.get(i).copied().unwrap_or(f64::NAN);
                    if x.is_finite() {
                        out.push(Shape::Rect {
                            x: px + (x - xlo) / (xhi - xlo) * plot_w - 3.0,
                            y: py + plot_h - value_at(*v) * plot_h - 3.0,
                            w: 6.0,
                            h: 6.0,
                            fill: color,
                        });
                    }
                }
            }
            ChartKind::Pie => {}
        }
    }
    if legend_h > 0.0 {
        let mut x = px;
        let y = h - 12.0;
        for (si, s) in data.series.iter().enumerate() {
            let label = clip_label(&s.name, 120.0, 10.0);
            out.push(Shape::Rect {
                x,
                y: y - 8.0,
                w: 9.0,
                h: 9.0,
                fill: PALETTE[si % PALETTE.len()],
            });
            out.push(text(x + 13.0, y, &label, 10.0, Anchor::Start));
            x += 24.0 + label.chars().count() as f64 * 5.6;
        }
    }
    out
}

fn pie(chart: &Chart, data: &Dataset, w: f64, h: f64, top: f64, out: &mut Vec<Shape>) {
    let values: Vec<f64> = data.series[0]
        .values
        .iter()
        .map(|v| if v.is_finite() && *v > 0.0 { *v } else { 0.0 })
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        out.push(text(
            w / 2.0,
            h / 2.0,
            "Nothing to show",
            12.0,
            Anchor::Middle,
        ));
        return;
    }
    let legend_w = if chart.legend {
        (w * 0.32).min(160.0)
    } else {
        0.0
    };
    let radius = ((w - legend_w) / 2.0 - 16.0)
        .min((h - top) / 2.0 - 8.0)
        .max(10.0);
    let (cx, cy) = ((w - legend_w) / 2.0, top + (h - top) / 2.0);
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, v) in values.iter().enumerate().filter(|(_, v)| **v > 0.0) {
        let sweep = v / total * std::f64::consts::TAU;
        let steps = ((sweep / 0.1).ceil() as usize).max(2);
        let mut points = vec![(cx, cy)];
        for k in 0..=steps {
            let a = angle + sweep * k as f64 / steps as f64;
            points.push((cx + radius * a.cos(), cy + radius * a.sin()));
        }
        out.push(Shape::Polygon {
            points,
            fill: PALETTE[i % PALETTE.len()],
        });
        if v / total >= 0.04 {
            let mid = angle + sweep / 2.0;
            out.push(Shape::Text {
                x: cx + radius * 0.62 * mid.cos(),
                y: cy + radius * 0.62 * mid.sin() + 3.5,
                text: format!("{}%", (v / total * 100.0).round()),
                size: 10.0,
                anchor: Anchor::Middle,
                color: 0x00FF_FFFF,
                bold: true,
            });
        }
        angle += sweep;
    }
    if chart.legend {
        let x = w - legend_w + 8.0;
        for (i, c) in data
            .categories
            .iter()
            .enumerate()
            .take(((h - top) / 16.0) as usize)
        {
            let y = top + 8.0 + i as f64 * 16.0;
            out.push(Shape::Rect {
                x,
                y: y - 8.0,
                w: 9.0,
                h: 9.0,
                fill: PALETTE[i % PALETTE.len()],
            });
            out.push(text(
                x + 13.0,
                y,
                &clip_label(c, legend_w - 24.0, 10.0),
                10.0,
                Anchor::Start,
            ));
        }
    }
}

fn hex(c: u32) -> String {
    format!("#{c:06x}")
}

fn escape(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The shapes as a standalone SVG document `w` by `h` pixels, on a white background.
#[must_use]
pub fn to_svg(shapes: &[Shape], w: f64, h: f64) -> String {
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"Helvetica, Arial, sans-serif\"><rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>"
    );
    let pts = |p: &[(f64, f64)]| {
        p.iter()
            .map(|(x, y)| format!("{x:.1},{y:.1}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    for shape in shapes {
        match shape {
            Shape::Rect { x, y, w, h, fill } => {
                let _ = write!(
                    s,
                    "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>",
                    w.max(0.0),
                    h.max(0.0),
                    hex(*fill)
                );
            }
            Shape::Line {
                points,
                width,
                color,
            } => {
                let _ = write!(
                    s,
                    "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linejoin=\"round\"/>",
                    pts(points),
                    hex(*color)
                );
            }
            Shape::Polygon { points, fill } => {
                let _ = write!(
                    s,
                    "<polygon points=\"{}\" fill=\"{}\" stroke=\"#ffffff\" stroke-width=\"1\"/>",
                    pts(points),
                    hex(*fill)
                );
            }
            Shape::Text {
                x,
                y,
                text,
                size,
                anchor,
                color,
                bold,
            } => {
                let a = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let weight = if *bold { " font-weight=\"bold\"" } else { "" };
                let _ = write!(
                    s,
                    "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"{size}\" text-anchor=\"{a}\" fill=\"{}\"{weight}>{}</text>",
                    hex(*color),
                    escape(text)
                );
            }
        }
    }
    s.push_str("</svg>");
    s
}

/// A sparkline as an SVG document `w` by `h` pixels with a transparent background.
#[must_use]
pub fn sparkline_svg(
    kind: sheets_model::SparkKind,
    values: &[f64],
    w: f64,
    h: f64,
    color: u32,
) -> String {
    use sheets_model::SparkKind;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">"
    );
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() || values.is_empty() {
        s.push_str("</svg>");
        return s;
    }
    let (min, max) = (
        finite.iter().copied().fold(f64::INFINITY, f64::min),
        finite.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (pad, n) = (3.0, values.len() as f64);
    let (pw, ph) = ((w - 2.0 * pad).max(1.0), (h - 2.0 * pad).max(1.0));
    let step = pw / n;
    let hexc = hex(color);
    match kind {
        SparkKind::Line => {
            let (lo, hi) = (
                min,
                if (max - min).abs() < f64::EPSILON {
                    min + 1.0
                } else {
                    max
                },
            );
            let pts: Vec<(f64, f64)> = values
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
                .map(|(i, v)| {
                    (
                        pad + step * (i as f64 + 0.5),
                        pad + ph - (v - lo) / (hi - lo) * ph,
                    )
                })
                .collect();
            if pts.len() > 1 {
                let joined = pts
                    .iter()
                    .map(|(x, y)| format!("{x:.1},{y:.1}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = write!(
                    s,
                    "<polyline points=\"{joined}\" fill=\"none\" stroke=\"{hexc}\" stroke-width=\"1.5\" stroke-linejoin=\"round\"/>"
                );
            }
            if let Some((x, y)) = pts.last() {
                let _ = write!(
                    s,
                    "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"2\" fill=\"{hexc}\"/>"
                );
            }
        }
        SparkKind::Column => {
            let (lo, hi) = (min.min(0.0), max.max(0.0));
            let span = if (hi - lo).abs() < f64::EPSILON {
                1.0
            } else {
                hi - lo
            };
            let zero = pad + ph - (0.0 - lo) / span * ph;
            for (i, v) in values.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                let y = pad + ph - (v - lo) / span * ph;
                let (top, height) = if y < zero {
                    (y, zero - y)
                } else {
                    (zero, y - zero)
                };
                let _ = write!(
                    s,
                    "<rect x=\"{:.1}\" y=\"{top:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{hexc}\"/>",
                    pad + step * i as f64 + step * 0.15,
                    step * 0.7,
                    height.max(1.0)
                );
            }
        }
        SparkKind::WinLoss => {
            let mid = h / 2.0;
            for (i, v) in values
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite() && **v != 0.0)
            {
                let up = *v > 0.0;
                let bar = ph / 2.0 - 1.0;
                let _ = write!(
                    s,
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{bar:.1}\" fill=\"{}\"/>",
                    pad + step * i as f64 + step * 0.15,
                    if up { mid - 1.0 - bar } else { mid + 1.0 },
                    step * 0.7,
                    if up {
                        hexc.clone()
                    } else {
                        "#e15759".to_string()
                    }
                );
            }
        }
    }
    s.push_str("</svg>");
    s
}
