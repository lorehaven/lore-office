//! Sheets on paper: the used part of each sheet is cut into pages, rows first and then columns (like a spreadsheet's
//! default print order), and written as a PDF. Cell text is drawn in Helvetica at its alignment, with fills, text colour and
//! underline, and thin grid lines; a footer names the sheet and the page.
//!
//! Text is clipped to its cell and does not wrap or spill into empty neighbours, and characters the standard fonts lack
//! are set in an embedded Noto Sans (see pdf-write).

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::struct_excessive_bools,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use pdf_write::{A4, Document, Family, Font, LETTER, Page, text_width};
use sheets_calc::Engine;
use sheets_model::{Align, Border, CellRef, Line, LineKind, Value};

/// CSS pixels are 3/4 of a point.
const PT: f64 = 0.75;
const FONT_SIZE: f64 = 10.0;
const FOOTER: f64 = 24.0;
const GRID: u32 = 0x00c8_c8c8;
const INK: u32 = 0x0000_0000;
const DEFAULT_COL_W: f64 = 96.0;
const DEFAULT_ROW_H: f64 = 24.0;

/// Which sheets to print.
#[derive(Clone, Debug)]
pub enum Which {
    /// Every sheet that has anything on it.
    All,
    /// One sheet, by position.
    One(usize),
}

/// A run of columns or rows that fit on a page: which ones, and where each starts, in points, from the page's edge.
struct Band {
    items: Vec<u32>,
    offsets: Vec<f64>,
}

impl Band {
    fn size(&self) -> f64 {
        self.offsets.last().copied().unwrap_or(0.0)
    }

    /// Where item `i` is in the band.
    fn position(&self, i: u32) -> Option<usize> {
        self.items.binary_search(&i).ok()
    }
}

/// Cut `items` (in order) into bands that each fit in `room` points, one item at least.
fn bands(items: &[u32], room: f64, size: impl Fn(u32) -> f64) -> Vec<Band> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < items.len() {
        let mut offsets = vec![0.0];
        let first = at;
        while at < items.len() {
            let s = size(items[at]);
            let used = offsets.last().copied().unwrap_or(0.0);
            // At least one per page, even if it alone is too big.
            if at > first && used + s > room {
                break;
            }
            offsets.push(used + s);
            at += 1;
        }
        out.push(Band {
            items: items[first..at].to_vec(),
            offsets,
        });
    }
    out
}

struct Cell {
    at: CellRef,
    text: String,
    number: bool,
    error: bool,
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<u32>,
    fill: Option<u32>,
    align: Align,
    border: Border,
    /// Points.
    size: f64,
    family: Family,
}

/// The family a font name belongs to, for the standard PDF fonts.
fn family_of(name: Option<&str>) -> Family {
    let n = name.unwrap_or_default().to_lowercase();
    if n.contains("courier") || n.contains("consolas") || n.contains("mono") {
        Family::Mono
    } else if ["times", "georgia", "cambria", "garamond", "serif"]
        .iter()
        .any(|k| n.contains(k))
        && !n.contains("sans")
    {
        Family::Serif
    } else {
        Family::Sans
    }
}

fn cells_of(e: &Engine, sheet: usize) -> Vec<Cell> {
    let wb = e.workbook();
    let Some(s) = wb.sheets.get(sheet) else {
        return Vec::new();
    };
    let mut at: Vec<CellRef> = s.iter().map(|(a, _)| a).collect();
    at.extend(
        s.styled()
            .filter(|(_, id)| {
                wb.styles
                    .get(*id as usize)
                    .is_some_and(|st| st.fill.is_some())
            })
            .map(|(a, _)| a),
    );
    at.sort();
    at.dedup();
    at.into_iter()
        .map(|a| {
            let v = e.value(sheet, a);
            let style = wb
                .styles
                .get(e.style(sheet, a) as usize)
                .cloned()
                .unwrap_or_default();
            let text = match &v {
                Value::Empty => String::new(),
                n @ Value::Number(_) => sheets_format::format_value(&style.number_format, n),
                other => other.to_string(),
            };
            Cell {
                at: a,
                text,
                number: matches!(v, Value::Number(_)),
                error: matches!(v, Value::Error(_)),
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                color: style.color,
                fill: style.fill,
                align: style.align,
                border: style.border,
                size: style.font_size.map_or(FONT_SIZE, |h| f64::from(h) / 2.0),
                family: family_of(style.font_name.as_deref()),
            }
        })
        .collect()
}

/// One page to draw: a sheet and the band of rows and of columns on it.
struct Spec {
    rows: usize,
    cols: usize,
}

