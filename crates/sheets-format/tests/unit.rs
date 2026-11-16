#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use sheets_format::*;
use sheets_model::{CellError, Value};

const fn num(x: f64) -> Value {
    Value::Number(x)
}

#[track_caller]
fn check(fmt: &str, x: f64, want: &str) {
    assert_eq!(format_value(fmt, &num(x)), want, "{fmt} of {x}");
}

#[quench_test::test]
fn digits_and_decimals() {
    check("0", 7.0, "7");
    check("0", 7.5, "8");
    check("0", -7.5, "-8");
    check("0.00", 3.149, "3.15");
    check("0.00", 3.0, "3.00");
    check("0.00", 1.005, "1.01");
    check("000", 7.0, "007");
    check("#", 0.0, "");
    check("#", 5.0, "5");
    check("0.##", 5.0, "5.");
    check("0.##", 5.25, "5.25");
    check("0.##", 5.5, "5.5");
    check("#.00", 0.5, ".50");
    check("0.0", 0.04, "0.0");
    check("00.0", 5.25, "05.3");
    check("???.??", 1.5, "  1.5 ");
    check("0", 12345.0, "12345");
    check("00", 123.0, "123"); // more digits than placeholders are kept
    check("0.0", 0.0, "0.0");
}

#[quench_test::test]
fn thousands_and_scaling() {
    check("#,##0", 1_234_567.8, "1,234,568");
    check("#,##0", 999.0, "999");
    check("#,##0", 1000.0, "1,000");
    check("#,##0.00", -1234.5, "-1,234.50");
    check("#,##0", 0.0, "0");
    check("000,000", 5.0, "000,005");
    check("#,##0,", 1_234_567.0, "1,235");
    check("0.0,,", 1_500_000.0, "1.5");
    check("0,", 999.0, "1");
    check("#,###", 1_234_567_890_123.0, "1,234,567,890,123");
}

#[quench_test::test]
fn percent_and_scientific() {
    check("0%", 0.256, "26%");
    check("0.0%", 0.12345, "12.3%");
    check("0.00%", 1.0, "100.00%");
    check("0.00E+00", 12345.678, "1.23E+04");
    check("0.0E+0", 0.00012, "1.2E-4");
    check("0.00E+00", 0.0, "0.00E+00");
    check("0.00E+00", 9.999, "1.00E+01");
    check("0E+0", 5.0e10, "5E+10");
}

#[quench_test::test]
fn literals_and_sections() {
    check("$#,##0.00", 1234.5, "$1,234.50");
    check("\"x\"0", 5.0, "x5");
    check("0\" units\"", 5.0, "5 units");
    check("0\\%", 5.0, "5%");
    check("(000) 000-0000", 5_551_234_567.0, "(555) 123-4567");
    check("0;(0)", -5.0, "(5)");
    check("0;(0)", 5.0, "5");
    check("0;-0;zero", 0.0, "zero");
    check("0;-0;zero", 3.0, "3");
    check("0;-0;zero", -3.0, "-3");
    check("[>100]0.0;0", 150.0, "150.0");
    check("[>100]0.0;0", 50.0, "50");
    check("[<=0]\"neg\";0", -4.0, "neg");
    check("0_);(0)", 5.0, "5 ");
    check("[$€-407]#,##0.00", 1234.5, "€1,234.50");
    check("[Red]0", 5.0, "5");
    check("*-0", 5.0, "5");
    let f = Format::parse("0;[Red]-0").unwrap();
    assert_eq!(
        f.render(&num(-2.0)),
        Rendered {
            text: "-2".into(),
            color: Some(Color::Red)
        }
    );
    assert_eq!(f.render(&num(2.0)).color, None);
    assert_eq!(
        Format::parse("[Color5]0").unwrap().render(&num(1.0)).color,
        Some(Color::Indexed(5))
    );
}

#[quench_test::test]
fn text_and_other_values() {
    let at = |fmt: &str, v: &str| format_value(fmt, &Value::Text(v.into()));
    assert_eq!(at("0.00", "abc"), "abc");
    assert_eq!(at("@", "abc"), "abc");
    assert_eq!(at("\"[\"@\"]\"", "abc"), "[abc]");
    assert_eq!(at("0;0;0;\"t:\"@", "abc"), "t:abc");
    assert_eq!(format_value("0.00", &Value::Empty), "");
    assert_eq!(format_value("0.00", &Value::Bool(true)), "TRUE");
    assert_eq!(
        format_value("0.00", &Value::Error(CellError::Div0)),
        "#DIV/0!"
    );
    assert_eq!(format_value("0.00", &num(f64::NAN)), "#NUM!");
    assert_eq!(format_value("", &num(1.5)), "1.5");
    assert_eq!(format_value("General", &num(1.5)), "1.5");
    assert_eq!(format_value("0.0;;", &num(-1.0)), ""); // an empty section shows nothing
}

