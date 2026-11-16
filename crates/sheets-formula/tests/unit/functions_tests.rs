#![allow(clippy::cast_precision_loss)]
use super::{Grid, err, n, t};
use sheets_formula::{eval, parse};
use sheets_model::{CellError, CellRef, Value};

fn data() -> Grid {
    let mut m = std::collections::HashMap::new();
    let mut put = |a: &str, v: Value| {
        m.insert((None, CellRef::parse(a).unwrap()), v);
    };
    for (i, v) in [n(1.0), n(2.0), n(3.0), t("apple"), t("Apricot")]
        .into_iter()
        .enumerate()
    {
        put(&format!("A{}", i + 1), v);
        put(&format!("B{}", i + 1), n(10.0 * (i + 1) as f64));
    }
    for (i, v) in ["a", "b", "c", "d"].into_iter().enumerate() {
        put(&format!("D{}", i + 1), n((i + 1) as f64));
        put(&format!("E{}", i + 1), t(v));
    }
    put("F1", t("apple"));
    put("G1", n(1.0));
    put("F2", t("pear"));
    put("G2", n(2.0));
    for (c, top, bottom) in [("H", 10.0, "x"), ("I", 20.0, "y"), ("J", 30.0, "z")] {
        put(&format!("{c}1"), n(top));
        put(&format!("{c}2"), t(bottom));
    }
    for (i, v) in [30.0, 20.0, 10.0].into_iter().enumerate() {
        put(&format!("K{}", i + 1), n(v));
    }
    for (i, (a, b)) in [(1.0, "p"), (1.0, "q"), (2.0, "r")].into_iter().enumerate() {
        put(&format!("M{}", i + 1), n(a));
        put(&format!("N{}", i + 1), t(b));
    }
    for (i, v) in [1.0, 2.0, 3.0, 4.0, 5.0].into_iter().enumerate() {
        put(&format!("Q{}", i + 1), n(v));
        put(&format!("R{}", i + 1), n(v * 2.0));
    }
    for (i, v) in [-70_000.0, 12_000.0, 15_000.0, 18_000.0, 21_000.0, 26_000.0]
        .into_iter()
        .enumerate()
    {
        put(&format!("P{}", i + 1), n(v));
    }
    Grid(m)
}

fn run(f: &str) -> Value {
    eval(&parse(f).unwrap(), &data())
}

#[track_caller]
fn near(f: &str, want: f64) {
    match run(f) {
        Value::Number(x) => assert!((x - want).abs() < 1e-9, "{f} = {x}, want {want}"),
        other => panic!("{f} = {other:?}, want {want}"),
    }
}

#[quench_test::test]
fn math_library() {
    assert_eq!(run("SIGN(-5)"), n(-1.0));
    assert_eq!(run("SIGN(0)"), n(0.0));
    assert_eq!(run("EXP(0)"), n(1.0));
    assert_eq!(run("LN(1)"), n(0.0));
    assert_eq!(run("LN(0)"), err(CellError::Num));
    near("LOG(100)", 2.0);
    near("LOG(8,2)", 3.0);
    near("LOG10(1000)", 3.0);
    assert_eq!(run("LOG(5,1)"), err(CellError::Div0));
    near("ATAN2(1,1)", std::f64::consts::FRAC_PI_4);
    near("DEGREES(PI())", 180.0);
    near("RADIANS(180)", std::f64::consts::PI);
    near("SIN(0)+COS(0)+TAN(0)", 1.0);
    near("ASIN(1)+ACOS(1)+ATAN(0)", std::f64::consts::FRAC_PI_2);
    assert_eq!(run("ASIN(2)"), err(CellError::Num));
    assert_eq!(run("FACT(5)"), n(120.0));
    assert_eq!(run("FACT(0)"), n(1.0));
    assert_eq!(run("FACT(-1)"), err(CellError::Num));
    assert_eq!(run("FACT(171)"), err(CellError::Num));
    assert_eq!(run("GCD(12,18)"), n(6.0));
    assert_eq!(run("GCD(0,5)"), n(5.0));
    assert_eq!(run("LCM(4,6)"), n(12.0));
    assert_eq!(run("LCM(A1:A3)"), n(6.0));
    assert_eq!(run("GCD(-1)"), err(CellError::Num));
    assert_eq!(run("EVEN(3)"), n(4.0));
    assert_eq!(run("EVEN(-1)"), n(-2.0));
    assert_eq!(run("ODD(2)"), n(3.0));
    assert_eq!(run("ODD(-2)"), n(-3.0));
    assert_eq!(run("ODD(0)"), n(1.0));
    assert_eq!(run("QUOTIENT(7,2)"), n(3.0));
    assert_eq!(run("QUOTIENT(-7,2)"), n(-3.0));
    assert_eq!(run("QUOTIENT(1,0)"), err(CellError::Div0));
    assert_eq!(run("PRODUCT(2,3,4)"), n(24.0));
    assert_eq!(run("SUMSQ(3,4)"), n(25.0));
    assert_eq!(run("POWER(0,-1)"), err(CellError::Div0));
}

