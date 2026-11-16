//! Financial, statistical and calendar functions beyond the first set, and a few lookups.

// Financial formulas are clearer as written in the textbooks than rearranged for fused multiply-add; the comparisons are of
// values that were typed (a rate of exactly 0, a type of exactly 1), not of results.
#![allow(
    clippy::suboptimal_flops,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::while_float,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use super::{Out, arity, array, as_f64, finite, flatten, index, int, nm, nm_or, numbers, sc};
use crate::eval::Operand;
use sheets_model::date::{from_serial_day, month_len, to_serial, weekday_from_sunday};
use sheets_model::{CellError, Value};

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "PMT" => payment(a),
        "FV" => future_value(a),
        "PV" => present_value(a),
        "NPER" => periods(a),
        "RATE" => rate(a),
        "NPV" => npv(a),
        "IRR" => irr(a),
        "IPMT" => interest(a, false),
        "PPMT" => interest(a, true),
        "PERCENTILE" | "PERCENTILE.INC" => {
            numbers(&a[..a.len().min(1)]).and_then(|n| percentile(&n, nm(a, 1)?))
        }
        "QUARTILE" | "QUARTILE.INC" => {
            numbers(&a[..a.len().min(1)]).and_then(|n| percentile(&n, nm(a, 1)? / 4.0))
        }
        "CORREL" => pairs(a).and_then(|(x, y)| correl(&x, &y)),
        "SLOPE" => pairs(a).and_then(|(y, x)| line(&x, &y).map(|(s, _)| Value::Number(s))),
        "INTERCEPT" => pairs(a).and_then(|(y, x)| line(&x, &y).map(|(_, i)| Value::Number(i))),
        "GEOMEAN" => geomean(a),
        "COMBIN" => combin(a, false),
        "PERMUT" => combin(a, true),
        "MROUND" => mround(a),
        "SUBTOTAL" => subtotal(a),
        "WEEKNUM" => weeknum(a),
        "NETWORKDAYS" => networkdays(a),
        "WORKDAY" => workday(a),
        "DATEVALUE" => datevalue(a),
        "ADDRESS" => address(a),
        "HYPERLINK" => arity(a, 1, 2).map(|()| if a.len() == 2 { sc(a, 1) } else { sc(a, 0) }),
        "LOOKUP" => lookup(a),
        _ => return None,
    })
}

// ---- financial: payments are negative when money goes out, as in a spreadsheet ----

fn kind(a: &[Operand], i: usize) -> Result<f64, CellError> {
    Ok(if nm_or(a, i, 0.0)? == 0.0 { 0.0 } else { 1.0 })
}

fn pmt(rate: f64, nper: f64, pv: f64, fv: f64, due: f64) -> Result<f64, CellError> {
    if nper == 0.0 {
        return Err(CellError::Num);
    }
    if rate == 0.0 {
        return Ok(-(pv + fv) / nper);
    }
    let growth = (1.0 + rate).powf(nper);
    Ok(-(rate * (pv * growth + fv)) / ((1.0 + rate * due) * (growth - 1.0)))
}

fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, due: f64) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    let growth = (1.0 + rate).powf(nper);
    -(pv * growth + pmt * (1.0 + rate * due) * (growth - 1.0) / rate)
}

fn payment(a: &[Operand]) -> Out {
    arity(a, 3, 5)?;
    finite(pmt(
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm_or(a, 3, 0.0)?,
        kind(a, 4)?,
    )?)
}

fn future_value(a: &[Operand]) -> Out {
    arity(a, 3, 5)?;
    finite(fv(
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm_or(a, 3, 0.0)?,
        kind(a, 4)?,
    ))
}

fn present_value(a: &[Operand]) -> Out {
    arity(a, 3, 5)?;
    let (rate, nper, pmt, fv, due) = (
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm_or(a, 3, 0.0)?,
        kind(a, 4)?,
    );
    if rate == 0.0 {
        return finite(-(fv + pmt * nper));
    }
    let growth = (1.0 + rate).powf(nper);
    finite(-(fv + pmt * (1.0 + rate * due) * (growth - 1.0) / rate) / growth)
}

fn periods(a: &[Operand]) -> Out {
    arity(a, 3, 5)?;
    let (rate, pmt, pv, fv, due) = (
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm_or(a, 3, 0.0)?,
        kind(a, 4)?,
    );
    if rate == 0.0 {
        return if pmt == 0.0 {
            Err(CellError::Num)
        } else {
            finite(-(pv + fv) / pmt)
        };
    }
    let k = pmt * (1.0 + rate * due);
    let ratio = (k - fv * rate) / (k + pv * rate);
    if ratio <= 0.0 {
        Err(CellError::Num)
    } else {
        finite(ratio.ln() / rate.ln_1p())
    }
}

