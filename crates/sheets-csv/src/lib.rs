//! CSV and TSV in and out of a sheet.
//!
//! Reading decides what each field is: numbers, `TRUE`/`FALSE`, ISO dates (`2024-02-29`) become values, everything else
//! stays text (so `007` and `+1 555 0100` are not damaged). A field starting with `=` stays text unless asked for, because
//! a file from elsewhere should not be able to run formulas. Writing gives what the cells show, like "Save as CSV".

use quench_convert::csv;
use sheets_calc::Engine;
use sheets_model::date::{MAX_SERIAL, month_len, to_serial};
use sheets_model::{CellRef, Content, MAX_COLS, MAX_ROWS, Sheet, Style, Value, Workbook};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub delimiter: u8,
    /// Turn numbers, booleans and ISO dates into values; otherwise everything is text.
    pub detect_types: bool,
    /// Read a field that starts with `=` as a formula.
    pub formulas: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            detect_types: true,
            formulas: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub delimiter: u8,
    /// Write numbers as their cells show them (`1,234.50`, `2024-02-29`) instead of raw.
    pub formatted: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            formatted: true,
        }
    }
}

/// A workbook of one sheet named `name` holding the fields. Dates found get a `yyyy-mm-dd` style.
pub fn read(text: &str, name: &str, options: &ReadOptions) -> Result<Workbook, Error> {
    let rows = csv::records(
        text.trim_start_matches('\u{feff}'),
        options.delimiter,
        false,
    )
    .map_err(|e| Error(e.to_string()))?;
    if rows.len() > MAX_ROWS as usize || rows.iter().any(|r| r.len() > MAX_COLS as usize) {
        return Err(Error("more rows or columns than a sheet holds".into()));
    }
    let mut wb = Workbook::new();
    let date = wb.style_id(&Style {
        number_format: "yyyy-mm-dd".into(),
        ..Default::default()
    });
    let mut sheet = Sheet::new(name);
    for (r, row) in rows.iter().enumerate() {
        for (c, field) in row.iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let at = CellRef::new(u32::try_from(r).unwrap_or(0), u32::try_from(c).unwrap_or(0));
            if options.formulas && field.starts_with('=') && field.len() > 1 {
                sheet.set(at, Content::Formula(field[1..].to_string()));
                continue;
            }
            let value = if options.detect_types {
                detect(field)
            } else {
                Value::Text(field.clone())
            };
            if matches!((&value, iso_date(field)), (Value::Number(_), Some(_))) {
                sheet.set_style(at, date);
            }
            sheet.set(at, Content::Value(value));
        }
    }
    wb.sheets = vec![sheet];
    Ok(wb)
}

fn detect(field: &str) -> Value {
    if let Some(serial) = iso_date(field) {
        return Value::Number(serial);
    }
    if field.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if field.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    let b = field.as_bytes();
    let digits = b.iter().filter(|c| c.is_ascii_digit()).count();
    let clean = digits > 0
        && b.iter()
            .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'));
    let leading_zero = field.trim_start_matches(['+', '-']).starts_with('0')
        && field.trim_start_matches(['+', '-']).len() > 1
        && !field.trim_start_matches(['+', '-']).starts_with("0.");
    if clean
        && !leading_zero
        && let Ok(n) = field.parse::<f64>()
        && n.is_finite()
    {
        return Value::Number(n);
    }
    Value::Text(field.to_string())
}

/// `2024-02-29` as a serial number; `None` for anything else, including dates that do not exist.
#[allow(clippy::many_single_char_names)]
fn iso_date(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let part = |r: std::ops::Range<usize>| {
        s.get(r)
            .filter(|t| t.bytes().all(|c| c.is_ascii_digit()))?
            .parse::<i64>()
            .ok()
    };
    let (y, m, d) = (part(0..4)?, part(5..7)?, part(8..10)?);
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > month_len(y, m) {
        return None;
    }
    let serial = to_serial(y, m, d);
    #[allow(clippy::cast_precision_loss)]
    (1..=MAX_SERIAL).contains(&serial).then_some(serial as f64)
}

/// The sheet as text: rows up to the last used one, columns up to the last used one, `\r\n`-free (`\n` ends a row).
#[must_use]
pub fn write(engine: &Engine, sheet: usize, options: &WriteOptions) -> String {
    let Some(used) = engine
        .workbook()
        .sheets
        .get(sheet)
        .and_then(Sheet::used_range)
    else {
        return String::new();
    };
    let wb = engine.workbook();
    let delim = char::from(options.delimiter);
    let mut out = String::new();
    for row in 0..=used.end.row {
        for col in 0..=used.end.col {
            if col > 0 {
                out.push(delim);
            }
            let at = CellRef::new(row, col);
            let v = engine.value(sheet, at);
            let text = match (&v, options.formatted) {
                (Value::Number(_), true) => {
                    sheets_format::format_value(wb.number_format(sheet, at), &v)
                }
                (Value::Error(e), _) => e.to_string(),
                _ => v.to_text().unwrap_or_default(),
            };
            quote(&text, delim, &mut out);
        }
        out.push('\n');
    }
    out
}

/// The records of delimited text, with quotes undone (a tab-separated clipboard, a `.csv`).
pub fn records(text: &str, delimiter: u8) -> Result<Vec<Vec<String>>, Error> {
    csv::records(text, delimiter, false).map_err(|e| Error(e.to_string()))
}

/// Append a field, quoted when it holds the delimiter, a quote or a line break.
pub fn quote(field: &str, delim: char, out: &mut String) {
    let needs =
        field.contains([delim, '"', '\n', '\r']) || field.starts_with(' ') || field.ends_with(' ');
    if needs {
        out.push('"');
        out.push_str(&field.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(field);
    }
}
