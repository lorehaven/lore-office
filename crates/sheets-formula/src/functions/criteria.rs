//! The conditions of `COUNTIF` and friends: `5`, `">3"`, `"<>x"`, `"a*"`.

use super::array;
use crate::eval::Operand;
use sheets_model::{CellError, Value};
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

pub(super) struct Criteria {
    op: Op,
    operand: Value,
}

impl Criteria {
    pub(super) fn parse(c: &Value) -> Self {
        let Value::Text(t) = c else {
            return Self {
                op: Op::Eq,
                operand: c.clone(),
            };
        };
        let (op, rest) = ["<=", ">=", "<>", "<", ">", "="]
            .iter()
            .find_map(|p| t.strip_prefix(p).map(|r| (*p, r)))
            .map_or((Op::Eq, t.as_str()), |(p, r)| {
                let op = match p {
                    "<=" => Op::Le,
                    ">=" => Op::Ge,
                    "<>" => Op::Ne,
                    "<" => Op::Lt,
                    ">" => Op::Gt,
                    _ => Op::Eq,
                };
                (op, r)
            });
        let operand = if let Ok(n) = rest.trim().parse::<f64>()
            && n.is_finite()
        {
            Value::Number(n)
        } else if rest.eq_ignore_ascii_case("true") {
            Value::Bool(true)
        } else if rest.eq_ignore_ascii_case("false") {
            Value::Bool(false)
        } else if let Some(e) = CellError::parse(rest) {
            Value::Error(e)
        } else {
            Value::Text(rest.to_string())
        };
        Self { op, operand }
    }

    pub(super) fn matches(&self, v: &Value) -> bool {
        let ord = match (&self.operand, v) {
            (Value::Number(x), Value::Number(y)) => y.partial_cmp(x),
            (Value::Text(p), Value::Text(t)) => {
                if matches!(self.op, Op::Eq | Op::Ne) {
                    let hit = wild_eq(p, t);
                    return hit == (self.op == Op::Eq);
                }
                Some(t.to_lowercase().cmp(&p.to_lowercase()))
            }
            (Value::Bool(x), Value::Bool(y)) => Some(y.cmp(x)),
            (Value::Error(x), Value::Error(y)) => Some(if x == y {
                Ordering::Equal
            } else {
                Ordering::Less
            }),
            (Value::Text(p), Value::Empty) if p.is_empty() => Some(Ordering::Equal),
            _ => None,
        };
        ord.map_or(self.op == Op::Ne, |o| match self.op {
            Op::Eq => o == Ordering::Equal,
            Op::Ne => o != Ordering::Equal,
            Op::Lt => o == Ordering::Less,
            Op::Le => o != Ordering::Greater,
            Op::Gt => o == Ordering::Greater,
            Op::Ge => o != Ordering::Less,
        })
    }
}

/// Positions where every `(range, criteria)` pair holds. The ranges must all be the same size.
pub(super) fn matching(pairs: &[Operand]) -> Result<Vec<usize>, CellError> {
    let mut out: Option<Vec<usize>> = None;
    let mut len = None;
    for pair in pairs.chunks(2) {
        let range = array(&pair[0]);
        if *len.get_or_insert(range.data.len()) != range.data.len() {
            return Err(CellError::Value);
        }
        let c = Criteria::parse(&pair[1].clone().scalar());
        let here: Vec<usize> = range
            .data
            .iter()
            .enumerate()
            .filter(|(_, v)| c.matches(v))
            .map(|(i, _)| i)
            .collect();
        out = Some(match out {
            None => here,
            Some(prev) => prev.into_iter().filter(|i| here.contains(i)).collect(),
        });
    }
    Ok(out.unwrap_or_default())
}

/// Whole-text match, ignoring case, with `*` (any run), `?` (one character) and `~` to escape them.
pub(super) fn wild_eq(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    wild_at(&p, &t, 0, true).is_some_and(|end| end == t.len())
}

/// Whether `pattern` matches a prefix of `text[from..]`; `whole` makes it prefer the longest. Returns where it ends.
pub(super) fn wild_at(p: &[char], t: &[char], from: usize, whole: bool) -> Option<usize> {
    fn go(p: &[char], t: &[char], i: usize, whole: bool) -> Option<usize> {
        let Some(&c) = p.first() else { return Some(i) };
        match c {
            '*' => {
                let range: Box<dyn Iterator<Item = usize>> = if whole {
                    Box::new((i..=t.len()).rev())
                } else {
                    Box::new(i..=t.len())
                };
                for j in range {
                    if let Some(e) = go(&p[1..], t, j, whole) {
                        return Some(e);
                    }
                }
                None
            }
            '?' => (i < t.len())
                .then(|| go(&p[1..], t, i + 1, whole))
                .flatten(),
            '~' if p.len() > 1 => (t.get(i) == Some(&p[1]))
                .then(|| go(&p[2..], t, i + 1, whole))
                .flatten(),
            c => (t.get(i) == Some(&c))
                .then(|| go(&p[1..], t, i + 1, whole))
                .flatten(),
        }
    }
    go(p, t, from, whole)
}
