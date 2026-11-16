//! Numbers under a format of digit placeholders.

use crate::parse::Tok;

/// `x` rounded half away from zero (after trimming float noise to 15 digits) to `decimals` places, as the digits before
/// and after the point.
pub fn fixed(x: f64, decimals: usize) -> (String, String) {
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let p = 10f64.powi(decimals as i32);
    let scaled = format!("{:.14e}", x * p)
        .parse::<f64>()
        .unwrap_or(x * p)
        .round();
    let mut digits = if scaled.abs() >= 1e15 {
        // Beyond 15 digits a float's exact expansion is noise: the digits are the 15 significant ones, then zeros.
        let sci = format!("{:.14e}", scaled.abs());
        let (mantissa, exponent) = sci.split_once('e').unwrap_or((&sci, "0"));
        let mut d: String = mantissa.chars().filter(char::is_ascii_digit).collect();
        let want = exponent.parse::<usize>().unwrap_or(0) + 1;
        while d.len() < want {
            d.push('0');
        }
        if scaled < 0.0 { format!("-{d}") } else { d }
    } else {
        format!("{scaled:.0}")
    };
    while digits.len() < decimals + 1 {
        digits.insert(0, '0');
    }
    let split = digits.len() - decimals;
    (digits[..split].to_string(), digits[split..].to_string())
}

pub fn render(toks: &[Tok], x: f64) -> String {
    if let Some(s) = fraction(toks, x) {
        return s;
    }
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let percent = toks.iter().filter(|t| **t == Tok::Percent).count() as i32;
    let (toks, thousands, scale) = commas(toks);
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let x = x * 100f64.powi(percent) / 1000f64.powi(scale as i32);
    toks.iter()
        .position(|t| matches!(t, Tok::Exp(_)))
        .map_or_else(|| plain(&toks, x, thousands), |e| scientific(&toks, e, x))
}

/// Commas between digit placeholders mean thousands separators; commas after the last one divide by a thousand each.
fn commas(toks: &[Tok]) -> (Vec<Tok>, bool, usize) {
    let point = toks
        .iter()
        .position(|t| *t == Tok::Point)
        .unwrap_or(toks.len());
    let limit = toks
        .iter()
        .position(|t| matches!(t, Tok::Exp(_)))
        .unwrap_or(toks.len());
    let (mut out, mut thousands, mut scale) = (Vec::new(), false, 0);
    for (i, t) in toks.iter().enumerate() {
        if *t != Tok::Comma {
            out.push(t.clone());
            continue;
        }
        let before = toks[..i].iter().any(|t| matches!(t, Tok::Digit(_)));
        let after = i < limit
            && toks[i + 1..limit]
                .iter()
                .any(|t| matches!(t, Tok::Digit(_)));
        match (before, after) {
            (true, true) if i < point => thousands = true,
            (true, false) if i < limit => scale += 1,
            _ => out.push(Tok::Lit(",".into())),
        }
    }
    (out, thousands, scale)
}

fn plain(toks: &[Tok], x: f64, thousands: bool) -> String {
    let point = toks.iter().position(|t| *t == Tok::Point);
    let (int_toks, frac_toks) = point.map_or_else(
        || (toks, &toks[toks.len()..]),
        |p| (&toks[..p], &toks[p + 1..]),
    );
    let places = frac_toks
        .iter()
        .filter(|t| matches!(t, Tok::Digit(_)))
        .count();
    let (int, frac) = fixed(x, places);
    let has_digits = toks.iter().any(|t| matches!(t, Tok::Digit(_)));
    let mut digits: Vec<char> = if int == "0" || !has_digits {
        Vec::new()
    } else {
        int.chars().collect()
    };
    let mut parts: Vec<String> = vec![String::new(); int_toks.len()];
    let mut count = 0;
    for (i, t) in int_toks.iter().enumerate().rev() {
        match t {
            Tok::Digit(k) => {
                let d = digits.pop().or(match k {
                    '0' => Some('0'),
                    '?' => Some(' '),
                    _ => None,
                });
                if let Some(d) = d {
                    count += 1;
                    let more = !digits.is_empty()
                        || int_toks[..i]
                            .iter()
                            .any(|t| matches!(t, Tok::Digit('0' | '?')));
                    let sep = thousands && count % 3 == 0 && more;
                    parts[i] = if sep { format!(",{d}") } else { d.to_string() };
                }
            }
            other => parts[i] = literal(other),
        }
    }
    let mut lead = String::new();
    while let Some(d) = digits.pop() {
        count += 1;
        lead.insert(0, d);
        if thousands && count % 3 == 0 && !digits.is_empty() {
            lead.insert(0, ',');
        }
    }
    let first = int_toks.iter().position(|t| matches!(t, Tok::Digit(_)));
    let mut out = String::new();
    if first.is_none() {
        out.push_str(&lead);
    }
    for (i, p) in parts.iter().enumerate() {
        if Some(i) == first {
            out.push_str(&lead);
        }
        out.push_str(p);
    }
    if point.is_some() {
        out.push('.');
        let last_used = frac.rfind(|c| c != '0');
        let mut k = 0;
        for t in frac_toks {
            if let Tok::Digit(kind) = t {
                let ch = frac.as_bytes()[k] as char;
                let trailing = last_used.is_none_or(|l| k > l);
                match (kind, trailing) {
                    ('0', _) | (_, false) => out.push(ch),
                    ('?', true) => out.push(' '),
                    _ => {}
                }
                k += 1;
            } else {
                out.push_str(&literal(t));
            }
        }
    }
    out
}

