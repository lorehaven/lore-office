use super::criteria::{Criteria, matching};
use super::{Out, arity, array, as_f64, finite, flatten, index, nm, nm_or, numbers, sc};
use crate::eval::Operand;
use sheets_model::{CellError, Value};

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "AVERAGE" => numbers(a).and_then(|n| mean(&n).map(Value::Number)),
        "MIN" => numbers(a).map(|n| Value::Number(n.into_iter().reduce(f64::min).unwrap_or(0.0))),
        "MAX" => numbers(a).map(|n| Value::Number(n.into_iter().reduce(f64::max).unwrap_or(0.0))),
        "COUNT" => Ok(Value::Number(as_f64(count(
            a,
            |v| matches!(v, Value::Number(_)),
            |v| v.to_number().is_ok(),
        )))),
        "COUNTA" => Ok(Value::Number(as_f64(count(a, |v| !v.is_empty(), |_| true)))),
        "COUNTBLANK" => Ok(Value::Number(as_f64(
            flatten(a)
                .iter()
                .filter(|v| v.is_empty() || matches!(v, Value::Text(t) if t.is_empty()))
                .count(),
        ))),
        "MEDIAN" => numbers(a).and_then(|n| median(&n)),
        "MODE" | "MODE.SNGL" => numbers(a).and_then(|n| mode(&n)),
        "VAR" | "VAR.S" => numbers(a).and_then(|n| variance(&n, 1)),
        "VARP" | "VAR.P" => numbers(a).and_then(|n| variance(&n, 0)),
        "STDEV" | "STDEV.S" => numbers(a).and_then(|n| variance(&n, 1)).and_then(sqrt),
        "STDEVP" | "STDEV.P" => numbers(a).and_then(|n| variance(&n, 0)).and_then(sqrt),
        "LARGE" => nth(a, true),
        "SMALL" => nth(a, false),
        "RANK" | "RANK.EQ" => rank(a),
        "SUMPRODUCT" => sumproduct(a),
        "SUMIF" => sumif(a),
        "COUNTIF" => countif(a),
        "AVERAGEIF" => averageif(a),
        "SUMIFS" => ifs(a, Agg::Sum),
        "COUNTIFS" => ifs_count(a),
        "AVERAGEIFS" => ifs(a, Agg::Average),
        "MINIFS" => ifs(a, Agg::Min),
        "MAXIFS" => ifs(a, Agg::Max),
        _ => return None,
    })
}

fn count(
    a: &[Operand],
    in_range: impl Fn(&Value) -> bool,
    direct: impl Fn(&Value) -> bool,
) -> usize {
    a.iter()
        .map(|op| match op {
            Operand::Scalar(v) => usize::from(direct(v)),
            Operand::Array(arr) => arr.data.iter().filter(|v| in_range(v)).count(),
        })
        .sum()
}

fn mean(n: &[f64]) -> Result<f64, CellError> {
    if n.is_empty() {
        Err(CellError::Div0)
    } else {
        Ok(n.iter().sum::<f64>() / as_f64(n.len()))
    }
}

fn sqrt(v: Value) -> Out {
    match v {
        Value::Number(x) => finite(x.sqrt()),
        other => Ok(other),
    }
}

fn median(n: &[f64]) -> Out {
    if n.is_empty() {
        return Err(CellError::Num);
    }
    let mut s = n.to_vec();
    s.sort_by(f64::total_cmp);
    let mid = s.len() / 2;
    Ok(Value::Number(if s.len() % 2 == 1 {
        s[mid]
    } else {
        f64::midpoint(s[mid - 1], s[mid])
    }))
}

fn mode(n: &[f64]) -> Out {
    let mut best: Option<(f64, usize)> = None;
    for x in n {
        let c = n.iter().filter(|y| (**y - *x).abs() < f64::EPSILON).count();
        if c > 1 && best.is_none_or(|(_, bc)| c > bc) {
            best = Some((*x, c));
        }
    }
    best.map(|(x, _)| Value::Number(x)).ok_or(CellError::NA)
}

