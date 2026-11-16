//! Format text to sections of tokens, and the choice of section for a value.

use crate::{datetime, general, number};
use sheets_model::Value;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

/// The colours a format may ask for (`[Red]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Black,
    Blue,
    Cyan,
    Green,
    Magenta,
    Red,
    White,
    Yellow,
    /// `[Color12]`: an index into the workbook palette.
    Indexed(u8),
}

/// A formatted value: the text and the colour the format asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Lit(String),
    Digit(char),
    Point,
    Comma,
    Percent,
    /// `E+` (true) or `E-`.
    Exp(bool),
    Text,
    General,
    Slash,
    Year(u8),
    Month(u8),
    Day(u8),
    Hour(u8),
    Minute(u8),
    Second(u8),
    AmPm {
        short: bool,
        lower: bool,
    },
    /// `[h]`, `[m]`, `[s]` with the number of digits.
    Elapsed(char, u8),
    /// Fractions of a second: `.0`, `.00`.
    SubSec(u8),
    /// A month or a minute, until the neighbours decide.
    M(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub toks: Vec<Tok>,
    pub cond: Option<(Cmp, f64)>,
    pub color: Option<Color>,
}

impl Section {
    pub(crate) fn is_date(&self) -> bool {
        self.toks.iter().any(|t| {
            matches!(
                t,
                Tok::Year(_)
                    | Tok::Month(_)
                    | Tok::Day(_)
                    | Tok::Hour(_)
                    | Tok::Minute(_)
                    | Tok::Second(_)
                    | Tok::AmPm { .. }
                    | Tok::Elapsed(..)
            )
        })
    }

    fn has_text(&self) -> bool {
        self.toks.contains(&Tok::Text)
    }
}

/// A parsed number format.
#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub(crate) sections: Vec<Section>,
}

impl Format {
    /// `General`.
    #[must_use]
    pub fn general() -> Self {
        Self {
            sections: vec![Section {
                toks: vec![Tok::General],
                cond: None,
                color: None,
            }],
        }
    }

    /// Parse a format. At most four sections; quotes and brackets must close.
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        if text.trim().is_empty() {
            return Ok(Self::general());
        }
        let parts = split_sections(text)?;
        if parts.len() > 4 {
            return Err(FormatError("a format has at most four sections".into()));
        }
        let sections = parts
            .iter()
            .map(|p| lex(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { sections })
    }

    /// Whether numbers under this format are dates or times (the first section decides, as in a spreadsheet).
    #[must_use]
    pub fn is_date(&self) -> bool {
        self.sections.iter().take(3).any(Section::is_date)
    }

    /// Show a value. Empty cells show nothing; booleans and errors show as they are.
    #[must_use]
    pub fn render(&self, value: &Value) -> Rendered {
        match value {
            Value::Empty => Rendered {
                text: String::new(),
                color: None,
            },
            Value::Error(e) => Rendered {
                text: e.to_string(),
                color: None,
            },
            Value::Bool(_) => Rendered {
                text: value.to_string(),
                color: None,
            },
            Value::Text(t) => self.render_text(t),
            Value::Number(x) => self.render_number(*x),
        }
    }

    fn render_text(&self, t: &str) -> Rendered {
        let by_position = (self.sections.len() == 4).then(|| &self.sections[3]);
        let sec = by_position.or_else(|| self.sections.iter().find(|s| s.has_text()));
        sec.map_or_else(
            || Rendered {
                text: t.to_string(),
                color: None,
            },
            |s| Rendered {
                text: s
                    .toks
                    .iter()
                    .map(|k| {
                        if *k == Tok::Text {
                            t.to_string()
                        } else {
                            lit(k)
                        }
                    })
                    .collect(),
                color: s.color,
            },
        )
    }

    fn render_number(&self, x: f64) -> Rendered {
        if !x.is_finite() {
            return Rendered {
                text: "#NUM!".into(),
                color: None,
            };
        }
        let idx = self.pick(x);
        let sec = &self.sections[idx];
        let (neg, v) = (
            x < 0.0 && idx == 0,
            if self.sections.len() > 1 && idx > 0 {
                x.abs()
            } else {
                x
            },
        );
        let body = if sec.is_date() {
            datetime::render(&sec.toks, v)
        } else if sec.toks.contains(&Tok::General) {
            general::general(v)
        } else {
            number::render(&sec.toks, v.abs())
        };
        let numeric = sec.toks.iter().any(|t| matches!(t, Tok::Digit(_)));
        let sign = if neg && numeric { "-" } else { "" };
        Rendered {
            text: format!("{sign}{body}"),
            color: sec.color,
        }
    }

