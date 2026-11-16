//! Checking what a cell may hold (data validation) and how conditional formats look.

use sheets_calc::Engine;
use sheets_formula::{Expr, parse};
use sheets_model::{Allow, CellRef, Compare, Cond, CondRule, NumberKind, Range, Validation, Value};
use std::collections::HashMap;

const DEFAULT_MESSAGE: &str = "This value does not match the rules set for this cell.";

fn number_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(x) => Some(*x),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// The text of a value as a person sees it in a list.
#[must_use]
pub fn text_of(v: &Value) -> String {
    match v {
        Value::Empty => String::new(),
        other => other.to_string(),
    }
}

/// The choices of a list rule, or `None` for a rule that is not a list.
#[must_use]
pub fn list_items(e: &Engine, sheet: usize, v: &Validation) -> Option<Vec<String>> {
    match &v.allow {
        Allow::List(items) => Some(items.clone()),
        Allow::ListFrom(r) => Some(
            e.list_values(sheet, r)
                .iter()
                .filter(|x| !matches!(x, Value::Empty))
                .map(text_of)
                .collect(),
        ),
        _ => None,
    }
}

/// Whether the value now in a cell satisfies a rule; `Err` carries the message to show.
///
/// # Errors
///
/// The rule's own message, or a general one, when the value does not satisfy it.
pub fn check(e: &Engine, sheet: usize, at: CellRef, v: &Validation) -> Result<(), String> {
    let value = e.value(sheet, at);
    let fail = || {
        let message = if v.message.is_empty() {
            DEFAULT_MESSAGE.to_string()
        } else {
            v.message.clone()
        };
        Err(if v.title.is_empty() {
            message
        } else {
            format!("{}: {message}", v.title)
        })
    };
    if matches!(value, Value::Empty) {
        return Ok(()); // clearing a cell is always allowed
    }
    let bound = |text: &str| number_of(&e.evaluate_text(sheet, at, text));
    let ok = match &v.allow {
        Allow::List(_) | Allow::ListFrom(_) => {
            let shown = text_of(&value).to_lowercase();
            list_items(e, sheet, v)
                .is_some_and(|items| items.iter().any(|i| i.to_lowercase() == shown))
        }
        Allow::Number { kind, op, a, b } => {
            let x = match kind {
                NumberKind::Length => Some(text_of(&value).chars().count() as f64),
                NumberKind::Whole => number_of(&value).filter(|x| (x - x.round()).abs() < 1e-9),
                _ => number_of(&value),
            };
            match (x, bound(a)) {
                (Some(x), Some(lo)) => {
                    let hi = if matches!(op, Compare::Between | Compare::NotBetween) {
                        bound(b)
                    } else {
                        Some(lo)
                    };
                    hi.is_some_and(|hi| op.holds(x, lo, hi))
                }
                _ => false,
            }
        }
        Allow::Custom(f) => {
            // The formula is written for the top left cell of the range.
            let (dr, dc) = (
                i64::from(at.row) - i64::from(v.range.start.row),
                i64::from(at.col) - i64::from(v.range.start.col),
            );
            parse(f.trim_start_matches('=')).is_ok_and(|expr| {
                matches!(
                    e.evaluate_expr(sheet, at, &expr.shifted(dr, dc)),
                    Value::Bool(true)
                ) || number_of(&e.evaluate_expr(sheet, at, &expr.shifted(dr, dc)))
                    .is_some_and(|n| n != 0.0)
            })
        }
    };
    if ok { Ok(()) } else { fail() }
}

// ---- conditional formats ----

/// What the rules do to a cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub bold: bool,
    pub italic: bool,
    pub color: Option<u32>,
    pub fill: Option<u32>,
    /// A bar filling this part (0 to 1) of the cell, and its colour.
    pub bar: Option<(f64, u32)>,
}

