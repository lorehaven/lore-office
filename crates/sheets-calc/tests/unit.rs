#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use sheets_calc::*;
use sheets_model::{CellError, CellRef, Content, Sheet, Style, Value, Workbook};

fn a(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

const fn num(n: f64) -> Content {
    Content::Value(Value::Number(n))
}

fn f(s: &str) -> Content {
    Content::Formula(s.into())
}

const fn n(x: f64) -> Value {
    Value::Number(x)
}

fn keys(list: &[(usize, &str)]) -> Vec<Key> {
    let mut v: Vec<Key> = list.iter().map(|(s, c)| (*s, a(c))).collect();
    v.sort_unstable();
    v
}

#[quench_test::test]
fn a_chain_follows_its_input() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), num(1.0));
    e.set(0, a("B1"), f("A1+1"));
    e.set(0, a("C1"), f("B1*10"));
    assert_eq!(e.value(0, a("C1")), n(20.0));
    let ch = e.set(0, a("A1"), num(4.0));
    assert_eq!(e.value(0, a("C1")), n(50.0));
    assert_eq!(ch, keys(&[(0, "A1"), (0, "B1"), (0, "C1")]));
}

#[quench_test::test]
fn only_changed_values_are_reported() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), num(1.0));
    e.set(0, a("B1"), f("A1>0"));
    e.set(0, a("C1"), f("IF(B1,1,2)"));
    // A1 changes, B1 stays TRUE, so C1 is not reported.
    assert_eq!(e.set(0, a("A1"), num(5.0)), keys(&[(0, "A1")]));
    assert_eq!(e.set(0, a("A1"), num(5.0)), Vec::<Key>::new());
    assert_eq!(
        e.set(0, a("A1"), num(-1.0)),
        keys(&[(0, "A1"), (0, "B1"), (0, "C1")])
    );
}

#[quench_test::test]
fn a_range_reads_every_cell_in_it() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("D1"), f("SUM(A1:A3)"));
    assert_eq!(e.value(0, a("D1")), n(0.0));
    let ch = e.set(0, a("A2"), num(7.0));
    assert_eq!(e.value(0, a("D1")), n(7.0));
    assert_eq!(ch, keys(&[(0, "A2"), (0, "D1")]));
    // A cell outside the range does not touch it.
    assert_eq!(e.set(0, a("A4"), num(1.0)), keys(&[(0, "A4")]));
    // Clearing a cell inside it does.
    e.clear(0, a("A2"));
    assert_eq!(e.value(0, a("D1")), n(0.0));
}

#[quench_test::test]
fn a_diamond_is_calculated_in_order() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("D1"), f("B1+C1")); // set before its inputs exist
    e.set(0, a("B1"), f("A1+1"));
    e.set(0, a("C1"), f("A1*2"));
    e.set(0, a("A1"), num(10.0));
    assert_eq!(e.value(0, a("D1")), n(31.0));
    let ch = e.set(0, a("A1"), num(1.0));
    assert_eq!(e.value(0, a("D1")), n(4.0));
    assert_eq!(ch.len(), 4);
}

#[quench_test::test]
fn replacing_a_formula_drops_its_old_inputs() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), num(1.0));
    e.set(0, a("B1"), f("A1+1"));
    e.set(0, a("B1"), num(100.0));
    assert_eq!(e.set(0, a("A1"), num(2.0)), keys(&[(0, "A1")]));
    assert_eq!(e.value(0, a("B1")), n(100.0));
    // And a new formula reads its new inputs only.
    e.set(0, a("B1"), f("C1"));
    assert_eq!(e.set(0, a("A1"), num(3.0)), keys(&[(0, "A1")]));
    assert_eq!(e.set(0, a("C1"), num(9.0)), keys(&[(0, "B1"), (0, "C1")]));
}

#[quench_test::test]
fn cycles_are_marked_and_recover() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), f("B1"));
    e.set(0, a("B1"), f("A1"));
    assert!(e.is_circular(0, a("A1")));
    assert!(e.is_circular(0, a("B1")));
    assert_eq!(e.value(0, a("A1")), Value::Error(CellError::Circular));
    e.set(0, a("C1"), f("A1+1"));
    assert!(e.is_circular(0, a("C1")));
    // Break the cycle.
    let ch = e.set(0, a("B1"), num(5.0));
    assert_eq!(e.value(0, a("A1")), n(5.0));
    assert_eq!(e.value(0, a("C1")), n(6.0));
    assert!(!e.is_circular(0, a("C1")));
    assert_eq!(ch, keys(&[(0, "A1"), (0, "B1"), (0, "C1")]));
    // A formula that reads itself.
    e.set(0, a("D1"), f("D1+1"));
    assert!(e.is_circular(0, a("D1")));
    // ... also through a range.
    e.set(0, a("E1"), f("SUM(E1:E3)"));
    assert!(e.is_circular(0, a("E1")));
    e.set(0, a("E1"), f("SUM(E2:E3)"));
    assert_eq!(e.value(0, a("E1")), n(0.0));
}

