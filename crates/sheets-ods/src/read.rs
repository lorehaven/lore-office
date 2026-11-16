//! Reading `content.xml`.

use crate::{Error, PX_PER_CM, from_odf};
use office_fmt::Package;
use office_fmt::xml::{self, Value};
use sheets_model::{Align, CellRef, Content, Range, Sheet, Style, Value as Cell, Workbook, date};
use std::collections::HashMap;

/// The most rows and columns a repeat may cover, so that a damaged count cannot fill memory.
const MAX_ROWS: u32 = 1_048_576;
const MAX_COLS: u32 = 16_384;
const MAX_REPEAT_FILLED: u32 = 1000;

fn attr<'a>(e: &'a Value, name: &str) -> Option<&'a str> {
    xml::attr(e, name)
}

fn children<'a>(e: &'a Value, tag: &'a str) -> impl Iterator<Item = &'a Value> {
    xml::children(e)
        .iter()
        .filter(move |c| xml::tag(c) == Some(tag))
}

/// A length such as `2.267cm`, `0.8in`, `15pt` or `12mm` in pixels.
fn pixels(text: &str) -> Option<f64> {
    let t = text.trim();
    let split = t.find(|c: char| c.is_alphabetic())?;
    let value: f64 = t[..split].parse().ok()?;
    Some(match &t[split..] {
        "cm" => value * PX_PER_CM,
        "mm" => value * PX_PER_CM / 10.0,
        "in" => value * 96.0,
        "pt" => value * 96.0 / 72.0,
        "px" => value,
        _ => return None,
    })
}

fn color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// What a named data style (`number:number-style`...) means as a number format.
fn data_format(e: &Value) -> Option<String> {
    let tag = xml::tag(e)?;
    let decimals = children(e, "number:number")
        .find_map(|n| attr(n, "number:decimal-places").and_then(|d| d.parse::<usize>().ok()))
        .unwrap_or(0);
    let grouping = children(e, "number:number").any(|n| attr(n, "number:grouping") == Some("true"));
    let digits = |grouped: bool| {
        let int = if grouped { "#,##0" } else { "0" };
        if decimals > 0 {
            format!("{int}.{}", "0".repeat(decimals))
        } else {
            int.to_string()
        }
    };
    match tag {
        "number:percentage-style" => Some(format!(
            "{}%",
            if decimals > 0 {
                format!("0.{}", "0".repeat(decimals))
            } else {
                "0".to_string()
            }
        )),
        "number:currency-style" => Some(digits(true)),
        "number:number-style" => {
            // No digits chosen is the format a spreadsheet calls General.
            let chosen =
                children(e, "number:number").any(|n| attr(n, "number:decimal-places").is_some());
            (chosen || grouping)
                .then(|| digits(grouping))
                .or_else(|| Some("General".to_string()))
        }
        "number:date-style" | "number:time-style" => {
            let mut f = String::new();
            for part in xml::children(e).iter().filter(|c| c.is_object()) {
                let long = attr(part, "number:style") == Some("long");
                match xml::tag(part) {
                    Some("number:year") => f.push_str(if long { "yyyy" } else { "yy" }),
                    Some("number:month") => {
                        f.push_str(if attr(part, "number:textual") == Some("true") {
                            if long { "mmmm" } else { "mmm" }
                        } else if long {
                            "mm"
                        } else {
                            "m"
                        })
                    }
                    Some("number:day") => f.push_str(if long { "dd" } else { "d" }),
                    Some("number:day-of-week") => f.push_str(if long { "dddd" } else { "ddd" }),
                    Some("number:hours") => f.push_str(if long { "hh" } else { "h" }),
                    Some("number:minutes") => f.push_str(if long { "mm" } else { "m" }),
                    Some("number:seconds") => f.push_str(if long { "ss" } else { "s" }),
                    Some("number:am-pm") => f.push_str("AM/PM"),
                    Some("number:text") => {
                        let t = xml::text(part);
                        f.push_str(&if t
                            .chars()
                            .all(|c| matches!(c, '-' | '/' | ':' | ' ' | '.' | ','))
                        {
                            t
                        } else {
                            format!("\"{t}\"")
                        });
                    }
                    _ => {}
                }
            }
            Some(f)
        }
        _ => None,
    }
}

