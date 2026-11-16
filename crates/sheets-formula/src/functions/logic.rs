use super::{Out, arity, sc};
use crate::ast::Expr;
use crate::eval::{Context, Operand, compare, operand};
use sheets_model::{CellError, Value};
use std::cmp::Ordering;

/// Functions that must not evaluate all their arguments: only the branch taken runs.
pub(super) fn lazy(name: &str, args: &[Expr], ctx: &dyn Context) -> Option<Out> {
    let ev = |e: &Expr| operand(e, ctx).scalar();
    Some(match name {
        "IF" => {
            if !(2..=3).contains(&args.len()) {
                return Some(Err(CellError::Value));
            }
            ev(&args[0]).to_bool().map(|c| match (c, args.get(2)) {
                (true, _) => ev(&args[1]),
                (false, Some(e)) => ev(e),
                (false, None) => Value::Bool(false),
            })
        }
        "IFERROR" | "IFNA" => {
            let [v, alt] = args else {
                return Some(Err(CellError::Value));
            };
            Ok(match ev(v) {
                Value::Error(e) if name == "IFERROR" || e == CellError::NA => ev(alt),
                ok => ok,
            })
        }
        "IFS" => {
            if args.is_empty() || args.len() % 2 == 1 {
                return Some(Err(CellError::Value));
            }
            for pair in args.chunks(2) {
                match ev(&pair[0]).to_bool() {
                    Ok(true) => return Some(Ok(ev(&pair[1]))),
                    Ok(false) => {}
                    Err(e) => return Some(Err(e)),
                }
            }
            Err(CellError::NA)
        }
        "SWITCH" => {
            if args.len() < 3 {
                return Some(Err(CellError::Value));
            }
            let subject = ev(&args[0]);
            if let Some(e) = subject.error() {
                return Some(Err(e));
            }
            let mut rest = &args[1..];
            while rest.len() >= 2 {
                let candidate = ev(&rest[0]);
                if candidate.error().is_none()
                    && compare(&subject, &candidate) == Ordering::Equal
                    && same_kind(&subject, &candidate)
                {
                    return Some(Ok(ev(&rest[1])));
                }
                rest = &rest[2..];
            }
            match rest {
                [default] => Ok(ev(default)),
                _ => Err(CellError::NA),
            }
        }
        _ => return None,
    })
}

fn same_kind(a: &Value, b: &Value) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "AND" => logic(a, |acc, b| acc && b, true),
        "OR" => logic(a, |acc, b| acc || b, false),
        "XOR" => logic(a, |acc, b| acc != b, false),
        "NOT" => arity(a, 1, 1).and_then(|()| Ok(Value::Bool(!sc(a, 0).to_bool()?))),
        "TRUE" => arity(a, 0, 0).map(|()| Value::Bool(true)),
        "FALSE" => arity(a, 0, 0).map(|()| Value::Bool(false)),
        "NA" => Err(CellError::NA),
        "ISNUMBER" => is(a, |v| matches!(v, Value::Number(_))),
        "ISTEXT" => is(a, |v| matches!(v, Value::Text(_))),
        "ISNONTEXT" => is(a, |v| !matches!(v, Value::Text(_))),
        "ISLOGICAL" => is(a, |v| matches!(v, Value::Bool(_))),
        "ISBLANK" => is(a, Value::is_empty),
        "ISERROR" => is(a, |v| v.error().is_some()),
        "ISERR" => is(a, |v| v.error().is_some_and(|e| e != CellError::NA)),
        "ISNA" => is(a, |v| v.error() == Some(CellError::NA)),
        "ISEVEN" | "ISODD" => arity(a, 1, 1).and_then(|()| {
            let n = sc(a, 0).to_number()?.trunc();
            Ok(Value::Bool(
                ((n % 2.0).abs() < f64::EPSILON) == (name == "ISEVEN"),
            ))
        }),
        "N" => arity(a, 1, 1).map(|()| match sc(a, 0) {
            Value::Number(n) => Value::Number(n),
            Value::Bool(b) => Value::Number(f64::from(u8::from(b))),
            Value::Error(e) => Value::Error(e),
            _ => Value::Number(0.0),
        }),
        _ => return None,
    })
}

fn is(a: &[Operand], f: fn(&Value) -> bool) -> Out {
    arity(a, 1, 1)?;
    Ok(Value::Bool(f(&sc(a, 0))))
}

/// `AND`, `OR`, `XOR`: inside a range, text and empty cells are ignored; nothing left is `#VALUE!`.
fn logic(a: &[Operand], f: fn(bool, bool) -> bool, init: bool) -> Out {
    let mut seen = false;
    let mut acc = init;
    for op in a {
        let items: Vec<Value> = match op {
            Operand::Scalar(v) => vec![v.clone()],
            Operand::Array(arr) => arr
                .data
                .iter()
                .filter(|v| !matches!(v, Value::Text(_) | Value::Empty))
                .cloned()
                .collect(),
        };
        for v in items {
            let b = v.to_bool()?;
            seen = true;
            acc = f(acc, b);
        }
    }
    if seen {
        Ok(Value::Bool(acc))
    } else {
        Err(CellError::Value)
    }
}
