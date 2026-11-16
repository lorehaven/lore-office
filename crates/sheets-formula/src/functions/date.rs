#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

//! Dates are serial numbers: days since 1899-12-30, with the 1900 leap-year quirk of the original spreadsheets
//! (serial 60 is the day that never was, 1900-02-29). Serials before 1 and after 9999-12-31 are `#NUM!`.

use super::{Out, arity, finite, int, nm, nm_or, sc};
use crate::eval::Operand;
use sheets_model::date as cal;
use sheets_model::date::{MAX_SERIAL, month_len, to_serial, weekday_from_sunday};
use sheets_model::{CellError, Value};

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "DATE" => date(a),
        "YEAR" => part(a, |(y, _, _)| y),
        "MONTH" => part(a, |(_, m, _)| m),
        "DAY" => part(a, |(_, _, d)| d),
        "WEEKDAY" => weekday(a),
        "DAYS" => arity(a, 2, 2).and_then(|()| finite(nm(a, 0)?.trunc() - nm(a, 1)?.trunc())),
        "EDATE" => shift(a, false),
        "EOMONTH" => shift(a, true),
        "DATEDIF" => datedif(a),
        "TIME" => time(a),
        "HOUR" => clock(a, |s| s / 3600),
        "MINUTE" => clock(a, |s| s / 60 % 60),
        "SECOND" => clock(a, |s| s % 60),
        _ => return None,
    })
}

/// The calendar date of a serial, `#NUM!` for serials the calendar does not have (0 included).
fn from_serial(serial: f64) -> Result<(i64, i64, i64), CellError> {
    if serial.floor() < 1.0 {
        return Err(CellError::Num);
    }
    cal::from_serial(serial).ok_or(CellError::Num)
}

fn date(a: &[Operand]) -> Out {
    arity(a, 3, 3)?;
    let (mut y, m, d) = (int(&sc(a, 0))?, int(&sc(a, 1))?, int(&sc(a, 2))?);
    if (0..1900).contains(&y) {
        y += 1900;
    }
    // Months roll over into years; days roll over by counting from the first of the month.
    let total = y * 12 + (m - 1);
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if !(1900..=9999).contains(&y) {
        return Err(CellError::Num);
    }
    let s = to_serial(y, m, 1) + d - 1;
    if (1..=MAX_SERIAL).contains(&s) {
        #[allow(clippy::cast_precision_loss)]
        Ok(Value::Number(s as f64))
    } else {
        Err(CellError::Num)
    }
}

fn part(a: &[Operand], f: fn((i64, i64, i64)) -> i64) -> Out {
    arity(a, 1, 1)?;
    let ymd = from_serial(nm(a, 0)?)?;
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(f(ymd) as f64))
}

fn weekday(a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let serial = nm(a, 0)?.floor();
    let kind = nm_or(a, 1, 1.0)?.trunc();
    from_serial(serial)?;
    // 1900-01-01 (serial 1) was a Sunday in the original numbering: serial % 7 == 1 is Sunday.
    #[allow(clippy::cast_possible_truncation)]
    let sun0 = weekday_from_sunday(serial); // 0 = Sunday
    let r = match kind as i64 {
        1 => sun0 + 1,
        2 => (sun0 + 6) % 7 + 1,
        3 => (sun0 + 6) % 7,
        _ => return Err(CellError::Num),
    };
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(r as f64))
}

fn shift(a: &[Operand], end_of_month: bool) -> Out {
    arity(a, 2, 2)?;
    let (y, m, d) = from_serial(nm(a, 0)?)?;
    let total = y * 12 + (m - 1) + int(&sc(a, 1))?;
    let (ny, nm_) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if !(1900..=9999).contains(&ny) {
        return Err(CellError::Num);
    }
    let day = if end_of_month {
        month_len(ny, nm_)
    } else {
        d.min(month_len(ny, nm_))
    };
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(to_serial(ny, nm_, day) as f64))
}

fn datedif(a: &[Operand]) -> Out {
    arity(a, 3, 3)?;
    let (s, e) = (nm(a, 0)?, nm(a, 1)?);
    if s > e {
        return Err(CellError::Num);
    }
    let ((y1, m1, d1), (y2, m2, d2)) = (from_serial(s)?, from_serial(e)?);
    let months = (y2 - y1) * 12 + (m2 - m1) - i64::from(d2 < d1);
    let unit = sc(a, 2).to_text()?.to_uppercase();
    let r = match unit.as_str() {
        "Y" => months.div_euclid(12),
        "M" => months,
        "D" => (e.floor() - s.floor()) as i64,
        "YM" => months.rem_euclid(12),
        "MD" => {
            if d2 >= d1 {
                d2 - d1
            } else {
                let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
                month_len(py, pm) - d1 + d2
            }
        }
        "YD" => {
            let mut y = y2;
            if (m2, d2) < (m1, d1) {
                y -= 1;
            }
            let anchor = to_serial(y, m1, d1.min(month_len(y, m1)));
            (e.floor() as i64) - anchor
        }
        _ => return Err(CellError::Num),
    };
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(r as f64))
}

fn time(a: &[Operand]) -> Out {
    arity(a, 3, 3)?;
    let secs = int(&sc(a, 0))? * 3600 + int(&sc(a, 1))? * 60 + int(&sc(a, 2))?;
    if secs < 0 {
        return Err(CellError::Num);
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number((secs % 86_400) as f64 / 86_400.0))
}

fn clock(a: &[Operand], f: fn(i64) -> i64) -> Out {
    arity(a, 1, 1)?;
    let x = nm(a, 0)?;
    if x < 0.0 {
        return Err(CellError::Num);
    }
    // Round to the nearest second so 0.5 is exactly 12:00:00.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let secs = ((x.fract() * 86_400.0).round() as i64) % 86_400;
    #[allow(clippy::cast_precision_loss)]
    Ok(Value::Number(f(secs) as f64))
}