struct Laid {
    name: String,
    size: (f64, f64),
    margin: f64,
    scale: f64,
    gridlines: bool,
    cells: Vec<Cell>,
    /// Charts: where each starts, its size in pixels, and what it draws.
    charts: Vec<(sheets_model::CellRef, f64, f64, Vec<sheets_chart::Shape>)>,
    /// The rows repeated at the top of every page.
    header: Option<Band>,
    rows: Vec<Band>,
    cols: Vec<Band>,
}

fn lay_out(e: &Engine, sheet: usize) -> Option<Laid> {
    let wb = e.workbook();
    let s = wb.sheets.get(sheet)?;
    let page = s.page();
    let mut cells = cells_of(e, sheet);
    // Only the print area, when there is one.
    if let Some(area) = page.print_area {
        let repeated = |r: u32| page.repeat_rows.is_some_and(|(a, b)| (a..=b).contains(&r));
        cells.retain(|c| {
            area.contains(c.at)
                || (repeated(c.at.row) && c.at.col >= area.start.col && c.at.col <= area.end.col)
        });
    }
    if cells.is_empty() && s.charts().is_empty() {
        return None;
    }
    let (max_row, max_col) = cells.iter().fold((0, 0), |(r, c), cell| {
        (r.max(cell.at.row), c.max(cell.at.col))
    });
    let (first_row, first_col) = page
        .print_area
        .map_or((0, 0), |a| (a.start.row, a.start.col));
    // Charts extend the sheet to where they end.
    let (chart_rows, chart_cols) = s.charts().iter().fold((0, 0), |(r, c), ch| {
        (
            r.max(ch.at.row + ch.height.div_ceil(24)),
            c.max(ch.at.col + ch.width.div_ceil(96)),
        )
    });
    let (last_row, last_col) = page.print_area.map_or_else(
        || (max_row.max(chart_rows), max_col.max(chart_cols)),
        |a| (a.end.row, a.end.col),
    );
    let col_list: Vec<u32> = (first_col..=last_col)
        .filter(|c| !s.col_hidden(*c))
        .collect();
    let all_rows: Vec<u32> = (first_row..=last_row)
        .filter(|r| !s.row_hidden(*r))
        .collect();
    // The repeated rows are drawn at the top of every page, even when they are above the print area.
    let (header_rows, body_rows): (Vec<u32>, Vec<u32>) = match page.repeat_rows {
        Some((a, b)) => (
            (a..=b).filter(|r| !s.row_hidden(*r)).collect(),
            all_rows
                .into_iter()
                .filter(|r| !(a..=b).contains(r))
                .collect(),
        ),
        None => (Vec::new(), all_rows),
    };
    let paper = match page.paper {
        sheets_model::Paper::A4 => A4,
        sheets_model::Paper::Letter => LETTER,
    };
    let margin = f64::from(page.margin_mm) / 25.4 * 72.0;
    let col_w = |c: u32| s.col_width(c).unwrap_or(DEFAULT_COL_W) * PT;
    let row_h = |r: u32| s.row_height(r).unwrap_or(DEFAULT_ROW_H) * PT;
    let total_w: f64 = col_list.iter().map(|c| col_w(*c)).sum();
    let size = match page.orientation {
        sheets_model::Orientation::Landscape => (paper.1, paper.0),
        // Landscape when the sheet is wider than a portrait page.
        sheets_model::Orientation::Auto if total_w > paper.0 - 2.0 * margin && !page.fit_width => {
            (paper.1, paper.0)
        }
        sheets_model::Orientation::Auto | sheets_model::Orientation::Portrait => paper,
    };
    let (room_w, room_h) = (size.0 - 2.0 * margin, size.1 - 2.0 * margin - FOOTER);
    // Fitting the width shrinks everything (never enlarges it).
    let scale = if page.fit_width && total_w > room_w {
        room_w / total_w
    } else {
        1.0
    };
    let (cw, rh) = (|c: u32| col_w(c) * scale, |r: u32| row_h(r) * scale);
    let header = (!header_rows.is_empty()).then(|| bands(&header_rows, f64::MAX, rh).remove(0));
    let header_h = header.as_ref().map_or(0.0, Band::size);
    Some(Laid {
        name: s.name.clone(),
        size,
        margin,
        scale,
        gridlines: page.gridlines,
        rows: bands(
            &body_rows,
            (room_h - header_h).max(rh(body_rows.first().copied().unwrap_or(0))),
            rh,
        ),
        cols: bands(&col_list, room_w + 0.5, cw),
        header,
        charts: s
            .charts()
            .iter()
            .map(|c| {
                let cells: Vec<Vec<Value>> = (c.data.start.row
                    ..=c.data.end.row.min(c.data.start.row + 5000))
                    .map(|r| {
                        (c.data.start.col..=c.data.end.col)
                            .map(|col| e.value(sheet, CellRef::new(r, col)))
                            .collect()
                    })
                    .collect();
                let data = sheets_chart::dataset(c, &cells);
                let (w, h) = (f64::from(c.width), f64::from(c.height));
                (c.at, w, h, sheets_chart::layout(c, &data, w, h))
            })
            .collect(),
        cells,
    })
}

