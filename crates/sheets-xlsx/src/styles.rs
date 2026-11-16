//! `styles.xml`: the formats of cells (`cellXfs`) as this suite understands them, read, and added to when the workbook has
//! new ones.
//!
//! A new format is made from the one it came from (`Style::base`): that cell format is copied and only the parts that
//! differ from it (number format, font, fill, alignment) are replaced. Borders, protection, theme colours and anything else
//! the suite does not describe are kept as they were.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use office_fmt::xml::{self, Value};
use sheets_format::builtin;
use sheets_model::{Align, Border, Dxf, Line, LineKind, Style};
use std::collections::HashMap;

pub const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// The order of the children of a `<font>`, which a reader requires.
const FONT_ORDER: [&str; 15] = [
    "b",
    "i",
    "strike",
    "condense",
    "extend",
    "outline",
    "shadow",
    "u",
    "vertAlign",
    "sz",
    "color",
    "name",
    "family",
    "charset",
    "scheme",
];

/// The format text of a `numFmtId`: a custom one from the file, a built-in one, or `General`.
fn code_of(id: u32, custom: &HashMap<u32, String>) -> String {
    if let Some(c) = custom.get(&id) {
        return c.clone();
    }
    match id {
        // Locale-dependent date formats of the original spreadsheets.
        27..=36 | 50..=58 => "yyyy-mm-dd".into(),
        _ => builtin(id).unwrap_or("General").to_string(),
    }
}

