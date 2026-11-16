//! The function library. Names are upper case; an unknown name is `#NAME?`, a wrong argument count is `#VALUE!`.
//! Each module answers `None` for names it does not own.

mod arrays;
mod context;
mod criteria;
mod date;
mod known;
mod logic;
mod lookup;
mod math;
mod more;
mod stats;
mod text;

use crate::ast::Expr;
use crate::eval::{Array, Context, Operand, operand};
use sheets_model::{CellError, Value};

pub use context::VOLATILE;
pub use known::{KNOWN, is_known};

pub(crate) type Out = Result<Value, CellError>;

pub(crate) fn call(name: &str, args: &[Expr], ctx: &dyn Context) -> Operand {
    if let Some(o) = arrays::lazy(name, args, ctx) {
        return o;
    }
    if let Some(v) = logic::lazy(name, args, ctx).or_else(|| context::lazy(name, args, ctx)) {
        return Operand::Scalar(v.unwrap_or_else(Value::Error));
    }
    let ops: Vec<Operand> = args.iter().map(|a| operand(a, ctx)).collect();
    if let Some(made) = arrays::eager(name, &ops) {
        return made.unwrap_or_else(|e| Operand::Scalar(Value::Error(e)));
    }
    Operand::Scalar(eager(name, &ops).unwrap_or_else(Value::Error))
}

pub(crate) fn eager(name: &str, a: &[Operand]) -> Out {
    math::call(name, a)
        .or_else(|| stats::call(name, a))
        .or_else(|| logic::call(name, a))
        .or_else(|| text::call(name, a))
        .or_else(|| date::call(name, a))
        .or_else(|| lookup::call(name, a))
        .or_else(|| more::call(name, a))
        .unwrap_or(Err(CellError::Name))
}

// ---- helpers shared by the modules ----

/// The argument count must be within `min..=max`.
pub(crate) const fn arity(a: &[Operand], min: usize, max: usize) -> Result<(), CellError> {
    if a.len() >= min && a.len() <= max {
        Ok(())
    } else {
        Err(CellError::Value)
    }
}

/// Argument `i` as one value (`Empty` when it is not there).
pub(crate) fn sc(a: &[Operand], i: usize) -> Value {
    a.get(i).map_or(Value::Empty, |o| o.clone().scalar())
}

pub(crate) fn nm(a: &[Operand], i: usize) -> Result<f64, CellError> {
    sc(a, i).to_number()
}

/// A number argument that may be left out.
pub(crate) fn nm_or(a: &[Operand], i: usize, default: f64) -> Result<f64, CellError> {
    if i < a.len() { nm(a, i) } else { Ok(default) }
}

pub(crate) fn tx(a: &[Operand], i: usize) -> Result<String, CellError> {
    sc(a, i).to_text()
}

pub(crate) const fn finite(n: f64) -> Out {
    if n.is_finite() {
        Ok(Value::Number(n))
    } else {
        Err(CellError::Num)
    }
}

#[allow(clippy::cast_precision_loss)]
pub(crate) const fn as_f64(n: usize) -> f64 {
    n as f64
}

/// A whole number argument: truncated toward zero.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn int(v: &Value) -> Result<i64, CellError> {
    let n = v.to_number()?.trunc();
    if n.abs() > 1e15 {
        Err(CellError::Num)
    } else {
        Ok(n as i64)
    }
}

/// A count or position that cannot be negative.
pub(crate) fn index(v: &Value) -> Result<usize, CellError> {
    usize::try_from(int(v)?).map_err(|_| CellError::Value)
}

pub(crate) fn flatten(a: &[Operand]) -> Vec<Value> {
    a.iter()
        .flat_map(|op| match op {
            Operand::Scalar(v) => vec![v.clone()],
            Operand::Array(Array { data, .. }) => data.clone(),
        })
        .collect()
}

/// An argument as a rectangle (a single value is one cell).
pub(crate) fn array(op: &Operand) -> Array {
    match op {
        Operand::Array(a) => a.clone(),
        Operand::Scalar(v) => Array {
            cols: 1,
            data: vec![v.clone()],
        },
    }
}

pub(crate) fn rows(a: &Array) -> usize {
    a.data.len().checked_div(a.cols).unwrap_or(0)
}

/// The numbers an aggregate sees: inside a range only numbers count (text, booleans and empty cells are skipped), but
/// a value given directly must be a number or read as one. An error anywhere is the result.
pub(crate) fn numbers(a: &[Operand]) -> Result<Vec<f64>, CellError> {
    let mut out = Vec::new();
    for op in a {
        match op {
            Operand::Scalar(v) => out.push(v.to_number()?),
            Operand::Array(arr) => {
                for v in &arr.data {
                    match v {
                        Value::Number(n) => out.push(*n),
                        Value::Error(e) => return Err(*e),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(out)
}