    /// The section a number uses.
    fn pick(&self, x: f64) -> usize {
        let n = self.sections.len().min(3);
        for i in 0..n {
            let s = &self.sections[i];
            let hit = match s.cond {
                Some((cmp, k)) => compare(cmp, x, k),
                None if i == 0 && n == 2 => x >= 0.0,
                None if i == 0 => x > 0.0,
                None if i == 1 && n == 3 => x < 0.0,
                None => true,
            };
            if hit {
                return i;
            }
        }
        0
    }
}

fn compare(c: Cmp, x: f64, k: f64) -> bool {
    match c {
        Cmp::Lt => x < k,
        Cmp::Le => x <= k,
        Cmp::Gt => x > k,
        Cmp::Ge => x >= k,
        Cmp::Eq => (x - k).abs() < f64::EPSILON,
        Cmp::Ne => (x - k).abs() >= f64::EPSILON,
    }
}

fn lit(t: &Tok) -> String {
    match t {
        Tok::Lit(s) => s.clone(),
        _ => String::new(),
    }
}

fn split_sections(text: &str) -> Result<Vec<String>, FormatError> {
    let (mut out, mut cur) = (Vec::new(), String::new());
    let (mut quote, mut bracket, mut escape) = (false, false, false);
    for c in text.chars() {
        cur.push(c);
        if escape {
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if quote {
            quote = c != '"';
        } else if bracket {
            bracket = c != ']';
        } else if c == '"' {
            quote = true;
        } else if c == '[' {
            bracket = true;
        } else if c == ';' {
            cur.pop();
            out.push(std::mem::take(&mut cur));
        }
    }
    if quote || bracket {
        return Err(FormatError("a quote or bracket is not closed".into()));
    }
    out.push(cur);
    Ok(out)
}

fn run(chars: &[char], i: usize, c: char) -> usize {
    chars[i..]
        .iter()
        .take_while(|x| x.eq_ignore_ascii_case(&c))
        .count()
}

fn starts_with_ci(chars: &[char], i: usize, s: &str) -> bool {
    let n = s.chars().count();
    chars.len() >= i + n
        && chars[i..i + n]
            .iter()
            .zip(s.chars())
            .all(|(a, b)| a.eq_ignore_ascii_case(&b))
}

#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::cast_possible_truncation
)]
fn lex(text: &str) -> Result<Section, FormatError> {
    let chars: Vec<char> = text.chars().collect();
    let (mut toks, mut cond, mut color) = (Vec::new(), None, None);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|x| *x == '"')
                    .ok_or_else(|| FormatError("quote not closed".into()))?;
                toks.push(Tok::Lit(chars[i + 1..=i + end].iter().collect()));
                i += end + 2;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    toks.push(Tok::Lit(n.to_string()));
                }
                i += 2;
            }
            '_' => {
                if i + 1 < chars.len() {
                    toks.push(Tok::Lit(" ".into()));
                }
                i += 2;
            }
            '*' => i += 2,
            '[' => {
                let end = chars[i..]
                    .iter()
                    .position(|x| *x == ']')
                    .ok_or_else(|| FormatError("bracket not closed".into()))?;
                let inner: String = chars[i + 1..i + end].iter().collect();
                bracket(&inner, &mut toks, &mut cond, &mut color);
                i += end + 1;
            }
            '0' | '#' | '?' => {
                toks.push(Tok::Digit(c));
                i += 1;
            }
            '.' => {
                toks.push(Tok::Point);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '%' => {
                toks.push(Tok::Percent);
                i += 1;
            }
            '@' => {
                toks.push(Tok::Text);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+' | '-')) => {
                toks.push(Tok::Exp(chars[i + 1] == '+'));
                i += 2;
            }
            'G' | 'g' if starts_with_ci(&chars, i, "general") => {
                toks.push(Tok::General);
                i += 7;
            }
            'y' | 'Y' => {
                let n = run(&chars, i, c);
                toks.push(Tok::Year(if n <= 2 { 2 } else { 4 }));
                i += n;
            }
            'm' | 'M' => {
                let n = run(&chars, i, c);
                toks.push(Tok::M(n.min(5) as u8));
                i += n;
            }
            'd' | 'D' => {
                let n = run(&chars, i, c);
                toks.push(Tok::Day(n.min(4) as u8));
                i += n;
            }
            'h' | 'H' => {
                let n = run(&chars, i, c);
                toks.push(Tok::Hour(n.min(2) as u8));
                i += n;
            }
            's' | 'S' => {
                let n = run(&chars, i, c);
                toks.push(Tok::Second(n.min(2) as u8));
                i += n;
            }
            'A' | 'a' if starts_with_ci(&chars, i, "am/pm") => {
                toks.push(Tok::AmPm {
                    short: false,
                    lower: c == 'a',
                });
                i += 5;
            }
            'A' | 'a' if starts_with_ci(&chars, i, "a/p") => {
                toks.push(Tok::AmPm {
                    short: true,
                    lower: c == 'a',
                });
                i += 3;
            }
            other => {
                toks.push(Tok::Lit(other.to_string()));
                i += 1;
            }
        }
    }
    Ok(Section {
        toks: finish(toks),
        cond,
        color,
    })
}

