#![allow(
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]
use sheets_formula::*;
use sheets_model::{CellError, CellRef, Value};
use std::collections::HashMap;

#[path = "unit/functions_tests.rs"]
mod functions_tests;

#[derive(Debug)]
pub struct Grid(HashMap<(Option<String>, CellRef), Value>);

impl Context for Grid {
    fn cell(&self, sheet: Option<&str>, at: CellRef) -> Value {
        if sheet == Some("Missing") {
            return Value::Error(CellError::Ref);
        }
        self.0
            .get(&(sheet.map(str::to_string), at))
            .cloned()
            .unwrap_or_default()
    }
}

fn grid() -> Grid {
    let mut m = HashMap::new();
    for (a, v) in [
        ("A1", Value::Number(1.0)),
        ("A2", Value::Number(2.0)),
        ("A3", Value::Number(3.0)),
        ("B1", Value::Text("x".into())),
        ("B2", Value::Bool(true)),
        ("C1", Value::Error(CellError::Div0)),
    ] {
        m.insert((None, CellRef::parse(a).unwrap()), v);
    }
    m.insert(
        (Some("Other".into()), CellRef::parse("A1").unwrap()),
        Value::Number(10.0),
    );
    Grid(m)
}

fn run(f: &str) -> Value {
    eval(&parse(f).unwrap(), &grid())
}