/// What a rule needs to know about all the cells it covers, worked out once.
#[derive(Debug, Default)]
pub enum Stats {
    #[default]
    Nothing,
    Span {
        min: f64,
        max: f64,
        median: f64,
    },
    Cut(f64),
    Counts(HashMap<String, u32>),
    Bounds(Option<f64>, Option<f64>),
    Formula(Option<Expr>),
}

fn values_in(e: &Engine, sheet: usize, range: Range) -> Vec<(CellRef, Value)> {
    e.workbook()
        .sheets
        .get(sheet)
        .map(|s| {
            s.cells_in(range)
                .map(|(at, _)| (at, e.value(sheet, at)))
                .collect()
        })
        .unwrap_or_default()
}

/// The rule's facts about its range.
#[must_use]
pub fn stats(e: &Engine, sheet: usize, rule: &CondRule) -> Stats {
    let numbers = || -> Vec<f64> {
        let mut n: Vec<f64> = values_in(e, sheet, rule.range)
            .iter()
            .filter_map(|(_, v)| {
                if let Value::Number(x) = v {
                    Some(*x)
                } else {
                    None
                }
            })
            .collect();
        n.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        n
    };
    match &rule.cond {
        Cond::ColorScale { .. } | Cond::DataBar { .. } => {
            let n = numbers();
            match (n.first(), n.last()) {
                (Some(min), Some(max)) => Stats::Span {
                    min: *min,
                    max: *max,
                    median: n[n.len() / 2],
                },
                _ => Stats::Nothing,
            }
        }
        Cond::Top {
            n: count,
            bottom,
            percent,
        } => {
            let mut n = numbers();
            if n.is_empty() {
                return Stats::Nothing;
            }
            if !*bottom {
                n.reverse();
            }
            let take = if *percent {
                (n.len() as f64 * f64::from(*count) / 100.0).ceil() as usize
            } else {
                *count as usize
            };
            n.get(take.clamp(1, n.len()) - 1)
                .map_or(Stats::Nothing, |x| Stats::Cut(*x))
        }
        Cond::AboveAverage { .. } => {
            let n = numbers();
            if n.is_empty() {
                Stats::Nothing
            } else {
                Stats::Cut(n.iter().sum::<f64>() / n.len() as f64)
            }
        }
        Cond::Duplicates | Cond::Unique => {
            let mut counts: HashMap<String, u32> = HashMap::new();
            for (_, v) in values_in(e, sheet, rule.range) {
                if !matches!(v, Value::Empty) {
                    *counts.entry(text_of(&v).to_lowercase()).or_default() += 1;
                }
            }
            Stats::Counts(counts)
        }
        Cond::CellIs { a, b, .. } => {
            let at = rule.range.start;
            Stats::Bounds(
                number_of(&e.evaluate_text(sheet, at, a)),
                number_of(&e.evaluate_text(sheet, at, b)),
            )
        }
        Cond::Expression(f) => Stats::Formula(parse(f.trim_start_matches('=')).ok()),
        _ => Stats::Nothing,
    }
}

fn lerp(a: u32, b: u32, t: f64) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |shift: u32| {
        let (x, y) = (f64::from((a >> shift) & 255), f64::from((b >> shift) & 255));
        ((x + (y - x) * t).round() as u32) & 255
    };
    mix(16) << 16 | mix(8) << 8 | mix(0)
}

