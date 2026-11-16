//! An [`Expr`] back to formula text, and the shifting of references that copying and shared formulas need.

use crate::ast::{BinOp, Expr, RangeRef, Reference, UnOp};
use sheets_model::{CellError, CellRef, MAX_COLS, MAX_ROWS, col_name};

/// The text of a formula, without the leading `=`. Parsing it again gives the same tree.
#[must_use]
pub fn print(e: &Expr) -> String {
    let mut out = String::new();
    write(e, 0, &mut out);
    out
}

const fn prec(op: BinOp) -> u8 {
    match op {
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 1,
        BinOp::Concat => 2,
        BinOp::Add | BinOp::Sub => 3,
        BinOp::Mul | BinOp::Div => 4,
        BinOp::Pow => 5,
    }
}

const fn symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Pow => "^",
        BinOp::Concat => "&",
        BinOp::Eq => "=",
        BinOp::Ne => "<>",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
    }
}

/// `min` is the binding strength the context needs: a binary operator weaker than that gets parentheses.
fn write(e: &Expr, min: u8, out: &mut String) {
    match e {
        Expr::Number(n) => out.push_str(&n.to_string()),
        Expr::Text(s) => {
            out.push('"');
            out.push_str(&s.replace('"', "\"\""));
            out.push('"');
        }
        Expr::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
        Expr::Error(c) => out.push_str(c.as_str()),
        Expr::Name(n) => out.push_str(n),
        Expr::Ref(r) => {
            sheet(r.sheet.as_deref(), out);
            reference(r, out);
        }
        Expr::Range(r) => {
            sheet(r.sheet.as_deref(), out);
            reference(&r.start, out);
            out.push(':');
            reference(&r.end, out);
        }
        Expr::Unary(op, x) => {
            let (needs, p) = (min > 6, 6);
            if needs {
                out.push('(');
            }
            out.push(if *op == UnOp::Neg { '-' } else { '+' });
            write(x, p, out);
            if needs {
                out.push(')');
            }
        }
        Expr::Percent(x) => {
            write(x, 8, out);
            out.push('%');
        }
        Expr::Binary(op, l, r) => {
            let p = prec(*op);
            let needs = p < min;
            if needs {
                out.push('(');
            }
            write(l, p, out);
            out.push_str(symbol(*op));
            write(r, p + 1, out);
            if needs {
                out.push(')');
            }
        }
        Expr::Call(name, args) => {
            out.push_str(name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(a, 0, out);
            }
            out.push(')');
        }
    }
}

fn reference(r: &Reference, out: &mut String) {
    if r.col_abs {
        out.push('$');
    }
    out.push_str(&col_name(r.cell.col));
    if r.row_abs {
        out.push('$');
    }
    out.push_str(&(r.cell.row + 1).to_string());
}

fn sheet(name: Option<&str>, out: &mut String) {
    let Some(n) = name else { return };
    let plain = !n.is_empty()
        && n.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !n.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && CellRef::parse(n).is_none();
    if plain {
        out.push_str(n);
    } else {
        out.push('\'');
        out.push_str(&n.replace('\'', "''"));
        out.push('\'');
    }
    out.push('!');
}

impl Expr {
    /// The same formula as if copied `rows` down and `cols` right: references without `$` move, `$` ones stay. A
    /// reference pushed off the sheet becomes `#REF!`.
    #[must_use]
    pub fn shifted(&self, rows: i64, cols: i64) -> Self {
        match self {
            Self::Ref(r) => shift_ref(r, rows, cols).map_or(Self::Error(CellError::Ref), Self::Ref),
            Self::Range(r) => match (
                shift_ref(&r.start, rows, cols),
                shift_ref(&r.end, rows, cols),
            ) {
                (Some(start), Some(end)) => Self::Range(RangeRef {
                    sheet: r.sheet.clone(),
                    start,
                    end,
                }),
                _ => Self::Error(CellError::Ref),
            },
            Self::Unary(op, x) => Self::Unary(*op, Box::new(x.shifted(rows, cols))),
            Self::Percent(x) => Self::Percent(Box::new(x.shifted(rows, cols))),
            Self::Binary(op, l, r) => Self::Binary(
                *op,
                Box::new(l.shifted(rows, cols)),
                Box::new(r.shifted(rows, cols)),
            ),
            Self::Call(n, args) => Self::Call(
                n.clone(),
                args.iter().map(|a| a.shifted(rows, cols)).collect(),
            ),
            other => other.clone(),
        }
    }
}

impl Expr {
    /// The formula as if its cell were turned over the diagonal when moved from `from` to `to`: a reference that was `r` rows
    /// down and `c` columns right of the formula's cell is `c` rows down and `r` columns right of the new one. References
    /// with a `$` stay as they are.
    #[must_use]
    pub fn transposed(&self, from: CellRef, to: CellRef) -> Self {
        let turn = |r: &Reference| -> Option<Reference> {
            let (dr, dc) = (
                i64::from(r.cell.row) - i64::from(from.row),
                i64::from(r.cell.col) - i64::from(from.col),
            );
            let row = if r.row_abs {
                i64::from(r.cell.row)
            } else {
                i64::from(to.row) + dc
            };
            let col = if r.col_abs {
                i64::from(r.cell.col)
            } else {
                i64::from(to.col) + dr
            };
            let _ = (dr, dc);
            if !(0..i64::from(MAX_ROWS)).contains(&row) || !(0..i64::from(MAX_COLS)).contains(&col)
            {
                return None;
            }
            Some(Reference {
                cell: CellRef::new(u32::try_from(row).ok()?, u32::try_from(col).ok()?),
                ..r.clone()
            })
        };
        match self {
            Self::Ref(r) => turn(r).map_or(Self::Error(CellError::Ref), Self::Ref),
            Self::Range(r) => match (turn(&r.start), turn(&r.end)) {
                (Some(a), Some(b)) => {
                    // A range turned over keeps its corners in order.
                    let (lo, hi) = (
                        CellRef::new(a.cell.row.min(b.cell.row), a.cell.col.min(b.cell.col)),
                        CellRef::new(a.cell.row.max(b.cell.row), a.cell.col.max(b.cell.col)),
                    );
                    Self::Range(RangeRef {
                        sheet: r.sheet.clone(),
                        start: Reference { cell: lo, ..a },
                        end: Reference { cell: hi, ..b },
                    })
                }
                _ => Self::Error(CellError::Ref),
            },
            Self::Unary(op, x) => Self::Unary(*op, Box::new(x.transposed(from, to))),
            Self::Percent(x) => Self::Percent(Box::new(x.transposed(from, to))),
            Self::Binary(op, l, r) => Self::Binary(
                *op,
                Box::new(l.transposed(from, to)),
                Box::new(r.transposed(from, to)),
            ),
            Self::Call(n, args) => Self::Call(
                n.clone(),
                args.iter().map(|a| a.transposed(from, to)).collect(),
            ),
            other => other.clone(),
        }
    }
}

fn shift_ref(r: &Reference, rows: i64, cols: i64) -> Option<Reference> {
    let row = if r.row_abs {
        i64::from(r.cell.row)
    } else {
        i64::from(r.cell.row) + rows
    };
    let col = if r.col_abs {
        i64::from(r.cell.col)
    } else {
        i64::from(r.cell.col) + cols
    };
    if !(0..i64::from(MAX_ROWS)).contains(&row) || !(0..i64::from(MAX_COLS)).contains(&col) {
        return None;
    }
    Some(Reference {
        sheet: r.sheet.clone(),
        cell: CellRef::new(u32::try_from(row).ok()?, u32::try_from(col).ok()?),
        ..r.clone()
    })
}