#[quench_test::test]
fn rounding_matches_a_sheet() {
    assert_eq!(run("ROUND(1.005,2)"), n(1.01));
    assert_eq!(run("ROUND(2.675,2)"), n(2.68));
    assert_eq!(run("ROUNDUP(3.2,0)"), n(4.0));
    assert_eq!(run("ROUNDUP(-3.2,0)"), n(-4.0));
    assert_eq!(run("ROUNDUP(2.15,1)"), n(2.2));
    assert_eq!(run("ROUNDDOWN(-3.9,0)"), n(-3.0));
    assert_eq!(run("ROUNDDOWN(3.999,2)"), n(3.99));
    assert_eq!(run("TRUNC(8.9)"), n(8.0));
    assert_eq!(run("TRUNC(-8.9)"), n(-8.0));
    assert_eq!(run("CEILING(2.5,1)"), n(3.0));
    assert_eq!(run("CEILING(-2.5,2)"), n(-2.0));
    assert_eq!(run("CEILING(5,0)"), n(0.0));
    assert_eq!(run("CEILING(1,-1)"), err(CellError::Num));
    assert_eq!(run("FLOOR(2.5,1)"), n(2.0));
    assert_eq!(run("FLOOR(-2.5,2)"), n(-4.0));
    assert_eq!(run("FLOOR(7.3)"), n(7.0));
}

#[quench_test::test]
fn statistics() {
    assert_eq!(run("MEDIAN(1,3,2)"), n(2.0));
    assert_eq!(run("MEDIAN(1,2,3,4)"), n(2.5));
    assert_eq!(run("MEDIAN(B4:B5)"), n(45.0));
    assert_eq!(run("MEDIAN(E1:E4)"), err(CellError::Num));
    assert_eq!(run("MODE(1,2,2,3)"), n(2.0));
    assert_eq!(run("MODE.SNGL(1,2)"), err(CellError::NA));
    assert_eq!(run("VAR.S(A1:A3)"), n(1.0));
    assert_eq!(run("STDEV(A1:A3)"), n(1.0));
    near("VAR.P(A1:A3)", 2.0 / 3.0);
    near("STDEV.P(A1:A3)", (2.0f64 / 3.0).sqrt());
    assert_eq!(run("VAR(1)"), err(CellError::Div0));
    assert_eq!(run("VARP(5)"), n(0.0));
    assert_eq!(run("LARGE(A1:A3,1)"), n(3.0));
    assert_eq!(run("SMALL(A1:A3,2)"), n(2.0));
    assert_eq!(run("LARGE(A1:A3,4)"), err(CellError::Num));
    assert_eq!(run("LARGE(A1:A3,0)"), err(CellError::Num));
    assert_eq!(run("RANK(2,A1:A3)"), n(2.0));
    assert_eq!(run("RANK(3,A1:A3)"), n(1.0));
    assert_eq!(run("RANK(3,A1:A3,1)"), n(3.0));
    assert_eq!(run("RANK(9,A1:A3)"), err(CellError::NA));
    assert_eq!(run("COUNTBLANK(A1:A8)"), n(3.0));
    assert_eq!(run("SUMPRODUCT(A1:A3,A1:A3)"), n(14.0));
    assert_eq!(run("SUMPRODUCT(A1:A3,B1:B3)"), n(140.0));
    assert_eq!(run("SUMPRODUCT(A1:A3,B1:B2)"), err(CellError::Value));
    assert_eq!(run("SUMPRODUCT(A1:A5,B1:B5)"), n(140.0)); // text counts as zero
}

#[quench_test::test]
fn conditions_over_ranges() {
    assert_eq!(run("COUNTIF(A1:A5,\">1\")"), n(2.0));
    assert_eq!(run("COUNTIF(A1:A5,2)"), n(1.0));
    assert_eq!(run("COUNTIF(A1:A5,\"=3\")"), n(1.0));
    assert_eq!(run("COUNTIF(A1:A5,\"ap*\")"), n(2.0));
    assert_eq!(run("COUNTIF(A1:A5,\"APPL?\")"), n(1.0));
    assert_eq!(run("COUNTIF(A1:A5,\"<>apple\")"), n(4.0));
    assert_eq!(run("COUNTIF(A1:A5,\">=b\")"), n(0.0));
    assert_eq!(run("COUNTIF(A1:A8,\"\")"), n(3.0));
    assert_eq!(run("COUNTIF(A1:A8,\"<>\")"), n(5.0));
    assert_eq!(run("SUMIF(A1:A5,\">1\",B1:B5)"), n(50.0));
    assert_eq!(run("SUMIF(A1:A5,\"ap*\",B1:B5)"), n(90.0));
    assert_eq!(run("SUMIF(B1:B5,\">=30\")"), n(120.0));
    assert_eq!(run("AVERAGEIF(A1:A3,\">1\",B1:B3)"), n(25.0));
    assert_eq!(run("AVERAGEIF(A1:A5,\">100\",B1:B5)"), err(CellError::Div0));
    assert_eq!(run("SUMIFS(B1:B5,A1:A5,\">0\",B1:B5,\"<40\")"), n(60.0));
    assert_eq!(run("COUNTIFS(A1:A5,\">0\",B1:B5,\">=20\")"), n(2.0));
    assert_eq!(run("MINIFS(B1:B5,A1:A5,\">1\")"), n(20.0));
    assert_eq!(run("MAXIFS(B1:B5,A1:A5,\">1\")"), n(30.0));
    assert_eq!(run("AVERAGEIFS(B1:B5,A1:A5,\">1\")"), n(25.0));
    assert_eq!(run("SUMIFS(B1:B5,A1:A3,\">0\")"), err(CellError::Value));
    assert_eq!(run("SUMIFS(B1:B5,A1:A5)"), err(CellError::Value));
    assert_eq!(run("COUNTIFS(A1:A5)"), err(CellError::Value));
}