/// Whether the rule applies to this cell, and what it does for the ones that are colours or bars.
fn applies(
    e: &Engine,
    sheet: usize,
    at: CellRef,
    rule: &CondRule,
    st: &Stats,
    effect: &mut Effect,
) -> bool {
    let value = e.value(sheet, at);
    let number = number_of(&value);
    let text = text_of(&value);
    match (&rule.cond, st) {
        (Cond::CellIs { op, .. }, Stats::Bounds(a, b)) => match (number, a) {
            (Some(x), Some(a)) => op.holds(x, *a, b.unwrap_or(*a)),
            _ => false,
        },
        (Cond::Contains(t), _) => !t.is_empty() && text.to_lowercase().contains(&t.to_lowercase()),
        (Cond::NotContains(t), _) => !text.to_lowercase().contains(&t.to_lowercase()),
        (Cond::BeginsWith(t), _) => text.to_lowercase().starts_with(&t.to_lowercase()),
        (Cond::EndsWith(t), _) => text.to_lowercase().ends_with(&t.to_lowercase()),
        (Cond::Expression(_), Stats::Formula(Some(expr))) => {
            let (dr, dc) = (
                i64::from(at.row) - i64::from(rule.range.start.row),
                i64::from(at.col) - i64::from(rule.range.start.col),
            );
            let v = e.evaluate_expr(sheet, at, &expr.shifted(dr, dc));
            matches!(v, Value::Bool(true))
                || number_of(&v).is_some_and(|n| n != 0.0 && matches!(v, Value::Number(_)))
        }
        (Cond::Duplicates, Stats::Counts(c)) => {
            !matches!(value, Value::Empty) && c.get(&text.to_lowercase()).is_some_and(|n| *n > 1)
        }
        (Cond::Unique, Stats::Counts(c)) => {
            !matches!(value, Value::Empty) && c.get(&text.to_lowercase()).is_some_and(|n| *n == 1)
        }
        (Cond::Blanks, _) => text.trim().is_empty(),
        (Cond::NotBlanks, _) => !text.trim().is_empty(),
        (Cond::Errors, _) => matches!(value, Value::Error(_)),
        (Cond::Top { bottom, .. }, Stats::Cut(cut)) => {
            number.is_some_and(|x| if *bottom { x <= *cut } else { x >= *cut })
        }
        (Cond::AboveAverage { below }, Stats::Cut(avg)) => {
            number.is_some_and(|x| if *below { x < *avg } else { x > *avg })
        }
        (Cond::ColorScale { low, mid, high }, Stats::Span { min, max, median }) => {
            let Some(x) = number else { return false };
            let color = match mid {
                None => lerp(
                    *low,
                    *high,
                    if max > min {
                        (x - min) / (max - min)
                    } else {
                        0.0
                    },
                ),
                Some(m) if x <= *median => lerp(
                    *low,
                    *m,
                    if median > min {
                        (x - min) / (median - min)
                    } else {
                        1.0
                    },
                ),
                Some(m) => lerp(
                    *m,
                    *high,
                    if max > median {
                        (x - median) / (max - median)
                    } else {
                        1.0
                    },
                ),
            };
            effect.fill.get_or_insert(color);
            false
        }
        (Cond::DataBar { color }, Stats::Span { min, max, .. }) => {
            let Some(x) = number else { return false };
            let floor = min.min(0.0);
            let part = if max > &floor {
                ((x - floor) / (max - floor)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            effect.bar.get_or_insert((part, *color));
            false
        }
        _ => false,
    }
}

/// What the rules covering a cell do to it. `rules` is sorted by priority, `stats` has one entry per rule.
#[must_use]
pub fn effect(
    e: &Engine,
    sheet: usize,
    at: CellRef,
    rules: &[CondRule],
    stats: &[Stats],
) -> Option<Effect> {
    let mut out = Effect::default();
    let mut any = false;
    for (rule, st) in rules.iter().zip(stats) {
        if !rule.range.contains(at) {
            continue;
        }
        if applies(e, sheet, at, rule, st, &mut out) {
            any = true;
            out.bold |= rule.dxf.bold;
            out.italic |= rule.dxf.italic;
            if rule.dxf.color.is_some() && out.color.is_none() {
                out.color = rule.dxf.color;
            }
            if rule.dxf.fill.is_some() {
                out.fill = rule.dxf.fill;
            }
            if rule.stop_if_true {
                break;
            }
        }
    }
    (any || out.fill.is_some() || out.bar.is_some()).then_some(out)
}