fn bracket(
    inner: &str,
    toks: &mut Vec<Tok>,
    cond: &mut Option<(Cmp, f64)>,
    color: &mut Option<Color>,
) {
    let lower = inner.to_ascii_lowercase();
    let named = match lower.as_str() {
        "black" => Some(Color::Black),
        "blue" => Some(Color::Blue),
        "cyan" => Some(Color::Cyan),
        "green" => Some(Color::Green),
        "magenta" => Some(Color::Magenta),
        "red" => Some(Color::Red),
        "white" => Some(Color::White),
        "yellow" => Some(Color::Yellow),
        _ => lower
            .strip_prefix("color")
            .and_then(|n| n.trim().parse().ok())
            .map(Color::Indexed),
    };
    if named.is_some() {
        *color = named;
    } else if let Some((cmp, rest)) = [
        ("<=", Cmp::Le),
        (">=", Cmp::Ge),
        ("<>", Cmp::Ne),
        ("<", Cmp::Lt),
        (">", Cmp::Gt),
        ("=", Cmp::Eq),
    ]
    .iter()
    .find_map(|(p, c)| lower.strip_prefix(p).map(|r| (*c, r)))
    {
        if let Ok(k) = rest.trim().parse() {
            *cond = Some((cmp, k));
        }
    } else if matches!(lower.as_str(), "h" | "hh" | "m" | "mm" | "s" | "ss") {
        #[allow(clippy::cast_possible_truncation)]
        toks.push(Tok::Elapsed(
            lower.chars().next().unwrap_or('h'),
            lower.len() as u8,
        ));
    } else if let Some(sym) = inner.strip_prefix('$') {
        let sym = sym.split('-').next().unwrap_or("");
        if !sym.is_empty() {
            toks.push(Tok::Lit(sym.to_string()));
        }
    }
}

/// Decide months from minutes, join sub-second digits, merge literals.
fn finish(mut toks: Vec<Tok>) -> Vec<Tok> {
    for i in 0..toks.len() {
        let Tok::M(n) = toks[i] else { continue };
        let prev = toks[..i].iter().rev().find(|t| !matches!(t, Tok::Lit(_)));
        let next = toks[i + 1..].iter().find(|t| !matches!(t, Tok::Lit(_)));
        let after_hour = matches!(prev, Some(Tok::Hour(_) | Tok::Elapsed('h', _)));
        let before_second = matches!(next, Some(Tok::Second(_) | Tok::Elapsed('s', _)));
        toks[i] = if after_hour || before_second {
            Tok::Minute(n.min(2))
        } else {
            Tok::Month(n)
        };
    }
    // `ss.00`: a point and zeros right after seconds are fractions of a second.
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let after_seconds = matches!(out.last(), Some(Tok::Second(_) | Tok::Elapsed('s', _)));
        if after_seconds
            && toks[i] == Tok::Point
            && matches!(toks.get(i + 1), Some(Tok::Digit('0')))
        {
            let n = toks[i + 1..]
                .iter()
                .take_while(|t| **t == Tok::Digit('0'))
                .count();
            #[allow(clippy::cast_possible_truncation)]
            out.push(Tok::SubSec(n as u8));
            i += 1 + n;
            continue;
        }
        match (out.last_mut(), &toks[i]) {
            (Some(Tok::Lit(a)), Tok::Lit(b)) => a.push_str(b),
            _ => out.push(toks[i].clone()),
        }
        i += 1;
    }
    out
}
