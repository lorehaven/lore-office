//! Formulas as OpenDocument writes them (`of:=SUM([.A1:.B2];[$Other.C3])`) and as the sheets use them (`SUM(A1:B2,Other!C3)`).

use sheets_model::{CellRef, col_name};

fn quote_sheet(name: &str) -> String {
    if !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && CellRef::parse(name).is_none()
        && !name.starts_with(|c: char| c.is_ascii_digit())
    {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// One side of a reference: `[$Sheet.]$A$1`, `'My sheet'.A1` or `.A1` as (sheet, cell text).
fn split_part(part: &str) -> (Option<String>, String) {
    let part = part
        .strip_prefix('$')
        .filter(|p| p.starts_with(|c: char| c == '\'' || c.is_alphabetic() || c == '.'))
        .unwrap_or(part);
    if let Some(rest) = part.strip_prefix('\'') {
        // A quoted sheet name; `''` is a quote inside it.
        let mut name = String::new();
        let mut chars = rest.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '\'' {
                if chars.peek().is_some_and(|(_, n)| *n == '\'') {
                    name.push('\'');
                    chars.next();
                } else {
                    return (
                        Some(name),
                        rest[i + 1..].trim_start_matches('.').to_string(),
                    );
                }
            } else {
                name.push(c);
            }
        }
        return (Some(name), String::new());
    }
    match part.rfind('.') {
        Some(0) => (None, part[1..].to_string()),
        Some(i) => (Some(part[..i].to_string()), part[i + 1..].to_string()),
        None => (None, part.to_string()),
    }
}

/// A formula from a file, in the sheets' own syntax. Text that is not understood is returned as it was (minus the prefix).
#[must_use]
pub fn from_odf(text: &str) -> String {
    let body = text
        .split_once(":=")
        .map_or_else(|| text.trim_start_matches('='), |(_, b)| b);
    // Functions newer than the standard carry a vendor prefix.
    let body = body
        .replace("COM.MICROSOFT.", "")
        .replace("ORG.LIBREOFFICE.", "")
        .replace("ORG.OPENOFFICE.", "");
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(n) = chars.next() {
                    out.push(n);
                    if n == '"' {
                        if chars.peek() == Some(&'"') {
                            out.push('"');
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            '[' => {
                let mut inner = String::new();
                let mut quoted = false;
                for n in chars.by_ref() {
                    if n == '\'' {
                        quoted = !quoted;
                    }
                    if n == ']' && !quoted {
                        break;
                    }
                    inner.push(n);
                }
                let (mut a, mut b) = (inner.clone(), None);
                // The colon between the two ends is the first one outside quotes.
                let mut q = false;
                for (i, ch) in inner.char_indices() {
                    if ch == '\'' {
                        q = !q;
                    }
                    if ch == ':' && !q {
                        a = inner[..i].to_string();
                        b = Some(inner[i + 1..].to_string());
                        break;
                    }
                }
                let (sheet, first) = split_part(&a);
                if let Some(s) = &sheet {
                    out.push_str(&quote_sheet(s));
                    out.push('!');
                }
                out.push_str(&first);
                if let Some(b) = b {
                    out.push(':');
                    out.push_str(&split_part(&b).1);
                }
            }
            ';' => out.push(','),
            c => out.push(c),
        }
    }
    out
}

/// A formula for a file: its references in brackets, its arguments separated by semicolons, behind `of:=`.
#[must_use]
pub fn to_odf(text: &str) -> String {
    let printed = sheets_formula::parse(text)
        .map_or_else(|_| text.to_string(), |e| sheets_formula::print(&e));
    let chars: Vec<char> = printed.chars().collect();
    let mut out = String::from("of:=");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        out.push('"');
                        i += 1;
                    } else {
                        break;
                    }
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == ',' {
            out.push(';');
            i += 1;
            continue;
        }
        // A reference: an optional sheet and `!`, then a cell, then optionally `:` and a cell.
        if let Some((sheet, first, mut end)) = reference_at(&chars, i) {
            let mut text = String::from("[");
            if let Some(s) = &sheet {
                let _ = std::fmt::Write::write_fmt(
                    &mut text,
                    format_args!(
                        "${}",
                        if s.contains(' ') || s.contains('.') {
                            format!("'{}'", s.replace('\'', "''"))
                        } else {
                            s.clone()
                        }
                    ),
                );
            }
            text.push('.');
            text.push_str(&first);
            if chars.get(end) == Some(&':')
                && let Some((None, second, e2)) = reference_at(&chars, end + 1)
            {
                text.push_str(":.");
                text.push_str(&second);
                end = e2;
            }
            text.push(']');
            out.push_str(&text);
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A cell reference starting at `from` (with a sheet before it, when there is one): (sheet, cell text, where it ends).
fn reference_at(chars: &[char], from: usize) -> Option<(Option<String>, String, usize)> {
    let word_before = from > 0
        && (chars[from - 1].is_alphanumeric() || chars[from - 1] == '_' || chars[from - 1] == '.');
    if word_before {
        return None;
    }
    let mut i = from;
    let mut sheet = None;
    if chars.get(i) == Some(&'\'') {
        let mut name = String::new();
        i += 1;
        while i < chars.len() {
            if chars[i] == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    name.push('\'');
                    i += 2;
                    continue;
                }
                break;
            }
            name.push(chars[i]);
            i += 1;
        }
        if chars.get(i + 1) != Some(&'!') {
            return None;
        }
        sheet = Some(name);
        i += 2;
    } else {
        // A bare sheet name is followed by `!`.
        let mut j = i;
        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.')
        {
            j += 1;
        }
        if chars.get(j) == Some(&'!') && j > i {
            sheet = Some(chars[i..j].iter().collect());
            i = j + 1;
        }
    }
    let start = i;
    let mut cell = String::new();
    if chars.get(i) == Some(&'$') {
        cell.push('$');
        i += 1;
    }
    let letters = chars[i..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .count();
    if letters == 0 || letters > 3 {
        return None;
    }
    cell.extend(&chars[i..i + letters]);
    i += letters;
    if chars.get(i) == Some(&'$') {
        cell.push('$');
        i += 1;
    }
    let digits = chars[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    cell.extend(&chars[i..i + digits]);
    i += digits;
    // `LOG10(` and `A1B` are not references.
    if chars
        .get(i)
        .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '(' || *c == '.')
    {
        return None;
    }
    let _ = (start, col_name(0));
    Some((sheet, cell, i))
}