#[quench_test::test]
fn logic_and_information() {
    assert_eq!(run("IFS(A1>5,\"a\",A1>0,\"b\")"), t("b"));
    assert_eq!(run("IFS(FALSE,1)"), err(CellError::NA));
    assert_eq!(run("IFS(TRUE)"), err(CellError::Value));
    assert_eq!(run("SWITCH(2,1,\"a\",2,\"b\",\"z\")"), t("b"));
    assert_eq!(run("SWITCH(9,1,\"a\",\"z\")"), t("z"));
    assert_eq!(run("SWITCH(9,1,\"a\")"), err(CellError::NA));
    assert_eq!(run("SWITCH(\"A\",\"a\",1)"), n(1.0));
    assert_eq!(run("IFNA(NA(),5)"), n(5.0));
    assert_eq!(run("IFNA(1/0,5)"), err(CellError::Div0));
    assert_eq!(run("XOR(TRUE,TRUE,TRUE)"), Value::Bool(true));
    assert_eq!(run("XOR(TRUE,TRUE)"), Value::Bool(false));
    assert_eq!(run("ISNUMBER(A1)"), Value::Bool(true));
    assert_eq!(run("ISNUMBER(A4)"), Value::Bool(false));
    assert_eq!(run("ISTEXT(A4)"), Value::Bool(true));
    assert_eq!(run("ISNONTEXT(A1)"), Value::Bool(true));
    assert_eq!(run("ISLOGICAL(TRUE)"), Value::Bool(true));
    assert_eq!(run("ISBLANK(Z9)"), Value::Bool(true));
    assert_eq!(run("ISBLANK(A1)"), Value::Bool(false));
    assert_eq!(run("ISERROR(1/0)"), Value::Bool(true));
    assert_eq!(run("ISNA(NA())"), Value::Bool(true));
    assert_eq!(run("ISERR(NA())"), Value::Bool(false));
    assert_eq!(run("ISERR(1/0)"), Value::Bool(true));
    assert_eq!(run("ISEVEN(4)"), Value::Bool(true));
    assert_eq!(run("ISODD(3)"), Value::Bool(true));
    assert_eq!(run("ISEVEN(-3)"), Value::Bool(false));
    assert_eq!(run("N(\"x\")"), n(0.0));
    assert_eq!(run("N(TRUE)"), n(1.0));
    assert_eq!(run("N(7)"), n(7.0));
}

#[quench_test::test]
fn more_text() {
    assert_eq!(run("FIND(\"b\",\"abc\")"), n(2.0));
    assert_eq!(run("FIND(\"B\",\"abc\")"), err(CellError::Value));
    assert_eq!(run("FIND(\"c\",\"abcabc\",4)"), n(6.0));
    assert_eq!(run("FIND(\"\",\"abc\")"), n(1.0));
    assert_eq!(run("FIND(\"a\",\"abc\",0)"), err(CellError::Value));
    assert_eq!(run("SEARCH(\"B\",\"abc\")"), n(2.0));
    assert_eq!(run("SEARCH(\"a?c\",\"xabc\")"), n(2.0));
    assert_eq!(run("SEARCH(\"b*d\",\"abcd\")"), n(2.0));
    assert_eq!(run("SEARCH(\"z\",\"abc\")"), err(CellError::Value));
    assert_eq!(run("SUBSTITUTE(\"a-b-c\",\"-\",\"+\")"), t("a+b+c"));
    assert_eq!(run("SUBSTITUTE(\"a-b-c\",\"-\",\"+\",2)"), t("a-b+c"));
    assert_eq!(run("SUBSTITUTE(\"a-b-c\",\"-\",\"+\",5)"), t("a-b-c"));
    assert_eq!(run("SUBSTITUTE(\"abc\",\"\",\"x\")"), t("abc"));
    assert_eq!(run("REPLACE(\"abcdef\",2,3,\"X\")"), t("aXef"));
    assert_eq!(run("REPLACE(\"abc\",9,1,\"X\")"), t("abcX"));
    assert_eq!(run("REPT(\"ab\",3)"), t("ababab"));
    assert_eq!(run("REPT(\"a\",40000)"), err(CellError::Value));
    assert_eq!(run("EXACT(\"a\",\"A\")"), Value::Bool(false));
    assert_eq!(run("EXACT(\"a\",\"a\")"), Value::Bool(true));
    assert_eq!(run("VALUE(\"12.5\")"), n(12.5));
    assert_eq!(run("VALUE(\"50%\")"), n(0.5));
    assert_eq!(run("VALUE(\"x\")"), err(CellError::Value));
    assert_eq!(run("PROPER(\"hello wORLD-foo\")"), t("Hello World-Foo"));
    assert_eq!(run("CLEAN(\"a\"&CHAR(7)&\"b\")"), t("ab"));
    assert_eq!(run("CHAR(65)"), t("A"));
    assert_eq!(run("CHAR(0)"), err(CellError::Value));
    assert_eq!(run("CODE(\"A\")"), n(65.0));
    assert_eq!(run("CODE(\"\")"), err(CellError::Value));
    assert_eq!(run("UNICHAR(8364)"), t("€"));
    assert_eq!(run("UNICODE(\"€\")"), n(8364.0));
    assert_eq!(run("TEXTJOIN(\",\",TRUE,\"a\",\"\",\"b\")"), t("a,b"));
    assert_eq!(run("TEXTJOIN(\"-\",FALSE,\"a\",\"\",\"b\")"), t("a--b"));
    assert_eq!(run("TEXTJOIN(\",\",TRUE,E1:E4)"), t("a,b,c,d"));
    assert_eq!(run("T(1)"), t(""));
    assert_eq!(run("T(\"x\")"), t("x"));
}

