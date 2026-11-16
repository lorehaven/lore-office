//! Formula text as files write it (`_xlfn.XLOOKUP(...)`) and as the sheets use it (`XLOOKUP(...)`).

use sheets_formula::{Expr, parse, print};

/// Functions a file must write with the `_xlfn.` prefix, because they are newer than the original function set.
const NEWER: &[&str] = &[
    "IFNA",
    "IFS",
    "SWITCH",
    "TEXTJOIN",
    "CONCAT",
    "MAXIFS",
    "MINIFS",
    "XLOOKUP",
    "XOR",
    "DAYS",
    "UNICHAR",
    "UNICODE",
    "STDEV.S",
    "STDEV.P",
    "VAR.S",
    "VAR.P",
    "MODE.SNGL",
    "RANK.EQ",
    "SORTBY",
    "UNIQUE",
    "SEQUENCE",
    "VSTACK",
    "HSTACK",
    "LET",
];

fn map_calls(e: Expr, f: &dyn Fn(&str) -> String) -> Expr {
    match e {
        Expr::Call(n, args) => {
            Expr::Call(f(&n), args.into_iter().map(|a| map_calls(a, f)).collect())
        }
        Expr::Unary(op, x) => Expr::Unary(op, Box::new(map_calls(*x, f))),
        Expr::Percent(x) => Expr::Percent(Box::new(map_calls(*x, f))),
        Expr::Binary(op, l, r) => {
            Expr::Binary(op, Box::new(map_calls(*l, f)), Box::new(map_calls(*r, f)))
        }
        other => other,
    }
}

fn calls_unknown(e: &Expr) -> bool {
    match e {
        Expr::Call(n, args) => !sheets_formula::is_known(n) || args.iter().any(calls_unknown),
        Expr::Unary(_, x) | Expr::Percent(x) => calls_unknown(x),
        Expr::Binary(_, l, r) => calls_unknown(l) || calls_unknown(r),
        _ => false,
    }
}

/// Whether the formula (as sheets write it, without file prefixes) calls a function this library does not have.
pub fn uses_unknown_function(text: &str) -> bool {
    parse(text).is_ok_and(|e| calls_unknown(&e))
}

/// From a file: prefixes removed. Text that does not parse is kept as it is (minus the prefixes), so nothing is lost.
pub fn from_file(text: &str) -> String {
    let strip = |n: &str| {
        let mut n = n;
        while let Some(rest) = n
            .strip_prefix("_XLFN.")
            .or_else(|| n.strip_prefix("_XLWS."))
        {
            n = rest;
        }
        n.to_string()
    };
    parse(text).map_or_else(
        |_| text.replace("_xlfn.", ""),
        |e| print(&map_calls(e, &strip)),
    )
}

/// To a file: newer functions get their prefix.
pub fn to_file(text: &str) -> String {
    let add = |n: &str| {
        if n == "FILTER" || n == "SORT" {
            format!("_xlfn._xlws.{n}")
        } else if NEWER.contains(&n) {
            format!("_xlfn.{n}")
        } else {
            n.to_string()
        }
    };
    parse(text).map_or_else(|_| text.to_string(), |e| print(&map_calls(e, &add)))
}
