//! Functions that depend on where the formula is or when it runs: the clock, chance, and the cell's own address.

use super::{Out, int};
use crate::ast::Expr;
use crate::eval::{Context, operand};
use sheets_model::{CellError, Value};

/// Names of functions whose value can change without a cell they read changing.
pub const VOLATILE: [&str; 4] = ["NOW", "TODAY", "RAND", "RANDBETWEEN"];

pub(super) fn lazy(name: &str, args: &[Expr], ctx: &dyn Context) -> Option<Out> {
    Some(match name {
        "NOW" => none(args).map(|()| Value::Number(ctx.now())),
        "TODAY" => none(args).map(|()| Value::Number(ctx.now().floor())),
        "RAND" => none(args).map(|()| Value::Number(ctx.random())),
        "RANDBETWEEN" => between(args, ctx),
        "ROW" => position(args, ctx, true),
        "COLUMN" => position(args, ctx, false),
        _ => return None,
    })
}

const fn none(args: &[Expr]) -> Result<(), CellError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CellError::Value)
    }
}

fn between(args: &[Expr], ctx: &dyn Context) -> Out {
    let [lo, hi] = args else {
        return Err(CellError::Value);
    };
    let (lo, hi) = (
        int(&operand(lo, ctx).scalar())?,
        int(&operand(hi, ctx).scalar())?,
    );
    if hi < lo {
        return Err(CellError::Num);
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let pick = lo + (ctx.random() * ((hi - lo + 1) as f64)).floor() as i64;
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(pick.min(hi) as f64))
}

/// `ROW()` / `COLUMN()` of the cell the formula is in, or of the reference given.
fn position(args: &[Expr], ctx: &dyn Context, row: bool) -> Out {
    arity_exprs(args)?;
    let cell = match args.first() {
        None => ctx.origin().ok_or(CellError::Value)?,
        Some(Expr::Ref(r)) => r.cell,
        Some(Expr::Range(r)) => r.start.cell,
        Some(_) => return Err(CellError::Value),
    };
    Ok(Value::Number(f64::from(
        if row { cell.row } else { cell.col } + 1,
    )))
}

const fn arity_exprs(args: &[Expr]) -> Result<(), CellError> {
    if args.len() <= 1 {
        Ok(())
    } else {
        Err(CellError::Value)
    }
}