#[quench_test::test]
fn formulas_read_other_sheets() {
    let mut e = Engine::new(Workbook::new());
    assert_eq!(e.add_sheet("Data"), Some(1));
    assert_eq!(e.add_sheet("DATA"), None);
    assert_eq!(e.add_sheet(""), None);
    e.set(0, a("A1"), f("Data!A1*2+SUM(Data!B1:B2)"));
    e.set(1, a("A1"), num(5.0));
    e.set(1, a("B2"), num(1.0));
    assert_eq!(e.value(0, a("A1")), n(11.0));
    // A sheet that does not exist yet reads as #REF!, and starts working once it is added.
    e.set(0, a("A2"), f("Later!A1"));
    assert_eq!(e.value(0, a("A2")), Value::Error(CellError::Ref));
    let later = e.add_sheet("Later").unwrap();
    e.set(later, a("A1"), num(8.0));
    assert_eq!(e.value(0, a("A2")), n(8.0));
    assert_eq!(e.sheet_index("later"), Some(later));
}

#[quench_test::test]
fn sheets_can_be_renamed_and_removed() {
    let mut e = Engine::new(Workbook::new());
    e.add_sheet("Data");
    e.set(1, a("A1"), num(3.0));
    e.set(0, a("A1"), f("Data!A1"));
    assert_eq!(e.value(0, a("A1")), n(3.0));
    assert!(!e.rename_sheet(1, "sheet1")); // taken
    assert!(!e.rename_sheet(9, "x"));
    assert!(e.rename_sheet(1, "Numbers"));
    assert_eq!(
        e.value(0, a("A1")),
        n(3.0),
        "formulas follow the sheet to its new name"
    );
    assert_eq!(e.content(0, a("A1")), Some(&f("Numbers!A1")));
    assert!(e.remove_sheet(1));
    assert!(!e.remove_sheet(1));
    assert_eq!(e.value(0, a("A1")), Value::Error(CellError::Ref));
    assert_eq!(e.value(5, a("A1")), Value::Empty);
    assert_eq!(e.set(5, a("A1"), num(1.0)), Vec::<Key>::new());
}

#[quench_test::test]
fn a_loaded_workbook_is_calculated() {
    let mut wb = Workbook::new();
    let mut s = Sheet::new("Second");
    s.set(a("A1"), f("Sheet1!B1+1"));
    wb.sheets.push(s);
    wb.sheets[0].set(a("A1"), num(2.0));
    wb.sheets[0].set(a("B1"), f("A1*3"));
    let mut e = Engine::new(wb);
    assert_eq!(e.value(1, a("A1")), n(7.0));
    assert_eq!(e.workbook().sheets.len(), 2);
    e.recalc_all();
    assert_eq!(e.value(1, a("A1")), n(7.0));
}

#[quench_test::test]
fn a_formula_that_does_not_parse_says_why() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), f("1+"));
    assert_eq!(e.value(0, a("A1")), Value::Error(CellError::Name));
    assert_eq!(e.parse_error(0, a("A1")).unwrap().at, 2);
    assert!(e.parse_error(0, a("B1")).is_none());
    e.set(0, a("A1"), f("1+1"));
    assert!(e.parse_error(0, a("A1")).is_none());
    assert_eq!(e.value(0, a("A1")), n(2.0));
}

#[quench_test::test]
fn long_chains_do_not_overflow_the_stack() {
    let mut e = Engine::new(Workbook::new());
    let last = 20_000;
    for r in 2..=last {
        e.set(0, CellRef::new(r - 1, 0), f(&format!("A{}+1", r - 1)));
    }
    e.set(0, a("A1"), num(0.0));
    assert_eq!(
        e.value(0, CellRef::new(last - 1, 0)),
        n(f64::from(last - 1))
    );
    let ch = e.set(0, a("A1"), num(1.0));
    assert_eq!(ch.len(), last as usize);
    assert_eq!(e.value(0, CellRef::new(last - 1, 0)), n(f64::from(last)));
}

