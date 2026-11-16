use super::{Out, arity, finite, flatten, nm, nm_or, numbers};
use crate::eval::Operand;
use sheets_model::{CellError, Value};

pub(super) fn call(name: &str, a: &[Operand]) -> Option<Out> {
    Some(match name {
        "SUM" => numbers(a).map(|n| Value::Number(n.iter().sum())),
        "PRODUCT" => numbers(a).map(|n| Value::Number(n.iter().product())),
        "SUMSQ" => numbers(a).map(|n| Value::Number(n.iter().map(|x| x * x).sum())),
        "PI" => arity(a, 0, 0).map(|()| Value::Number(std::f64::consts::PI)),
        "ABS" => unary(a, f64::abs),
        "SIGN" => unary(a, |x| if x == 0.0 { 0.0 } else { x.signum() }),
        "INT" => unary(a, f64::floor),
        "SQRT" => unary(a, f64::sqrt),
        "EXP" => unary(a, f64::exp),
        "LN" => unary(a, f64::ln),
        "LOG10" => unary(a, f64::log10),
        "SIN" => unary(a, f64::sin),
        "COS" => unary(a, f64::cos),
        "TAN" => unary(a, f64::tan),
        "ASIN" => unary(a, f64::asin),
        "ACOS" => unary(a, f64::acos),
        "ATAN" => unary(a, f64::atan),
        "DEGREES" => unary(a, f64::to_degrees),
        "RADIANS" => unary(a, f64::to_radians),
        "EVEN" => unary(a, |x| x.signum() * 2.0 * (x.abs() / 2.0).ceil()),
        "ODD" => unary(a, |x| {
            if x == 0.0 {
                1.0
            } else {
                x.signum() * 2.0f64.mul_add(((x.abs() - 1.0) / 2.0).ceil(), 1.0)
            }
        }),
        "LOG" => log(a),
        "ATAN2" => arity(a, 2, 2).and_then(|()| finite(nm(a, 1)?.atan2(nm(a, 0)?))),
        "POWER" => power(a),
        "MOD" => modulo(a),
        "QUOTIENT" => quotient(a),
        "FACT" => fact(a),
        "GCD" | "LCM" => gcd_lcm(name == "GCD", a),
        "ROUND" => round(a, f64::round),
        "ROUNDUP" => round(a, |v| v.signum() * v.abs().ceil()),
        "ROUNDDOWN" | "TRUNC" => round(a, f64::trunc),
        "CEILING" => multiple(a, f64::ceil),
        "FLOOR" => multiple(a, f64::floor),
        _ => return None,
    })
}

fn unary(a: &[Operand], f: fn(f64) -> f64) -> Out {
    arity(a, 1, 1)?;
    finite(f(nm(a, 0)?))
}

fn log(a: &[Operand]) -> Out {
    arity(a, 1, 2)?;
    let (x, base) = (nm(a, 0)?, nm_or(a, 1, 10.0)?);
    if x <= 0.0 || base <= 0.0 {
        return Err(CellError::Num);
    }
    if (base - 1.0).abs() < f64::EPSILON {
        Err(CellError::Div0)
    } else {
        finite(x.log(base))
    }
}

fn power(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let (x, y) = (nm(a, 0)?, nm(a, 1)?);
    if x == 0.0 && y == 0.0 {
        return Err(CellError::Num);
    }
    if x == 0.0 && y < 0.0 {
        return Err(CellError::Div0);
    }
    finite(x.powf(y))
}

fn modulo(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let (n, d) = (nm(a, 0)?, nm(a, 1)?);
    if d == 0.0 {
        Err(CellError::Div0)
    } else {
        finite(d.mul_add(-(n / d).floor(), n))
    }
}

fn quotient(a: &[Operand]) -> Out {
    arity(a, 2, 2)?;
    let (n, d) = (nm(a, 0)?, nm(a, 1)?);
    if d == 0.0 {
        Err(CellError::Div0)
    } else {
        finite((n / d).trunc())
    }
}

fn fact(a: &[Operand]) -> Out {
    arity(a, 1, 1)?;
    let n = nm(a, 0)?.trunc();
    if !(0.0..=170.0).contains(&n) {
        return Err(CellError::Num);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = n as u32;
    Ok(Value::Number((2..=n).fold(1.0, |r, i| r * f64::from(i))))
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn gcd_lcm(is_gcd: bool, a: &[Operand]) -> Out {
    let vals = flatten(a);
    if vals.is_empty() {
        return Err(CellError::Value);
    }
    let mut acc = u64::from(!is_gcd);
    for v in vals {
        let n = v.to_number()?.trunc();
        if !(0.0..=9e15).contains(&n) {
            return Err(CellError::Num);
        }
        let n = n as u64;
        acc = if is_gcd {
            gcd(acc, n)
        } else if n == 0 {
            0
        } else {
            let g = gcd(acc, n);
            (acc / g).checked_mul(n).ok_or(CellError::Num)?
        };
    }
    Ok(Value::Number(acc as f64))
}

/// A number to 15 significant digits, so that `1.005 * 100` is 100.5 and not 100.49999999999999.
fn snap(x: f64) -> f64 {
    format!("{x:.14e}").parse().unwrap_or(x)
}

#[allow(clippy::many_single_char_names)]
fn round(a: &[Operand], f: fn(f64) -> f64) -> Out {
    arity(a, 1, 2)?;
    let (x, d) = (nm(a, 0)?, nm_or(a, 1, 0.0)?.trunc());
    if d.abs() > 300.0 {
        return Err(CellError::Num);
    }
    #[allow(clippy::cast_possible_truncation)]
    let p = 10f64.powi(d as i32);
    finite(f(snap(x * p)) / p)
}

fn multiple(a: &[Operand], f: fn(f64) -> f64) -> Out {
    arity(a, 1, 2)?;
    let x = nm(a, 0)?;
    let sig = if a.len() == 2 { nm(a, 1)? } else { 1.0 };
    if sig == 0.0 {
        return Ok(Value::Number(0.0));
    }
    if x > 0.0 && sig < 0.0 {
        return Err(CellError::Num);
    }
    finite(f(snap(x / sig)) * sig)
}
