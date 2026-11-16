//! Writing an `.ods` file.

use crate::{Error, PX_PER_CM, to_odf};
use office_fmt::Package;
use sheets_calc::Engine;
use sheets_model::{Align, Content, Style, Value, date};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cm(px: f64) -> String {
    format!("{:.3}cm", px / PX_PER_CM)
}

/// The data style (`number:...`) for a number format, as (name, element), or `None` for General.
fn data_style(name: &str, format: &str) -> Option<String> {
    if format == "General" || format.is_empty() {
        return None;
    }
    let decimals = format
        .split('.')
        .nth(1)
        .map_or(0, |d| d.chars().take_while(|c| *c == '0').count());
    let lower = format.to_lowercase();
    if lower.contains('y') || lower.contains('d') && !lower.contains('0') {
        // A date: the letters in order, with what is between them as text.
        let mut parts = String::new();
        let mut chars = format.chars().peekable();
        while let Some(c) = chars.next() {
            let mut n = 1;
            while chars.peek() == Some(&c) {
                chars.next();
                n += 1;
            }
            let long = if n >= 2 { " number:style=\"long\"" } else { "" };
            match c.to_ascii_lowercase() {
                'y' => {
                    let _ = write!(
                        parts,
                        "<number:year{}/>",
                        if n >= 3 { " number:style=\"long\"" } else { "" }
                    );
                }
                'm' if n >= 3 => {
                    let _ = write!(parts, "<number:month number:textual=\"true\"{long}/>");
                }
                'm' => {
                    let _ = write!(parts, "<number:month{long}/>");
                }
                'd' if n >= 3 => {
                    let _ = write!(
                        parts,
                        "<number:day-of-week{}/>",
                        if n >= 4 { " number:style=\"long\"" } else { "" }
                    );
                }
                'd' => {
                    let _ = write!(parts, "<number:day{long}/>");
                }
                _ => {
                    for _ in 0..n {
                        let _ = write!(parts, "<number:text>{}</number:text>", esc(&c.to_string()));
                    }
                }
            }
        }
        return Some(format!(
            "<number:date-style style:name=\"{name}\">{parts}</number:date-style>"
        ));
    }
    let grouping = if format.contains(',') {
        " number:grouping=\"true\""
    } else {
        ""
    };
    if format.contains('%') {
        return Some(format!(
            "<number:percentage-style style:name=\"{name}\"><number:number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"1\"/><number:text>%</number:text></number:percentage-style>"
        ));
    }
    Some(format!(
        "<number:number-style style:name=\"{name}\"><number:number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"1\"{grouping}/></number:number-style>"
    ))
}

fn cell_style(name: &str, data: Option<&str>, s: &Style) -> String {
    let mut text = String::new();
    if s.bold {
        text.push_str(" fo:font-weight=\"bold\"");
    }
    if s.italic {
        text.push_str(" fo:font-style=\"italic\"");
    }
    if s.underline {
        text.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
    }
    if let Some(c) = s.color {
        let _ = write!(text, " fo:color=\"#{c:06x}\"");
    }
    let mut cell = String::new();
    if let Some(c) = s.fill {
        let _ = write!(cell, " fo:background-color=\"#{c:06x}\"");
    }
    if s.wrap {
        cell.push_str(" fo:wrap-option=\"wrap\"");
    }
    let mut out = format!(
        "<style:style style:name=\"{name}\" style:family=\"table-cell\" style:parent-style-name=\"Default\""
    );
    if let Some(d) = data {
        let _ = write!(out, " style:data-style-name=\"{d}\"");
    }
    out.push('>');
    if !cell.is_empty() {
        let _ = write!(out, "<style:table-cell-properties{cell}/>");
    }
    match s.align {
        Align::General => {}
        a => {
            let value = match a {
                Align::Left => "start",
                Align::Center => "center",
                _ => "end",
            };
            let _ = write!(
                out,
                "<style:paragraph-properties fo:text-align=\"{value}\"/>"
            );
        }
    }
    if !text.is_empty() {
        let _ = write!(out, "<style:text-properties{text}/>");
    }
    out.push_str("</style:style>");
    out
}

