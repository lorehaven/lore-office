//! What a person types into a cell, and what they see when they edit it.

use sheets_model::value::number_text;
use sheets_model::{CellError, Content, Value};

/// A cell's content from typed text: `=...` is a formula, `TRUE`/`FALSE` a boolean, a number a number, a leading
/// apostrophe forces text, an error name an error, nothing clears the cell, anything else is text.
#[must_use]
pub fn parse_input(text: &str) -> Content {
    if text.is_empty() {
        return Content::Value(Value::Empty);
    }
    if let Some(f) = text.strip_prefix('=')
        && !f.trim().is_empty()
    {
        return Content::Formula(f.to_string());
    }
    if let Some(t) = text.strip_prefix('\'') {
        return Content::Value(Value::Text(t.to_string()));
    }
    if text.eq_ignore_ascii_case("true") {
        return Content::Value(Value::Bool(true));
    }
    if text.eq_ignore_ascii_case("false") {
        return Content::Value(Value::Bool(false));
    }
    if let Some(e) = CellError::parse(text) {
        return Content::Value(Value::Error(e));
    }
    let t = text.trim();
    if let Some(p) = t.strip_suffix('%')
        && let Some(n) = number(p)
    {
        return Content::Value(Value::Number(n / 100.0));
    }
    number(t).map_or_else(
        || Content::Value(Value::Text(text.to_string())),
        |n| Content::Value(Value::Number(n)),
    )
}

fn number(t: &str) -> Option<f64> {
    let t = t.strip_prefix('$').unwrap_or(t);
    let grouped = thousands(t);
    let t = grouped.as_deref().unwrap_or(t);
    let digits = t.bytes().filter(u8::is_ascii_digit).count();
    let clean = digits > 0
        && t.bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'));
    if !clean {
        return None;
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// `1,234,567.5` as `1234567.5`; `None` unless the commas are thousands separators (groups of three) or there are none.
fn thousands(t: &str) -> Option<String> {
    if !t.contains(',') {
        return None;
    }
    let (sign, rest) = t.strip_prefix(['-', '+']).map_or(("", t), |r| (&t[..1], r));
    let (int, frac) = rest.split_once('.').map_or((rest, ""), |(i, f)| (i, f));
    let mut groups = int.split(',');
    let first = groups.next()?;
    let ok = (1..=3).contains(&first.len())
        && first.bytes().all(|b| b.is_ascii_digit())
        && groups.all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()));
    ok.then(|| {
        format!(
            "{sign}{}{}",
            int.replace(',', ""),
            if frac.is_empty() && !rest.contains('.') {
                String::new()
            } else {
                format!(".{frac}")
            }
        )
    })
}

/// The text to edit for a cell: what [`parse_input`] turns back into the same content.
#[must_use]
pub fn edit_text(content: Option<&Content>) -> String {
    match content {
        None | Some(Content::Value(Value::Empty)) => String::new(),
        Some(Content::Formula(f)) => format!("={f}"),
        Some(Content::Value(Value::Number(n))) => number_text(*n),
        Some(Content::Value(Value::Bool(b))) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Some(Content::Value(Value::Error(e))) => e.to_string(),
        Some(Content::Value(Value::Text(t))) => {
            // Text that would read as something else needs the apostrophe to stay text.
            if parse_input(t) == Content::Value(Value::Text(t.clone())) {
                t.clone()
            } else {
                format!("'{t}")
            }
        }
    }
}
