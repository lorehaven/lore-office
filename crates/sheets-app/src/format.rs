//! Number formats as a toolbar offers them: the presets, and one more or one fewer decimal place.

/// `(label, format code)` of the formats offered in the list.
pub const PRESETS: [(&str, &str); 13] = [
    ("General", "General"),
    ("Number", "0.00"),
    ("Integer", "0"),
    ("Thousands", "#,##0"),
    ("Thousands, 2 decimals", "#,##0.00"),
    ("Currency", "$#,##0.00"),
    ("Percent", "0%"),
    ("Percent, 2 decimals", "0.00%"),
    ("Scientific", "0.00E+00"),
    ("Date", "yyyy-mm-dd"),
    ("Time", "hh:mm:ss"),
    ("Date and time", "yyyy-mm-dd hh:mm"),
    ("Text", "@"),
];

/// A colour from a colour input (`#rrggbb`) as `0xRRGGBB`.
#[must_use]
pub fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// `0xRRGGBB` as `#rrggbb`.
#[must_use]
pub fn color_css(color: u32) -> String {
    format!("#{:06x}", color & 0xFF_FFFF)
}

/// The format with `delta` more (or fewer) decimal places in each section that shows digits.
///
/// `General` becomes `0.0` when places are added and stays as it is otherwise; formats without digits (dates, text) are not
/// changed.
#[must_use]
pub fn more_decimals(format: &str, delta: i32) -> String {
    if format.trim().is_empty() || format.eq_ignore_ascii_case("general") {
        return if delta > 0 {
            format!("0.{}", "0".repeat(delta.unsigned_abs() as usize))
        } else {
            "General".to_string()
        };
    }
    format
        .split(';')
        .map(|section| section_decimals(section, delta))
        .collect::<Vec<_>>()
        .join(";")
}

fn section_decimals(section: &str, delta: i32) -> String {
    let chars: Vec<char> = section.chars().collect();
    // Positions of digit placeholders and the decimal point, outside quotes, brackets and the exponent.
    let (mut quote, mut bracket, mut escape) = (false, false, false);
    let (mut digits, mut point): (Vec<usize>, Option<usize>) = (Vec::new(), None);
    for (i, c) in chars.iter().enumerate() {
        if escape {
            escape = false;
        } else if *c == '\\' || *c == '_' || *c == '*' {
            escape = true;
        } else if quote {
            quote = *c != '"';
        } else if bracket {
            bracket = *c != ']';
        } else if *c == '"' {
            quote = true;
        } else if *c == '[' {
            bracket = true;
        } else if matches!(c, 'E' | 'e') && matches!(chars.get(i + 1), Some('+' | '-')) {
            break;
        } else if matches!(c, '0' | '#' | '?') {
            digits.push(i);
        } else if *c == '.' && point.is_none() {
            point = Some(i);
        }
    }
    let Some(&last) = digits.last() else {
        return section.to_string();
    };
    let places = point.map_or(0, |p| digits.iter().filter(|d| **d > p).count());
    let wanted = (i64::try_from(places).unwrap_or(0) + i64::from(delta)).max(0);
    let wanted = usize::try_from(wanted).unwrap_or(0);
    if wanted == places {
        return section.to_string();
    }
    // With no point yet the places go right after the last digit; with one, the run of places after it is replaced.
    let head_end = point.unwrap_or(last + 1);
    let head: String = chars[..head_end].iter().collect();
    let tail: String = chars[last + 1..].iter().collect();
    let places_text = if wanted == 0 {
        String::new()
    } else {
        format!(".{}", "0".repeat(wanted))
    };
    format!("{head}{places_text}{tail}")
}
