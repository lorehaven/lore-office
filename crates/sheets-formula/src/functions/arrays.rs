//! Functions that give back a rectangle of values, and the ones that reach cells by name or position
//! (`FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`, `TRANSPOSE`, `VSTACK`, `INDIRECT`, `OFFSET`, `LET`).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::unnecessary_wraps,
    clippy::match_wildcard_for_single_variants,
    clippy::many_single_char_names
)]

use super::{array, index, nm_or, rows, sc};
use crate::ast::{Expr, RangeRef, Reference};
use crate::eval::{Array, Context, Operand, compare, operand};
use sheets_model::{CellError, CellRef, Value};
use std::cmp::Ordering;

type Made = Result<Operand, CellError>;

/// A rectangle as an operand; one cell is just a value.
fn made(cols: usize, data: Vec<Value>) -> Operand {
    if data.len() == 1 {
        Operand::Scalar(data.into_iter().next().unwrap_or_default())
    } else {
        Operand::Array(Array { cols, data })
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0,
        Value::Text(t) => t.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn row_of(a: &Array, r: usize) -> Vec<Value> {
    a.data[r * a.cols..(r + 1) * a.cols].to_vec()
}

fn col_of(a: &Array, c: usize) -> Vec<Value> {
    (0..rows(a))
        .map(|r| a.data[r * a.cols + c].clone())
        .collect()
}

/// Build an array from rows.
fn from_rows(list: Vec<Vec<Value>>, cols: usize) -> Made {
    if list.is_empty() || cols == 0 {
        return Err(CellError::NA);
    }
    Ok(made(cols, list.into_iter().flatten().collect()))
}

fn transposed(a: &Array) -> Array {
    let n = rows(a);
    let mut data = Vec::with_capacity(a.data.len());
    for c in 0..a.cols {
        for r in 0..n {
            data.push(a.data[r * a.cols + c].clone());
        }
    }
    Array { cols: n, data }
}

fn same_row(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            matches!(compare(x, y), Ordering::Equal)
                && std::mem::discriminant(x) == std::mem::discriminant(y)
        })
}

fn flag(a: &[Operand], i: usize) -> Result<bool, CellError> {
    if i >= a.len() {
        Ok(false)
    } else {
        Ok(truthy(&sc(a, i)) || sc(a, i).to_number().is_ok_and(|n| n != 0.0))
    }
}

fn filter(a: &[Operand]) -> Made {
    if a.len() < 2 || a.len() > 3 {
        return Err(CellError::Value);
    }
    let (data, include) = (array(&a[0]), array(&a[1]));
    let n = rows(&data);
    if include.cols == 1 && rows(&include) == n {
        let keep: Vec<Vec<Value>> = (0..n)
            .filter(|r| truthy(&include.data[*r]))
            .map(|r| row_of(&data, r))
            .collect();
        if keep.is_empty() {
            return if a.len() == 3 {
                Ok(a[2].clone())
            } else {
                Err(CellError::NA)
            };
        }
        from_rows(keep, data.cols)
    } else if rows(&include) == 1 && include.cols == data.cols {
        let cols: Vec<usize> = (0..data.cols)
            .filter(|c| truthy(&include.data[*c]))
            .collect();
        if cols.is_empty() {
            return if a.len() == 3 {
                Ok(a[2].clone())
            } else {
                Err(CellError::NA)
            };
        }
        let t = transposed(&data);
        let kept: Vec<Vec<Value>> = cols.iter().map(|c| row_of(&t, *c)).collect();
        let back = from_rows(kept, n)?;
        Ok(match back {
            Operand::Array(x) => Operand::Array(transposed(&x)),
            other => other,
        })
    } else {
        Err(CellError::Value)
    }
}