fn rate(a: &[Operand]) -> Out {
    arity(a, 3, 6)?;
    let (nper, pmt, pv, fv, due) = (
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm_or(a, 3, 0.0)?,
        kind(a, 4)?,
    );
    let f = |r: f64| {
        if r == 0.0 {
            pv + pmt * nper + fv
        } else {
            pv * (1.0 + r).powf(nper)
                + pmt * (1.0 + r * due) * ((1.0 + r).powf(nper) - 1.0) / r
                + fv
        }
    };
    newton(f, nm_or(a, 5, 0.1)?).map(Value::Number)
}

/// A root of `f` near `guess` by Newton's method with a numerical slope.
fn newton(f: impl Fn(f64) -> f64, guess: f64) -> Result<f64, CellError> {
    let mut x = guess;
    for _ in 0..200 {
        let y = f(x);
        let h = 1e-7_f64.max(x.abs() * 1e-7);
        let slope = (f(x + h) - f(x - h)) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            return Err(CellError::Num);
        }
        let next = x - y / slope;
        if !next.is_finite() || next <= -1.0 {
            return Err(CellError::Num);
        }
        if (next - x).abs() < 1e-11 {
            return Ok(next);
        }
        x = next;
    }
    Err(CellError::Num)
}

fn npv(a: &[Operand]) -> Out {
    if a.len() < 2 {
        return Err(CellError::Value);
    }
    let rate = nm(a, 0)?;
    if rate <= -1.0 {
        return Err(CellError::Num);
    }
    let flows = numbers(&a[1..])?;
    finite(
        flows
            .iter()
            .enumerate()
            .map(|(i, v)| v / (1.0 + rate).powf(as_f64(i + 1)))
            .sum(),
    )
}

fn irr(a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let flows = numbers(&a[..1])?;
    if !flows.iter().any(|v| *v > 0.0) || !flows.iter().any(|v| *v < 0.0) {
        return Err(CellError::Num);
    }
    let f = |r: f64| {
        flows
            .iter()
            .enumerate()
            .map(|(i, v)| v / (1.0 + r).powf(as_f64(i)))
            .sum::<f64>()
    };
    newton(f, nm_or(a, 1, 0.1)?).map(Value::Number)
}

fn interest(a: &[Operand], principal: bool) -> Out {
    arity(a, 4, 6)?;
    let (rate, per, nper, pv, fvv, due) = (
        nm(a, 0)?,
        nm(a, 1)?,
        nm(a, 2)?,
        nm(a, 3)?,
        nm_or(a, 4, 0.0)?,
        kind(a, 5)?,
    );
    if per < 1.0 || per > nper {
        return Err(CellError::Num);
    }
    let payment = pmt(rate, nper, pv, fvv, due)?;
    // Interest is the rate times the balance before this payment.
    let interest = if due == 1.0 {
        if per == 1.0 {
            0.0
        } else {
            (fv(rate, per - 2.0, payment, pv, 1.0) - payment) * rate
        }
    } else {
        fv(rate, per - 1.0, payment, pv, 0.0) * rate
    };
    finite(if principal {
        payment - interest
    } else {
        interest
    })
}

// ---- statistics ----

fn percentile(values: &[f64], k: f64) -> Out {
    if values.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(CellError::Num);
    }
    let mut s = values.to_vec();
    s.sort_by(f64::total_cmp);
    let rank = k * as_f64(s.len() - 1);
    let (lo, frac) = (rank.floor(), rank.fract());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo_at = lo as usize;
    let hi = s.get(lo_at + 1).copied().unwrap_or(s[lo_at]);
    finite(frac.mul_add(hi - s[lo_at], s[lo_at]))
}

