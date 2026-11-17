//! Changes to cells, as a document in the cloud sends and receives them.
//!
//! A change is the cells a step wrote: where (sheet, row, column), what is in them and how they look. The look is sent as the
//! style itself, not as an index into the workbook's styles, because the tables of two pages need not agree. Changes that are
//! not cells (rows inserted, a sheet added, widths) are not described here: the page sends the whole file for those.

use quench_serde::{Value as Json, json};
use sheets_model::{Align, Border, CellError, CellRef, Content, Line, LineKind, Style, Value};

/// One cell as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub sheet: usize,
    pub at: CellRef,
    pub content: Option<Content>,
    pub style: Style,
}

fn align_word(a: Align) -> &'static str {
    match a {
        Align::General => "g",
        Align::Left => "l",
        Align::Center => "c",
        Align::Right => "r",
    }
}

fn align_of(word: &str) -> Align {
    match word {
        "l" => Align::Left,
        "c" => Align::Center,
        "r" => Align::Right,
        _ => Align::General,
    }
}

fn line_word(k: LineKind) -> &'static str {
    match k {
        LineKind::Thin => "thin",
        LineKind::Medium => "medium",
        LineKind::Thick => "thick",
        LineKind::Dashed => "dashed",
        LineKind::Dotted => "dotted",
        LineKind::Double => "double",
    }
}

fn line_of(word: &str) -> LineKind {
    match word {
        "medium" => LineKind::Medium,
        "thick" => LineKind::Thick,
        "dashed" => LineKind::Dashed,
        "dotted" => LineKind::Dotted,
        "double" => LineKind::Double,
        _ => LineKind::Thin,
    }
}

fn error_of(text: &str) -> CellError {
    [
        CellError::Div0,
        CellError::Value,
        CellError::Ref,
        CellError::Name,
        CellError::Num,
        CellError::NA,
        CellError::Null,
        CellError::Circular,
        CellError::Spill,
    ]
    .into_iter()
    .find(|e| e.as_str() == text)
    .unwrap_or(CellError::Value)
}

fn side(line: Option<Line>) -> String {
    line.map_or_else(
        || "null".to_string(),
        |l| {
            let color = l.color.map_or_else(String::new, |c| format!(",\"c\":{c}"));
            format!("{{\"k\":\"{}\"{color}}}", line_word(l.kind))
        },
    )
}

fn style_json(s: &Style) -> String {
    let opt = |v: Option<u32>| v.map_or_else(|| "null".to_string(), |n| n.to_string());
    let size = s
        .font_size
        .map_or_else(|| "null".to_string(), |n| n.to_string());
    let name = s
        .font_name
        .as_deref()
        .map_or_else(|| "null".to_string(), json::to_string);
    format!(
        "{{\"nf\":{},\"b\":{},\"i\":{},\"u\":{},\"co\":{},\"fi\":{},\"al\":\"{}\",\"w\":{},\"lk\":{},\"fs\":{size},\"fn\":{name},\"bt\":{},\"bb\":{},\"bl\":{},\"br\":{}}}",
        json::to_string(s.number_format.as_str()),
        s.bold,
        s.italic,
        s.underline,
        opt(s.color),
        opt(s.fill),
        align_word(s.align),
        s.wrap,
        s.locked,
        side(s.border.top),
        side(s.border.bottom),
        side(s.border.left),
        side(s.border.right),
    )
}

fn content_json(c: Option<&Content>) -> String {
    // Nothing, and a number that is not one (it cannot be written in JSON), are an empty cell.
    let empty = || "\"v\":null".to_string();
    match c {
        Some(Content::Formula(f)) => format!("\"f\":{}", json::to_string(f.as_str())),
        Some(Content::Value(Value::Number(n))) if n.is_finite() => format!("\"v\":{n:?}"),
        Some(Content::Value(Value::Text(t))) => format!("\"v\":{}", json::to_string(t.as_str())),
        Some(Content::Value(Value::Bool(b))) => format!("\"v\":{b}"),
        Some(Content::Value(Value::Error(e))) => format!("\"v\":{{\"e\":\"{}\"}}", e.as_str()),
        Some(Content::Value(Value::Number(_) | Value::Empty)) | None => empty(),
    }
}

