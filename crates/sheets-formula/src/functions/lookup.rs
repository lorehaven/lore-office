use super::criteria::wild_eq;
use super::{Out, arity, array, as_f64, index, nm_or, rows, sc};
use crate::eval::{Operand, compare};
use sheets_model::{CellError, Value};
use std::cmp::Ordering;

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "VLOOKUP" => table_lookup(a, true),
        "HLOOKUP" => table_lookup(a, false),
        "INDEX" => index_fn(a),
        "MATCH" => match_fn(a),
        "XLOOKUP" => xlookup(a),
        "CHOOSE" => choose(a),
        "ROWS" => arity(a, 1, 1).map(|()| Value::Number(as_f64(rows(&array(&a[0]))))),
        "COLUMNS" => arity(a, 1, 1).map(|()| Value::Number(as_f64(array(&a[0]).cols))),
        _ => return None,
    })
}

const fn kind(v: &Value) -> u8 {
    match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Empty => 3,
        Value::Error(_) => 4,
    }
}

/// Equal and of the same kind (a number never equals text); text ignores case and may hold `*`/`?` when `wild`.
fn same(a: &Value, b: &Value, wild: bool) -> bool {
    match (a, b) {
        (Value::Text(p), Value::Text(t)) => {
            if wild {
                wild_eq(p, t)
            } else {
                p.to_lowercase() == t.to_lowercase()
            }
        }
        _ => kind(a) == kind(b) && kind(a) < 3 && compare(a, b) == Ordering::Equal,
    }
}

/// Largest position whose value is `<= key` (or `>=` with `down`), scanning the way a sorted list is searched.
fn approx(list: &[Value], key: &Value, descending: bool) -> Option<usize> {
    let mut best = None;
    for (i, v) in list.iter().enumerate() {
        if kind(v) != kind(key) || kind(v) > 2 {
            continue;
        }
        let o = compare(v, key);
        let ok = if descending {
            o != Ordering::Less
        } else {
            o != Ordering::Greater
        };
        if ok {
            best = Some(i);
        } else {
            break;
        }
    }
    best
}

fn table_lookup(a: &[Operand], vertical: bool) -> Out {
    arity(a, 3, 4)?;
    let key = sc(a, 0);
    if let Some(e) = key.error() {
        return Err(e);
    }
    let t = array(&a[1]);
    let n = index(&sc(a, 2))?;
    let exact = nm_or(a, 3, 1.0)? == 0.0 && a.len() == 4;
    let (lines, depth) = if vertical {
        (rows(&t), t.cols)
    } else {
        (t.cols, rows(&t))
    };
    if n == 0 {
        return Err(CellError::Value);
    }
    if n > depth {
        return Err(CellError::Ref);
    }
    let at = |line: usize, k: usize| {
        if vertical {
            &t.data[line * t.cols + k]
        } else {
            &t.data[k * t.cols + line]
        }
    };
    let firsts: Vec<Value> = (0..lines).map(|l| at(l, 0).clone()).collect();
    let hit = if exact {
        firsts.iter().position(|v| same(&key, v, true))
    } else {
        approx(&firsts, &key, false)
    };
    hit.map(|l| at(l, n - 1).clone()).ok_or(CellError::NA)
}

fn index_fn(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let t = array(&a[0]);
    let r = rows(&t);
    let (row, col) = match (a.len(), r, t.cols) {
        (2, 1, _) => (1, index(&sc(a, 1))?),
        (2, _, _) => (index(&sc(a, 1))?, 1),
        _ => (index(&sc(a, 1))?, index(&sc(a, 2))?),
    };
    if row == 0 || col == 0 {
        return Err(CellError::Value);
    }
    if row > r || col > t.cols {
        return Err(CellError::Ref);
    }
    Ok(t.data[(row - 1) * t.cols + col - 1].clone())
}

fn vector(op: &Operand) -> Result<Vec<Value>, CellError> {
    let t = array(op);
    if t.cols == 1 || rows(&t) == 1 {
        Ok(t.data)
    } else {
        Err(CellError::NA)
    }
}

fn match_fn(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let key = sc(a, 0);
    if let Some(e) = key.error() {
        return Err(e);
    }
    let list = vector(&a[1])?;
    let kind_ = nm_or(a, 2, 1.0)?;
    let hit = if kind_ == 0.0 {
        list.iter().position(|v| same(&key, v, true))
    } else {
        approx(&list, &key, kind_ < 0.0)
    };
    hit.map(|i| Value::Number(as_f64(i + 1)))
        .ok_or(CellError::NA)
}

fn xlookup(a: &[Operand]) -> Out {
    arity(a, 3, 6)?;
    let key = sc(a, 0);
    if let Some(e) = key.error() {
        return Err(e);
    }
    let (look, ret) = (vector(&a[1])?, vector(&a[2])?);
    if look.len() != ret.len() {
        return Err(CellError::Value);
    }
    let mode = nm_or(a, 4, 0.0)?;
    let backwards = nm_or(a, 5, 1.0)? < 0.0;
    let order: Vec<usize> = if backwards {
        (0..look.len()).rev().collect()
    } else {
        (0..look.len()).collect()
    };
    let exact = order
        .iter()
        .copied()
        .find(|&i| same(&key, &look[i], (mode - 2.0).abs() < f64::EPSILON));
    let hit = exact.or_else(|| match mode {
        m if m < 0.0 => nearest(&look, &key, &order, Ordering::Less),
        m if m > 0.0 && m < 2.0 => nearest(&look, &key, &order, Ordering::Greater),
        _ => None,
    });
    match hit {
        Some(i) => Ok(ret[i].clone()),
        None if a.len() >= 4 => Ok(sc(a, 3)),
        None => Err(CellError::NA),
    }
}

/// The closest value on one side of the key (`Less`: the largest smaller one; `Greater`: the smallest larger one).
fn nearest(list: &[Value], key: &Value, order: &[usize], side: Ordering) -> Option<usize> {
    let mut best: Option<usize> = None;
    for &i in order {
        let v = &list[i];
        if kind(v) != kind(key) || kind(v) > 2 || compare(v, key) != side {
            continue;
        }
        if best.is_none_or(|b| compare(v, &list[b]) == side.reverse()) {
            best = Some(i);
        }
    }
    best
}

fn choose(a: &[Operand]) -> Out {
    if a.len() < 2 {
        return Err(CellError::Value);
    }
    let i = index(&sc(a, 0))?;
    if i == 0 || i >= a.len() {
        return Err(CellError::Value);
    }
    Ok(sc(a, i))
}