fn literal(t: &Tok) -> String {
    match t {
        Tok::Lit(s) => s.clone(),
        Tok::Percent => "%".into(),
        Tok::Slash => "/".into(),
        Tok::Point => ".".into(),
        Tok::Comma => ",".into(),
        _ => String::new(),
    }
}

fn scientific(toks: &[Tok], e: usize, x: f64) -> String {
    let Tok::Exp(plus) = toks[e] else {
        return String::new();
    };
    let mant = &toks[..e];
    let exp_digits = toks[e + 1..]
        .iter()
        .filter(|t| matches!(t, Tok::Digit(_)))
        .count()
        .max(1);
    let point = mant
        .iter()
        .position(|t| *t == Tok::Point)
        .unwrap_or(mant.len());
    let n_int = mant[..point]
        .iter()
        .filter(|t| matches!(t, Tok::Digit(_)))
        .count()
        .max(1);
    let places = mant[point..]
        .iter()
        .filter(|t| matches!(t, Tok::Digit(_)))
        .count();
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let (n_int_i, places_i) = (n_int as i32, places as i32);
    #[allow(clippy::cast_possible_truncation)]
    let mut exp = if x == 0.0 {
        0
    } else {
        x.log10().floor() as i32 - (n_int_i - 1)
    };
    let mut m = if x == 0.0 { 0.0 } else { x / 10f64.powi(exp) };
    let round = |m: f64| (m * 10f64.powi(places_i)).round() / 10f64.powi(places_i);
    if round(m) >= 10f64.powi(n_int_i) {
        exp += 1;
        m = x / 10f64.powi(exp);
    }
    let sign = if exp < 0 {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    format!(
        "{}E{sign}{:0width$}",
        plain(mant, m, false),
        exp.abs(),
        width = exp_digits
    )
}

/// `# ?/?`, `?/?`, `# ??/??`, `?/8`.
fn fraction(toks: &[Tok], x: f64) -> Option<String> {
    let slash = toks.iter().position(|t| *t == Tok::Slash)?;
    let num_start = toks[..slash]
        .iter()
        .rposition(|t| !matches!(t, Tok::Digit(_)))
        .map_or(0, |i| i + 1);
    if num_start == slash {
        return None;
    }
    let num_places = &toks[num_start..slash];
    let mut j = slash + 1;
    let den_places = toks[j..]
        .iter()
        .take_while(|t| matches!(t, Tok::Digit(_)))
        .count();
    j += den_places;
    let fixed_den: Option<u64> = match toks.get(j) {
        Some(Tok::Lit(s)) if den_places == 0 => {
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            j += 1;
            digits.parse().ok()
        }
        _ => None,
    };
    if den_places == 0 && fixed_den.is_none() {
        return None;
    }
    let pre = &toks[..num_start];
    let mixed = pre.iter().any(|t| matches!(t, Tok::Digit(_)));
    #[allow(clippy::cast_precision_loss)]
    let (whole, frac) = if mixed {
        (x.trunc(), x - x.trunc())
    } else {
        (0.0, x)
    };
    let max_den =
        fixed_den.unwrap_or_else(|| 10u64.pow(u32::try_from(den_places).unwrap_or(1)) - 1);
    let (mut n, d) = fixed_den.map_or_else(
        || best_fraction(frac, max_den),
        |d| (rounded_u64(frac * to_f64(d)), d),
    );
    let mut whole = whole;
    if mixed && n == d {
        whole += 1.0;
        n = 0;
    }
    let mut out = plain(pre, whole, false);
    let (nw, dw) = (
        num_places.len(),
        den_places.max(fixed_den.map_or(0, |d| d.to_string().len())),
    );
    if n == 0 && mixed {
        out.push_str(&" ".repeat(nw + 1 + dw));
    } else {
        let ns = n.to_string();
        out.push_str(&" ".repeat(nw.saturating_sub(ns.len())));
        out.push_str(&ns);
        out.push('/');
        let ds = d.to_string();
        out.push_str(&ds);
        out.push_str(&" ".repeat(dw.saturating_sub(ds.len())));
    }
    for t in &toks[j..] {
        out.push_str(&literal(t));
    }
    Some(out)
}

#[allow(clippy::cast_precision_loss)]
const fn to_f64(n: u64) -> f64 {
    n as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn rounded_u64(x: f64) -> u64 {
    x.round().max(0.0) as u64
}

/// The fraction `n/d` with `d <= max_den` closest to `x` (smaller denominators win ties).
fn best_fraction(x: f64, max_den: u64) -> (u64, u64) {
    let (mut best, mut err) = ((rounded_u64(x), 1), f64::MAX);
    for d in 1..=max_den {
        let n = rounded_u64(x * to_f64(d));
        let e = (x - to_f64(n) / to_f64(d)).abs();
        if e + 1e-12 < err {
            best = (n, d);
            err = e;
        }
    }
    best
}
