//! Running an [`Expr`] against a cell source.

use crate::ast::{BinOp, Expr, UnOp};
use crate::functions;
use sheets_model::{CellError, CellRef, Range, Value};
use std::cmp::Ordering;

/// Where cell values come from. A reference to a sheet that does not exist should answer `Value::Error(CellError::Ref)`.
pub trait Context {
    /// The value at a cell: `sheet` is `None` for the formula's own sheet.
    fn cell(&self, sheet: Option<&str>, at: CellRef) -> Value;

    /// The date and time now as a serial number (days since 1899-12-30, the fraction the time of day), in the person's local
    /// time. The default is 0: a source with no clock has none to give.
    fn now(&self) -> f64 {
        0.0
    }

    /// A random number from 0 up to but not including 1. The default is always one half.
    fn random(&self) -> f64 {
        0.5
    }

    /// What a defined name stands for, as a formula, or `None` when there is no such name.
    fn name(&self, name: &str) -> Option<Expr> {
        let _ = name;
        None
    }

    /// The cell the formula being run is in, for `ROW()` and `COLUMN()`.
    fn origin(&self) -> Option<CellRef> {
        None
    }
}

/// The most cells one range reference may hold when it is read into a formula.
pub const MAX_RANGE_CELLS: u64 = 4 << 20;

/// A rectangle of values read from a range reference.
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    pub cols: usize,
    pub data: Vec<Value>,
}

/// What an expression yields while it is being evaluated: one value or a rectangle of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Scalar(Value),
    Array(Array),
}

impl Operand {
    /// One value: the top left of an array (a simplified "implicit intersection").
    #[must_use]
    pub fn scalar(self) -> Value {
        match self {
            Self::Scalar(v) => v,
            Self::Array(a) => a.data.into_iter().next().unwrap_or(Value::Empty),
        }
    }
}

/// Evaluate a formula to one value. Errors are values (`Value::Error`), never panics.
#[must_use]
pub fn eval(expr: &Expr, ctx: &dyn Context) -> Value {
    operand(expr, ctx).scalar()
}

/// Evaluate a formula to what it yields: one value or a rectangle of them.
#[must_use]
pub fn eval_operand(expr: &Expr, ctx: &dyn Context) -> Operand {
    operand(expr, ctx)
}

/// Apply `f` to the pairs of values of two operands: an array against a single value uses the value for each element, two
/// arrays go element by element (a missing element of the smaller is `#N/A`).
#[allow(clippy::many_single_char_names)]
fn zip(a: &Operand, b: &Operand, f: impl Fn(&Value, &Value) -> Value) -> Operand {
    match (a, b) {
        (Operand::Scalar(x), Operand::Scalar(y)) => Operand::Scalar(f(x, y)),
        (Operand::Array(m), Operand::Scalar(y)) => Operand::Array(Array {
            cols: m.cols,
            data: m.data.iter().map(|x| f(x, y)).collect(),
        }),
        (Operand::Scalar(x), Operand::Array(m)) => Operand::Array(Array {
            cols: m.cols,
            data: m.data.iter().map(|y| f(x, y)).collect(),
        }),
        (Operand::Array(m), Operand::Array(n)) => {
            let (mr, nr) = (m.data.len() / m.cols.max(1), n.data.len() / n.cols.max(1));
            // A single row or column stretches to match; otherwise the sizes must agree or the rest is #N/A.
            let (rows, cols) = (mr.max(nr), m.cols.max(n.cols));
            let at = |a: &Array, ar: usize, r: usize, c: usize| -> Value {
                let (r, c) = (if ar == 1 { 0 } else { r }, if a.cols == 1 { 0 } else { c });
                if r < ar && c < a.cols {
                    a.data[r * a.cols + c].clone()
                } else {
                    Value::Error(CellError::NA)
                }
            };
            let mut data = Vec::with_capacity(rows * cols);
            for r in 0..rows {
                for c in 0..cols {
                    let (x, y) = (at(m, mr, r, c), at(n, nr, r, c));
                    data.push(f(&x, &y));
                }
            }
            Operand::Array(Array { cols, data })
        }
    }
}

fn map(a: &Operand, f: impl Fn(&Value) -> Value) -> Operand {
    match a {
        Operand::Scalar(x) => Operand::Scalar(f(x)),
        Operand::Array(m) => Operand::Array(Array {
            cols: m.cols,
            data: m.data.iter().map(f).collect(),
        }),
    }
}