const fn n(x: f64) -> Value {
    Value::Number(x)
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

const fn err(e: CellError) -> Value {
    Value::Error(e)
}

#[quench_test::test]
fn arithmetic_follows_precedence() {
    assert_eq!(run("=1+2*3"), n(7.0));
    assert_eq!(run("(1+2)*3"), n(9.0));
    assert_eq!(run("2^3^2"), n(64.0)); // left to right, as in Excel
    assert_eq!(run("-2^2"), n(4.0)); // unary binds tighter than ^
    assert_eq!(run("10-4-3"), n(3.0));
    assert_eq!(run("50%"), n(0.5));
    assert_eq!(run("200*10%"), n(20.0));
    assert_eq!(run("1.5e2+0.5"), n(150.5));
    assert_eq!(run("--3"), n(3.0));
    assert_eq!(run("+4"), n(4.0));
}

#[quench_test::test]
fn arithmetic_errors_are_values() {
    assert_eq!(run("1/0"), err(CellError::Div0));
    assert_eq!(run("0^0"), err(CellError::Num));
    assert_eq!(run("(-8)^0.5"), err(CellError::Num));
    assert_eq!(run("\"a\"+1"), err(CellError::Value));
    assert_eq!(run("\"3\"+1"), n(4.0));
    assert_eq!(run("C1+1"), err(CellError::Div0));
    assert_eq!(run("#N/A"), err(CellError::NA));
    assert_eq!(run("1e308*10"), err(CellError::Num));
}

#[quench_test::test]
fn references_and_ranges_read_the_grid() {
    assert_eq!(run("A1+A2"), n(3.0));
    assert_eq!(run("$A$1+A$2+$A3"), n(6.0));
    assert_eq!(run("Other!A1"), n(10.0));
    assert_eq!(run("'Other'!A1*2"), n(20.0));
    assert_eq!(run("Missing!A1"), err(CellError::Ref));
    assert_eq!(run("D9"), Value::Empty);
    assert_eq!(run("SUM(A1:A3)"), n(6.0));
    assert_eq!(run("SUM(A3:A1)"), n(6.0));
    assert_eq!(run("SUM(Other!A1:A2)"), n(10.0));
    assert_eq!(run("SUM(A1:Z1048576)"), err(CellError::Value));
}

#[quench_test::test]
fn comparisons_and_text() {
    assert_eq!(run("1<2"), Value::Bool(true));
    assert_eq!(run("2<=2"), Value::Bool(true));
    assert_eq!(run("3<>3"), Value::Bool(false));
    assert_eq!(run("\"a\"=\"A\""), Value::Bool(true));
    assert_eq!(run("\"a\"<\"b\""), Value::Bool(true));
    assert_eq!(run("1<\"a\""), Value::Bool(true)); // numbers sort before text
    assert_eq!(run("\"a\"<TRUE"), Value::Bool(true)); // text before booleans
    assert_eq!(run("D9=0"), Value::Bool(true));
    assert_eq!(run("D9=\"\""), Value::Bool(true));
    assert_eq!(run("\"a\"&\"b\"&1"), t("ab1"));
    assert_eq!(run("\"say \"\"hi\"\"\""), t("say \"hi\""));
    assert_eq!(run("1+1=2"), Value::Bool(true));
}

#[quench_test::test]
fn aggregates_skip_text_in_ranges() {
    assert_eq!(run("SUM(A1:A3,10)"), n(16.0));
    assert_eq!(run("SUM(A1:B2)"), n(3.0)); // "x" and TRUE in the range are skipped
    assert_eq!(run("SUM(\"4\",TRUE)"), n(5.0)); // directly given values are coerced
    assert_eq!(run("SUM(\"x\")"), err(CellError::Value));
    assert_eq!(run("SUM(A1:C1)"), err(CellError::Div0));
    assert_eq!(run("AVERAGE(A1:A3)"), n(2.0));
    assert_eq!(run("AVERAGE(B1:B2)"), err(CellError::Div0));
    assert_eq!(run("MIN(A1:A3)"), n(1.0));
    assert_eq!(run("MAX(A1:A3,7)"), n(7.0));
    assert_eq!(run("MAX(B1)"), err(CellError::Value));
    assert_eq!(run("MAX(B1:B2)"), n(0.0));
    assert_eq!(run("COUNT(A1:B3)"), n(3.0));
    assert_eq!(run("COUNT(1,\"2\",\"x\")"), n(2.0));
    assert_eq!(run("COUNTA(A1:B3)"), n(5.0));
}

#[quench_test::test]
fn logic_functions() {
    assert_eq!(run("IF(A1=1,\"y\",\"n\")"), t("y"));
    assert_eq!(run("IF(A1=2,\"y\",\"n\")"), t("n"));
    assert_eq!(run("IF(A1=2,\"y\")"), Value::Bool(false));
    assert_eq!(run("IF(TRUE,1,1/0)"), n(1.0)); // the other branch is not evaluated
    assert_eq!(run("IF(\"x\",1,2)"), err(CellError::Value));
    assert_eq!(run("IFERROR(1/0,-1)"), n(-1.0));
    assert_eq!(run("IFERROR(5,-1)"), n(5.0));
    assert_eq!(run("AND(TRUE,A1=1,2)"), Value::Bool(true));
    assert_eq!(run("AND(TRUE,0)"), Value::Bool(false));
    assert_eq!(run("OR(FALSE,A2=2)"), Value::Bool(true));
    assert_eq!(run("AND(B1)"), err(CellError::Value)); // text in a range is ignored, nothing is left
    assert_eq!(run("NOT(FALSE)"), Value::Bool(true));
    assert_eq!(run("TRUE()"), Value::Bool(true));
}

#[quench_test::test]
fn math_functions() {
    assert_eq!(run("ABS(-3)"), n(3.0));
    assert_eq!(run("INT(-1.5)"), n(-2.0));
    assert_eq!(run("SQRT(16)"), n(4.0));
    assert_eq!(run("SQRT(-1)"), err(CellError::Num));
    assert_eq!(run("POWER(2,10)"), n(1024.0));
    assert_eq!(run("MOD(7,3)"), n(1.0));
    assert_eq!(run("MOD(-7,3)"), n(2.0));
    assert_eq!(run("MOD(1,0)"), err(CellError::Div0));
    assert_eq!(run("ROUND(2.5,0)"), n(3.0));
    assert_eq!(run("ROUND(-2.5,0)"), n(-3.0));
    assert_eq!(run("ROUND(1.2345,2)"), n(1.23));
    assert_eq!(run("ROUND(1234,-2)"), n(1200.0));
    assert_eq!(run("ROUND(1,999)"), err(CellError::Num));
    assert!(matches!(run("PI()"), Value::Number(p) if (p - std::f64::consts::PI).abs() < 1e-12));
}

#[quench_test::test]
fn text_functions() {
    assert_eq!(run("LEN(\"héllo\")"), n(5.0));
    assert_eq!(run("UPPER(\"ab\")&LOWER(\"CD\")"), t("ABcd"));
    assert_eq!(run("TRIM(\"  a   b  \")"), t("a b"));
    assert_eq!(run("CONCAT(\"a\",A1:A2)"), t("a12"));
    assert_eq!(run("CONCATENATE(\"a\",\"b\")"), t("ab"));
    assert_eq!(run("LEFT(\"hello\",2)"), t("he"));
    assert_eq!(run("LEFT(\"hello\")"), t("h"));
    assert_eq!(run("RIGHT(\"hello\",3)"), t("llo"));
    assert_eq!(run("RIGHT(\"hi\",9)"), t("hi"));
    assert_eq!(run("MID(\"hello\",2,3)"), t("ell"));
    assert_eq!(run("MID(\"hello\",0,3)"), err(CellError::Value));
    assert_eq!(run("LEFT(\"a\",-1)"), err(CellError::Value));
}

#[quench_test::test]
fn unknown_things_are_errors() {
    assert_eq!(run("NOSUCH(1)"), err(CellError::Name));
    assert_eq!(run("Total+1"), err(CellError::Name));
    assert_eq!(run("ABS()"), err(CellError::Value));
    assert_eq!(run("ABS(1,2)"), err(CellError::Value));
    assert_eq!(run("IF(1)"), err(CellError::Value));
}

#[quench_test::test]
fn bad_formulas_report_where() {
    for (f, at) in [
        ("1+", 2),
        ("(1", 2),
        ("SUM(1", 5),
        ("1 2", 2),
        ("\"abc", 0),
        ("3 $ 4", 2),
        ("A1:", 3),
        ("#BOGUS", 0),
        ("'S'+1", 3),
    ] {
        let e = parse(f).unwrap_err();
        assert_eq!(e.at, at, "{f}: {e}");
        assert!(e.to_string().ends_with(&format!("(at {at})")));
    }
    let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
    assert!(parse(&deep).unwrap_err().message.contains("deeply"));
    let neg = format!("{}1", "-".repeat(200));
    assert!(parse(&neg).is_err());
}

#[quench_test::test]
fn parsing_keeps_anchors_and_the_leading_equals() {
    let Expr::Ref(r) = parse("=$B$3").unwrap() else {
        panic!("not a ref")
    };
    assert_eq!(
        (r.cell, r.col_abs, r.row_abs),
        (CellRef::new(2, 1), true, true)
    );
    assert_eq!(parse("1+2").unwrap(), parse("=1+2").unwrap());
    assert!(matches!(parse("IF(A1,,2)").unwrap(), Expr::Call(n, a) if n == "IF" && a.len() == 3));
    assert!(matches!(parse("sum(a1:a2)").unwrap(), Expr::Call(n, _) if n == "SUM"));
    assert!(matches!(parse("NOW()").unwrap(), Expr::Call(_, a) if a.is_empty()));
}

#[quench_test::test]
fn formulas_print_back_to_the_same_tree() {
    for f in [
        "1+2*3",
        "(1+2)*3",
        "2^3^2",
        "2^(3^2)",
        "-2^2",
        "-(1+2)",
        "10-(4-3)",
        "10-4-3",
        "50%",
        "(1+2)%",
        "-A1%",
        "A1&\"x\"&B2",
        "IF(A1>0,\"y\",\"n\")",
        "SUM(A1:B2,$C$3)",
        "Other!A1+'My Sheet'!$B2",
        "'It''s'!A1",
        "1<>2",
        "1+1=2",
        "(1=1)=TRUE",
        "\"say \"\"hi\"\"\"",
        "#N/A",
        "Total*2",
        "SUM()",
        "1E+21",
        "0.000001",
        "A1:A3",
        "-(-A1)",
        "2*-3",
    ] {
        let tree = parse(f).unwrap();
        let text = print(&tree);
        assert_eq!(parse(&text).unwrap(), tree, "{f} printed as {text}");
        assert_eq!(print(&parse(&text).unwrap()), text, "{f} is not stable");
    }
    assert_eq!(print(&parse("(1+2)*3").unwrap()), "(1+2)*3");
    assert_eq!(print(&parse("1+(2*3)").unwrap()), "1+2*3");
    assert_eq!(print(&parse("sum(a1:b2)").unwrap()), "SUM(A1:B2)");
    assert_eq!(print(&parse("'Data'!A1").unwrap()), "Data!A1");
    assert_eq!(print(&parse("'a b'!A1").unwrap()), "'a b'!A1");
    assert_eq!(print(&parse("'A1'!A1").unwrap()), "'A1'!A1");
    assert_eq!(print(&parse("'1x'!A1").unwrap()), "'1x'!A1");
    assert_eq!(print(&parse("IF(A1,,2)").unwrap()), "IF(A1,0,2)");
}

#[quench_test::test]
fn copying_a_formula_moves_relative_references() {
    let shift = |f: &str, r, c| print(&parse(f).unwrap().shifted(r, c));
    assert_eq!(shift("A1+$B$2+C$3+$D4", 1, 1), "B2+$B$2+D$3+$D5");
    assert_eq!(shift("SUM(A1:B2)", 2, 0), "SUM(A3:B4)");
    assert_eq!(shift("Other!A1*2", 0, 3), "Other!D1*2");
    assert_eq!(shift("A2", -1, 0), "A1");
    assert_eq!(shift("A1", -1, 0), "#REF!");
    assert_eq!(shift("SUM(A1:B2)", -1, 0), "SUM(#REF!)");
    assert_eq!(shift("A1", 0, 16_384), "#REF!");
    assert_eq!(shift("-A1%+\"t\"&1", 1, 1), "-B2%+\"t\"&1");
    assert_eq!(shift("$A$1", 5, 5), "$A$1");
}

quench_test::main!();

#[quench_test::test]
fn any_mix_of_formula_pieces_parses_or_errors_and_evaluates_without_a_panic() {
    let pieces = [
        "SUM(",
        "IF(",
        "INDEX(",
        "XLOOKUP(",
        "LET(",
        "FILTER(",
        "SORT(",
        "SEQUENCE(",
        "OFFSET(",
        "INDIRECT(",
        "TEXT(",
        "A1",
        "B2:C3",
        "$A$1",
        "Other!A1",
        "'Sh eet'!B1",
        ")",
        ",",
        ";",
        "+",
        "-",
        "*",
        "/",
        "^",
        "&",
        "=",
        "<>",
        "%",
        "1",
        "0",
        "1E308",
        "\"txt\"",
        "\"",
        "TRUE",
        "#N/A",
        "{1,2;3,4}",
        "{",
        "}",
        ":",
        ".",
        " ",
        "x",
        "_xlfn.",
        "[",
        "]",
        "@",
    ];
    let g = grid();
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..4000 {
        let mut text = String::new();
        for _ in 0..(3 + state % 14) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            text.push_str(pieces[(state % pieces.len() as u64) as usize]);
        }
        if let Ok(expr) = parse(&text) {
            let _ = print(&expr);
            let _ = eval(&expr, &g);
        }
    }
}