#[quench_test::test]
fn dates() {
    assert_eq!(run("DATE(2024,2,29)"), n(45351.0));
    assert_eq!(run("DATE(2000,1,1)"), n(36526.0));
    assert_eq!(run("DATE(1900,1,1)"), n(1.0));
    assert_eq!(run("DATE(1900,3,1)"), n(61.0));
    assert_eq!(run("DATE(2024,14,1)"), n(45689.0)); // months roll over
    assert_eq!(run("DATE(2024,1,0)"), n(45291.0)); // so do days
    assert_eq!(run("DATE(24,1,1)"), run("DATE(1924,1,1)"));
    assert_eq!(run("DATE(10000,1,1)"), err(CellError::Num));
    assert_eq!(
        run("YEAR(45351)*10000+MONTH(45351)*100+DAY(45351)"),
        n(20_240_229.0)
    );
    assert_eq!(run("YEAR(60)*10000+MONTH(60)*100+DAY(60)"), n(19_000_229.0)); // the day that never was
    assert_eq!(run("MONTH(61)*100+DAY(61)"), n(301.0));
    assert_eq!(run("MONTH(1)*100+DAY(1)"), n(101.0));
    assert_eq!(run("YEAR(0)"), err(CellError::Num));
    assert_eq!(run("WEEKDAY(45351)"), n(5.0)); // a Thursday
    assert_eq!(run("WEEKDAY(45351,2)"), n(4.0));
    assert_eq!(run("WEEKDAY(45351,3)"), n(3.0));
    assert_eq!(run("WEEKDAY(45351,9)"), err(CellError::Num));
    assert_eq!(run("DAYS(45351,45292)"), n(59.0));
    assert_eq!(run("EDATE(DATE(2024,1,31),1)"), n(45351.0));
    assert_eq!(run("EDATE(DATE(2024,3,31),-1)"), n(45351.0));
    assert_eq!(run("EOMONTH(DATE(2024,1,15),1)"), n(45351.0));
    assert_eq!(run("EOMONTH(DATE(2024,1,15),0)"), n(45322.0));
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"Y\")"),
        n(4.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"M\")"),
        n(49.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"YM\")"),
        n(1.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"MD\")"),
        n(24.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"D\")"),
        n(1516.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2020,1,15),DATE(2024,3,10),\"YD\")"),
        n(55.0)
    );
    assert_eq!(
        run("DATEDIF(DATE(2024,3,10),DATE(2020,1,15),\"Y\")"),
        err(CellError::Num)
    );
    assert_eq!(run("DATEDIF(1,2,\"Q\")"), err(CellError::Num));
    assert_eq!(run("TIME(12,0,0)"), n(0.5));
    near("TIME(25,0,0)", 1.0 / 24.0);
    assert_eq!(run("TIME(-1,0,0)"), err(CellError::Num));
    assert_eq!(run("HOUR(0.75)"), n(18.0));
    assert_eq!(
        run("HOUR(TIME(13,45,30))*10000+MINUTE(TIME(13,45,30))*100+SECOND(TIME(13,45,30))"),
        n(134_530.0)
    );
}

#[quench_test::test]
fn lookups() {
    assert_eq!(run("VLOOKUP(3,D1:E4,2)"), t("c"));
    assert_eq!(run("VLOOKUP(2.5,D1:E4,2)"), t("b"));
    assert_eq!(run("VLOOKUP(2.5,D1:E4,2,FALSE)"), err(CellError::NA));
    assert_eq!(run("VLOOKUP(0,D1:E4,2)"), err(CellError::NA));
    assert_eq!(run("VLOOKUP(1,D1:E4,3)"), err(CellError::Ref));
    assert_eq!(run("VLOOKUP(1,D1:E4,0)"), err(CellError::Value));
    assert_eq!(run("VLOOKUP(99,D1:E4,2)"), t("d"));
    assert_eq!(run("VLOOKUP(\"p*\",F1:G2,2,FALSE)"), n(2.0));
    assert_eq!(run("VLOOKUP(\"APPLE\",F1:G2,2,FALSE)"), n(1.0));
    assert_eq!(run("VLOOKUP(\"1\",D1:E4,2,FALSE)"), err(CellError::NA)); // text is not a number
    assert_eq!(run("HLOOKUP(20,H1:J2,2,FALSE)"), t("y"));
    assert_eq!(run("HLOOKUP(25,H1:J2,2)"), t("y"));
    assert_eq!(run("INDEX(D1:E4,2,2)"), t("b"));
    assert_eq!(run("INDEX(D1:D4,3)"), n(3.0));
    assert_eq!(run("INDEX(H1:J1,2)"), n(20.0));
    assert_eq!(run("INDEX(D1:E4,9,1)"), err(CellError::Ref));
    assert_eq!(run("INDEX(D1:E4,0,1)"), err(CellError::Value));
    assert_eq!(run("MATCH(3,D1:D4,0)"), n(3.0));
    assert_eq!(run("MATCH(2.5,D1:D4,1)"), n(2.0));
    assert_eq!(run("MATCH(25,K1:K3,-1)"), n(1.0));
    assert_eq!(run("MATCH(\"b\",E1:E4,0)"), n(2.0));
    assert_eq!(run("MATCH(\"z\",E1:E4,0)"), err(CellError::NA));
    assert_eq!(run("MATCH(1,D1:E4,0)"), err(CellError::NA)); // not a single row or column
    assert_eq!(run("XLOOKUP(3,D1:D4,E1:E4)"), t("c"));
    assert_eq!(run("XLOOKUP(9,D1:D4,E1:E4,\"none\")"), t("none"));
    assert_eq!(run("XLOOKUP(9,D1:D4,E1:E4)"), err(CellError::NA));
    assert_eq!(run("XLOOKUP(2.5,D1:D4,E1:E4,\"none\",-1)"), t("b"));
    assert_eq!(run("XLOOKUP(2.5,D1:D4,E1:E4,\"none\",1)"), t("c"));
    assert_eq!(run("XLOOKUP(\"AP*\",A4:A5,B4:B5,0,2)"), n(40.0));
    assert_eq!(run("XLOOKUP(1,M1:M3,N1:N3,\"\",0,-1)"), t("q"));
    assert_eq!(run("XLOOKUP(1,M1:M3,N1:N3)"), t("p"));
    assert_eq!(run("XLOOKUP(1,M1:M3,N1:N2)"), err(CellError::Value));
    assert_eq!(run("CHOOSE(2,\"a\",\"b\",\"c\")"), t("b"));
    assert_eq!(run("CHOOSE(0,\"a\")"), err(CellError::Value));
    assert_eq!(run("CHOOSE(4,\"a\",\"b\")"), err(CellError::Value));
    assert_eq!(run("ROWS(D1:E4)"), n(4.0));
    assert_eq!(run("COLUMNS(D1:E4)"), n(2.0));
}

