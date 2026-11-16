//! Charts on a sheet: its drawing part (where each chart sits) and one chart part per chart (what it shows), reached from
//! the sheet's relationships. Only the usual kinds and the cell references of their series are understood; a sheet whose
//! drawing holds anything else (pictures, shapes) keeps its drawing exactly as it was.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines
)]

use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use office_fmt::{ContentTypes, Package};
use sheets_calc::Engine;
use sheets_model::{CellRef, Chart, ChartKind, Range, Sheet};

const KIND_DRAWING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const KIND_CHART: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
/// English Metric Units per pixel.
const EMU: f64 = 9525.0;

/// The name of an element without its prefix (`c:plotArea` is `plotArea`).
fn local(e: &Value) -> &str {
    xml::tag(e).map_or("", |t| t.rsplit(':').next().unwrap_or(t))
}

fn texts(e: &Value, out: &mut String) {
    if local(e) == "t" {
        out.push_str(&xml::text(e));
        return;
    }
    for k in xml::children(e) {
        if k.is_object() {
            texts(k, out);
        }
    }
}

/// The first element called `tag` anywhere under `e`.
fn find<'a>(e: &'a Value, tag: &str) -> Option<&'a Value> {
    if local(e) == tag {
        return Some(e);
    }
    xml::children(e)
        .iter()
        .filter(|k| k.is_object())
        .find_map(|k| find(k, tag))
}

fn find_all<'a>(e: &'a Value, tag: &str, out: &mut Vec<&'a Value>) {
    if local(e) == tag {
        out.push(e);
        return;
    }
    for k in xml::children(e).iter().filter(|k| k.is_object()) {
        find_all(k, tag, out);
    }
}

/// `Sheet1!$B$2:$B$9` as a range (the sheet name is not checked).
fn range_of(reference: &str) -> Option<Range> {
    let cells = reference
        .rsplit_once('!')
        .map_or(reference, |(_, c)| c)
        .replace('$', "");
    Range::parse(&cells).or_else(|| CellRef::parse(&cells).map(|c| Range::new(c, c)))
}

fn read_chart(part: &Value) -> Option<(Chart, bool)> {
    let plot = find(part, "plotArea")?;
    let (kind, group) = [
        ("barChart", ChartKind::Column),
        ("lineChart", ChartKind::Line),
        ("areaChart", ChartKind::Area),
        ("pieChart", ChartKind::Pie),
        ("scatterChart", ChartKind::Scatter),
    ]
    .into_iter()
    .find_map(|(tag, k)| find(plot, tag).map(|e| (k, e)))?;
    let kind = if kind == ChartKind::Column
        && find(group, "barDir").and_then(|d| xml::attr(d, "val")) == Some("bar")
    {
        ChartKind::Bar
    } else {
        kind
    };
    let mut series = Vec::new();
    find_all(group, "ser", &mut series);
    let mut ranges: Vec<Range> = Vec::new();
    let mut horizontal = false;
    for s in &series {
        for tag in ["tx", "cat", "val", "xVal", "yVal"] {
            if let Some(e) = find(s, tag)
                && let Some(r) = find(e, "f").and_then(|f| range_of(&xml::text(f)))
            {
                if matches!(tag, "val" | "yVal") && r.rows() == 1 && r.cols() > 1 {
                    horizontal = true;
                }
                ranges.push(r);
            }
        }
    }
    let first = *ranges.first()?;
    let data = ranges.iter().fold(first, |a, r| {
        Range::new(
            CellRef::new(a.start.row.min(r.start.row), a.start.col.min(r.start.col)),
            CellRef::new(a.end.row.max(r.end.row), a.end.col.max(r.end.col)),
        )
    });
    let mut title = String::new();
    if let Some(t) = find(part, "title") {
        texts(t, &mut title);
    }
    let mut chart = Chart::new(kind, data);
    chart.title = title;
    chart.rows_as_series = horizontal;
    chart.legend = find(part, "legend").is_some();
    Some((chart, true))
}

