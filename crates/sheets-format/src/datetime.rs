#![allow(clippy::cast_sign_loss)]

//! Dates and times under a format of date tokens.

use crate::parse::Tok;
use sheets_model::date::{from_serial, weekday_from_sunday};
use std::fmt::Write;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
pub fn render(toks: &[Tok], v: f64) -> String {
    let overflow = || "########".to_string();
    if v < 0.0 {
        return overflow();
    }
    let k = toks
        .iter()
        .filter_map(|t| {
            if let Tok::SubSec(n) = t {
                Some(u32::from((*n).min(3)))
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0);
    let unit = 10_i64.pow(k);
    let mut day = v.floor() as i64;
    let mut total = ((v - v.floor()) * 86_400.0 * unit as f64).round() as i64;
    if total >= 86_400 * unit {
        total -= 86_400 * unit;
        day += 1;
    }
    let (secs, sub) = (total / unit, total % unit);
    let (h, mi, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    let elapsed_secs = day * 86_400 + secs;
    let ampm = toks.iter().any(|t| matches!(t, Tok::AmPm { .. }));
    let needs_date = toks
        .iter()
        .any(|t| matches!(t, Tok::Year(_) | Tok::Month(_) | Tok::Day(_)));
    let Some((y, mo, d)) = from_serial(day as f64) else {
        return if needs_date {
            overflow()
        } else {
            String::new()
        };
    };
    let mut out = String::new();
    for t in toks {
        match t {
            Tok::Lit(l) => out.push_str(l),
            Tok::Year(2) => {
                let _ = write!(out, "{:02}", y % 100);
            }
            Tok::Year(_) => {
                let _ = write!(out, "{y:04}");
            }
            Tok::Month(1) => out.push_str(&mo.to_string()),
            Tok::Month(2) => {
                let _ = write!(out, "{mo:02}");
            }
            Tok::Month(3) => out.push_str(&MONTHS[(mo - 1) as usize][..3]),
            Tok::Month(4) => out.push_str(MONTHS[(mo - 1) as usize]),
            Tok::Month(_) => out.push_str(&MONTHS[(mo - 1) as usize][..1]),
            Tok::Day(1) => out.push_str(&d.to_string()),
            Tok::Day(2) => {
                let _ = write!(out, "{d:02}");
            }
            Tok::Day(n) => {
                let name = DAYS[weekday_from_sunday(day as f64) as usize];
                out.push_str(if *n == 3 { &name[..3] } else { name });
            }
            Tok::Hour(n) => {
                let hh = if ampm {
                    if h % 12 == 0 { 12 } else { h % 12 }
                } else {
                    h
                };
                out.push_str(&pad(hh, *n));
            }
            Tok::Minute(n) => out.push_str(&pad(mi, *n)),
            Tok::Second(n) => out.push_str(&pad(s, *n)),
            Tok::AmPm { short, lower } => {
                let text = match (h < 12, short) {
                    (true, false) => "AM",
                    (false, false) => "PM",
                    (true, true) => "A",
                    (false, true) => "P",
                };
                out.push_str(&if *lower {
                    text.to_lowercase()
                } else {
                    text.to_string()
                });
            }
            Tok::Elapsed(c, n) => {
                let value = match c {
                    'h' => elapsed_secs / 3600,
                    'm' => elapsed_secs / 60,
                    _ => elapsed_secs,
                };
                out.push_str(&pad(value, *n));
            }
            Tok::SubSec(n) => {
                let digits = format!("{sub:0width$}", width = k as usize);
                out.push('.');
                out.push_str(&digits[..(*n as usize).min(digits.len())]);
            }
            Tok::Comma => out.push(','),
            Tok::Point => out.push('.'),
            Tok::Slash => out.push('/'),
            Tok::Percent => out.push('%'),
            Tok::Digit(c) => out.push(*c),
            _ => {}
        }
    }
    out
}

fn pad(v: i64, width: u8) -> String {
    if width >= 2 {
        format!("{v:02}")
    } else {
        v.to_string()
    }
}