struct Styles {
    cell: HashMap<String, Style>,
    column: HashMap<String, f64>,
    row: HashMap<String, f64>,
}

fn read_styles(roots: &[&Value]) -> Styles {
    let mut formats: HashMap<String, String> = HashMap::new();
    let mut out = Styles {
        cell: HashMap::new(),
        column: HashMap::new(),
        row: HashMap::new(),
    };
    for root in roots {
        for e in xml::children(root).iter().filter(|c| c.is_object()) {
            if (xml::tag(e) == Some("office:automatic-styles")
                || xml::tag(e) == Some("office:styles"))
                && e.is_object()
            {
                for st in xml::children(e).iter().filter(|c| c.is_object()) {
                    if let (Some(name), Some(f)) = (attr(st, "style:name"), data_format(st)) {
                        formats.insert(name.to_string(), f);
                    }
                }
            }
        }
    }
    for root in roots {
        for e in xml::children(root).iter().filter(|c| c.is_object()) {
            if xml::tag(e) != Some("office:automatic-styles")
                && xml::tag(e) != Some("office:styles")
            {
                continue;
            }
            for st in children(e, "style:style") {
                let Some(name) = attr(st, "style:name") else {
                    continue;
                };
                match attr(st, "style:family") {
                    Some("table-column") => {
                        if let Some(w) = xml::child(st, "style:table-column-properties")
                            .and_then(|p| attr(p, "style:column-width"))
                            .and_then(pixels)
                        {
                            out.column.insert(name.to_string(), w);
                        }
                    }
                    Some("table-row") => {
                        if let Some(h) = xml::child(st, "style:table-row-properties")
                            .and_then(|p| attr(p, "style:row-height"))
                            .and_then(pixels)
                        {
                            out.row.insert(name.to_string(), h);
                        }
                    }
                    Some("table-cell") => {
                        let mut s = Style::default();
                        if let Some(f) =
                            attr(st, "style:data-style-name").and_then(|n| formats.get(n))
                        {
                            s.number_format.clone_from(f);
                        }
                        if let Some(t) = xml::child(st, "style:text-properties") {
                            s.bold = attr(t, "fo:font-weight") == Some("bold");
                            s.italic = attr(t, "fo:font-style") == Some("italic");
                            s.underline =
                                attr(t, "style:text-underline-style").is_some_and(|u| u != "none");
                            s.color = attr(t, "fo:color").and_then(color);
                        }
                        if let Some(c) = xml::child(st, "style:table-cell-properties") {
                            s.fill = attr(c, "fo:background-color").and_then(color);
                            s.wrap = attr(c, "fo:wrap-option") == Some("wrap");
                        }
                        if let Some(p) = xml::child(st, "style:paragraph-properties") {
                            s.align = match attr(p, "fo:text-align") {
                                Some("start" | "left") => Align::Left,
                                Some("center") => Align::Center,
                                Some("end" | "right") => Align::Right,
                                _ => Align::General,
                            };
                        }
                        out.cell.insert(name.to_string(), s);
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

fn cell_value(c: &Value) -> Option<Cell> {
    let kind = attr(c, "office:value-type")?;
    let number = |name: &str| attr(c, name).and_then(|v| v.parse::<f64>().ok());
    Some(match kind {
        "float" | "percentage" | "currency" => Cell::Number(number("office:value")?),
        "boolean" => Cell::Bool(attr(c, "office:boolean-value") == Some("true")),
        "date" => Cell::Number(iso_date(attr(c, "office:date-value")?)?),
        "time" => Cell::Number(duration(attr(c, "office:time-value")?)?),
        _ => {
            let text = attr(c, "office:string-value").map_or_else(|| paragraphs(c), str::to_string);
            Cell::Text(text)
        }
    })
}

/// The text paragraphs of a cell, joined by line breaks.
fn paragraphs(c: &Value) -> String {
    children(c, "text:p")
        .map(|p| flatten(p))
        .collect::<Vec<_>>()
        .join("\n")
}

fn flatten(e: &Value) -> String {
    let mut out = String::new();
    for k in xml::children(e) {
        match k {
            Value::String(s) => out.push_str(s),
            other if xml::tag(other) == Some("text:s") => out.push_str(
                &" ".repeat(
                    attr(other, "text:c")
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(1),
                ),
            ),
            other if xml::tag(other) == Some("text:tab") => out.push('\t'),
            other if xml::tag(other) == Some("text:line-break") => out.push('\n'),
            other if other.is_object() => out.push_str(&flatten(other)),
            _ => {}
        }
    }
    out
}

/// `2024-02-29` or `2024-02-29T13:45:10` as a serial number.
fn iso_date(text: &str) -> Option<f64> {
    let (day, time) = text
        .split_once('T')
        .map_or((text, None), |(d, t)| (d, Some(t)));
    let mut parts = day.split('-');
    let (y, m, d) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    let mut serial = date::to_serial(y, m, d) as f64;
    if let Some(t) = time {
        let mut p = t.trim_end_matches('Z').split(':');
        let (h, mi): (f64, f64) = (p.next()?.parse().ok()?, p.next()?.parse().ok()?);
        let s: f64 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        serial += (h * 3600.0 + mi * 60.0 + s) / 86_400.0;
    }
    Some(serial)
}

/// `PT13H45M10S` as a fraction of a day.
fn duration(text: &str) -> Option<f64> {
    let t = text.strip_prefix("PT")?;
    let (mut total, mut number) = (0.0, String::new());
    for c in t.chars() {
        match c {
            'H' => total += number.parse::<f64>().ok()? * 3600.0,
            'M' => total += number.parse::<f64>().ok()? * 60.0,
            'S' => total += number.parse::<f64>().ok()?,
            c => {
                number.push(c);
                continue;
            }
        }
        number.clear();
    }
    Some(total / 86_400.0)
}

fn repeat(e: &Value, name: &str) -> u32 {
    attr(e, name)
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

/// Read a `.ods` file.
///
/// # Errors
///
/// [`Error::Invalid`] when the bytes are not a zip with a readable `content.xml`.
pub fn read(bytes: &[u8]) -> Result<Workbook, Error> {
    let package = Package::read(bytes)?;
    let content = package
        .get("content.xml")
        .ok_or_else(|| Error::Invalid("content.xml is missing".into()))?;
    let content = xml::parse(content, false)?;
    let styles_root = package
        .get("styles.xml")
        .and_then(|b| xml::parse(b, false).ok());
    let mut roots: Vec<&Value> = vec![&content];
    if let Some(s) = &styles_root {
        roots.push(s);
    }
    let styles = read_styles(&roots);
    let Some(spreadsheet) =
        xml::child(&content, "office:body").and_then(|b| xml::child(b, "office:spreadsheet"))
    else {
        return Err(Error::Invalid("this is not a spreadsheet".into()));
    };
    let mut wb = Workbook {
        sheets: Vec::new(),
        styles: vec![Style::default()],
        names: Vec::new(),
    };
    let mut seen = 0_u64;
    for table in children(spreadsheet, "table:table") {
        let mut sheet = Sheet::new(attr(table, "table:name").unwrap_or("Sheet"));
        // Columns: their widths and visibility.
        let mut col = 0_u32;
        for c in xml::children(table).iter().filter(|c| c.is_object()) {
            let groups: Vec<&Value> = match xml::tag(c) {
                Some("table:table-column") => vec![c],
                Some("table:table-column-group" | "table:table-header-columns") => {
                    children(c, "table:table-column").collect()
                }
                _ => continue,
            };
            for column in groups {
                let n = repeat(column, "table:number-columns-repeated").min(MAX_COLS);
                let width = attr(column, "table:style-name")
                    .and_then(|s| styles.column.get(s))
                    .copied();
                let hidden = attr(column, "table:visibility") == Some("collapse");
                for k in col..(col + n).min(MAX_COLS) {
                    if n <= 1000 {
                        if let Some(w) = width {
                            sheet.set_col_width(k, Some(w));
                        }
                        if hidden {
                            sheet.set_col_hidden(k, true);
                        }
                    }
                }
                col += n;
            }
        }
        let mut row = 0_u32;
        let mut rows: Vec<&Value> = Vec::new();
        for c in xml::children(table).iter().filter(|c| c.is_object()) {
            match xml::tag(c) {
                Some("table:table-row") => rows.push(c),
                Some("table:table-row-group" | "table:table-header-rows" | "table:table-rows") => {
                    rows.extend(children(c, "table:table-row"))
                }
                _ => {}
            }
        }
        for r in rows {
            let times = repeat(r, "table:number-rows-repeated");
            let height = attr(r, "table:style-name")
                .and_then(|s| styles.row.get(s))
                .copied();
            let hidden = attr(r, "table:visibility") == Some("collapse");
            let mut filled = false;
            let mut col = 0_u32;
            let mut cells: Vec<(u32, u32, Option<Cell>, Option<String>, Style, (u32, u32))> =
                Vec::new();
            for c in xml::children(r).iter().filter(|c| {
                matches!(
                    xml::tag(c),
                    Some("table:table-cell" | "table:covered-table-cell")
                )
            }) {
                let n = repeat(c, "table:number-columns-repeated");
                let value = cell_value(c);
                let formula = attr(c, "table:formula").map(from_odf);
                let style = attr(c, "table:style-name")
                    .and_then(|s| styles.cell.get(s))
                    .cloned()
                    .unwrap_or_default();
                let span = (
                    repeat(c, "table:number-rows-spanned"),
                    repeat(c, "table:number-columns-spanned"),
                );
                if value.is_some() || formula.is_some() || style != Style::default() {
                    cells.push((col, n, value, formula, style, span));
                    filled = true;
                }
                col = col.saturating_add(n);
            }
            if filled {
                for rep in 0..times.min(MAX_REPEAT_FILLED) {
                    let rr = row + rep;
                    if rr >= MAX_ROWS {
                        break;
                    }
                    for (c0, n, value, formula, style, span) in &cells {
                        for k in 0..(*n).min(MAX_REPEAT_FILLED) {
                            let cc = c0 + k;
                            if cc >= MAX_COLS {
                                break;
                            }
                            seen += 1;
                            if seen > 10_000_000 {
                                return Err(Error::Invalid("more than ten million cells".into()));
                            }
                            let at = CellRef::new(rr, cc);
                            match (formula, value) {
                                (Some(f), _) => sheet.set(at, Content::Formula(f.clone())),
                                (None, Some(v)) => sheet.set(at, Content::Value(v.clone())),
                                _ => {}
                            }
                            if *style != Style::default() {
                                let id = wb.style_id(style);
                                sheet.set_style(at, id);
                            }
                            if (span.0 > 1 || span.1 > 1) && rep == 0 && k == 0 {
                                let end = CellRef::new(
                                    (rr + span.0 - 1).min(MAX_ROWS - 1),
                                    (cc + span.1 - 1).min(MAX_COLS - 1),
                                );
                                sheet.merge(Range::new(at, end));
                            }
                        }
                    }
                }
            }
            if times <= 1000 {
                for rep in 0..times {
                    let rr = row + rep;
                    if let Some(h) = height.filter(|_| attr(r, "table:style-name").is_some()) {
                        if (h - 24.0).abs() > 0.5 {
                            sheet.set_row_height(rr, Some(h));
                        }
                    }
                    if hidden {
                        sheet.set_row_hidden(rr, true);
                    }
                }
            }
            row = row.saturating_add(times);
        }
        wb.sheets.push(sheet);
    }
    if wb.sheets.is_empty() {
        return Err(Error::Invalid("there are no sheets".into()));
    }
    Ok(wb)
}