/// Read the sheet's charts into the model.
pub fn read(package: &Package, sheet_part: &str, sheet: &mut Sheet) {
    let Some(list) = package
        .get(&rels::rels_part(sheet_part))
        .and_then(|b| rels::parse(b).ok())
    else {
        return;
    };
    let mut charts = Vec::new();
    for rel in list
        .iter()
        .filter(|r| r.kind == KIND_DRAWING && !r.external)
    {
        let part = rels::resolve(sheet_part, &rel.target);
        let Some(drawing) = package.get(&part).and_then(|b| xml::parse(b, false).ok()) else {
            continue;
        };
        let drawing_rels = package
            .get(&rels::rels_part(&part))
            .and_then(|b| rels::parse(b).ok())
            .unwrap_or_default();
        for anchor in xml::children(&drawing).iter().filter(|k| k.is_object()) {
            let Some(frame) = find(anchor, "graphicFrame") else {
                continue;
            };
            let Some(id) = find(frame, "chart").and_then(|c| xml::attr_local(c, "id")) else {
                continue;
            };
            let Some(target) = drawing_rels.iter().find(|r| r.id == id) else {
                continue;
            };
            let chart_part = rels::resolve(&part, &target.target);
            let Some((mut chart, _)) = package
                .get(&chart_part)
                .and_then(|b| xml::parse(b, false).ok())
                .as_ref()
                .and_then(read_chart)
            else {
                continue;
            };
            // Where it sits and how big it is.
            let num = |e: &Value, tag: &str| {
                find(e, tag)
                    .and_then(|c| xml::text(c).trim().parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            if let Some(from) = find(anchor, "from") {
                chart.at = CellRef::new(num(from, "row") as u32, num(from, "col") as u32);
            }
            if let Some(ext) = find(anchor, "ext") {
                let size = |name: &str| {
                    xml::attr(ext, name)
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0)
                        / EMU
                };
                (chart.width, chart.height) =
                    (size("cx").max(60.0) as u32, size("cy").max(40.0) as u32);
            } else if let Some(to) = find(anchor, "to") {
                let cells =
                    |from: u32, to: u32, each: f64| f64::from(to.saturating_sub(from)) * each;
                chart.width = cells(chart.at.col, num(to, "col") as u32, 64.0).max(120.0) as u32;
                chart.height = cells(chart.at.row, num(to, "row") as u32, 20.0).max(80.0) as u32;
            }
            charts.push(chart);
        }
    }
    sheet.set_charts(charts);
}

fn quote(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

fn cell_ref(sheet: &str, at: CellRef) -> String {
    format!(
        "{}!${}${}",
        quote(sheet),
        sheets_model::col_name(at.col),
        at.row + 1
    )
}

fn range_ref(sheet: &str, a: CellRef, b: CellRef) -> String {
    format!(
        "{}!${}${}:${}${}",
        quote(sheet),
        sheets_model::col_name(a.col),
        a.row + 1,
        sheets_model::col_name(b.col),
        b.row + 1
    )
}

fn el(name: &str, attrs: &[(&str, &str)], kids: Vec<Value>) -> Value {
    xml::el(name, attrs, kids)
}

fn val(tag: &str, v: &str) -> Value {
    el(tag, &[("val", v)], vec![])
}

fn reference(kind: &str, formula: &str) -> Value {
    el(kind, &[], vec![el("f", &[], vec![xml::txt(formula)])])
}

/// The XML of one chart part.
fn chart_xml(chart: &Chart, sheet_name: &str, engine: &Engine, sheet: usize) -> Value {
    let (first, last) = (chart.data.start, chart.data.end);
    let cells: Vec<Vec<sheets_model::Value>> = (first.row..=last.row.min(first.row + 5000))
        .map(|r| {
            (first.col..=last.col)
                .map(|c| engine.value(sheet, CellRef::new(r, c)))
                .collect()
        })
        .collect();
    let data = sheets_chart::dataset(chart, &cells);
    let by_row = chart.rows_as_series;
    // Where the pieces of series `i` are.
    let (header, labels) = (u32::from(data.header), u32::from(data.labels));
    let series: Vec<Value> = (0..data.series.len())
        .map(|i| {
            let i = u32::try_from(i).unwrap_or(0);
            let (name_at, values, cats) = if by_row {
                let row = first.row + labels + i;
                (
                    CellRef::new(row, first.col),
                    (
                        CellRef::new(row, first.col + header),
                        CellRef::new(row, last.col),
                    ),
                    (
                        CellRef::new(first.row, first.col + header),
                        CellRef::new(first.row, last.col),
                    ),
                )
            } else {
                let col = first.col + labels + i;
                (
                    CellRef::new(first.row, col),
                    (
                        CellRef::new(first.row + header, col),
                        CellRef::new(last.row, col),
                    ),
                    (
                        CellRef::new(first.row + header, first.col),
                        CellRef::new(last.row, first.col),
                    ),
                )
            };
            let mut kids = vec![val("idx", &i.to_string()), val("order", &i.to_string())];
            if data.header {
                kids.push(el(
                    "tx",
                    &[],
                    vec![reference("strRef", &cell_ref(sheet_name, name_at))],
                ));
            }
            let scatter = chart.kind == ChartKind::Scatter;
            let color = sheets_chart::PALETTE
                [usize::try_from(i).unwrap_or(0) % sheets_chart::PALETTE.len()];
            let rgb = format!("{color:06X}");
            let fill = || {
                el(
                    "a:solidFill",
                    &[],
                    vec![el("a:srgbClr", &[("val", &rgb)], vec![])],
                )
            };
            match chart.kind {
                ChartKind::Column | ChartKind::Bar | ChartKind::Area => {
                    kids.push(el("spPr", &[], vec![fill()]));
                }
                ChartKind::Line => {
                    kids.push(el(
                        "spPr",
                        &[],
                        vec![el("a:ln", &[("w", "28575")], vec![fill()])],
                    ));
                    kids.push(el(
                        "marker",
                        &[],
                        vec![
                            val("symbol", "circle"),
                            val("size", "5"),
                            el("spPr", &[], vec![fill()]),
                        ],
                    ));
                }
                ChartKind::Scatter => {
                    kids.push(el(
                        "spPr",
                        &[],
                        vec![el(
                            "a:ln",
                            &[("w", "19050")],
                            vec![el("a:noFill", &[], vec![])],
                        )],
                    ));
                    kids.push(el(
                        "marker",
                        &[],
                        vec![
                            val("symbol", "circle"),
                            val("size", "6"),
                            el("spPr", &[], vec![fill()]),
                        ],
                    ));
                }
                ChartKind::Pie => {
                    for (k, _) in data.categories.iter().enumerate() {
                        let c = format!(
                            "{:06X}",
                            sheets_chart::PALETTE[k % sheets_chart::PALETTE.len()]
                        );
                        kids.push(el(
                            "dPt",
                            &[],
                            vec![
                                val("idx", &k.to_string()),
                                val("bubble3D", "0"),
                                el(
                                    "spPr",
                                    &[],
                                    vec![el(
                                        "a:solidFill",
                                        &[],
                                        vec![el("a:srgbClr", &[("val", &c)], vec![])],
                                    )],
                                ),
                            ],
                        ));
                    }
                }
            }
            if data.labels {
                let tag = if scatter { "xVal" } else { "cat" };
                let kind = if scatter { "numRef" } else { "strRef" };
                kids.push(el(
                    tag,
                    &[],
                    vec![reference(kind, &range_ref(sheet_name, cats.0, cats.1))],
                ));
            }
            kids.push(el(
                if scatter { "yVal" } else { "val" },
                &[],
                vec![reference(
                    "numRef",
                    &range_ref(sheet_name, values.0, values.1),
                )],
            ));
            if chart.kind == ChartKind::Line {
                kids.push(val("smooth", "0"));
            }
            el("ser", &[], kids)
        })
        .collect();
    let axes = || (val("axId", "111111111"), val("axId", "222222222"));
    let (a1, a2) = axes();
    let mut group_kids = Vec::new();
    let group = match chart.kind {
        ChartKind::Column | ChartKind::Bar => {
            group_kids.extend([
                val(
                    "barDir",
                    if chart.kind == ChartKind::Bar {
                        "bar"
                    } else {
                        "col"
                    },
                ),
                val("grouping", "clustered"),
                val("varyColors", "0"),
            ]);
            "barChart"
        }
        ChartKind::Line => {
            group_kids.extend([val("grouping", "standard"), val("varyColors", "0")]);
            "lineChart"
        }
        ChartKind::Area => {
            group_kids.extend([val("grouping", "standard"), val("varyColors", "0")]);
            "areaChart"
        }
        ChartKind::Pie => {
            group_kids.push(val("varyColors", "1"));
            "pieChart"
        }
        ChartKind::Scatter => {
            group_kids.extend([val("scatterStyle", "lineMarker"), val("varyColors", "0")]);
            "scatterChart"
        }
    };
    group_kids.extend(series);
    if chart.kind == ChartKind::Pie {
        group_kids.push(val("firstSliceAng", "0"));
    } else {
        if matches!(chart.kind, ChartKind::Column | ChartKind::Bar) {
            group_kids.push(val("gapWidth", "100"));
        }
        group_kids.push(a1);
        group_kids.push(a2);
    }
    let mut plot = vec![el("layout", &[], vec![]), el(group, &[], group_kids)];
    if chart.kind != ChartKind::Pie {
        let scatter = chart.kind == ChartKind::Scatter;
        let (pos_x, pos_y) = if chart.kind == ChartKind::Bar {
            ("l", "b")
        } else {
            ("b", "l")
        };
        let axis = |tag: &str, id: &str, cross: &str, pos: &str| {
            el(
                tag,
                &[],
                vec![
                    val("axId", id),
                    el("scaling", &[], vec![val("orientation", "minMax")]),
                    val("delete", "0"),
                    val("axPos", pos),
                    el(
                        "numFmt",
                        &[("formatCode", "General"), ("sourceLinked", "0")],
                        vec![],
                    ),
                    val("majorTickMark", "out"),
                    val("minorTickMark", "none"),
                    val("tickLblPos", "nextTo"),
                    val("crossAx", cross),
                    val("crosses", "autoZero"),
                ],
            )
        };
        let mut first_axis = axis(
            if scatter { "valAx" } else { "catAx" },
            "111111111",
            "222222222",
            pos_x,
        );
        let mut second_axis = axis("valAx", "222222222", "111111111", pos_y);
        if let Some(k) = xml::children_mut(&mut first_axis) {
            k.retain(|e| xml::tag(e) != Some("numFmt"));
        }
        if let Some(k) = xml::children_mut(&mut second_axis) {
            // Gridlines come right after the axis position; the value axis also says how it meets the categories.
            if let Some(at) = k.iter().position(|e| xml::tag(e) == Some("axPos")) {
                k.insert(at + 1, el("majorGridlines", &[], vec![]));
            }
            k.push(val(
                "crossBetween",
                if chart.kind == ChartKind::Scatter {
                    "midCat"
                } else {
                    "between"
                },
            ));
        }
        plot.extend([first_axis, second_axis]);
    }
    let mut chart_kids = Vec::new();
    if chart.title.is_empty() {
        chart_kids.push(val("autoTitleDeleted", "1"));
    } else {
        let run = el(
            "a:r",
            &[],
            vec![el("a:t", &[], vec![xml::txt(&chart.title)])],
        );
        chart_kids.push(el(
            "title",
            &[],
            vec![
                el(
                    "tx",
                    &[],
                    vec![el(
                        "rich",
                        &[],
                        vec![el("a:bodyPr", &[], vec![]), el("a:p", &[], vec![run])],
                    )],
                ),
                val("overlay", "0"),
            ],
        ));
        chart_kids.push(val("autoTitleDeleted", "0"));
    }
    chart_kids.push(el("plotArea", &[], plot));
    if chart.legend {
        chart_kids.push(el(
            "legend",
            &[],
            vec![
                val(
                    "legendPos",
                    if chart.kind == ChartKind::Pie {
                        "r"
                    } else {
                        "b"
                    },
                ),
                val("overlay", "0"),
            ],
        ));
    }
    chart_kids.push(val("plotVisOnly", "1"));
    el(
        "chartSpace",
        &[("xmlns:c", NS_C), ("xmlns:a", NS_A), ("xmlns:r", NS_R)],
        vec![el("chart", &[], chart_kids)],
    )
}

/// The element and all under it with `c:` in front of the names that have no prefix.
fn prefixed(e: &Value) -> Value {
    let Some(name) = xml::tag(e) else {
        return e.clone();
    };
    let attrs = match e.get("attrs") {
        Some(Value::Object(m)) => m.clone(),
        _ => xml::Map::new(),
    };
    let kids = xml::children(e)
        .iter()
        .map(|k| {
            if k.is_object() {
                prefixed(k)
            } else {
                k.clone()
            }
        })
        .collect();
    let full = if name.contains(':') {
        name.to_string()
    } else {
        format!("c:{name}")
    };
    xml::element(&full, attrs, kids)
}

fn drawing_xml(charts: &[Chart], ids: &[String]) -> Value {
    let anchors: Vec<Value> = charts
        .iter()
        .zip(ids)
        .enumerate()
        .map(|(i, (c, id))| {
            let n = (i + 2).to_string();
            let frame = el(
                "xdr:graphicFrame",
                &[("macro", "")],
                vec![
                    el(
                        "xdr:nvGraphicFramePr",
                        &[],
                        vec![
                            el(
                                "xdr:cNvPr",
                                &[("id", &n), ("name", &format!("Chart {}", i + 1))],
                                vec![],
                            ),
                            el("xdr:cNvGraphicFramePr", &[], vec![]),
                        ],
                    ),
                    el(
                        "xdr:xfrm",
                        &[],
                        vec![
                            el("a:off", &[("x", "0"), ("y", "0")], vec![]),
                            el("a:ext", &[("cx", "0"), ("cy", "0")], vec![]),
                        ],
                    ),
                    el(
                        "a:graphic",
                        &[],
                        vec![el(
                            "a:graphicData",
                            &[("uri", NS_C)],
                            vec![el(
                                "c:chart",
                                &[("xmlns:c", NS_C), ("xmlns:r", NS_R), ("r:id", id)],
                                vec![],
                            )],
                        )],
                    ),
                ],
            );
            let from = el(
                "xdr:from",
                &[],
                vec![
                    el("xdr:col", &[], vec![xml::txt(&c.at.col.to_string())]),
                    el("xdr:colOff", &[], vec![xml::txt("0")]),
                    el("xdr:row", &[], vec![xml::txt(&c.at.row.to_string())]),
                    el("xdr:rowOff", &[], vec![xml::txt("0")]),
                ],
            );
            let ext = el(
                "xdr:ext",
                &[
                    ("cx", &(f64::from(c.width) * EMU).round().to_string()),
                    ("cy", &(f64::from(c.height) * EMU).round().to_string()),
                ],
                vec![],
            );
            el(
                "xdr:oneCellAnchor",
                &[],
                vec![from, ext, frame, el("xdr:clientData", &[], vec![])],
            )
        })
        .collect();
    el(
        "xdr:wsDr",
        &[("xmlns:xdr", NS_XDR), ("xmlns:a", NS_A)],
        anchors,
    )
}

/// Write the sheet's charts, replacing the drawing it had; leave a drawing that holds more than charts alone.
pub fn write(
    pkg: &mut Package,
    ct: &mut ContentTypes,
    sheet_part: &str,
    tree: &mut Value,
    engine: &Engine,
    index: usize,
) {
    let sheet = &engine.workbook().sheets[index];
    let rels_name = rels::rels_part(sheet_part);
    let mut list = pkg
        .get(&rels_name)
        .and_then(|b| rels::parse(b).ok())
        .unwrap_or_default();
    let old: Vec<Rel> = list
        .iter()
        .filter(|r| r.kind == KIND_DRAWING && !r.external)
        .cloned()
        .collect();
    // A drawing with pictures or shapes in it stays as it is (and its charts with it).
    for r in &old {
        let part = rels::resolve(sheet_part, &r.target);
        if let Some(d) = pkg.get(&part).and_then(|b| xml::parse(b, false).ok())
            && xml::children(&d)
                .iter()
                .filter(|k| k.is_object())
                .any(|a| find(a, "graphicFrame").is_none())
        {
            return;
        }
    }
    if old.is_empty() && sheet.charts().is_empty() {
        return;
    }
    for r in &old {
        let part = rels::resolve(sheet_part, &r.target);
        for c in pkg
            .get(&rels::rels_part(&part))
            .and_then(|b| rels::parse(b).ok())
            .unwrap_or_default()
            .iter()
            .filter(|c| c.kind == KIND_CHART)
        {
            let chart_part = rels::resolve(&part, &c.target);
            pkg.remove(&chart_part);
            ct.remove_override(&chart_part);
        }
        pkg.remove(&rels::rels_part(&part));
        pkg.remove(&part);
        ct.remove_override(&part);
    }
    list.retain(|r| r.kind != KIND_DRAWING);
    if sheet.charts().is_empty() {
        crate::extras::put(tree, "drawing", Vec::new());
    } else {
        let drawing_part = (1..=u32::MAX)
            .map(|n| format!("xl/drawings/drawing{n}.xml"))
            .find(|p| pkg.get(p).is_none())
            .unwrap_or_default();
        let mut chart_rels = Vec::new();
        let mut ids = Vec::new();
        for (i, chart) in sheet.charts().iter().enumerate() {
            let part = (1..=u32::MAX)
                .map(|n| format!("xl/charts/chart{n}.xml"))
                .find(|p| pkg.get(p).is_none())
                .unwrap_or_default();
            let x = prefixed(&chart_xml(chart, &sheet.name, engine, index));
            pkg.set(&part, xml::write(&x));
            ct.set_override(&part, CT_CHART);
            let id = format!("rId{}", i + 1);
            chart_rels.push(Rel {
                id: id.clone(),
                kind: KIND_CHART.into(),
                target: format!("../charts/{}", part.rsplit('/').next().unwrap_or_default()),
                external: false,
            });
            ids.push(id);
        }
        pkg.set(
            &drawing_part,
            xml::write(&drawing_xml(sheet.charts(), &ids)),
        );
        ct.set_override(&drawing_part, CT_DRAWING);
        pkg.set(&rels::rels_part(&drawing_part), rels::to_bytes(&chart_rels));
        let id = (1..=u32::MAX)
            .map(|n| format!("rId{n}"))
            .find(|id| list.iter().all(|r| r.id != *id))
            .unwrap_or_default();
        let ups = sheet_part
            .rsplit_once('/')
            .map_or(0, |(d, _)| d.split('/').filter(|s| !s.is_empty()).count());
        list.push(Rel {
            id: id.clone(),
            kind: KIND_DRAWING.into(),
            target: format!("{}{}", "../".repeat(ups), drawing_part),
            external: false,
        });
        if xml::attr(tree, "xmlns:r").is_none() {
            xml::set_attr(tree, "xmlns:r", NS_R);
        }
        crate::extras::put(
            tree,
            "drawing",
            vec![el("drawing", &[("r:id", &id)], vec![])],
        );
    }
    if list.is_empty() {
        pkg.remove(&rels_name);
    } else {
        pkg.set(&rels_name, rels::to_bytes(&list));
    }
}