#[quench_test::test]
fn text_formatting_functions() {
    assert_eq!(run("TEXT(1234.5,\"#,##0.00\")"), t("1,234.50"));
    assert_eq!(run("TEXT(0.256,\"0%\")"), t("26%"));
    assert_eq!(run("TEXT(45351,\"yyyy-mm-dd\")"), t("2024-02-29"));
    assert_eq!(run("TEXT(DATE(2024,2,29),\"dddd\")"), t("Thursday"));
    assert_eq!(run("TEXT(\"5\",\"0.0\")"), t("5.0"));
    assert_eq!(run("TEXT(\"abc\",\"0\")"), t("abc"));
    assert_eq!(run("TEXT(Z9,\"0.0\")"), t("0.0"));
    assert_eq!(run("TEXT(-1,\"0;(0)\")"), t("(1)"));
    assert_eq!(run("TEXT(1,\"0.00\"\"\")"), err(CellError::Value));
    assert_eq!(run("TEXT(1/0,\"0\")"), err(CellError::Div0));
    assert_eq!(
        run("\"Total: \"&TEXT(1234.5,\"$#,##0\")"),
        t("Total: $1,235")
    );
    assert_eq!(run("FIXED(1234.567,1)"), t("1,234.6"));
    assert_eq!(run("FIXED(1234.567,0,TRUE)"), t("1235"));
    assert_eq!(run("FIXED(-1234.5)"), t("-1,234.50"));
    assert_eq!(run("FIXED(1234,-2)"), t("1,200"));
    assert_eq!(run("DOLLAR(1234.5)"), t("$1,234.50"));
    assert_eq!(run("DOLLAR(-5)"), t("($5.00)"));
    assert_eq!(run("DOLLAR(1250,-2)"), t("$1,300"));
}

#[track_caller]
fn close(f: &str, want: f64, tolerance: f64) {
    match run(f) {
        Value::Number(x) => assert!(
            (x - want).abs() <= tolerance,
            "{f} = {x}, want {want} (within {tolerance})"
        ),
        other => panic!("{f} = {other:?}, want {want}"),
    }
}

#[quench_test::test]
fn financial_functions_match_the_documented_values() {
    close("PMT(0.05/12,360,200000)", -1_073.643_246_2, 1e-4);
    close("PMT(0,10,1000)", -100.0, 1e-4);
    close("PMT(0.08/12,10,10000,0,1)", -1_030.16, 1e-2);
    close("FV(0.06/12,10,-200,-500,1)", 2_581.403_374, 1e-4);
    close("FV(0,10,-100)", 1000.0, 1e-4);
    close("PV(0.08/12,240,500)", -59_777.145_851, 1e-4);
    close("NPER(0.12/12,-100,-1000,10000,1)", 59.673_865_6, 1e-4);
    close("RATE(48,-200,8000)", 0.007_701_472_5, 1e-4);
    close("NPV(0.1,-10000,3000,4200,6800)", 1_188.443_412, 1e-4);
    close(
        "NPV(0.1,P2:P5)",
        12_000.0 / 1.1 + 15_000.0 / 1.21 + 18_000.0 / 1.331 + 21_000.0 / 1.4641,
        1e-4,
    );
    close("IRR(P1:P5)", -0.021_244_8, 1e-4);
    close("IRR(P1:P6)", 0.086_630_9, 1e-4);
    close("IPMT(0.1/12,1,36,8000)", -66.666_666_7, 1e-4);
    close("PPMT(0.1/12,1,24,2000)", -75.623_2, 1e-4);
    assert_eq!(
        run("IPMT(0.1/12,1,36,8000,0,1)"),
        n(0.0),
        "paid at the start of the first period: no interest yet"
    );
    assert_eq!(run("IPMT(0.1,40,36,8000)"), err(CellError::Num));
    assert_eq!(
        run("IRR(A1:A3)"),
        err(CellError::Num),
        "needs a gain and a loss"
    );
    assert_eq!(run("NPV(-1,1,2)"), err(CellError::Num));
    assert_eq!(run("PMT(0.05,0,100)"), err(CellError::Num));
}

