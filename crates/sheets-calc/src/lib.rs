//! The calculation engine of a workbook: it owns the cells, keeps the graph of which formula reads what, and when a cell
//! changes recomputes exactly the formulas that depend on it, in dependency order, and says which cells changed.
//!
//! ```
//! use sheets_calc::Engine;
//! use sheets_model::{CellRef, Content, Value, Workbook};
//!
//! let mut e = Engine::new(Workbook::new());
//! let a = |s| CellRef::parse(s).unwrap();
//! e.set(0, a("A1"), Content::Value(Value::Number(2.0)));
//! e.set(0, a("A2"), Content::Formula("A1*10".into()));
//! assert_eq!(e.value(0, a("A2")), Value::Number(20.0));
//! let changed = e.set(0, a("A1"), Content::Value(Value::Number(3.0)));
//! assert_eq!(e.value(0, a("A2")), Value::Number(30.0));
//! assert_eq!(changed.len(), 2); // A1 and A2
//! ```

mod engine;

pub use engine::{Engine, Key};