#[quench_test::test]
fn general_format() {
    for (x, want) in [
        (1.0, "1"),
        (-3.0, "-3"),
        (12.5, "12.5"),
        (0.1 + 0.2, "0.3"),
        (1_234_567.891_234, "1234567.891"),
        (99_999_999_999.0, "99999999999"),
        (1e11, "1E+11"),
        (123_456_789_012.0, "1.23457E+11"),
        (0.0001, "0.0001"),
        (0.00001, "1E-05"),
        (0.123_456_789_012, "0.123456789"),
        (-0.5, "-0.5"),
        (0.0, "0"),
        (1e-10, "1E-10"),
    ] {
        assert_eq!(general(x), want, "{x}");
    }
}

#[quench_test::test]
fn dates_and_times() {
    check("yyyy-mm-dd", 45351.0, "2024-02-29");
    check("yy/m/d", 45351.0, "24/2/29");
    check("dddd, mmmm d, yyyy", 45351.0, "Thursday, February 29, 2024");
    check("ddd mmm d", 45351.0, "Thu Feb 29");
    check("mmmmm", 45351.0, "F");
    check("yyyy-mm-dd hh:mm", 45351.5, "2024-02-29 12:00");
    check("h:mm AM/PM", 0.75, "6:00 PM");
    check("h:mm AM/PM", 0.0, "12:00 AM");
    check("h:mm a/p", 0.25, "6:00 a");
    check("hh:mm:ss", 0.5, "12:00:00");
    check("hh:mm:ss", 0.999_999, "00:00:00");
    check("[h]:mm", 1.5, "36:00");
    check("[mm]:ss", 0.5, "720:00");
    check("[s]", 0.5, "43200");
    check("mm:ss", 0.0005, "00:43");
    check("mm:ss.0", 0.0005, "00:43.2");
    check("h:mm:ss.00", 0.5 + 1.0 / 86_400.0 / 4.0, "12:00:00.25");
    check("m/d/yyyy", 1.0, "1/1/1900");
    check("m/d/yyyy", 60.0, "2/29/1900");
    check("m/d/yyyy", 61.0, "3/1/1900");
    check("yyyy-mm-dd", 0.0, "1900-01-00");
    check("yyyy-mm-dd", -1.0, "########");
    check("yyyy-mm-dd", 3e6, "########");
    check("m", 45351.0, "2");
    check("h:m", 0.5, "12:0");
    check("mm", 45351.0, "02");
    check("d \"of\" mmmm", 45351.0, "29 of February");
    assert!(Format::parse("yyyy-mm-dd").unwrap().is_date());
    assert!(Format::parse("[h]:mm").unwrap().is_date());
    assert!(!Format::parse("0.00").unwrap().is_date());
    assert!(!Format::parse("General").unwrap().is_date());
    assert!(Format::parse("0;0;0;@").is_ok());
}

#[quench_test::test]
fn fractions() {
    check("# ?/?", 1.5, "1 1/2");
    check("# ?/?", 2.25, "2 1/4");
    check("# ?/?", 2.0, "2    ");
    check("?/?", 0.75, "3/4");
    check("# ??/??", 0.3333, "  1/3 ");
    check("# ??/??", 2.14, "2  7/50");
    check("?/?", 1.5, "3/2");
    check("# ?/8", 1.3, "1 2/8");
}

#[quench_test::test]
fn bad_formats_and_builtins() {
    assert!(Format::parse("0.00\"").is_err());
    assert!(Format::parse("[Red").is_err());
    assert!(Format::parse("0;0;0;0;0").is_err());
    assert_eq!(format_value("0.00\"", &num(1.5)), "1.5"); // shown as General
    assert!(
        Format::parse("0\"")
            .unwrap_err()
            .to_string()
            .contains("quote")
    );
    assert_eq!(builtin(0), Some("General"));
    assert_eq!(builtin(14), Some("m/d/yyyy"));
    assert_eq!(builtin(49), Some("@"));
    assert_eq!(builtin(164), None);
    for id in 0..50 {
        if let Some(f) = builtin(id) {
            assert!(Format::parse(f).is_ok(), "builtin {id}: {f}");
        }
    }
    assert_eq!(
        format_value(builtin(4).unwrap(), &num(1234.567)),
        "1,234.57"
    );
    assert_eq!(format_value(builtin(10).unwrap(), &num(0.5)), "50.00%");
    assert_eq!(
        format_value(builtin(22).unwrap(), &num(45351.75)),
        "2/29/2024 18:00"
    );
    assert_eq!(
        format_value(builtin(40).unwrap(), &num(-1234.5)),
        "(1,234.50)"
    );
}

#[quench_test::test]
fn digits_past_fifteen_are_zeros_not_float_noise() {
    assert_eq!(
        format_value("0.000000000", &Value::Number(123_456_789_012.0)),
        "123456789012.000000000"
    );
    assert_eq!(
        format_value("#,##0", &Value::Number(123_456_789_012_345_678.0)),
        "123,456,789,012,346,000"
    );
    assert_eq!(
        format_value("0", &Value::Number(-1e20)),
        "-100000000000000000000"
    );
}

quench_test::main!();