#[quench_test::test]
fn more_statistics_and_combinatorics() {
    assert_eq!(run("PERCENTILE(B1:B5,0.5)"), n(30.0));
    assert_eq!(run("PERCENTILE(B1:B5,0.25)"), n(20.0));
    assert_eq!(run("PERCENTILE.INC(B1:B5,0.1)"), n(14.0));
    assert_eq!(run("PERCENTILE(B1:B5,1.5)"), err(CellError::Num));
    assert_eq!(run("QUARTILE(B1:B5,3)"), n(40.0));
    assert_eq!(run("SLOPE(R1:R5,Q1:Q5)"), n(2.0));
    assert_eq!(run("INTERCEPT(R1:R5,Q1:Q5)"), n(0.0));
    near("CORREL(Q1:Q5,R1:R5)", 1.0);
    assert_eq!(run("CORREL(Q1:Q5,R1:R4)"), err(CellError::NA));
    near("GEOMEAN(1,4,16)", 4.0);
    assert_eq!(run("GEOMEAN(0,4)"), err(CellError::Num));
    assert_eq!(run("COMBIN(5,2)"), n(10.0));
    assert_eq!(run("COMBIN(52,5)"), n(2_598_960.0));
    assert_eq!(run("PERMUT(5,2)"), n(20.0));
    assert_eq!(run("COMBIN(2,5)"), err(CellError::Num));
    assert_eq!(run("MROUND(10,3)"), n(9.0));
    assert_eq!(run("MROUND(-10,-3)"), n(-9.0));
    assert_eq!(run("MROUND(-10,3)"), err(CellError::Num));
    assert_eq!(run("SUBTOTAL(9,B1:B5)"), n(150.0));
    assert_eq!(run("SUBTOTAL(1,B1:B5)"), n(30.0));
    assert_eq!(run("SUBTOTAL(109,B1:B5)"), n(150.0));
    assert_eq!(run("SUBTOTAL(4,B1:B5)"), n(50.0));
    assert_eq!(run("SUBTOTAL(12,B1:B5)"), err(CellError::Value));
}

#[quench_test::test]
fn more_calendar_and_reference_functions() {
    assert_eq!(run("WEEKNUM(DATE(2024,1,1))"), n(1.0));
    assert_eq!(run("WEEKNUM(DATE(2024,12,31))"), n(53.0));
    assert_eq!(run("WEEKNUM(DATE(2024,1,7),2)"), n(1.0));
    assert_eq!(run("WEEKNUM(DATE(2024,1,8),2)"), n(2.0));
    assert_eq!(
        run("WEEKNUM(DATE(2021,1,3),21)"),
        n(53.0),
        "ISO: the end of 2020"
    );
    assert_eq!(run("WEEKNUM(DATE(2024,1,1),21)"), n(1.0));
    assert_eq!(run("WEEKNUM(DATE(2024,3,15),21)"), n(11.0));
    assert_eq!(run("NETWORKDAYS(DATE(2024,2,26),DATE(2024,3,1))"), n(5.0));
    assert_eq!(run("NETWORKDAYS(DATE(2024,2,24),DATE(2024,2,25))"), n(0.0));
    assert_eq!(run("NETWORKDAYS(DATE(2024,3,1),DATE(2024,2,26))"), n(-5.0));
    assert_eq!(
        run("NETWORKDAYS(DATE(2024,2,26),DATE(2024,3,1),DATE(2024,2,28))"),
        n(4.0)
    );
    assert_eq!(run("WORKDAY(DATE(2024,2,29),3)"), run("DATE(2024,3,5)"));
    assert_eq!(run("WORKDAY(DATE(2024,3,5),-3)"), run("DATE(2024,2,29)"));
    assert_eq!(run("DATEVALUE(\"2024-02-29\")"), n(45351.0));
    assert_eq!(run("DATEVALUE(\"2/29/2024\")"), n(45351.0));
    assert_eq!(run("DATEVALUE(\"2024-02-30\")"), err(CellError::Value));
    assert_eq!(run("DATEVALUE(\"yesterday\")"), err(CellError::Value));
    assert_eq!(run("ADDRESS(2,3)"), t("$C$2"));
    assert_eq!(run("ADDRESS(2,3,4)"), t("C2"));
    assert_eq!(run("ADDRESS(2,3,2)"), t("C$2"));
    assert_eq!(run("ADDRESS(2,3,1,FALSE)"), t("R2C3"));
    assert_eq!(run("ADDRESS(1,1,1,TRUE,\"Data\")"), t("Data!$A$1"));
    assert_eq!(run("ADDRESS(0,1)"), err(CellError::Value));
    assert_eq!(run("HYPERLINK(\"http://x\",\"click\")"), t("click"));
    assert_eq!(run("HYPERLINK(\"http://x\")"), t("http://x"));
    assert_eq!(run("LOOKUP(2.5,D1:D4,E1:E4)"), t("b"));
    assert_eq!(run("LOOKUP(4,D1:D4)"), n(4.0));
    assert_eq!(run("LOOKUP(0,D1:D4,E1:E4)"), err(CellError::NA));
}

/// A cell source with a clock, dice and a place.
struct Placed<'a> {
    cells: &'a Grid,
    now: f64,
    dice: f64,
    at: Option<CellRef>,
}