#[quench_test::test]
fn styles_and_content_can_be_read_and_set_without_recalculating() {
    let mut e = Engine::new(Workbook::new());
    e.set(0, a("A1"), f("1+1"));
    assert_eq!(e.content(0, a("A1")), Some(&f("1+1")));
    assert_eq!(e.content(0, a("B1")), None);
    assert_eq!(e.content(9, a("A1")), None);
    assert_eq!(e.style(0, a("A1")), 0);
    let pct = e.style_id(&Style {
        number_format: "0%".into(),
        ..Default::default()
    });
    assert_eq!(
        e.style_id(&Style {
            number_format: "0%".into(),
            ..Default::default()
        }),
        pct,
        "the same style is one index"
    );
    assert!(e.set_style(0, a("A1"), pct));
    assert!(!e.set_style(0, a("A1"), pct), "no change the second time");
    assert!(
        e.set_style(0, a("C3"), pct),
        "a styled cell that is otherwise empty"
    );
    assert_eq!(e.style(0, a("A1")), pct);
    assert_eq!(e.workbook().number_format(0, a("A1")), "0%");
    assert_eq!(e.value(0, a("A1")), n(2.0));
    assert!(!e.set_style(7, a("A1"), pct), "no such sheet");
    assert!(e.set_style(0, a("A1"), 0));
    assert_eq!(e.style(0, a("A1")), 0);
}