/// Draw the cells of a block of rows and columns with its top left corner at (`x0`, `y0`).
fn draw_block(page: &mut Page, laid: &Laid, rows: &Band, cols: &Band, x0: f64, y0: f64) {
    let in_page: Vec<(&Cell, usize, usize)> = laid
        .cells
        .iter()
        .filter_map(|c| Some((c, cols.position(c.at.col)?, rows.position(c.at.row)?)))
        .collect();
    for (c, i, j) in &in_page {
        let (x, y) = (x0 + cols.offsets[*i], y0 + rows.offsets[*j]);
        let (w, h) = (
            cols.offsets[*i + 1] - cols.offsets[*i],
            rows.offsets[*j + 1] - rows.offsets[*j],
        );
        if let Some(fill) = c.fill {
            page.fill_rect(x, y, w, h, fill);
        }
        if c.text.is_empty() {
            continue;
        }
        let size = c.size * laid.scale;
        let font = Font::in_family(c.family, c.bold, c.italic);
        let tw = text_width(&c.text, font, size);
        let pad = 3.0 * laid.scale;
        let right = match c.align {
            Align::Right => true,
            Align::General => c.number,
            Align::Left | Align::Center => false,
        };
        let tx = match c.align {
            Align::Center => x + (w - tw) / 2.0,
            _ if right => x + w - pad - tw,
            _ => x + pad,
        };
        let color = if c.error {
            0x00c5_221f
        } else {
            c.color.unwrap_or(INK)
        };
        let baseline = y + h / 2.0 + size * 0.35;
        page.clipped(x, y, w, h, |p| {
            p.text(tx.max(x + 1.0), baseline, &c.text, font, size, color);
            if c.underline {
                p.line(
                    tx.max(x + 1.0),
                    baseline + 1.5,
                    (tx + tw).min(x + w),
                    baseline + 1.5,
                    0.5,
                    color,
                );
            }
        });
    }
    // Grid lines around every cell of the block.
    if laid.gridlines {
        let (width, height) = (cols.size(), rows.size());
        for off in &rows.offsets {
            page.line(x0, y0 + off, x0 + width, y0 + off, 0.25, GRID);
        }
        for off in &cols.offsets {
            page.line(x0 + off, y0, x0 + off, y0 + height, 0.25, GRID);
        }
    }
    // Borders over the grid lines.
    for (c, i, j) in &in_page {
        if c.border.is_none() {
            continue;
        }
        let (x1, x2) = (x0 + cols.offsets[*i], x0 + cols.offsets[*i + 1]);
        let (y1, y2) = (y0 + rows.offsets[*j], y0 + rows.offsets[*j + 1]);
        let mut side = |line: Option<Line>, a: (f64, f64), b: (f64, f64)| {
            let Some(l) = line else { return };
            let color = l.color.unwrap_or(INK);
            let (w, count) = match l.kind {
                LineKind::Thin | LineKind::Dashed | LineKind::Dotted => (0.5, 1),
                LineKind::Medium => (1.0, 1),
                LineKind::Thick => (2.0, 1),
                LineKind::Double => (0.5, 2),
            };
            for n in 0..count {
                // A double line is two thin ones a point apart, either side of the edge.
                let d = if count == 2 {
                    if n == 0 { -0.8 } else { 0.8 }
                } else {
                    0.0
                };
                let (dx, dy) = if a.0 == b.0 { (d, 0.0) } else { (0.0, d) };
                page.line(a.0 + dx, a.1 + dy, b.0 + dx, b.1 + dy, w, color);
            }
        };
        side(c.border.top, (x1, y1), (x2, y1));
        side(c.border.bottom, (x1, y2), (x2, y2));
        side(c.border.left, (x1, y1), (x1, y2));
        side(c.border.right, (x2, y1), (x2, y2));
    }
}