/// `FFRRGGBB` or `RRGGBB` as `0xRRGGBB`.
fn rgb(text: &str) -> Option<u32> {
    let hex = if text.len() == 8 { &text[2..] } else { text };
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

fn flag(e: Option<&Value>, off: &[&str]) -> bool {
    e.is_some_and(|e| xml::attr(e, "val").is_none_or(|v| !off.contains(&v)))
}

/// What the suite takes from a `<font>`.
#[derive(Clone, Debug, Default, PartialEq)]
struct FontInfo {
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<u32>,
    /// Half points.
    size: Option<u16>,
    name: Option<String>,
}

fn font_info(font: &Value) -> FontInfo {
    let color = xml::child(font, "color")
        .and_then(|c| xml::attr(c, "rgb"))
        .and_then(rgb);
    FontInfo {
        bold: flag(xml::child(font, "b"), &["0", "false"]),
        italic: flag(xml::child(font, "i"), &["0", "false"]),
        underline: flag(xml::child(font, "u"), &["none"]),
        color,
        size: xml::child(font, "sz")
            .and_then(|e| xml::attr(e, "val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|pt| (pt * 2.0).round().clamp(2.0, 2000.0) as u16),
        name: xml::child(font, "name")
            .and_then(|e| xml::attr(e, "val"))
            .map(str::to_string),
    }
}

fn kind_of(style: &str) -> LineKind {
    match style {
        "medium" => LineKind::Medium,
        "thick" => LineKind::Thick,
        "double" => LineKind::Double,
        "dotted" => LineKind::Dotted,
        "dashed" | "mediumDashed" | "dashDot" | "mediumDashDot" | "dashDotDot"
        | "mediumDashDotDot" | "slantDashDot" => LineKind::Dashed,
        _ => LineKind::Thin,
    }
}

const fn kind_name(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Thin => "thin",
        LineKind::Medium => "medium",
        LineKind::Thick => "thick",
        LineKind::Dashed => "dashed",
        LineKind::Dotted => "dotted",
        LineKind::Double => "double",
    }
}

fn line_of(border: &Value, side: &str) -> Option<Line> {
    let e = xml::child(border, side)?;
    let style = xml::attr(e, "style").filter(|v| *v != "none")?;
    Some(Line {
        kind: kind_of(style),
        color: xml::child(e, "color")
            .and_then(|c| xml::attr(c, "rgb"))
            .and_then(rgb),
    })
}

fn border_info(border: &Value) -> Border {
    Border {
        top: line_of(border, "top"),
        bottom: line_of(border, "bottom"),
        left: line_of(border, "left"),
        right: line_of(border, "right"),
    }
}

/// The colour of a solid `<fill>`.
fn fill_color(fill: &Value) -> Option<u32> {
    let pattern = xml::child(fill, "patternFill")?;
    if xml::attr(pattern, "patternType") != Some("solid") {
        return None;
    }
    xml::child(pattern, "fgColor")
        .and_then(|c| xml::attr(c, "rgb"))
        .and_then(rgb)
}

fn alignment_of(xf: &Value) -> (Align, bool) {
    let Some(a) = xml::child(xf, "alignment") else {
        return (Align::General, false);
    };
    let align = match xml::attr(a, "horizontal") {
        Some("left" | "justify") => Align::Left,
        Some("center") => Align::Center,
        Some("right") => Align::Right,
        _ => Align::General,
    };
    (
        align,
        matches!(xml::attr(a, "wrapText"), Some("1" | "true")),
    )
}

/// One style per `cellXfs` entry, in order; each is its own `base`.
pub fn read(root: &Value) -> Vec<Style> {
    let custom: HashMap<u32, String> = xml::child(root, "numFmts")
        .map(|n| {
            xml::elements(n, "numFmt")
                .filter_map(|f| {
                    Some((
                        xml::attr(f, "numFmtId")?.parse().ok()?,
                        xml::attr(f, "formatCode")?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let fonts: Vec<FontInfo> = xml::child(root, "fonts")
        .map(|f| xml::elements(f, "font").map(font_info).collect())
        .unwrap_or_default();
    let fills: Vec<Option<u32>> = xml::child(root, "fills")
        .map(|f| xml::elements(f, "fill").map(fill_color).collect())
        .unwrap_or_default();
    let borders: Vec<Border> = xml::child(root, "borders")
        .map(|f| xml::elements(f, "border").map(border_info).collect())
        .unwrap_or_default();
    // The first font is the workbook's default: a cell that has it shows no size or name of its own.
    let default_font = fonts.first().cloned().unwrap_or_default();
    let styles: Vec<Style> = xml::child(root, "cellXfs")
        .map(|x| {
            xml::elements(x, "xf")
                .enumerate()
                .map(|(i, xf)| {
                    let id = |name: &str| {
                        xml::attr(xf, name)
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0)
                    };
                    let font = fonts.get(id("fontId")).cloned().unwrap_or_default();
                    let (bold, italic, underline, color) =
                        (font.bold, font.italic, font.underline, font.color);
                    let (align, wrap) = alignment_of(xf);
                    Style {
                        number_format: code_of(
                            xml::attr(xf, "numFmtId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0),
                            &custom,
                        ),
                        bold,
                        italic,
                        underline,
                        color,
                        fill: fills.get(id("fillId")).copied().flatten(),
                        align,
                        wrap,
                        border: borders.get(id("borderId")).copied().unwrap_or_default(),
                        locked: xml::child(xf, "protection")
                            .and_then(|p| xml::attr(p, "locked"))
                            .is_none_or(|v| !matches!(v, "0" | "false")),
                        font_size: font.size.filter(|s| Some(*s) != default_font.size),
                        font_name: font.name.filter(|n| Some(n) != default_font.name.as_ref()),
                        base: u32::try_from(i).unwrap_or(0),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if styles.is_empty() {
        vec![Style::default()]
    } else {
        styles
    }
}

/// The `numFmtId` for a format, adding a custom `numFmt` when no built-in one has that text.
fn id_for(code: &str, root: &mut Value) -> u32 {
    if code == "General" {
        return 0;
    }
    if let Some(id) = (1..50).find(|id| builtin(*id) == Some(code)) {
        return id;
    }
    if xml::child(root, "numFmts").is_none()
        && let Some(k) = xml::children_mut(root)
    {
        k.insert(0, xml::el("numFmts", &[("count", "0")], Vec::new()));
    }
    let Some(list) = xml::child_mut(root, "numFmts") else {
        return 0;
    };
    let mut next = 164;
    for f in xml::elements(list, "numFmt") {
        let id: u32 = xml::attr(f, "numFmtId")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if xml::attr(f, "formatCode") == Some(code) {
            return id;
        }
        next = next.max(id + 1);
    }
    if let Some(kids) = xml::children_mut(list) {
        kids.push(xml::el(
            "numFmt",
            &[("numFmtId", &next.to_string()), ("formatCode", code)],
            Vec::new(),
        ));
        let n = kids.len().to_string();
        xml::set_attr(list, "count", &n);
    }
    next
}

fn plain_xf() -> Value {
    xml::el(
        "xf",
        &[
            ("numFmtId", "0"),
            ("fontId", "0"),
            ("fillId", "0"),
            ("borderId", "0"),
            ("xfId", "0"),
        ],
        Vec::new(),
    )
}

fn argb(color: u32) -> String {
    format!("FF{color:06X}")
}

/// A minimal `styles.xml` for a file that had none.
pub fn new_root() -> Value {
    let fonts = xml::el(
        "fonts",
        &[("count", "1")],
        vec![xml::el(
            "font",
            &[],
            vec![
                xml::el("sz", &[("val", "11")], vec![]),
                xml::el("name", &[("val", "Calibri")], vec![]),
            ],
        )],
    );
    let fills = xml::el(
        "fills",
        &[("count", "2")],
        vec![
            xml::el(
                "fill",
                &[],
                vec![xml::el("patternFill", &[("patternType", "none")], vec![])],
            ),
            xml::el(
                "fill",
                &[],
                vec![xml::el(
                    "patternFill",
                    &[("patternType", "gray125")],
                    vec![],
                )],
            ),
        ],
    );
    let borders = xml::el(
        "borders",
        &[("count", "1")],
        vec![xml::el(
            "border",
            &[],
            ["left", "right", "top", "bottom", "diagonal"]
                .iter()
                .map(|s| xml::el(s, &[], vec![]))
                .collect(),
        )],
    );
    let cell_style_xfs = xml::el(
        "cellStyleXfs",
        &[("count", "1")],
        vec![xml::el(
            "xf",
            &[
                ("numFmtId", "0"),
                ("fontId", "0"),
                ("fillId", "0"),
                ("borderId", "0"),
            ],
            vec![],
        )],
    );
    let cell_xfs = xml::el("cellXfs", &[("count", "1")], vec![plain_xf()]);
    let cell_styles = xml::el(
        "cellStyles",
        &[("count", "1")],
        vec![xml::el(
            "cellStyle",
            &[("name", "Normal"), ("xfId", "0"), ("builtinId", "0")],
            vec![],
        )],
    );
    xml::el(
        "styleSheet",
        &[("xmlns", MAIN_NS)],
        vec![fonts, fills, borders, cell_style_xfs, cell_xfs, cell_styles],
    )
}

/// The index of `item` in the list `group` (`fonts`, `fills`), adding it (and fixing `count`) when it is not there.
fn index_in(root: &mut Value, group: &str, item_tag: &str, item: Value) -> usize {
    if xml::child(root, group).is_none()
        && let Some(k) = xml::children_mut(root)
    {
        let at = k
            .iter()
            .position(|e| matches!(xml::tag(e), Some("borders" | "cellStyleXfs" | "cellXfs")))
            .unwrap_or(k.len());
        k.insert(at, xml::el(group, &[("count", "0")], Vec::new()));
    }
    let Some(list) = xml::child_mut(root, group) else {
        return 0;
    };
    if let Some(at) = xml::elements(list, item_tag).position(|e| *e == item) {
        return at;
    }
    let Some(kids) = xml::children_mut(list) else {
        return 0;
    };
    kids.push(item);
    let n = kids.len();
    xml::set_attr(list, "count", &n.to_string());
    n - 1
}

fn nth(root: &Value, group: &str, tag: &str, at: usize) -> Option<Value> {
    xml::child(root, group).and_then(|g| xml::elements(g, tag).nth(at).cloned())
}

/// The font with these properties, made from the font `from` (everything it has stays except bold, italic, underline, colour).
fn font_for(root: &mut Value, from: usize, s: &Style) -> usize {
    let base = nth(root, "fonts", "font", from).unwrap_or_else(|| {
        xml::el(
            "font",
            &[],
            vec![
                xml::el("sz", &[("val", "11")], vec![]),
                xml::el("name", &[("val", "Calibri")], vec![]),
            ],
        )
    });
    let mut kept: Vec<Value> = xml::children(&base)
        .iter()
        .filter(|k| match xml::tag(k) {
            Some("b" | "i" | "u" | "color") => false,
            Some("sz") => s.font_size.is_none(),
            Some("name") => s.font_name.is_none(),
            _ => true,
        })
        .cloned()
        .collect();
    if let Some(half) = s.font_size {
        let pt = if half % 2 == 0 {
            (half / 2).to_string()
        } else {
            format!("{}.5", half / 2)
        };
        kept.push(xml::el("sz", &[("val", &pt)], vec![]));
    }
    if let Some(name) = &s.font_name {
        kept.push(xml::el("name", &[("val", name)], vec![]));
    }
    // A colour the suite cannot describe (a theme colour) stays unless a colour is chosen.
    let old_color = xml::child(&base, "color").cloned();
    if s.bold {
        kept.push(xml::el("b", &[], vec![]));
    }
    if s.italic {
        kept.push(xml::el("i", &[], vec![]));
    }
    if s.underline {
        kept.push(xml::el("u", &[], vec![]));
    }
    match (s.color, old_color) {
        (Some(c), _) => kept.push(xml::el("color", &[("rgb", &argb(c))], vec![])),
        (None, Some(old)) if xml::attr(&old, "rgb").is_none() => kept.push(old),
        _ => {}
    }
    kept.sort_by_key(|k| {
        FONT_ORDER
            .iter()
            .position(|t| Some(*t) == xml::tag(k))
            .unwrap_or(FONT_ORDER.len())
    });
    index_in(root, "fonts", "font", xml::el("font", &[], kept))
}

/// The border with these lines, made from border `from`: its sides are replaced, anything else it has (a diagonal) stays.
fn border_for(root: &mut Value, from: usize, border: &Border) -> usize {
    let base =
        nth(root, "borders", "border", from).unwrap_or_else(|| xml::el("border", &[], vec![]));
    let side = |tag: &str, line: Option<Line>| {
        line.map_or_else(
            || xml::el(tag, &[], vec![]),
            |l| {
                let color = l.color.map_or_else(
                    || xml::el("color", &[("auto", "1")], vec![]),
                    |c| xml::el("color", &[("rgb", &argb(c))], vec![]),
                );
                xml::el(tag, &[("style", kind_name(l.kind))], vec![color])
            },
        )
    };
    let mut kids = vec![
        side("left", border.left),
        side("right", border.right),
        side("top", border.top),
        side("bottom", border.bottom),
    ];
    kids.extend(
        xml::children(&base)
            .iter()
            .filter(|k| !matches!(xml::tag(k), Some("left" | "right" | "top" | "bottom")))
            .cloned(),
    );
    index_in(
        root,
        "borders",
        "border",
        xml::el("border", &xml::attrs(&base), kids),
    )
}

fn fill_for(root: &mut Value, color: Option<u32>) -> usize {
    color.map_or(0, |c| {
        let pattern = xml::el(
            "patternFill",
            &[("patternType", "solid")],
            vec![
                xml::el("fgColor", &[("rgb", &argb(c))], vec![]),
                xml::el("bgColor", &[("indexed", "64")], vec![]),
            ],
        );
        index_in(root, "fills", "fill", xml::el("fill", &[], vec![pattern]))
    })
}

const fn horizontal(a: Align) -> Option<&'static str> {
    match a {
        Align::General => None,
        Align::Left => Some("left"),
        Align::Center => Some("center"),
        Align::Right => Some("right"),
    }
}

/// Put `alignment` on an `xf`: its `horizontal` and `wrapText`, keeping other alignment attributes.
fn set_alignment(xf: &mut Value, s: &Style) {
    let mut al = xml::child(xf, "alignment")
        .cloned()
        .unwrap_or_else(|| xml::el("alignment", &[], vec![]));
    xml::remove_attr(&mut al, "horizontal");
    xml::remove_attr(&mut al, "wrapText");
    if let Some(h) = horizontal(s.align) {
        xml::set_attr(&mut al, "horizontal", h);
    }
    if s.wrap {
        xml::set_attr(&mut al, "wrapText", "1");
    }
    let Some(kids) = xml::children_mut(xf) else {
        return;
    };
    kids.retain(|k| xml::tag(k) != Some("alignment"));
    if !xml::attrs(&al).is_empty() {
        kids.insert(0, al);
    }
    xml::set_attr(xf, "applyAlignment", "1");
}

/// Make the file's `cellXfs` as long as the workbook's styles: the ones past `have` are new.
pub fn extend(root: &mut Value, styles: &[Style], have: usize) {
    if xml::child(root, "cellXfs").is_none()
        && let Some(kids) = xml::children_mut(root)
    {
        kids.push(xml::el("cellXfs", &[("count", "0")], Vec::new()));
    }
    for s in styles.iter().skip(have) {
        let base_at = (s.base as usize).min(have.saturating_sub(1));
        let base_style = styles.get(base_at).cloned().unwrap_or_default();
        let mut xf = nth(root, "cellXfs", "xf", base_at).unwrap_or_else(plain_xf);
        if s.number_format != base_style.number_format {
            let id = id_for(&s.number_format, root);
            xml::set_attr(&mut xf, "numFmtId", &id.to_string());
            xml::set_attr(&mut xf, "applyNumberFormat", "1");
        }
        if (
            s.bold,
            s.italic,
            s.underline,
            s.color,
            &s.font_size,
            &s.font_name,
        ) != (
            base_style.bold,
            base_style.italic,
            base_style.underline,
            base_style.color,
            &base_style.font_size,
            &base_style.font_name,
        ) {
            let from = xml::attr(&xf, "fontId")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let id = font_for(root, from, s);
            xml::set_attr(&mut xf, "fontId", &id.to_string());
            xml::set_attr(&mut xf, "applyFont", "1");
        }
        if s.locked != base_style.locked {
            let mut protection = xml::child(&xf, "protection")
                .cloned()
                .unwrap_or_else(|| xml::el("protection", &[], Vec::new()));
            xml::set_attr(&mut protection, "locked", if s.locked { "1" } else { "0" });
            if let Some(kids) = xml::children_mut(&mut xf) {
                kids.retain(|k| xml::tag(k) != Some("protection"));
                kids.push(protection);
            }
            xml::set_attr(&mut xf, "applyProtection", "1");
        }
        if s.border != base_style.border {
            let from = xml::attr(&xf, "borderId")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let id = border_for(root, from, &s.border);
            xml::set_attr(&mut xf, "borderId", &id.to_string());
            xml::set_attr(&mut xf, "applyBorder", "1");
        }
        if s.fill != base_style.fill {
            let id = fill_for(root, s.fill);
            xml::set_attr(&mut xf, "fillId", &id.to_string());
            xml::set_attr(&mut xf, "applyFill", "1");
        }
        if (s.align, s.wrap) != (base_style.align, base_style.wrap) {
            set_alignment(&mut xf, s);
        }
        if let Some(list) = xml::child_mut(root, "cellXfs")
            && let Some(kids) = xml::children_mut(list)
        {
            kids.push(xf);
            let n = kids.len().to_string();
            xml::set_attr(list, "count", &n);
        }
    }
}

/// The differential formats (`<dxfs>`) the conditional formats refer to, in order.
pub fn read_dxfs(root: &Value) -> Vec<Dxf> {
    let Some(list) = xml::child(root, "dxfs") else {
        return Vec::new();
    };
    xml::elements(list, "dxf")
        .map(|d| {
            let font = xml::child(d, "font");
            let fill = xml::child(d, "fill").and_then(|f| xml::child(f, "patternFill"));
            Dxf {
                bold: font.is_some_and(|f| flag(xml::child(f, "b"), &["0", "false"])),
                italic: font.is_some_and(|f| flag(xml::child(f, "i"), &["0", "false"])),
                color: font
                    .and_then(|f| xml::child(f, "color"))
                    .and_then(|c| xml::attr(c, "rgb"))
                    .and_then(rgb),
                fill: fill
                    .and_then(|p| xml::child(p, "bgColor").or_else(|| xml::child(p, "fgColor")))
                    .and_then(|c| xml::attr(c, "rgb"))
                    .and_then(rgb),
            }
        })
        .collect()
}

/// The index of a differential format, added to `<dxfs>` when it is new.
pub fn dxf_for(root: &mut Value, d: &Dxf) -> usize {
    if xml::child(root, "dxfs").is_none()
        && let Some(kids) = xml::children_mut(root)
    {
        let at = kids
            .iter()
            .position(|e| matches!(xml::tag(e), Some("tableStyles" | "colors" | "extLst")))
            .unwrap_or(kids.len());
        kids.insert(at, xml::el("dxfs", &[("count", "0")], Vec::new()));
    }
    let mut parts = Vec::new();
    if d.bold || d.italic || d.color.is_some() {
        let mut font = Vec::new();
        if d.bold {
            font.push(xml::el("b", &[], vec![]));
        }
        if d.italic {
            font.push(xml::el("i", &[], vec![]));
        }
        if let Some(c) = d.color {
            font.push(xml::el("color", &[("rgb", &argb(c))], vec![]));
        }
        parts.push(xml::el("font", &[], font));
    }
    if let Some(c) = d.fill {
        parts.push(xml::el(
            "fill",
            &[],
            vec![xml::el(
                "patternFill",
                &[("patternType", "solid")],
                vec![
                    xml::el("fgColor", &[("rgb", &argb(c))], vec![]),
                    xml::el("bgColor", &[("rgb", &argb(c))], vec![]),
                ],
            )],
        ));
    }
    let item = xml::el("dxf", &[], parts);
    let Some(list) = xml::child_mut(root, "dxfs") else {
        return 0;
    };
    if let Some(at) = xml::elements(list, "dxf").position(|e| *e == item) {
        return at;
    }
    let Some(kids) = xml::children_mut(list) else {
        return 0;
    };
    kids.push(item);
    let n = kids.len();
    xml::set_attr(list, "count", &n.to_string());
    n - 1
}

/// The custom table looks of a file (`<tableStyles>`): their names, as the model names a look of three colours. Only looks
/// that colour the heading and the first stripe are understood.
pub fn read_table_styles(root: &Value) -> std::collections::HashMap<String, String> {
    let dxfs = read_dxfs(root);
    let mut out = std::collections::HashMap::new();
    let Some(list) = xml::child(root, "tableStyles") else {
        return out;
    };
    for style in xml::elements(list, "tableStyle") {
        let Some(name) = xml::attr(style, "name") else {
            continue;
        };
        let dxf = |kind: &str| {
            xml::elements(style, "tableStyleElement")
                .find(|e| xml::attr(e, "type") == Some(kind))
                .and_then(|e| xml::attr(e, "dxfId"))
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| dxfs.get(i))
        };
        let head = dxf("headerRow");
        let band = dxf("firstRowStripe");
        if let Some(h) = head.and_then(|d| d.fill) {
            let text = head.and_then(|d| d.color).unwrap_or(0x00FF_FFFF);
            let tint = band.and_then(|d| d.fill).unwrap_or(0x00F2_F2F2);
            out.insert(
                name.to_string(),
                sheets_model::custom_table_style(h, text, tint),
            );
        }
    }
    out
}

/// Make sure `<tableStyles>` defines the look a table uses, when it is one of ours made of three colours.
pub fn ensure_table_style(root: &mut Value, name: &str) {
    let Some((head, text, band)) = sheets_model::parse_custom_table_style(name) else {
        return;
    };
    let exists = xml::child(root, "tableStyles").is_some_and(|l| {
        xml::elements(l, "tableStyle").any(|s| xml::attr(s, "name") == Some(name))
    });
    if exists {
        return;
    }
    let head_id = dxf_for(
        root,
        &Dxf {
            bold: true,
            italic: false,
            color: Some(text),
            fill: Some(head),
        },
    );
    let band_id = dxf_for(
        root,
        &Dxf {
            bold: false,
            italic: false,
            color: None,
            fill: Some(band),
        },
    );
    if xml::child(root, "tableStyles").is_none()
        && let Some(kids) = xml::children_mut(root)
    {
        let at = kids
            .iter()
            .position(|e| matches!(xml::tag(e), Some("colors" | "extLst")))
            .unwrap_or(kids.len());
        kids.insert(
            at,
            xml::el(
                "tableStyles",
                &[
                    ("count", "0"),
                    ("defaultTableStyle", "TableStyleMedium2"),
                    ("defaultPivotStyle", "PivotStyleLight16"),
                ],
                Vec::new(),
            ),
        );
    }
    let item = xml::el(
        "tableStyle",
        &[("name", name), ("pivot", "0"), ("count", "2")],
        vec![
            xml::el(
                "tableStyleElement",
                &[("type", "headerRow"), ("dxfId", &head_id.to_string())],
                Vec::new(),
            ),
            xml::el(
                "tableStyleElement",
                &[("type", "firstRowStripe"), ("dxfId", &band_id.to_string())],
                Vec::new(),
            ),
        ],
    );
    if let Some(list) = xml::child_mut(root, "tableStyles")
        && let Some(kids) = xml::children_mut(list)
    {
        kids.push(item);
        let n = kids.len().to_string();
        xml::set_attr(list, "count", &n);
    }
}