/// Sort the rows (or, `by_col`, the columns) by one or more keys: `(values per row, descending)`.
fn sort_by(data: &Array, keys: &[(Vec<Value>, bool)], by_col: bool) -> Made {
    let source = if by_col {
        transposed(data)
    } else {
        data.clone()
    };
    let n = rows(&source);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| {
        for (values, descending) in keys {
            let (a, b) = (&values[x], &values[y]);
            let o = match (matches!(a, Value::Empty), matches!(b, Value::Empty)) {
                (true, true) => Ordering::Equal,
                (true, false) => return Ordering::Greater,
                (false, true) => return Ordering::Less,
                _ => compare(a, b),
            };
            let o = if *descending { o.reverse() } else { o };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    let sorted: Vec<Vec<Value>> = order.into_iter().map(|r| row_of(&source, r)).collect();
    let out = from_rows(sorted, source.cols)?;
    Ok(match (out, by_col) {
        (Operand::Array(x), true) => Operand::Array(transposed(&x)),
        (other, _) => other,
    })
}

fn sort(a: &[Operand]) -> Made {
    if a.is_empty() || a.len() > 4 {
        return Err(CellError::Value);
    }
    let data = array(&a[0]);
    let by_col = flag(a, 3)?;
    let source = if by_col {
        transposed(&data)
    } else {
        data.clone()
    };
    let which = if a.len() > 1 { index(&sc(a, 1))? } else { 1 };
    let descending = a.len() > 2 && nm_or(a, 2, 1.0)? < 0.0;
    if which == 0 || which > source.cols {
        return Err(CellError::Value);
    }
    sort_by(&data, &[(col_of(&source, which - 1), descending)], by_col)
}

fn sortby(a: &[Operand]) -> Made {
    if a.len() < 2 {
        return Err(CellError::Value);
    }
    let data = array(&a[0]);
    let n = rows(&data);
    let mut keys = Vec::new();
    let mut i = 1;
    while i < a.len() {
        let by = array(&a[i]);
        if by.cols != 1 || rows(&by) != n {
            return Err(CellError::Value);
        }
        let descending = i + 1 < a.len() && nm_or(a, i + 1, 1.0)? < 0.0;
        keys.push((by.data, descending));
        i += 2;
    }
    sort_by(&data, &keys, false)
}

fn unique(a: &[Operand]) -> Made {
    if a.is_empty() || a.len() > 3 {
        return Err(CellError::Value);
    }
    let data = array(&a[0]);
    let by_col = flag(a, 1)?;
    let once = flag(a, 2)?;
    let source = if by_col { transposed(&data) } else { data };
    let n = rows(&source);
    let all: Vec<Vec<Value>> = (0..n).map(|r| row_of(&source, r)).collect();
    let mut kept: Vec<(Vec<Value>, usize)> = Vec::new();
    for row in all {
        match kept.iter_mut().find(|(k, _)| same_row(k, &row)) {
            Some((_, count)) => *count += 1,
            None => kept.push((row, 1)),
        }
    }
    let list: Vec<Vec<Value>> = kept
        .into_iter()
        .filter(|(_, c)| !once || *c == 1)
        .map(|(r, _)| r)
        .collect();
    let out = from_rows(list, source.cols)?;
    Ok(match (out, by_col) {
        (Operand::Array(x), true) => Operand::Array(transposed(&x)),
        (other, _) => other,
    })
}

fn sequence(a: &[Operand]) -> Made {
    if a.is_empty() || a.len() > 4 {
        return Err(CellError::Value);
    }
    let (r, c) = (
        index(&sc(a, 0))?,
        if a.len() > 1 { index(&sc(a, 1))? } else { 1 },
    );
    let (start, step) = (nm_or(a, 2, 1.0)?, nm_or(a, 3, 1.0)?);
    if r == 0 || c == 0 || r.saturating_mul(c) > 1_000_000 {
        return Err(CellError::Value);
    }
    let data = (0..r * c)
        .map(|i| Value::Number(step.mul_add(i as f64, start)))
        .collect();
    Ok(made(c, data))
}

fn transpose(a: &[Operand]) -> Made {
    if a.len() != 1 {
        return Err(CellError::Value);
    }
    let t = transposed(&array(&a[0]));
    Ok(made(t.cols, t.data))
}

fn stack(a: &[Operand], vertical: bool) -> Made {
    if a.is_empty() {
        return Err(CellError::Value);
    }
    let parts: Vec<Array> = a.iter().map(array).collect();
    if vertical {
        let cols = parts.iter().map(|p| p.cols).max().unwrap_or(1);
        let mut data = Vec::new();
        for p in &parts {
            for r in 0..rows(p) {
                for c in 0..cols {
                    data.push(if c < p.cols {
                        p.data[r * p.cols + c].clone()
                    } else {
                        Value::Error(CellError::NA)
                    });
                }
            }
        }
        Ok(made(cols, data))
    } else {
        let n = parts.iter().map(rows).max().unwrap_or(1);
        let cols: usize = parts.iter().map(|p| p.cols).sum();
        let mut data = Vec::new();
        for r in 0..n {
            for p in &parts {
                for c in 0..p.cols {
                    data.push(if r < rows(p) {
                        p.data[r * p.cols + c].clone()
                    } else {
                        Value::Error(CellError::NA)
                    });
                }
            }
        }
        Ok(made(cols, data))
    }
}

pub(super) fn eager(name: &str, a: &[Operand]) -> Option<Made> {
    Some(match name {
        "FILTER" => filter(a),
        "SORT" => sort(a),
        "SORTBY" => sortby(a),
        "UNIQUE" => unique(a),
        "SEQUENCE" => sequence(a),
        "TRANSPOSE" => transpose(a),
        "VSTACK" => stack(a, true),
        "HSTACK" => stack(a, false),
        _ => return None,
    })
}

// ---- reaching cells ----

/// A reference written as text (`B2`, `$A$1:C9`, `Sheet2!A1`) as an expression.
fn reference(text: &str) -> Option<Expr> {
    match crate::parser::parse(text.trim()).ok()? {
        e @ (Expr::Ref(_) | Expr::Range(_)) => Some(e),
        _ => None,
    }
}

/// Resolve what an argument names to a plain reference (following a defined name).
fn as_reference(e: &Expr, ctx: &dyn Context) -> Option<Expr> {
    match e {
        Expr::Ref(_) | Expr::Range(_) => Some(e.clone()),
        Expr::Name(n) => ctx.name(n).and_then(|inner| as_reference(&inner, ctx)),
        _ => None,
    }
}

fn indirect(args: &[Expr], ctx: &dyn Context) -> Operand {
    if args.is_empty() || args.len() > 2 {
        return Operand::Scalar(Value::Error(CellError::Value));
    }
    let text = match operand(&args[0], ctx).scalar().to_text() {
        Ok(t) => t,
        Err(e) => return Operand::Scalar(Value::Error(e)),
    };
    // R1C1 style is not supported.
    if args.len() == 2 && !truthy(&operand(&args[1], ctx).scalar()) {
        return Operand::Scalar(Value::Error(CellError::Ref));
    }
    reference(&text).map_or(Operand::Scalar(Value::Error(CellError::Ref)), |e| {
        operand(&e, ctx)
    })
}

fn offset(args: &[Expr], ctx: &dyn Context) -> Operand {
    let fail = |e| Operand::Scalar(Value::Error(e));
    if args.len() < 3 || args.len() > 5 {
        return fail(CellError::Value);
    }
    let Some(base) = as_reference(&args[0], ctx) else {
        return fail(CellError::Value);
    };
    let num = |i: usize| operand(&args[i], ctx).scalar().to_number().map(f64::trunc);
    let (dr, dc) = match (num(1), num(2)) {
        (Ok(r), Ok(c)) => (r as i64, c as i64),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let (sheet, first, rows_n, cols_n) = match &base {
        Expr::Ref(r) => (r.sheet.clone(), r.cell, 1_i64, 1_i64),
        Expr::Range(r) => {
            let (a, b) = (r.start.cell, r.end.cell);
            (
                r.sheet.clone().or_else(|| r.start.sheet.clone()),
                CellRef::new(a.row.min(b.row), a.col.min(b.col)),
                i64::from(a.row.abs_diff(b.row)) + 1,
                i64::from(a.col.abs_diff(b.col)) + 1,
            )
        }
        _ => return fail(CellError::Value),
    };
    let size = |i: usize, default: i64| -> Result<i64, CellError> {
        if args.len() > i {
            num(i).map(|v| v as i64)
        } else {
            Ok(default)
        }
    };
    let (h, w) = match (size(3, rows_n), size(4, cols_n)) {
        (Ok(h), Ok(w)) => (h, w),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let (top, left) = (i64::from(first.row) + dr, i64::from(first.col) + dc);
    if h < 1
        || w < 1
        || top < 0
        || left < 0
        || top + h > i64::from(sheets_model::MAX_ROWS)
        || left + w > i64::from(sheets_model::MAX_COLS)
    {
        return fail(CellError::Ref);
    }
    let cell = |r: i64, c: i64| Reference {
        sheet: sheet.clone(),
        cell: CellRef::new(u32::try_from(r).unwrap_or(0), u32::try_from(c).unwrap_or(0)),
        col_abs: false,
        row_abs: false,
    };
    let expr = if h == 1 && w == 1 {
        Expr::Ref(cell(top, left))
    } else {
        Expr::Range(RangeRef {
            sheet: sheet.clone(),
            start: cell(top, left),
            end: cell(top + h - 1, left + w - 1),
        })
    };
    operand(&expr, ctx)
}

/// `LET(name, value, ..., result)`: the names are replaced by their values in what follows.
fn let_fn(args: &[Expr], ctx: &dyn Context) -> Operand {
    if args.len() < 3 || args.len().is_multiple_of(2) {
        return Operand::Scalar(Value::Error(CellError::Value));
    }
    let mut bound: Vec<(String, Expr)> = Vec::new();
    for pair in args[..args.len() - 1].chunks(2) {
        let Expr::Name(name) = &pair[0] else {
            return Operand::Scalar(Value::Error(CellError::Value));
        };
        let mut value = pair[1].clone();
        for (n, v) in &bound {
            substitute(&mut value, n, v);
        }
        bound.push((name.clone(), value));
    }
    let mut result = args[args.len() - 1].clone();
    for (n, v) in bound.iter().rev() {
        substitute(&mut result, n, v);
    }
    operand(&result, ctx)
}

fn substitute(e: &mut Expr, name: &str, with: &Expr) {
    match e {
        Expr::Name(n) if n.eq_ignore_ascii_case(name) => *e = with.clone(),
        Expr::Unary(_, a) | Expr::Percent(a) => substitute(a, name, with),
        Expr::Binary(_, a, b) => {
            substitute(a, name, with);
            substitute(b, name, with);
        }
        Expr::Call(_, args) => args.iter_mut().for_each(|a| substitute(a, name, with)),
        _ => {}
    }
}

pub(super) fn lazy(name: &str, args: &[Expr], ctx: &dyn Context) -> Option<Operand> {
    Some(match name {
        "INDIRECT" => indirect(args, ctx),
        "OFFSET" => offset(args, ctx),
        "LET" => let_fn(args, ctx),
        _ => return None,
    })
}