/// A cell's value as attributes and text, for a number format that may make it a date or a percentage.
fn cell_xml(
    value: &Value,
    format: &str,
    formula: Option<&str>,
    style: &str,
    span: (u32, u32),
) -> String {
    let lower = format.to_lowercase();
    let date_like = lower.contains('y') || (lower.contains('d') && !lower.contains('0'));
    let mut out = String::from("<table:table-cell");
    if !style.is_empty() {
        let _ = write!(out, " table:style-name=\"{style}\"");
    }
    if let Some(f) = formula {
        let _ = write!(out, " table:formula=\"{}\"", esc(&to_odf(f)));
    }
    if span.0 > 1 {
        let _ = write!(out, " table:number-rows-spanned=\"{}\"", span.0);
    }
    if span.1 > 1 {
        let _ = write!(out, " table:number-columns-spanned=\"{}\"", span.1);
    }
    let shown = if let Value::Number(_) = value {
        sheets_format::format_value(format, value)
    } else {
        value.to_string()
    };
    match value {
        Value::Number(n) if date_like && (0.0..2_958_466.0).contains(n) => {
            let (y, m, d) = date::from_serial(*n).unwrap_or((1900, 1, 1));
            let seconds = ((n - n.floor()) * 86_400.0).round() as i64;
            let stamp = if seconds == 0 {
                format!("{y:04}-{m:02}-{d:02}")
            } else {
                format!(
                    "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60
                )
            };
            let _ = write!(
                out,
                " office:value-type=\"date\" office:date-value=\"{stamp}\"><text:p>{}</text:p></table:table-cell>",
                esc(&shown)
            );
        }
        Value::Number(n) => {
            let kind = if format.contains('%') {
                "percentage"
            } else {
                "float"
            };
            let _ = write!(
                out,
                " office:value-type=\"{kind}\" office:value=\"{n}\"><text:p>{}</text:p></table:table-cell>",
                esc(&shown)
            );
        }
        Value::Bool(b) => {
            let _ = write!(
                out,
                " office:value-type=\"boolean\" office:boolean-value=\"{b}\"><text:p>{}</text:p></table:table-cell>",
                if *b { "TRUE" } else { "FALSE" }
            );
        }
        Value::Text(t) => {
            let lines: Vec<String> = t
                .split('\n')
                .map(|l| format!("<text:p>{}</text:p>", esc(l)))
                .collect();
            let _ = write!(
                out,
                " office:value-type=\"string\" office:string-value=\"{}\">{}</table:table-cell>",
                esc(t),
                lines.join("")
            );
        }
        Value::Error(e) => {
            let _ = write!(
                out,
                " office:value-type=\"string\" office:string-value=\"{}\"><text:p>{}</text:p></table:table-cell>",
                e.as_str(),
                e.as_str()
            );
        }
        Value::Empty => out.push_str("/>"),
    }
    out
}