impl sheets_formula::Context for Placed<'_> {
    fn cell(&self, sheet: Option<&str>, at: CellRef) -> Value {
        self.cells.cell(sheet, at)
    }

    fn now(&self) -> f64 {
        self.now
    }

    fn random(&self) -> f64 {
        self.dice
    }

    fn origin(&self) -> Option<CellRef> {
        self.at
    }
}

#[quench_test::test]
fn the_clock_chance_and_the_formulas_own_place() {
    let cells = data();
    let at = |now, dice, cell: Option<&str>| Placed {
        cells: &cells,
        now,
        dice,
        at: cell.map(|c| CellRef::parse(c).unwrap()),
    };
    let go = |f: &str, ctx: &Placed| eval(&parse(f).unwrap(), ctx);
    let ctx = at(45_351.75, 0.0, Some("B7"));
    assert_eq!(go("NOW()", &ctx), n(45_351.75));
    assert_eq!(go("TODAY()", &ctx), n(45_351.0));
    assert_eq!(go("RAND()", &ctx), n(0.0));
    assert_eq!(go("RANDBETWEEN(1,6)", &ctx), n(1.0));
    assert_eq!(go("RANDBETWEEN(1,6)", &at(0.0, 0.999_999, None)), n(6.0));
    assert_eq!(go("RANDBETWEEN(-3,3)", &at(0.0, 0.5, None)), n(0.0));
    assert_eq!(go("RANDBETWEEN(5,1)", &ctx), err(CellError::Num));
    assert_eq!(go("ROW()", &ctx), n(7.0));
    assert_eq!(go("COLUMN()", &ctx), n(2.0));
    assert_eq!(go("ROW(C9)", &ctx), n(9.0));
    assert_eq!(go("COLUMN(C9:E12)", &ctx), n(3.0));
    assert_eq!(go("ROW(A1:A9)+ROW()", &ctx), n(8.0));
    assert_eq!(
        go("ROW()", &at(0.0, 0.0, None)),
        err(CellError::Value),
        "a source that does not say where it is"
    );
    assert_eq!(go("ROW(1)", &ctx), err(CellError::Value));
    assert_eq!(go("NOW(1)", &ctx), err(CellError::Value));
    assert_eq!(go("TODAY()-DATE(2024,1,1)", &ctx), n(59.0));
    // Plain sources with no clock answer 0 for the time and one half for chance.
    assert_eq!(run("NOW()"), n(0.0));
    assert_eq!(run("RAND()"), n(0.5));
    assert!(parse("NOW()+1").unwrap().is_volatile());
    assert!(parse("IF(A1>0,RAND(),1)").unwrap().is_volatile());
    assert!(!parse("SUM(A1:A3)*2").unwrap().is_volatile());
}

#[quench_test::test]
fn the_list_of_known_functions_is_true() {
    // Every name on the list is run by the evaluator: none answers `#NAME?` however it is called.
    for name in sheets_formula::KNOWN {
        for args in ["", "1", "1,2", "1,2,3", "A1:A3", "\"x\""] {
            let v = run(&format!("{name}({args})"));
            assert_ne!(v, err(CellError::Name), "{name}({args})");
        }
        assert!(sheets_formula::is_known(name));
    }
    assert!(
        sheets_formula::KNOWN.windows(2).all(|w| w[0] < w[1]),
        "sorted and without repeats"
    );
    for unknown in [
        "FROBNICATE",
        "XLOOKUPX",
        "SUMX2MY2",
        "LAMBDA",
        "TEXTSPLIT",
        "TOCOL",
    ] {
        assert!(!sheets_formula::is_known(unknown), "{unknown}");
        assert_eq!(
            run(&format!("{unknown}(1)")),
            err(CellError::Name),
            "{unknown}"
        );
    }
    assert!(
        sheets_formula::VOLATILE
            .iter()
            .all(|n| sheets_formula::is_known(n))
    );
    assert!(sheets_formula::KNOWN.len() >= 180);
}

/// A formula's result as rows of values (a single value is one row of one).
fn grid(f: &str) -> Vec<Vec<Value>> {
    match sheets_formula::eval_operand(&parse(f).unwrap(), &data()) {
        sheets_formula::Operand::Scalar(v) => vec![vec![v]],
        sheets_formula::Operand::Array(a) => a.data.chunks(a.cols).map(<[Value]>::to_vec).collect(),
    }
}

fn nums(f: &str) -> Vec<Vec<f64>> {
    grid(f)
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|v| {
                    if let Value::Number(x) = v {
                        x
                    } else {
                        f64::NAN
                    }
                })
                .collect()
        })
        .collect()
}

#[quench_test::test]
fn arithmetic_goes_element_by_element_over_arrays() {
    assert_eq!(nums("SEQUENCE(3)"), [[1.0], [2.0], [3.0]]);
    assert_eq!(
        nums("SEQUENCE(2,3,10,5)"),
        [[10.0, 15.0, 20.0], [25.0, 30.0, 35.0]]
    );
    assert_eq!(nums("SEQUENCE(3)*2+1"), [[3.0], [5.0], [7.0]]);
    assert_eq!(nums("-SEQUENCE(2)"), [[-1.0], [-2.0]]);
    assert_eq!(
        nums("SEQUENCE(1,3)+SEQUENCE(3)"),
        [[2.0, 3.0, 4.0], [3.0, 4.0, 5.0], [4.0, 5.0, 6.0]],
        "a row against a column"
    );
    assert_eq!(
        grid("SEQUENCE(3)>1"),
        [
            [Value::Bool(false)],
            [Value::Bool(true)],
            [Value::Bool(true)]
        ]
    );
    near("SUM(SEQUENCE(4)^2)", 30.0);
    assert_eq!(
        run("SEQUENCE(3)*2"),
        n(2.0),
        "in a single cell only the first shows"
    );
    // Arrays of different sizes: the part with nothing to pair is #N/A.
    assert_eq!(grid("SEQUENCE(2)+SEQUENCE(3)")[2][0], err(CellError::NA));
    near("SUMPRODUCT((B1:B5>20)*B1:B5)", 120.0);
}

