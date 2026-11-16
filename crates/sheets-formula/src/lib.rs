//! The formula language of the sheets: a parser for Excel-style formulas and an evaluator over a cell source.
//!
//! `parse("SUM(A1:A3)*2")` gives an [`Expr`]; [`eval`] runs it against anything that implements [`Context`].

pub mod ast;
pub mod eval;
pub mod functions;
pub mod parser;
pub mod printer;

pub use ast::{BinOp, Expr, RangeRef, Reference, UnOp};
pub use eval::{Array, Context, Operand, eval, eval_operand};
pub use functions::{KNOWN, VOLATILE, is_known};
pub use parser::{ParseError, parse};
pub use printer::print;