/// Two ranges of the same size as number pairs; pairs with a non-number on either side are left out.
fn pairs(a: &[Operand]) -> Result<(Vec<f64>, Vec<f64>), CellError> {
    arity(a, 2, 2)?;
    let (x, y) = (array(&a[0]), array(&a[1]));
    if x.data.len() != y.data.len() {
        return Err(CellError::NA);
    }
    let mut out = (Vec::new(), Vec::new());
    for (p, q) in x.data.iter().zip(&y.data) {
        if let Some(e) = p.error().or_else(|| q.error()) {
            return Err(e);
        }
        if let (Value::Number(p), Value::Number(q)) = (p, q) {
            out.0.push(*p);
            out.1.push(*q);
        }
    }
    if out.0.len() < 2 {
        Err(CellError::Div0)
    } else {
        Ok(out)
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / as_f64(v.len())
}

fn correl(x: &[f64], y: &[f64]) -> Out {
    let (mx, my) = (mean(x), mean(y));
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let (sxx, syy): (f64, f64) = (
        x.iter().map(|a| (a - mx).powi(2)).sum(),
        y.iter().map(|b| (b - my).powi(2)).sum(),
    );
    if sxx == 0.0 || syy == 0.0 {
        Err(CellError::Div0)
    } else {
        finite(sxy / (sxx * syy).sqrt())
    }
}

/// Least squares: `(slope, intercept)` of y on x.
fn line(x: &[f64], y: &[f64]) -> Result<(f64, f64), CellError> {
    let (mx, my) = (mean(x), mean(y));
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    if sxx == 0.0 {
        return Err(CellError::Div0);
    }
    let slope = x
        .iter()
        .zip(y)
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / sxx;
    Ok((slope, slope.mul_add(-mx, my)))
}

fn geomean(a: &[Operand]) -> Out {
    let n = numbers(a)?;
    if n.is_empty() || n.iter().any(|v| *v <= 0.0) {
        return Err(CellError::Num);
    }
    finite((n.iter().map(|v| v.ln()).sum::<f64>() / as_f64(n.len())).exp())
}

fn combin(a: &[Operand], ordered: bool) -> Out {
    arity(a, 2, 2)?;
    let (n, k) = (nm(a, 0)?.trunc(), nm(a, 1)?.trunc());
    if n < 0.0 || k < 0.0 || k > n {
        return Err(CellError::Num);
    }
    let k_used = if ordered { k } else { k.min(n - k) };
    let mut r = 1.0;
    let mut i = 0.0;
    while i < k_used {
        r *= (n - i) / if ordered { 1.0 } else { i + 1.0 };
        i += 1.0;
    }
    finite(r.round())
}

fn mround(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let (n, m) = (nm(a, 0)?, nm(a, 1)?);
    if m == 0.0 {
        return Ok(Value::Number(0.0));
    }
    if n * m < 0.0 {
        return Err(CellError::Num);
    }
    finite((n / m).round() * m)
}

fn subtotal(a: &[Operand]) -> Out {
    if a.len() < 2 {
        return Err(CellError::Value);
    }
    let code = int(&sc(a, 0))?;
    let name = match code % 100 {
        1 => "AVERAGE",
        2 => "COUNT",
        3 => "COUNTA",
        4 => "MAX",
        5 => "MIN",
        6 => "PRODUCT",
        7 => "STDEV",
        8 => "STDEVP",
        9 => "SUM",
        10 => "VAR",
        11 => "VARP",
        _ => return Err(CellError::Value),
    };
    if !(1..=11).contains(&code) && !(101..=111).contains(&code) {
        return Err(CellError::Value);
    }
    super::eager(name, &a[1..])
}

// ---- calendar ----

fn weeknum(a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let serial = nm(a, 0)?.floor();
    let kind = nm_or(a, 1, 1.0)?.trunc();
    let (y, m, d) = from_serial_day(serial).ok_or(CellError::Num)?;
    let doy =
        sheets_model::date::days_from_civil(y, m, d) - sheets_model::date::days_from_civil(y, 1, 1);
    let jan1 = weekday_from_sunday(as_f64(usize::try_from(to_serial(y, 1, 1)).unwrap_or(0)));
    let week = match kind as i64 {
        1 => (doy + jan1) / 7 + 1,
        2 => (doy + (jan1 + 6) % 7) / 7 + 1,
        21 => {
            // ISO 8601: the week with the year's first Thursday is week 1.
            let wd = (weekday_from_sunday(serial) + 6) % 7; // Monday = 0
            let thursday = doy - wd + 3;
            if thursday < 0 {
                return weeknum(&[
                    Operand::Scalar(Value::Number(to_serial(y - 1, 12, 31) as f64)),
                    Operand::Scalar(Value::Number(21.0)),
                ]);
            }
            let year_len = if month_len(y, 2) == 29 { 366 } else { 365 };
            if thursday >= year_len {
                1
            } else {
                thursday / 7 + 1
            }
        }
        _ => return Err(CellError::Num),
    };
    Ok(Value::Number(week as f64))
}

const fn is_weekend(serial: f64) -> bool {
    matches!(weekday_from_sunday(serial), 0 | 6)
}

fn holidays(a: &[Operand], from: usize) -> Result<Vec<f64>, CellError> {
    if a.len() > from {
        numbers(&a[from..=from]).map(|v| v.into_iter().map(f64::floor).collect())
    } else {
        Ok(Vec::new())
    }
}

fn networkdays(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let (start, end) = (nm(a, 0)?.floor(), nm(a, 1)?.floor());
    let off = holidays(a, 2)?;
    let (lo, hi) = (start.min(end), start.max(end));
    if hi - lo > 3_000_000.0 {
        return Err(CellError::Num);
    }
    let mut count = 0.0;
    let mut day = lo;
    while day <= hi {
        if !is_weekend(day) && !off.contains(&day) {
            count += 1.0;
        }
        day += 1.0;
    }
    Ok(Value::Number(if start <= end { count } else { -count }))
}

fn workday(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let (mut day, mut left) = (nm(a, 0)?.floor(), int(&sc(a, 1))?);
    let off = holidays(a, 2)?;
    let step = if left < 0 { -1.0 } else { 1.0 };
    let mut guard = 0;
    while left != 0 {
        day += step;
        guard += 1;
        if guard > 4_000_000 {
            return Err(CellError::Num);
        }
        if !is_weekend(day) && !off.contains(&day) {
            left -= step as i64;
        }
    }
    from_serial_day(day)
        .map(|_| Value::Number(day))
        .ok_or(CellError::Num)
}

fn datevalue(a: &[Operand]) -> Out {
    arity(a, 1, 1)?;
    let text = sc(a, 0).to_text()?;
    let t = text.trim();
    let parts: Vec<&str> = t.split(['-', '/']).collect();
    let nums: Vec<i64> = parts.iter().filter_map(|p| p.trim().parse().ok()).collect();
    let (y, m, d) = match (parts.len(), nums.len()) {
        (3, 3) if parts[0].len() == 4 => (nums[0], nums[1], nums[2]),
        (3, 3) => (nums[2], nums[0], nums[1]),
        _ => return Err(CellError::Value),
    };
    if !(1..=12).contains(&m) || d < 1 || d > month_len(y, m) || !(1900..=9999).contains(&y) {
        return Err(CellError::Value);
    }
    Ok(Value::Number(as_f64(
        usize::try_from(to_serial(y, m, d)).map_err(|_| CellError::Value)?,
    )))
}

// ---- references as text, lookups ----

fn address(a: &[Operand]) -> Out {
    arity(a, 2, 5)?;
    let (row, col) = (index(&sc(a, 0))?, index(&sc(a, 1))?);
    let abs = nm_or(a, 2, 1.0)? as i64;
    if row == 0
        || col == 0
        || col > sheets_model::MAX_COLS as usize
        || row > sheets_model::MAX_ROWS as usize
    {
        return Err(CellError::Value);
    }
    let (row_abs, col_abs) = match abs {
        1 => (true, true),
        2 => (true, false),
        3 => (false, true),
        4 => (false, false),
        _ => return Err(CellError::Value),
    };
    let a1 = if a.len() > 3 {
        sc(a, 3).to_bool()?
    } else {
        true
    };
    let letters = sheets_model::col_name(u32::try_from(col - 1).unwrap_or(0));
    let mut text = if a1 {
        format!(
            "{}{letters}{}{row}",
            if col_abs { "$" } else { "" },
            if row_abs { "$" } else { "" }
        )
    } else {
        format!(
            "R{}{row}{}C{}{col}{}",
            if row_abs { "" } else { "[" },
            if row_abs { "" } else { "]" },
            if col_abs { "" } else { "[" },
            if col_abs { "" } else { "]" }
        )
    };
    if a.len() > 4 {
        text = format!("{}!{text}", sc(a, 4).to_text()?);
    }
    Ok(Value::Text(text))
}

fn lookup(a: &[Operand]) -> Out {
    arity(a, 2, 3)?;
    let key = sc(a, 0);
    let keys = flatten(&a[1..2]);
    let result = if a.len() == 3 {
        flatten(&a[2..3])
    } else {
        keys.clone()
    };
    let mut best = None;
    for (i, v) in keys.iter().enumerate() {
        let comparable = std::mem::discriminant(v) == std::mem::discriminant(&key) && !v.is_empty();
        if !comparable {
            continue;
        }
        if crate::eval::compare(v, &key) == std::cmp::Ordering::Greater {
            break;
        }
        best = Some(i);
    }
    best.and_then(|i| result.get(i).cloned())
        .ok_or(CellError::NA)
}
