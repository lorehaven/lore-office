use super::criteria::wild_at;
use super::{Out, arity, as_f64, flatten, index, sc, tx};
use crate::eval::Operand;
use sheets_model::{CellError, Value};

const MAX_TEXT: usize = 32_767;

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "LEN" => arity(a, 1, 1).and_then(|()| Ok(Value::Number(as_f64(tx(a, 0)?.chars().count())))),
        "UPPER" => map(a, str::to_uppercase),
        "LOWER" => map(a, str::to_lowercase),
        "TRIM" => map(a, |s| {
            s.split(' ')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        }),
        "PROPER" => map(a, proper),
        "CLEAN" => map(a, |s| s.chars().filter(|c| !c.is_control()).collect()),
        "CONCAT" => flatten(a)
            .iter()
            .map(Value::to_text)
            .collect::<Result<String, _>>()
            .map(Value::Text),
        "CONCATENATE" => (0..a.len())
            .map(|i| tx(a, i))
            .collect::<Result<String, _>>()
            .map(Value::Text),
        "TEXTJOIN" => textjoin(a),
        "LEFT" | "RIGHT" => left_right(name == "LEFT", a),
        "MID" => mid(a),
        "FIND" => find(a, true),
        "SEARCH" => find(a, false),
        "SUBSTITUTE" => substitute(a),
        "REPLACE" => replace(a),
        "REPT" => rept(a),
        "EXACT" => arity(a, 2, 2).and_then(|()| Ok(Value::Bool(tx(a, 0)? == tx(a, 1)?))),
        "VALUE" => value(a),
        "TEXT" => text_fn(a),
        "FIXED" => fixed(a),
        "DOLLAR" => dollar(a),
        "T" => arity(a, 1, 1).map(|()| match sc(a, 0) {
            Value::Text(t) => Value::Text(t),
            Value::Error(e) => Value::Error(e),
            _ => Value::Text(String::new()),
        }),
        "CHAR" => arity(a, 1, 1).and_then(|()| {
            let n = index(&sc(a, 0))?;
            if (1..=255).contains(&n) {
                Ok(Value::Text(
                    char::from(u8::try_from(n).unwrap_or(b'?')).to_string(),
                ))
            } else {
                Err(CellError::Value)
            }
        }),
        "UNICHAR" => arity(a, 1, 1).and_then(|()| {
            u32::try_from(index(&sc(a, 0))?)
                .ok()
                .and_then(char::from_u32)
                .filter(|c| *c != '\0')
                .map(|c| Value::Text(c.to_string()))
                .ok_or(CellError::Value)
        }),
        "CODE" | "UNICODE" => arity(a, 1, 1).and_then(|()| {
            tx(a, 0)?
                .chars()
                .next()
                .map(|c| Value::Number(f64::from(u32::from(c))))
                .ok_or(CellError::Value)
        }),
        _ => return None,
    })
}

fn map(a: &[Operand], f: fn(&str) -> String) -> Out {
    arity(a, 1, 1)?;
    Ok(Value::Text(f(&tx(a, 0)?)))
}