/// The workbook as an `.ods` file.
///
/// # Errors
///
/// [`Error::Invalid`] when the file cannot be put together.
pub fn write(engine: &Engine) -> Result<Vec<u8>, Error> {
    let wb = engine.workbook();
    // One cell style (and one data style) for each distinct format.
    let mut data_styles: BTreeMap<String, String> = BTreeMap::new();
    let mut cell_styles: Vec<(u32, String)> = Vec::new();
    let mut style_xml = String::new();
    let mut data_xml = String::new();
    for (id, s) in wb.styles.iter().enumerate().skip(1) {
        let data = data_style(&format!("N{id}"), &s.number_format);
        if let Some(d) = &data {
            data_styles.insert(s.number_format.clone(), format!("N{id}"));
            data_xml.push_str(d);
        }
        let name = format!("ce{id}");
        style_xml.push_str(&cell_style(
            &name,
            data.map(|_| format!("N{id}")).as_deref(),
            s,
        ));
        cell_styles.push((u32::try_from(id).unwrap_or(0), name));
    }
    let mut body = String::new();
    for (si, sheet) in wb.sheets.iter().enumerate() {
        let _ = write!(body, "<table:table table:name=\"{}\">", esc(&sheet.name));
        let used = sheet.used_range();
        let (max_row, max_col) = used.map_or((0, 0), |r| (r.end.row, r.end.col));
        let styled_max = sheet
            .styled()
            .fold((0, 0), |(r, c), (at, _)| (r.max(at.row), c.max(at.col)));
        let (rows, cols) = (
            (max_row.max(styled_max.0)) + 1,
            (max_col.max(styled_max.1)) + 1,
        );
        let cols = cols
            .max(sheet.col_widths().map(|(c, _)| c + 1).max().unwrap_or(0))
            .max(sheet.hidden_cols().map(|c| c + 1).max().unwrap_or(0));
        for c in 0..cols {
            let width = sheet.col_width(c).unwrap_or(96.0);
            let hidden = if sheet.col_hidden(c) {
                " table:visibility=\"collapse\""
            } else {
                ""
            };
            let _ = write!(
                body,
                "<table:table-column table:style-name=\"co-{si}-{c}\"{hidden}/>"
            );
            let _ = write!(
                style_xml,
                "<style:style style:name=\"co-{si}-{c}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{}\"/></style:style>",
                cm(width)
            );
        }
        for r in 0..rows {
            let height = sheet.row_height(r);
            let hidden = if sheet.row_hidden(r) {
                " table:visibility=\"collapse\""
            } else {
                ""
            };
            if let Some(h) = height {
                let _ = write!(
                    body,
                    "<table:table-row table:style-name=\"ro-{si}-{r}\"{hidden}>"
                );
                let _ = write!(
                    style_xml,
                    "<style:style style:name=\"ro-{si}-{r}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}\" style:use-optimal-row-height=\"false\"/></style:style>",
                    cm(h)
                );
            } else {
                let _ = write!(body, "<table:table-row{hidden}>");
            }
            let mut c = 0;
            while c < cols {
                let at = sheets_model::CellRef::new(r, c);
                let merge = sheet.merge_at(at);
                if let Some(m) = merge.filter(|m| m.start != at) {
                    let _ = m;
                    body.push_str("<table:covered-table-cell/>");
                    c += 1;
                    continue;
                }
                let content = sheet.get(at);
                let style_id = sheet.style(at);
                let format = wb
                    .styles
                    .get(style_id as usize)
                    .map_or("General", |s| s.number_format.as_str());
                let style_name = cell_styles
                    .iter()
                    .find(|(i, _)| *i == style_id)
                    .map_or("", |(_, n)| n.as_str());
                let spanned = merge.map_or((1, 1), |m| (m.rows(), m.cols()));
                let value = engine.value(si, at);
                let formula = if let Some(Content::Formula(f)) = content {
                    Some(f.as_str())
                } else {
                    None
                };
                if content.is_none() && style_name.is_empty() && !engine.is_spilled(si, at) {
                    // A run of empty cells is one element.
                    let mut n = 1;
                    while c + n < cols {
                        let next = sheets_model::CellRef::new(r, c + n);
                        if sheet.get(next).is_some()
                            || sheet.style(next) != 0
                            || sheet.merge_at(next).is_some()
                            || engine.is_spilled(si, next)
                        {
                            break;
                        }
                        n += 1;
                    }
                    if n > 1 {
                        let _ = write!(
                            body,
                            "<table:table-cell table:number-columns-repeated=\"{n}\"/>"
                        );
                    } else {
                        body.push_str("<table:table-cell/>");
                    }
                    c += n;
                    continue;
                }
                body.push_str(&cell_xml(&value, format, formula, style_name, spanned));
                c += 1;
            }
            body.push_str("</table:table-row>");
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\" office:version=\"1.2\"><office:automatic-styles>{data_xml}{style_xml}</office:automatic-styles><office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>"
    );
    let mut pkg = Package::new();
    pkg.set(
        "mimetype",
        b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
    );
    pkg.set("content.xml", content.into_bytes());
    pkg.set(
        "META-INF/manifest.xml",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.spreadsheet\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>".to_vec(),
    );
    Ok(pkg.write()?)
}