pub(crate) fn operand(expr: &Expr, ctx: &dyn Context) -> Operand {
    match expr {
        Expr::Number(n) => Operand::Scalar(Value::Number(*n)),
        Expr::Text(s) => Operand::Scalar(Value::Text(s.clone())),
        Expr::Bool(b) => Operand::Scalar(Value::Bool(*b)),
        Expr::Error(e) => Operand::Scalar(Value::Error(*e)),
        Expr::Name(n) => ctx
            .name(n)
            .map_or(Operand::Scalar(Value::Error(CellError::Name)), |e| {
                operand(&e, ctx)
            }),
        Expr::Ref(r) => Operand::Scalar(ctx.cell(r.sheet.as_deref(), r.cell)),
        Expr::Range(r) => {
            let range = Range::new(r.start.cell, r.end.cell);
            if u64::from(range.rows()) * u64::from(range.cols()) > MAX_RANGE_CELLS {
                return Operand::Scalar(Value::Error(CellError::Value));
            }
            let data = range
                .cells()
                .map(|c| ctx.cell(r.sheet.as_deref(), c))
                .collect();
            Operand::Array(Array {
                cols: range.cols() as usize,
                data,
            })
        }
        Expr::Unary(op, e) => map(&operand(e, ctx), |v| unary(*op, v)),
        Expr::Percent(e) => map(&operand(e, ctx), |v| match v.to_number() {
            Ok(n) => Value::Number(n / 100.0),
            Err(e) => Value::Error(e),
        }),
        Expr::Binary(op, a, b) => zip(&operand(a, ctx), &operand(b, ctx), |x, y| binary(*op, x, y)),
        Expr::Call(name, args) => functions::call(name, args, ctx),
    }
}

fn unary(op: UnOp, v: &Value) -> Value {
    match (op, v) {
        (UnOp::Plus, v) => v.clone(),
        (UnOp::Neg, v) => match v.to_number() {
            Ok(n) => Value::Number(-n),
            Err(e) => Value::Error(e),
        },
    }
}

const fn number(n: f64) -> Value {
    if n.is_finite() {
        Value::Number(n)
    } else {
        Value::Error(CellError::Num)
    }
}

fn binary(op: BinOp, a: &Value, b: &Value) -> Value {
    if let Some(e) = a.error().or_else(|| b.error()) {
        return Value::Error(e);
    }
    match op {
        BinOp::Concat => match (a.to_text(), b.to_text()) {
            (Ok(x), Ok(y)) => Value::Text(x + &y),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        },
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let o = compare(a, b);
            Value::Bool(match op {
                BinOp::Eq => o == Ordering::Equal,
                BinOp::Ne => o != Ordering::Equal,
                BinOp::Lt => o == Ordering::Less,
                BinOp::Le => o != Ordering::Greater,
                BinOp::Gt => o == Ordering::Greater,
                _ => o != Ordering::Less,
            })
        }
        _ => {
            let (x, y) = match (a.to_number(), b.to_number()) {
                (Ok(x), Ok(y)) => (x, y),
                (Err(e), _) | (_, Err(e)) => return Value::Error(e),
            };
            match op {
                BinOp::Add => number(x + y),
                BinOp::Sub => number(x - y),
                BinOp::Mul => number(x * y),
                BinOp::Div if y == 0.0 => Value::Error(CellError::Div0),
                BinOp::Div => number(x / y),
                _ if x == 0.0 && y == 0.0 => Value::Error(CellError::Num),
                _ => number(x.powf(y)),
            }
        }
    }
}

/// The order of two values as a sheet compares them: numbers, then text (ignoring case), then booleans; an empty cell
/// takes the type of the other side.
pub(crate) fn compare(a: &Value, b: &Value) -> Ordering {
    const fn rank(v: &Value) -> u8 {
        match v {
            Value::Number(_) | Value::Empty | Value::Error(_) => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
        }
    }
    let (a, b) = match (a, b) {
        (Value::Empty, Value::Text(_)) => (&Value::Text(String::new()), b),
        (Value::Text(_), Value::Empty) => (a, &Value::Text(String::new())),
        (Value::Empty, Value::Bool(_)) => (&Value::Bool(false), b),
        (Value::Bool(_), Value::Empty) => (a, &Value::Bool(false)),
        _ => (a, b),
    };
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ if rank(a) != rank(b) => rank(a).cmp(&rank(b)),
        _ => {
            let x = a.to_number().unwrap_or(0.0);
            let y = b.to_number().unwrap_or(0.0);
            x.partial_cmp(&y).unwrap_or(Ordering::Equal)
        }
    }
}