fn proper(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

fn textjoin(a: &[Operand]) -> Out {
    if a.len() < 3 {
        return Err(CellError::Value);
    }
    let delim = tx(a, 0)?;
    let ignore = sc(a, 1).to_bool()?;
    let mut parts = Vec::new();
    for v in flatten(&a[2..]) {
        let t = v.to_text()?;
        if !(ignore && t.is_empty()) {
            parts.push(t);
        }
    }
    Ok(Value::Text(parts.join(&delim)))
}

fn left_right(left: bool, a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let n = if a.len() == 2 { index(&sc(a, 1))? } else { 1 };
    let chars: Vec<char> = tx(a, 0)?.chars().collect();
    let n = n.min(chars.len());
    let part = if left {
        &chars[..n]
    } else {
        &chars[chars.len() - n..]
    };
    Ok(Value::Text(part.iter().collect()))
}

fn mid(a: &[Operand]) -> Out {
    arity(a, 3, 3)?;
    let (start, len) = (index(&sc(a, 1))?, index(&sc(a, 2))?);
    if start == 0 {
        return Err(CellError::Value);
    }
    Ok(Value::Text(
        tx(a, 0)?.chars().skip(start - 1).take(len).collect(),
    ))
}

fn find(a: &[Operand], exact: bool) -> Out {
    arity(a, 2, 3)?;
    let start = if a.len() == 3 { index(&sc(a, 2))? } else { 1 };
    let (pat, hay) = (tx(a, 0)?, tx(a, 1)?);
    let (pat, hay): (Vec<char>, Vec<char>) = if exact {
        (pat.chars().collect(), hay.chars().collect())
    } else {
        (
            pat.to_lowercase().chars().collect(),
            hay.to_lowercase().chars().collect(),
        )
    };
    if start == 0 || start > hay.len() + 1 {
        return Err(CellError::Value);
    }
    for i in start - 1..=hay.len() {
        let hit = if exact {
            hay[i..].starts_with(&pat)
        } else {
            wild_at(&pat, &hay, i, false).is_some()
        };
        if hit {
            return Ok(Value::Number(as_f64(i + 1)));
        }
    }
    Err(CellError::Value)
}

fn substitute(a: &[Operand]) -> Out {
    arity(a, 3, 4)?;
    let (text, old, new) = (tx(a, 0)?, tx(a, 1)?, tx(a, 2)?);
    if old.is_empty() {
        return Ok(Value::Text(text));
    }
    if a.len() == 3 {
        return Ok(Value::Text(text.replace(&old, &new)));
    }
    let which = index(&sc(a, 3))?;
    if which == 0 {
        return Err(CellError::Value);
    }
    let mut out = String::new();
    let (mut rest, mut seen) = (text.as_str(), 0);
    while let Some(i) = rest.find(&old) {
        seen += 1;
        out.push_str(&rest[..i]);
        out.push_str(if seen == which { &new } else { &old });
        rest = &rest[i + old.len()..];
        if seen == which {
            break;
        }
    }
    out.push_str(rest);
    Ok(Value::Text(out))
}

fn replace(a: &[Operand]) -> Out {
    arity(a, 4, 4)?;
    let (start, len) = (index(&sc(a, 1))?, index(&sc(a, 2))?);
    if start == 0 {
        return Err(CellError::Value);
    }
    let chars: Vec<char> = tx(a, 0)?.chars().collect();
    let head: String = chars.iter().take(start - 1).collect();
    let tail: String = chars.iter().skip(start - 1 + len).collect();
    Ok(Value::Text(format!("{head}{}{tail}", tx(a, 3)?)))
}

fn rept(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let (s, n) = (tx(a, 0)?, index(&sc(a, 1))?);
    if s.chars().count().saturating_mul(n) > MAX_TEXT {
        return Err(CellError::Value);
    }
    Ok(Value::Text(s.repeat(n)))
}

fn value(a: &[Operand]) -> Out {
    arity(a, 1, 1)?;
    match sc(a, 0) {
        Value::Text(t) => {
            let t = t.trim();
            if let Some(p) = t.strip_suffix('%') {
                return Value::Text(p.to_string())
                    .to_number()
                    .map(|n| Value::Number(n / 100.0));
            }
            Value::Text(t.to_string()).to_number().map(Value::Number)
        }
        other => other.to_number().map(Value::Number),
    }
}

/// `TEXT(value, format)`: numbers and text that reads as a number are formatted; other text stays as it is.
fn text_fn(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let format = sheets_format::Format::parse(&tx(a, 1)?).map_err(|_| CellError::Value)?;
    let v = match sc(a, 0) {
        Value::Empty => Value::Number(0.0),
        t @ Value::Text(_) => t.to_number().map_or(t, Value::Number),
        other => other,
    };
    if let Some(e) = v.error() {
        return Err(e);
    }
    Ok(Value::Text(format.render(&v).text))
}

/// Round to a number of digits (negative ones round to tens, hundreds, ...), half away from zero.
fn round_to(x: f64, digits: i32) -> f64 {
    let p = 10f64.powi(digits);
    format!("{:.14e}", x * p)
        .parse::<f64>()
        .unwrap_or(x * p)
        .round()
        / p
}

fn fixed(a: &[Operand]) -> Out {
    arity(a, 1, 3)?;
    let x = super::nm(a, 0)?;
    #[allow(clippy::cast_possible_truncation)]
    let d = super::nm_or(a, 1, 2.0)?.trunc().clamp(-100.0, 100.0) as i32;
    let plain = a.len() == 3 && sc(a, 2).to_bool()?;
    let places = usize::try_from(d.max(0)).unwrap_or(0);
    let fmt = format!(
        "{}{}",
        if plain { "0" } else { "#,##0" },
        if places > 0 {
            format!(".{}", "0".repeat(places))
        } else {
            String::new()
        }
    );
    let shown = if d < 0 { round_to(x, d) } else { x };
    let text = sheets_format::format_value(&fmt, &Value::Number(shown));
    Ok(Value::Text(text))
}

fn dollar(a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let x = super::nm(a, 0)?;
    #[allow(clippy::cast_possible_truncation)]
    let d = super::nm_or(a, 1, 2.0)?.trunc().clamp(-100.0, 100.0) as i32;
    let places = usize::try_from(d.max(0)).unwrap_or(0);
    let body = if places > 0 {
        format!("#,##0.{}", "0".repeat(places))
    } else {
        "#,##0".to_string()
    };
    let shown = if d < 0 { round_to(x, d) } else { x };
    Ok(Value::Text(sheets_format::format_value(
        &format!("${body};(${body})"),
        &Value::Number(shown),
    )))
}
