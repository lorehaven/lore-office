#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use sheets_calc::Engine;
use sheets_csv::*;
use sheets_model::{CellRef, Content, Value};

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

fn read_default(text: &str) -> sheets_model::Workbook {
    read(text, "Data", &ReadOptions::default()).unwrap()
}

#[quench_test::test]
fn fields_become_values() {
    let wb = read_default(
        "name,qty,price,ok,when\nbolt,4,1.5,TRUE,2024-02-29\n\"nut, hex\",007,-2e3,false,2024-02-30\n",
    );
    let s = &wb.sheets[0];
    assert_eq!(s.name, "Data");
    let v = |a: &str| match s.get(at(a)) {
        Some(Content::Value(v)) => v.clone(),
        other => panic!("{a}: {other:?}"),
    };
    assert_eq!(v("A1"), Value::Text("name".into()));
    assert_eq!(v("A3"), Value::Text("nut, hex".into()));
    assert_eq!(v("B2"), Value::Number(4.0));
    assert_eq!(v("B3"), Value::Text("007".into())); // leading zeros are kept
    assert_eq!(v("C2"), Value::Number(1.5));
    assert_eq!(v("C3"), Value::Number(-2000.0));
    assert_eq!(v("D2"), Value::Bool(true));
    assert_eq!(v("D3"), Value::Bool(false));
    assert_eq!(v("E2"), Value::Number(45351.0));
    assert_eq!(v("E3"), Value::Text("2024-02-30".into())); // not a date
    assert_eq!(wb.number_format(0, at("E2")), "yyyy-mm-dd");
    assert_eq!(wb.number_format(0, at("B2")), "General");
}

#[quench_test::test]
fn text_stays_text_when_it_only_looks_odd() {
    let wb = read_default("+1 555 0100,1e,--1,inf,NaN,1.2.3,0.5,0,-0.5,.5\n");
    let kinds: Vec<bool> = (0..10)
        .map(|c| {
            matches!(
                wb.sheets[0].get(sheets_model::CellRef::new(0, c)),
                Some(Content::Value(Value::Number(_)))
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            false, false, false, false, false, false, true, true, true, true
        ]
    );
}

#[quench_test::test]
fn options_change_what_is_read() {
    let wb = read(
        "a;1\n=1+2;x\n",
        "S",
        &ReadOptions {
            delimiter: b';',
            detect_types: false,
            formulas: false,
        },
    )
    .unwrap();
    assert_eq!(
        wb.sheets[0].get(at("B1")),
        Some(&Content::Value(Value::Text("1".into())))
    );
    assert_eq!(
        wb.sheets[0].get(at("A2")),
        Some(&Content::Value(Value::Text("=1+2".into())))
    );
    let wb = read(
        "=1+2,x\n=\n",
        "S",
        &ReadOptions {
            formulas: true,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        wb.sheets[0].get(at("A1")),
        Some(&Content::Formula("1+2".into()))
    );
    assert_eq!(
        wb.sheets[0].get(at("A2")),
        Some(&Content::Value(Value::Text("=".into())))
    );
    let wb = read("\u{feff}a,b\n", "S", &ReadOptions::default()).unwrap();
    assert_eq!(
        wb.sheets[0].get(at("A1")),
        Some(&Content::Value(Value::Text("a".into())))
    );
    assert!(read("", "S", &ReadOptions::default()).unwrap().sheets[0].is_empty());
}

#[quench_test::test]
fn a_sheet_writes_what_it_shows() {
    let mut wb = read_default("item,qty,total\nbolt,4,\n\"say \"\"hi\"\"\",2.5,\n");
    let money = wb.style_id(&sheets_model::Style {
        number_format: "#,##0.00".into(),
        ..Default::default()
    });
    wb.sheets[0].set(at("C2"), Content::Formula("B2*1000".into()));
    wb.sheets[0].set_style(at("C2"), money);
    let e = Engine::new(wb);
    assert_eq!(
        write(&e, 0, &WriteOptions::default()),
        "item,qty,total\nbolt,4,\"4,000.00\"\n\"say \"\"hi\"\"\",2.5,\n"
    );
    let raw = WriteOptions {
        delimiter: b'\t',
        formatted: false,
    };
    assert_eq!(
        write(&e, 0, &raw),
        "item\tqty\ttotal\nbolt\t4\t4000\n\"say \"\"hi\"\"\"\t2.5\t\n"
    );
    assert_eq!(write(&e, 5, &raw), "");
}

#[quench_test::test]
fn what_is_written_reads_back() {
    let text = "a,b,c\n\" lead\",\"line\nbreak\",x\n1,2,3\n,,\n";
    let e = Engine::new(read_default(text));
    let out = write(&e, 0, &WriteOptions::default());
    let e2 = Engine::new(read_default(&out));
    let cells = |e: &Engine| {
        e.workbook().sheets[0]
            .iter()
            .map(|(c, v)| (c, v.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(&e), cells(&e2));
}

#[quench_test::test]
fn tables_in_and_out_for_the_clipboard() {
    assert_eq!(
        records("a\tb\n1\t\"x\ty\"\n", b'\t').unwrap(),
        vec![vec!["a", "b"], vec!["1", "x\ty"]]
    );
    let mut out = String::new();
    quote("plain", '\t', &mut out);
    out.push('|');
    quote("a\tb", '\t', &mut out);
    out.push('|');
    quote("say \"hi\"\nnow", '\t', &mut out);
    assert_eq!(out, "plain|\"a\tb\"|\"say \"\"hi\"\"\nnow\"");
    assert_eq!(
        records(&out.replace('|', "\t"), b'\t').unwrap(),
        vec![vec!["plain", "a\tb", "say \"hi\"\nnow"]]
    );
}

quench_test::main!();