#[quench_test::test]
fn the_clock_chance_and_volatile_cells_follow_edits() {
    let mut e = Engine::new(Workbook::new());
    let time = std::rc::Rc::new(std::cell::Cell::new(45_351.5));
    let t = time.clone();
    e.set_clock(move || t.get());
    e.set(0, a("A1"), f("NOW()"));
    e.set(0, a("B1"), f("A1*2"));
    e.set(0, a("C1"), f("RAND()"));
    e.set(0, a("D1"), f("ROW()*100+COLUMN()"));
    e.set(0, a("E1"), f("TODAY()"));
    e.set(0, a("F1"), f("5+5")); // not volatile
    assert_eq!(e.value(0, a("A1")), n(45_351.5));
    assert_eq!(
        e.value(0, a("B1")),
        n(90_703.0),
        "what reads a volatile cell follows it"
    );
    assert_eq!(
        e.value(0, a("D1")),
        n(104.0),
        "ROW() and COLUMN() are the cell's own"
    );
    assert_eq!(e.value(0, a("E1")), n(45_351.0));
    let first = e.value(0, a("C1"));
    let Value::Number(r1) = first else {
        panic!("RAND is a number")
    };
    assert!((0.0..1.0).contains(&r1));
    // Time passes: nothing changes until the cells are calculated again, which every edit does.
    time.set(45_351.75);
    assert_eq!(e.value(0, a("A1")), n(45_351.5));
    let changed = e.set(0, a("G1"), num(1.0));
    assert_eq!(e.value(0, a("A1")), n(45_351.75));
    assert_eq!(e.value(0, a("B1")), n(90_703.5));
    assert_ne!(e.value(0, a("C1")), first, "chance moved on");
    assert!(
        changed.contains(&(0, a("A1")))
            && changed.contains(&(0, a("C1")))
            && changed.contains(&(0, a("G1")))
    );
    assert!(
        !changed.contains(&(0, a("F1"))),
        "a plain formula is not touched"
    );
    // The same without an edit.
    time.set(45_352.0);
    let again = e.recalc_volatile();
    assert!(again.contains(&(0, a("A1"))) && again.contains(&(0, a("E1"))));
    assert_eq!(e.value(0, a("E1")), n(45_352.0));
    assert!(!again.contains(&(0, a("F1"))));
    // Taking the formula away stops it being volatile.
    e.set(0, a("A1"), num(1.0));
    time.set(46_000.0);
    assert!(!e.recalc_volatile().contains(&(0, a("A1"))));
    // A hundred rolls are all different enough and all in range.
    for r in 2..=101 {
        e.set(0, CellRef::new(r, 5), f("RANDBETWEEN(1,6)"));
    }
    let faces: std::collections::BTreeSet<i64> = (2..=101)
        .map(|r| match e.value(0, CellRef::new(r, 5)) {
            Value::Number(v) => v as i64,
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(
        faces.iter().all(|v| (1..=6).contains(v)) && faces.len() >= 5,
        "{faces:?}"
    );
}

#[quench_test::test]
fn what_a_file_said_stands_in_for_a_function_that_cannot_be_calculated() {
    let mut wb = Workbook::new();
    wb.sheets[0].set(a("A1"), f("FROBNICATE(1)"));
    wb.sheets[0].set_cached(a("A1"), n(42.0));
    wb.sheets[0].set(a("B1"), f("A1+1")); // reads the stand-in
    wb.sheets[0].set(a("C1"), f("1+1"));
    wb.sheets[0].set_cached(a("C1"), n(99.0)); // calculable: the cache is ignored
    wb.sheets[0].set(a("D1"), f("1+"));
    wb.sheets[0].set_cached(a("D1"), Value::Text("was fine".into())); // does not parse here
    let mut e = Engine::new(wb);
    assert_eq!(e.value(0, a("A1")), n(42.0));
    assert_eq!(e.value(0, a("B1")), n(43.0));
    assert_eq!(e.value(0, a("C1")), n(2.0));
    assert_eq!(e.value(0, a("D1")), Value::Text("was fine".into()));
    // Changing the cell forgets it.
    e.set(0, a("A1"), f("FROBNICATE(2)"));
    assert_eq!(e.value(0, a("A1")), Value::Error(CellError::Name));
    assert_eq!(e.value(0, a("B1")), Value::Error(CellError::Name));
    e.set(0, a("D1"), num(5.0));
    assert_eq!(e.value(0, a("D1")), n(5.0));
    assert_eq!(e.workbook().sheets[0].cached(a("D1")), None);
}

#[quench_test::test]
fn renaming_a_sheet_rewrites_every_kind_of_reference_to_it() {
    let mut e = Engine::new(Workbook::new());
    e.add_sheet("Old Name");
    e.set(1, a("A1"), num(2.0));
    e.set(1, a("A2"), num(3.0));
    e.set(
        0,
        a("A1"),
        f("SUM('Old Name'!A1:A2)*'old name'!A1+IF(TRUE,-'Old Name'!A2%,0)"),
    );
    e.set(0, a("B1"), f("Sheet1!A1+1"));
    e.set(0, a("C1"), f("\"Old Name\"&A1")); // text is not a reference
    assert!(e.rename_sheet(1, "It's"));
    assert_eq!(
        e.content(0, a("A1")),
        Some(&f("SUM('It''s'!A1:A2)*'It''s'!A1+IF(TRUE,-'It''s'!A2%,0)"))
    );
    assert_eq!(
        e.content(0, a("B1")),
        Some(&f("Sheet1!A1+1")),
        "other sheets are not touched"
    );
    assert_eq!(e.content(0, a("C1")), Some(&f("\"Old Name\"&A1")));
    assert_eq!(e.value(0, a("A1")), n(9.97));
    assert!(e.rename_sheet(1, "old name")); // only the case changes
    assert_eq!(e.value(0, a("A1")), n(9.97));
}

#[quench_test::test]
fn inserting_and_deleting_rows_and_columns_moves_formulas_with_what_they_read() {
    use sheets_model::Axis;
    let mut e = Engine::new(Workbook::new());
    e.add_sheet("Other");
    for r in 1..=5 {
        e.set(0, CellRef::new(r - 1, 0), num(f64::from(r)));
    }
    e.set(0, a("B1"), f("SUM(A1:A5)"));
    e.set(0, a("B2"), f("A3*10"));
    e.set(0, a("B3"), f("$A$5+A$1"));
    e.set(1, a("A1"), f("Sheet1!A3+SUM(Sheet1!A1:A2)"));
    e.set(0, a("C1"), f("Other!A1")); // another sheet's cell does not move
    assert!(e.insert(0, Axis::Row, 2, 2)); // two rows above row 3
    assert_eq!(
        e.content(0, a("A5")),
        Some(&num(3.0)),
        "the cells moved down"
    );
    assert_eq!(e.content(0, a("A3")), None);
    assert_eq!(
        e.content(0, a("B1")),
        Some(&f("SUM(A1:A7)")),
        "a range grows around the insertion"
    );
    assert_eq!(e.content(0, a("B2")), Some(&f("A5*10")));
    assert_eq!(
        e.content(0, a("B5")),
        Some(&f("$A$7+A$1")),
        "this one moved with its row"
    );
    assert_eq!(
        e.content(1, a("A1")),
        Some(&f("Sheet1!A5+SUM(Sheet1!A1:A2)")),
        "other sheets follow too"
    );
    assert_eq!(e.content(0, a("C1")), Some(&f("Other!A1")));
    assert_eq!(e.value(0, a("B1")), n(15.0));
    assert_eq!(e.value(1, a("A1")), n(6.0));
    // Delete one of the new rows and one real row.
    assert!(e.delete(0, Axis::Row, 2, 1));
    assert_eq!(e.content(0, a("B1")), Some(&f("SUM(A1:A6)")));
    assert!(e.delete(0, Axis::Row, 2, 1));
    assert_eq!(e.content(0, a("B1")), Some(&f("SUM(A1:A5)")));
    assert_eq!(e.value(0, a("B1")), n(15.0));
    // Deleting the row a formula reads leaves #REF!, and a range just shrinks.
    assert!(e.delete(0, Axis::Row, 2, 1)); // the row that held 3
    assert_eq!(e.value(0, a("B1")), n(12.0));
    assert_eq!(e.content(0, a("B1")), Some(&f("SUM(A1:A4)")));
    assert_eq!(e.content(0, a("B2")), Some(&f("#REF!*10")));
    assert_eq!(e.value(0, a("B2")), Value::Error(CellError::Ref));
    assert_eq!(e.value(1, a("A1")), Value::Error(CellError::Ref));
    // Columns: B moves right, formulas that read column A are not touched.
    assert!(e.insert(0, Axis::Col, 1, 1));
    assert_eq!(e.content(0, a("C1")), Some(&f("SUM(A1:A4)")));
    assert!(e.insert(0, Axis::Col, 0, 1));
    assert_eq!(e.content(0, a("D1")), Some(&f("SUM(B1:B4)")));
    assert_eq!(e.value(0, a("D1")), n(12.0));
    assert!(e.delete(0, Axis::Col, 1, 1)); // the column the sum reads
    assert_eq!(e.value(0, a("C1")), Value::Error(CellError::Ref));
    assert!(!e.insert(9, Axis::Row, 0, 1), "no such sheet");
}

#[quench_test::test]
fn row_heights_and_styles_move_and_a_whole_workbook_can_be_restored() {
    use sheets_model::Axis;
    let mut e = Engine::new(Workbook::new());
    e.set_row_height(0, 3, Some(40.0));
    e.set_col_width(0, 2, Some(150.0));
    let bold = e.style_id(&sheets_model::Style {
        bold: true,
        ..sheets_model::Style::default()
    });
    e.set_style(0, a("A4"), bold);
    let before = e.workbook().clone();
    e.insert(0, Axis::Row, 1, 2);
    assert_eq!(e.workbook().sheets[0].row_height(5), Some(40.0));
    assert_eq!(e.workbook().sheets[0].row_height(3), None);
    assert_eq!(e.style(0, a("A6")), bold);
    assert_eq!(
        e.workbook().sheets[0].col_width(2),
        Some(150.0),
        "columns did not move"
    );
    e.delete(0, Axis::Row, 4, 3);
    assert_eq!(e.style(0, a("A4")), 0, "a deleted row's style is gone");
    e.restore(before.clone());
    assert_eq!(e.workbook(), &before);
}

#[quench_test::test]
fn defined_names_stand_for_ranges_and_constants_and_follow_their_cells() {
    let mut e = Engine::new(Workbook::new());
    for r in 1..=4 {
        e.set(0, CellRef::new(r - 1, 0), num(f64::from(r)));
    }
    assert!(e.define_name("Data", "Sheet1!$A$1:$A$4"));
    assert!(e.define_name("Rate", "0.25"));
    assert!(e.define_name("Twice", "Rate*2"));
    e.set(0, a("C1"), f("SUM(Data)*Rate"));
    e.set(0, a("C2"), f("Twice"));
    e.set(0, a("C3"), f("Missing+1"));
    assert_eq!(e.value(0, a("C1")), n(2.5));
    assert_eq!(e.value(0, a("C2")), n(0.5), "names may use names");
    assert_eq!(e.value(0, a("C3")), Value::Error(CellError::Name));
    // A cell the name covers changes: the formula follows.
    e.set(0, a("A4"), num(10.0));
    assert_eq!(e.value(0, a("C1")), n(4.0));
    // Redefining and removing.
    assert!(e.define_name("rate", "0.5")); // not case sensitive
    assert_eq!(e.value(0, a("C1")), n(8.0));
    assert!(e.define_name("Data", ""));
    assert_eq!(e.value(0, a("C1")), Value::Error(CellError::Name));
    // Names that are not names.
    for bad in ["A1", "1st", "my name", "", "R", "x-y"] {
        assert!(!e.define_name(bad, "1"), "{bad:?}");
    }
    assert!(!e.define_name("Fine", "1+"), "the formula must parse");
    // A name that refers to itself does not hang.
    assert!(e.define_name("Loop", "Loop+1"));
    e.set(0, a("D1"), f("Loop"));
    assert_eq!(e.value(0, a("D1")), Value::Error(CellError::Name));
}

#[quench_test::test]
fn a_formula_that_gives_several_values_fills_the_cells_below_and_what_reads_them_follows() {
    let mut e = Engine::new(Workbook::new());
    for (r, v) in [1.0, 5.0, 3.0, 8.0, 2.0].into_iter().enumerate() {
        e.set(0, CellRef::new(u32::try_from(r).unwrap(), 0), num(v));
    }
    let changed = e.set(0, a("B1"), f("FILTER(A1:A5,A1:A5>2)"));
    assert_eq!(
        (
            e.value(0, a("B1")),
            e.value(0, a("B2")),
            e.value(0, a("B3"))
        ),
        (n(5.0), n(3.0), n(8.0))
    );
    assert_eq!(e.value(0, a("B4")), Value::Empty);
    assert_eq!(
        e.content(0, a("B2")),
        None,
        "the cells hold nothing of their own"
    );
    assert!(
        changed.contains(&(0, a("B2"))) && changed.contains(&(0, a("B3"))),
        "{changed:?}"
    );
    e.set(0, a("C1"), f("SUM(B1:B4)"));
    e.set(0, a("C2"), f("B3*2"));
    assert_eq!(
        (e.value(0, a("C1")), e.value(0, a("C2"))),
        (n(16.0), n(16.0))
    );
    // The source changes: the spill grows and shrinks, and readers follow.
    e.set(0, a("A1"), num(10.0));
    assert_eq!(e.value(0, a("B1")), n(10.0));
    assert_eq!(
        (e.value(0, a("B4")), e.value(0, a("C1"))),
        (n(8.0), n(26.0))
    );
    assert_eq!(e.value(0, a("C2")), n(6.0), "B3 is 3 now");
    e.set(0, a("A2"), num(0.0));
    assert_eq!(
        (e.value(0, a("B3")), e.value(0, a("B4"))),
        (n(8.0), Value::Empty)
    );
    assert_eq!(e.value(0, a("C1")), n(21.0));
    // Something in the way: #SPILL!, and it comes back when the way is clear.
    e.set(0, a("B3"), num(99.0));
    assert_eq!(e.value(0, a("B1")), Value::Error(CellError::Spill));
    assert_eq!(
        e.value(0, a("B2")),
        Value::Empty,
        "nothing is filled while blocked"
    );
    assert_eq!(e.value(0, a("B3")), n(99.0));
    e.set(0, a("B3"), Content::Value(Value::Empty));
    assert_eq!(
        (e.value(0, a("B1")), e.value(0, a("B3"))),
        (n(10.0), n(8.0))
    );
    // Typing into a spilled cell blocks it too; replacing the formula frees the cells.
    e.set(0, a("B2"), num(7.0));
    assert_eq!(e.value(0, a("B1")), Value::Error(CellError::Spill));
    e.set(0, a("B1"), num(1.0));
    assert_eq!(
        (
            e.value(0, a("B1")),
            e.value(0, a("B2")),
            e.value(0, a("B3"))
        ),
        (n(1.0), n(7.0), Value::Empty)
    );
    // Other shapes: a block, a spill that reads another, a whole sheet rebuild.
    e.set(0, a("E1"), f("SEQUENCE(2,3)"));
    e.set(0, a("E4"), f("E1:G2*10"));
    assert_eq!(
        (
            e.value(0, a("G2")),
            e.value(0, a("G5")),
            e.value(0, a("E4"))
        ),
        (n(6.0), n(60.0), n(10.0))
    );
    let mut again = Engine::new(e.workbook().clone());
    assert_eq!(
        (again.value(0, a("G2")), again.value(0, a("G5"))),
        (n(6.0), n(60.0)),
        "a rebuilt engine spills the same"
    );
    again.set(0, a("E1"), num(5.0));
    assert_eq!(
        (again.value(0, a("E4")), again.value(0, a("F4"))),
        (n(50.0), n(0.0)),
        "the source is one number now and the empty cells around it multiply to 0"
    );
    assert_eq!(again.value(0, a("G2")), Value::Empty, "E1 no longer spills");
}

quench_test::main!();