#[quench_test::test]
fn filter_sort_unique_and_friends_give_back_rectangles() {
    assert_eq!(nums("FILTER(B1:B5,B1:B5>20)"), [[30.0], [40.0], [50.0]]);
    assert_eq!(run("FILTER(B1:B5,B1:B5>99,\"none\")"), t("none"));
    assert_eq!(run("FILTER(B1:B5,B1:B5>99)"), err(CellError::NA));
    assert_eq!(
        nums("FILTER(HSTACK(B1:B5,SEQUENCE(5)),SEQUENCE(5)<3)"),
        [[10.0, 1.0], [20.0, 2.0]],
        "whole rows are kept"
    );
    assert_eq!(
        nums("FILTER(SEQUENCE(2,4),SEQUENCE(1,4)<>2)"),
        [[1.0, 3.0, 4.0], [5.0, 7.0, 8.0]],
        "or columns"
    );
    assert_eq!(
        nums("SORT(B1:B5,1,-1)"),
        [[50.0], [40.0], [30.0], [20.0], [10.0]]
    );
    assert_eq!(nums("SORT(VSTACK(3,1,2))"), [[1.0], [2.0], [3.0]]);
    assert_eq!(
        nums("SORT(HSTACK(SEQUENCE(3),VSTACK(9,7,8)),2)"),
        [[2.0, 7.0], [3.0, 8.0], [1.0, 9.0]],
        "by the second column"
    );
    assert_eq!(
        nums("SORT(HSTACK(3,1,2),1,1,TRUE)"),
        [[1.0, 2.0, 3.0]],
        "across a row"
    );
    assert_eq!(
        grid("SORT(VSTACK(\"pear\",\"Apple\",\"fig\"))"),
        [[t("Apple")], [t("fig")], [t("pear")]]
    );
    assert_eq!(
        nums("SORTBY(SEQUENCE(3),VSTACK(30,10,20))"),
        [[2.0], [3.0], [1.0]]
    );
    assert_eq!(
        nums("SORTBY(SEQUENCE(3),VSTACK(1,1,0),-1,SEQUENCE(3),-1)"),
        [[2.0], [1.0], [3.0]]
    );
    assert_eq!(nums("UNIQUE(VSTACK(1,2,2,3,1))"), [[1.0], [2.0], [3.0]]);
    assert_eq!(nums("UNIQUE(VSTACK(1,2,2,3,1),FALSE,TRUE)"), [[3.0]]);
    assert_eq!(
        grid("UNIQUE(VSTACK(\"a\",\"A\",\"b\"))"),
        [[t("a")], [t("b")]],
        "case does not matter"
    );
    assert_eq!(nums("UNIQUE(HSTACK(1,2,1),TRUE)"), [[1.0, 2.0]]);
    assert_eq!(
        nums("TRANSPOSE(SEQUENCE(2,3))"),
        [[1.0, 4.0], [2.0, 5.0], [3.0, 6.0]]
    );
    assert_eq!(
        nums("VSTACK(SEQUENCE(1,2),SEQUENCE(1,3))")[0].len(),
        3,
        "the narrower is padded"
    );
    assert_eq!(
        grid("VSTACK(SEQUENCE(1,2),SEQUENCE(1,3))")[0][2],
        err(CellError::NA)
    );
    assert_eq!(
        nums("HSTACK(SEQUENCE(2),SEQUENCE(2,1,10))"),
        [[1.0, 10.0], [2.0, 11.0]]
    );
    assert_eq!(run("SEQUENCE(0)"), err(CellError::Value));
    assert_eq!(run("FILTER(B1:B5,B1:B4)"), err(CellError::Value));
    near("ROWS(FILTER(B1:B5,B1:B5>10))", 4.0);
}

#[quench_test::test]
fn let_indirect_and_offset_reach_cells_and_names() {
    near("LET(x,2,y,x*3,x+y)", 8.0);
    near("LET(a,SUM(B1:B3),a/2)", 30.0);
    assert_eq!(run("LET(x,1)"), err(CellError::Value));
    assert_eq!(run("LET(1,2,3)"), err(CellError::Value), "a name is needed");
    assert_eq!(run("INDIRECT(\"B2\")"), n(20.0));
    near("SUM(INDIRECT(\"B1:B3\"))", 60.0);
    assert_eq!(run("INDIRECT(\"B\"&\"3\")"), n(30.0));
    assert_eq!(run("INDIRECT(\"nonsense\")"), err(CellError::Ref));
    assert_eq!(run("OFFSET(B1,2,0)"), n(30.0));
    near("SUM(OFFSET(B1,1,0,3,1))", 90.0);
    near("SUM(OFFSET(B1:B2,0,0,5))", 150.0);
    assert_eq!(run("OFFSET(B3,-5,0)"), err(CellError::Ref), "off the sheet");
    assert_eq!(
        run("OFFSET(5,1,1)"),
        err(CellError::Value),
        "needs a reference"
    );
    assert_eq!(nums("OFFSET(B1,0,0,2,1)"), [[10.0], [20.0]]);
}