fn variance(n: &[f64], ddof: usize) -> Out {
    if n.len() <= ddof {
        return Err(CellError::Div0);
    }
    let m = mean(n)?;
    let ss: f64 = n.iter().map(|x| (x - m) * (x - m)).sum();
    finite(ss / as_f64(n.len() - ddof))
}

fn nth(a: &[Operand], largest: bool) -> Out {
    arity(a, 2, 2)?;
    let mut n = numbers(&a[..1])?;
    let k = index(&sc(a, 1))?;
    if k == 0 || k > n.len() {
        return Err(CellError::Num);
    }
    n.sort_by(f64::total_cmp);
    Ok(Value::Number(if largest {
        n[n.len() - k]
    } else {
        n[k - 1]
    }))
}

fn rank(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let x = nm(a, 0)?;
    let n = numbers(&a[1..2])?;
    let ascending = nm_or(a, 2, 0.0)? != 0.0;
    if !n.iter().any(|y| (y - x).abs() < f64::EPSILON) {
        return Err(CellError::NA);
    }
    let better = n
        .iter()
        .filter(|y| if ascending { **y < x } else { **y > x })
        .count();
    Ok(Value::Number(as_f64(better + 1)))
}

fn sumproduct(a: &[Operand]) -> Out {
    if a.is_empty() {
        return Err(CellError::Value);
    }
    let arrays: Vec<_> = a.iter().map(array).collect();
    let (len, cols) = (arrays[0].data.len(), arrays[0].cols);
    if arrays.iter().any(|x| x.data.len() != len || x.cols != cols) {
        return Err(CellError::Value);
    }
    let mut total = 0.0;
    for i in 0..len {
        let mut p = 1.0;
        for arr in &arrays {
            match &arr.data[i] {
                Value::Number(n) => p *= n,
                Value::Error(e) => return Err(*e),
                _ => p = 0.0,
            }
        }
        total += p;
    }
    finite(total)
}

/// Cells of `range` that meet `criteria`, as positions.
fn hits(range: &Operand, criteria: &Value) -> Vec<usize> {
    let c = Criteria::parse(criteria);
    array(range)
        .data
        .iter()
        .enumerate()
        .filter(|(_, v)| c.matches(v))
        .map(|(i, _)| i)
        .collect()
}

fn pick_numbers(vals: &Operand, at: &[usize]) -> Vec<f64> {
    let arr = array(vals);
    at.iter()
        .filter_map(|i| arr.data.get(*i))
        .filter_map(|v| {
            if let Value::Number(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .collect()
}

fn sumif(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let at = hits(&a[0], &sc(a, 1));
    let target = a.get(2).unwrap_or(&a[0]);
    Ok(Value::Number(pick_numbers(target, &at).iter().sum()))
}

fn countif(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    Ok(Value::Number(as_f64(hits(&a[0], &sc(a, 1)).len())))
}

fn averageif(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let at = hits(&a[0], &sc(a, 1));
    let target = a.get(2).unwrap_or(&a[0]);
    mean(&pick_numbers(target, &at)).map(Value::Number)
}

#[derive(Clone, Copy)]
enum Agg {
    Sum,
    Average,
    Min,
    Max,
}

/// `SUMIFS(sum_range, range1, crit1, ...)` and the others that start with the range to aggregate.
fn ifs(a: &[Operand], agg: Agg) -> Out {
    if a.len() < 3 || a.len().is_multiple_of(2) {
        return Err(CellError::Value);
    }
    let at = matching(&a[1..])?;
    let vals = array(&a[0]);
    if vals.data.len() != array(&a[1]).data.len() {
        return Err(CellError::Value);
    }
    let n = pick_numbers(&a[0], &at);
    match agg {
        Agg::Sum => Ok(Value::Number(n.iter().sum())),
        Agg::Average => mean(&n).map(Value::Number),
        Agg::Min => Ok(Value::Number(n.into_iter().reduce(f64::min).unwrap_or(0.0))),
        Agg::Max => Ok(Value::Number(n.into_iter().reduce(f64::max).unwrap_or(0.0))),
    }
}

fn ifs_count(a: &[Operand]) -> Out {
    if a.len() < 2 || !a.len().is_multiple_of(2) {
        return Err(CellError::Value);
    }
    Ok(Value::Number(as_f64(matching(a)?.len())))
}
