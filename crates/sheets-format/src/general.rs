//! `General`: as many digits as fit in about 11 characters.

use crate::number::fixed;

/// A number under the `General` format: whole numbers as they are, otherwise up to ten significant digits, scientific
/// notation for the very large and the very small.
#[must_use]
pub fn general(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    let sign = if x < 0.0 { "-" } else { "" };
    if !(1e-4..1e11).contains(&a) {
        return format!("{sign}{}", scientific(a));
    }
    #[allow(clippy::cast_possible_truncation)]
    let int_digits = if a >= 1.0 {
        a.log10().floor() as i32 + 1
    } else {
        1
    };
    let decimals = if a >= 1.0 {
        (10 - int_digits).clamp(0, 9)
    } else {
        9
    };
    #[allow(clippy::cast_sign_loss)]
    let (int, frac) = fixed(a, decimals as usize);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

fn scientific(a: f64) -> String {
    let s = format!("{a:.5e}"); // 1.23457e11
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let m = if m.contains('.') {
        m.trim_end_matches('0').trim_end_matches('.')
    } else {
        m
    };
    let e: i32 = e.parse().unwrap_or(0);
    format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}