/// The change that writes `cells`, as JSON.
#[must_use]
pub fn encode(cells: &[Written]) -> Vec<u8> {
    let list: Vec<String> = cells
        .iter()
        .map(|w| {
            format!(
                "{{\"s\":{},\"r\":{},\"c\":{},{},\"st\":{}}}",
                w.sheet,
                w.at.row,
                w.at.col,
                content_json(w.content.as_ref()),
                style_json(&w.style)
            )
        })
        .collect();
    format!("{{\"cells\":[{}]}}", list.join(",")).into_bytes()
}

fn num(v: Option<&Json>) -> Option<u64> {
    match v? {
        Json::UInt(n) => Some(*n),
        Json::Int(n) => u64::try_from(*n).ok(),
        Json::Float(f) if *f >= 0.0 && f.fract() == 0.0 => Some(*f as u64),
        _ => None,
    }
}

fn flag(v: Option<&Json>, default: bool) -> bool {
    match v {
        Some(Json::Bool(b)) => *b,
        _ => default,
    }
}

fn color(v: Option<&Json>) -> Option<u32> {
    num(v).and_then(|n| u32::try_from(n).ok())
}

fn line_from(v: Option<&Json>) -> Option<Line> {
    let v = v?;
    if matches!(v, Json::Null) {
        return None;
    }
    let kind = line_of(v.get("k").and_then(Json::as_str).unwrap_or("thin"));
    Some(Line {
        kind,
        color: color(v.get("c")),
    })
}

fn style_from(v: &Json) -> Style {
    Style {
        number_format: v
            .get("nf")
            .and_then(Json::as_str)
            .unwrap_or("General")
            .to_string(),
        bold: flag(v.get("b"), false),
        italic: flag(v.get("i"), false),
        underline: flag(v.get("u"), false),
        color: color(v.get("co")),
        fill: color(v.get("fi")),
        align: align_of(v.get("al").and_then(Json::as_str).unwrap_or("g")),
        wrap: flag(v.get("w"), false),
        border: Border {
            top: line_from(v.get("bt")),
            bottom: line_from(v.get("bb")),
            left: line_from(v.get("bl")),
            right: line_from(v.get("br")),
        },
        locked: flag(v.get("lk"), true),
        font_size: num(v.get("fs")).and_then(|n| u16::try_from(n).ok()),
        font_name: v.get("fn").and_then(Json::as_str).map(str::to_string),
        base: 0,
    }
}

fn content_from(cell: &Json) -> Option<Content> {
    if let Some(f) = cell.get("f").and_then(Json::as_str) {
        return Some(Content::Formula(f.to_string()));
    }
    match cell.get("v")? {
        Json::Bool(b) => Some(Content::Value(Value::Bool(*b))),
        Json::String(t) => Some(Content::Value(Value::Text(t.clone()))),
        Json::Int(n) => Some(Content::Value(Value::Number(*n as f64))),
        Json::UInt(n) => Some(Content::Value(Value::Number(*n as f64))),
        Json::Float(n) => Some(Content::Value(Value::Number(*n))),
        obj @ Json::Object(_) => obj
            .get("e")
            .and_then(Json::as_str)
            .map(|e| Content::Value(Value::Error(error_of(e)))),
        Json::Null | Json::Array(_) => None,
    }
}

/// The cells a change writes.
///
/// # Errors
///
/// A message when the text is not a change.
pub fn decode(bytes: &[u8]) -> Result<Vec<Written>, String> {
    let root = json::parse(&String::from_utf8_lossy(bytes)).map_err(|e| e.to_string())?;
    let Some(Json::Array(list)) = root.get("cells") else {
        return Err("not a change of cells".to_string());
    };
    let mut out = Vec::new();
    for cell in list {
        let (Some(sheet), Some(row), Some(col)) =
            (num(cell.get("s")), num(cell.get("r")), num(cell.get("c")))
        else {
            return Err("a cell with no place".to_string());
        };
        out.push(Written {
            sheet: usize::try_from(sheet).map_err(|e| e.to_string())?,
            at: CellRef::new(
                u32::try_from(row).map_err(|e| e.to_string())?,
                u32::try_from(col).map_err(|e| e.to_string())?,
            ),
            content: content_from(cell),
            style: cell.get("st").map_or_else(Style::default, style_from),
        });
    }
    Ok(out)
}