/// Draw a chart's shapes with the box's top left corner at (`x`, `y`) points.
fn draw_chart(
    page: &mut Page,
    shapes: &[sheets_chart::Shape],
    size: (f64, f64),
    x: f64,
    y: f64,
    k: f64,
) {
    use sheets_chart::{Anchor, Shape};
    let (w, h) = (size.0 * k, size.1 * k);
    page.fill_rect(x, y, w, h, 0x00ff_ffff);
    page.line(x, y, x + w, y, 0.5, 0x00cc_cccc);
    page.line(x, y + h, x + w, y + h, 0.5, 0x00cc_cccc);
    page.line(x, y, x, y + h, 0.5, 0x00cc_cccc);
    page.line(x + w, y, x + w, y + h, 0.5, 0x00cc_cccc);
    let at = |(px, py): (f64, f64)| (x + px * k, y + py * k);
    page.clipped(x, y, w, h, |p| {
        for shape in shapes {
            match shape {
                Shape::Rect {
                    x: rx,
                    y: ry,
                    w: rw,
                    h: rh,
                    fill,
                } => {
                    let (a, b) = at((*rx, *ry));
                    p.fill_rect(a, b, rw * k, rh * k, *fill);
                }
                Shape::Line {
                    points,
                    width,
                    color,
                } => {
                    let pts: Vec<(f64, f64)> = points.iter().map(|q| at(*q)).collect();
                    p.polyline(&pts, width * k, *color);
                }
                Shape::Polygon { points, fill } => {
                    let pts: Vec<(f64, f64)> = points.iter().map(|q| at(*q)).collect();
                    p.fill_polygon(&pts, *fill);
                }
                Shape::Text {
                    x: tx,
                    y: ty,
                    text,
                    size,
                    anchor,
                    color,
                    bold,
                } => {
                    let font = if *bold {
                        Font::HelveticaBold
                    } else {
                        Font::Helvetica
                    };
                    let width = text_width(text, font, size * k);
                    let (px, py) = at((*tx, *ty));
                    let left = match anchor {
                        Anchor::Start => px,
                        Anchor::Middle => px - width / 2.0,
                        Anchor::End => px - width,
                    };
                    p.text(left, py, text, font, size * k, *color);
                }
            }
        }
    });
}

fn draw(page: &mut Page, laid: &Laid, spec: &Spec, number: usize, total: usize) {
    let (rows, cols) = (&laid.rows[spec.rows], &laid.cols[spec.cols]);
    let mut y = laid.margin;
    if let Some(header) = &laid.header {
        draw_block(page, laid, header, cols, laid.margin, y);
        y += header.size();
    }
    draw_block(page, laid, rows, cols, laid.margin, y);
    // A chart is drawn on the page that holds its top left cell.
    for (at, w, h, shapes) in &laid.charts {
        if let (Some(i), Some(j)) = (cols.position(at.col), rows.position(at.row)) {
            let k = PT * laid.scale;
            draw_chart(
                page,
                shapes,
                (*w, *h),
                laid.margin + cols.offsets[i],
                y + rows.offsets[j],
                k,
            );
        }
    }
    let footer = format!("{}  -  page {number} of {total}", laid.name);
    let (_, ph) = page.size();
    page.text(
        laid.margin,
        ph - laid.margin + 6.0,
        &footer,
        Font::Helvetica,
        8.0,
        0x0080_8080,
    );
}

/// The sheets as a PDF file. Nothing printable (every chosen sheet empty) gives a one-page document saying so.
#[must_use]
pub fn to_pdf(e: &Engine, which: &Which, title: &str) -> Vec<u8> {
    let wanted: Vec<usize> = match which {
        Which::All => (0..e.workbook().sheets.len()).collect(),
        Which::One(i) => vec![*i],
    };
    let laid: Vec<(usize, Laid)> = wanted
        .into_iter()
        .filter_map(|s| lay_out(e, s).map(|l| (s, l)))
        .collect();
    let mut specs: Vec<(usize, Spec)> = Vec::new();
    for (index, (_, l)) in laid.iter().enumerate() {
        for rows in 0..l.rows.len() {
            for cols in 0..l.cols.len() {
                specs.push((index, Spec { rows, cols }));
            }
        }
    }
    let mut doc = Document::new();
    doc.title(title);
    let total = specs.len();
    for (n, (index, spec)) in specs.iter().enumerate() {
        let l = &laid[*index].1;
        let mut page = Page::new(l.size.0, l.size.1);
        draw(&mut page, l, spec, n + 1, total);
        doc.add(page);
    }
    if total == 0 {
        let mut page = Page::new(A4.0, A4.1);
        page.text(
            36.0,
            48.0,
            "Nothing to print: the sheet is empty.",
            Font::Helvetica,
            11.0,
            0x0057_606a,
        );
        doc.add(page);
    }
    doc.finish()
}
