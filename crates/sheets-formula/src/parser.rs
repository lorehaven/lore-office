//! Formula text to [`Expr`]. Precedence, loosest first: comparison, `&`, `+ -`, `* /`, `^`, unary sign, `%`, `:`.
//! A unary minus binds tighter than `^`, as in Excel: `-2^2` is 4.

use crate::ast::{BinOp, Expr, RangeRef, Reference, UnOp};
use sheets_model::{CellError, CellRef};
use std::fmt;

/// Why a formula did not parse, and where (a character offset into the text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub at: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    /// A name: function, reference, boolean or defined name, possibly `$`-anchored; or a quoted sheet name.
    Word(String),
    QuotedSheet(String),
    ErrorLit(CellError),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    Colon,
    Bang,
}

const MAX_DEPTH: usize = 64;

/// Parse a formula; a leading `=` is allowed.
pub fn parse(text: &str) -> Result<Expr, ParseError> {
    let text = text.strip_prefix('=').unwrap_or(text);
    let toks = lex(text)?;
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let e = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(p.err("unexpected input"));
    }
    Ok(e)
}

#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
fn lex(text: &str) -> Result<Vec<(Tok, usize)>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let err = |m: &str| ParseError {
            message: m.to_string(),
            at: start,
        };
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '0'..='9' | '.' => {
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') {
                    let mut k = j + 1;
                    if k < chars.len() && (chars[k] == '+' || chars[k] == '-') {
                        k += 1;
                    }
                    if k < chars.len() && chars[k].is_ascii_digit() {
                        while k < chars.len() && chars[k].is_ascii_digit() {
                            k += 1;
                        }
                        j = k;
                    }
                }
                let s: String = chars[i..j].iter().collect();
                let n: f64 = s.parse().map_err(|_| err("bad number"))?;
                out.push((Tok::Num(n), start));
                i = j;
            }
            '"' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(err("text is not closed")),
                        Some('"') if chars.get(i + 1) == Some(&'"') => {
                            s.push('"');
                            i += 2;
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some(ch) => {
                            s.push(*ch);
                            i += 1;
                        }
                    }
                }
                out.push((Tok::Str(s), start));
            }
            '\'' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(err("sheet name is not closed")),
                        Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                            s.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(ch) => {
                            s.push(*ch);
                            i += 1;
                        }
                    }
                }
                out.push((Tok::QuotedSheet(s), start));
            }
            '#' => {
                let mut j = i + 1;
                while j < chars.len()
                    && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '/' | '!' | '?'))
                {
                    j += 1;
                    if chars[j - 1] == '!' || chars[j - 1] == '?' {
                        break;
                    }
                }
                let s: String = chars[i..j].iter().collect();
                let e = CellError::parse(&s).ok_or_else(|| err("unknown error value"))?;
                out.push((Tok::ErrorLit(e), start));
                i = j;
            }
            '(' => {
                out.push((Tok::LParen, start));
                i += 1;
            }
            ')' => {
                out.push((Tok::RParen, start));
                i += 1;
            }
            ',' | ';' => {
                out.push((Tok::Comma, start));
                i += 1;
            }
            ':' => {
                out.push((Tok::Colon, start));
                i += 1;
            }
            '!' => {
                out.push((Tok::Bang, start));
                i += 1;
            }
            '+' | '-' | '*' | '/' | '^' | '&' | '%' | '=' => {
                let op = match c {
                    '+' => "+",
                    '-' => "-",
                    '*' => "*",
                    '/' => "/",
                    '^' => "^",
                    '&' => "&",
                    '%' => "%",
                    _ => "=",
                };
                out.push((Tok::Op(op), start));
                i += 1;
            }
            '<' | '>' => {
                let next = chars.get(i + 1);
                let (op, len) = match (c, next) {
                    ('<', Some('=')) => ("<=", 2),
                    ('<', Some('>')) => ("<>", 2),
                    ('<', _) => ("<", 1),
                    ('>', Some('=')) => (">=", 2),
                    _ => (">", 1),
                };
                out.push((Tok::Op(op), start));
                i += len;
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                let mut j = i;
                while j < chars.len()
                    && (chars[j].is_alphanumeric() || matches!(chars[j], '_' | '$' | '.'))
                {
                    j += 1;
                }
                out.push((Tok::Word(chars[i..j].iter().collect()), start));
                i = j;
            }
            _ => return Err(err("unexpected character")),
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn err(&self, m: &str) -> ParseError {
        let at = self
            .toks
            .get(self.pos)
            .map_or_else(|| self.toks.last().map_or(0, |(_, a)| a + 1), |(_, a)| *a);
        ParseError {
            message: m.to_string(),
            at,
        }
    }

    fn eat_op(&mut self, ops: &[&'static str]) -> Option<&'static str> {
        if let Some(Tok::Op(o)) = self.peek()
            && ops.contains(o)
        {
            let o = *o;
            self.pos += 1;
            return Some(o);
        }
        None
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("formula is nested too deeply"));
        }
        let e = self.comparison();
        self.depth -= 1;
        e
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.concat()?;
        while let Some(op) = self.eat_op(&["=", "<>", "<=", ">=", "<", ">"]) {
            let right = self.concat()?;
            let op = match op {
                "=" => BinOp::Eq,
                "<>" => BinOp::Ne,
                "<=" => BinOp::Le,
                ">=" => BinOp::Ge,
                "<" => BinOp::Lt,
                _ => BinOp::Gt,
            };
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn concat(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.additive()?;
        while self.eat_op(&["&"]).is_some() {
            let right = self.additive()?;
            left = Expr::Binary(BinOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        while let Some(op) = self.eat_op(&["+", "-"]) {
            let right = self.multiplicative()?;
            let op = if op == "+" { BinOp::Add } else { BinOp::Sub };
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.power()?;
        while let Some(op) = self.eat_op(&["*", "/"]) {
            let right = self.power()?;
            let op = if op == "*" { BinOp::Mul } else { BinOp::Div };
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn power(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        while self.eat_op(&["^"]).is_some() {
            let right = self.unary()?;
            left = Expr::Binary(BinOp::Pow, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if let Some(op) = self.eat_op(&["-", "+"]) {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(self.err("formula is nested too deeply"));
            }
            let inner = self.unary();
            self.depth -= 1;
            let op = if op == "-" { UnOp::Neg } else { UnOp::Plus };
            return Ok(Expr::Unary(op, Box::new(inner?)));
        }
        let mut e = self.primary()?;
        while self.eat_op(&["%"]).is_some() {
            e = Expr::Percent(Box::new(e));
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let Some((tok, _)) = self.toks.get(self.pos).cloned() else {
            return Err(self.err("the formula ends too early"));
        };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Number(n)),
            Tok::Str(s) => Ok(Expr::Text(s)),
            Tok::ErrorLit(e) => Ok(Expr::Error(e)),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect_rparen()?;
                Ok(e)
            }
            Tok::QuotedSheet(name) => {
                if self.peek() != Some(&Tok::Bang) {
                    return Err(self.err("expected ! after a sheet name"));
                }
                self.pos += 1;
                self.reference_after_sheet(Some(name))
            }
            Tok::Word(w) => {
                if self.peek() == Some(&Tok::Bang) {
                    self.pos += 1;
                    return self.reference_after_sheet(Some(w));
                }
                if self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    return self.call(&w);
                }
                self.word(&w)
            }
            _ => {
                self.pos -= 1;
                Err(self.err("unexpected input"))
            }
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ParseError> {
        if self.peek() == Some(&Tok::RParen) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err("expected )"))
        }
    }

    fn call(&mut self, name: &str) -> Result<Expr, ParseError> {
        let mut args = Vec::new();
        if self.peek() == Some(&Tok::RParen) {
            self.pos += 1;
            return Ok(Expr::Call(name.to_ascii_uppercase(), args));
        }
        loop {
            // An omitted argument (`IF(A1,,2)`) is an empty text-less value: 0 in arithmetic.
            if matches!(self.peek(), Some(Tok::Comma | Tok::RParen)) {
                args.push(Expr::Number(0.0));
            } else {
                args.push(self.expr()?);
            }
            match self.peek() {
                Some(Tok::Comma) => self.pos += 1,
                Some(Tok::RParen) => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected , or )")),
            }
        }
        Ok(Expr::Call(name.to_ascii_uppercase(), args))
    }

    fn reference_after_sheet(&mut self, sheet: Option<String>) -> Result<Expr, ParseError> {
        let Some((Tok::Word(w), _)) = self.toks.get(self.pos).cloned() else {
            return Err(self.err("expected a cell after the sheet name"));
        };
        self.pos += 1;
        let start = reference(&w, sheet.clone()).ok_or_else(|| self.err("not a cell reference"))?;
        self.finish_range(start, sheet)
    }

    fn word(&mut self, w: &str) -> Result<Expr, ParseError> {
        if let Some(r) = reference(w, None) {
            return self.finish_range(r, None);
        }
        if w.eq_ignore_ascii_case("true") {
            return Ok(Expr::Bool(true));
        }
        if w.eq_ignore_ascii_case("false") {
            return Ok(Expr::Bool(false));
        }
        Ok(Expr::Name(w.to_string()))
    }

    fn finish_range(
        &mut self,
        start: Reference,
        sheet: Option<String>,
    ) -> Result<Expr, ParseError> {
        if self.peek() != Some(&Tok::Colon) {
            return Ok(Expr::Ref(start));
        }
        self.pos += 1;
        let Some((Tok::Word(w), _)) = self.toks.get(self.pos).cloned() else {
            return Err(self.err("expected a cell after :"));
        };
        let end = reference(&w, sheet.clone()).ok_or_else(|| self.err("not a cell reference"))?;
        self.pos += 1;
        Ok(Expr::Range(RangeRef { sheet, start, end }))
    }
}

fn reference(w: &str, sheet: Option<String>) -> Option<Reference> {
    let (cell, col_abs, row_abs) = CellRef::parse_anchored(w)?;
    Some(Reference {
        sheet,
        cell,
        col_abs,
        row_abs,
    })
}
