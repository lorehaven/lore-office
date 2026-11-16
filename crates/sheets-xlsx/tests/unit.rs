#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::iter_on_single_items,
    clippy::assertions_on_result_states
)]
use office_fmt::rels::{self, Rel};
use office_fmt::{ContentTypes, Package};
use sheets_model::{Border, CellError, CellRef, Content, Line, LineKind, Style, Value, Workbook};
use sheets_xlsx::*;

const OPENPYXL: &[u8] = include_bytes!("../../../corpus/openpyxl_basic.xlsx");
const LIBREOFFICE: &[u8] = include_bytes!("../../../corpus/libreoffice_basic.xlsx");

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

fn val(x: &Xlsx, sheet: &str, a1: &str) -> Value {
    let i = x.engine.sheet_index(sheet).unwrap();
    x.engine.value(i, at(a1))
}

const fn n(v: f64) -> Value {
    Value::Number(v)
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

/// What both fixtures hold, as this suite calculates it.
#[track_caller]
fn check_basic(x: &Xlsx) {
    let names: Vec<&str> = x
        .engine
        .workbook()
        .sheets
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["Data", "Other"]);
    assert_eq!(val(x, "Data", "A1"), t("Name"));
    assert_eq!(val(x, "Data", "B2"), n(4.0));
    assert_eq!(val(x, "Data", "D2"), n(6.0));
    assert_eq!(val(x, "Data", "D3"), n(2.25));
    assert_eq!(val(x, "Data", "E2"), n(45351.0));
    assert_eq!(val(x, "Data", "F2"), Value::Bool(true));
    assert_eq!(val(x, "Data", "F3"), Value::Bool(false));
    assert_eq!(val(x, "Data", "A3"), t("nut, hex"));
    assert_eq!(val(x, "Data", "A4"), t("  padded "));
    assert_eq!(val(x, "Data", "B4"), n(13.0));
    assert_eq!(val(x, "Data", "C4"), n(0.875));
    assert_eq!(val(x, "Data", "D4"), n(8.25));
    assert_eq!(val(x, "Data", "E4"), n(1.0));
    assert_eq!(val(x, "Data", "F4"), Value::Bool(false));
    assert_eq!(val(x, "Data", "A5"), t("multi\nline"));
    assert_eq!(val(x, "Data", "B5"), n(-1e-7));
    assert_eq!(val(x, "Data", "A6"), n(9.0)); // XLOOKUP
    assert_eq!(val(x, "Data", "B6"), t("Thursday"));
    assert_eq!(val(x, "Data", "C6"), t("big"));
    assert_eq!(val(x, "Data", "D6"), Value::Error(CellError::Div0));
    assert_eq!(val(x, "Data", "E6"), n(6.0));
    assert_eq!(val(x, "Data", "F6"), n(27.0));
    assert_eq!(val(x, "Data", "A8"), t("merged title"));
    assert_eq!(val(x, "Other", "B2"), n(26.0));
    assert_eq!(val(x, "Other", "A1"), t("It's"));
    let wb = x.engine.workbook();
    let shown = |a: &str, v: f64| sheets_format::format_value(wb.number_format(0, at(a)), &n(v));
    assert_eq!(shown("E2", 45351.0), "2024-02-29"); // LibreOffice writes yyyy\-mm\-dd
    assert_eq!(shown("D2", 6.0), "$6.00");
    assert_eq!(shown("C2", 1234.5), "1,234.50");
    assert_eq!(wb.number_format(0, at("B2")), "General");
}

#[quench_test::test]
fn files_from_openpyxl_and_libreoffice_read() {
    check_basic(&read(OPENPYXL).unwrap());
    check_basic(&read(LIBREOFFICE).unwrap());
}

#[quench_test::test]
fn newer_functions_lose_their_prefix_when_read() {
    let x = read(OPENPYXL).unwrap();
    let Some(Content::Formula(f)) = x.engine.workbook().sheets[0].get(at("A6")) else {
        panic!("A6 is not a formula")
    };
    assert!(f.starts_with("XLOOKUP("), "{f}");
}

type Cells = Vec<(String, Content, String)>;

/// The contents of the workbook, for comparing two readings.
fn snapshot(x: &Xlsx) -> Vec<(String, Cells)> {
    let wb = x.engine.workbook();
    wb.sheets
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let cells = s
                .iter()
                .map(|(c, content)| {
                    (
                        c.to_string(),
                        content.clone(),
                        wb.number_format(i, c).to_string(),
                    )
                })
                .collect();
            (s.name.clone(), cells)
        })
        .collect()
}

#[quench_test::test]
fn a_file_written_and_read_again_is_the_same_workbook() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let x = read(bytes).unwrap();
        let again = read(&x.write().unwrap()).unwrap();
        check_basic(&again);
        assert_eq!(snapshot(&x), snapshot(&again));
        // And a second time: writing is stable.
        let third = read(&again.write().unwrap()).unwrap();
        assert_eq!(snapshot(&again), snapshot(&third));
    }
}

#[quench_test::test]
fn what_this_suite_does_not_know_survives() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let x = read(bytes).unwrap();
        let out = Package::read(&x.write().unwrap()).unwrap();
        let sheet =
            String::from_utf8(out.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        assert!(sheet.contains("<mergeCell ref=\"A8:C8\""), "merged cells");
        assert!(sheet.contains("width=\"20\""), "column width");
        assert!(sheet.contains("ht=\"30\""), "height of the empty row 7");
        assert!(
            sheet.contains("<f>_xlfn.XLOOKUP("),
            "newer functions are written with their prefix"
        );
        let book = String::from_utf8(out.get("xl/workbook.xml").unwrap().to_vec()).unwrap();
        assert!(
            book.contains("definedName") && book.contains("Data!$D$4"),
            "defined names"
        );
        assert!(book.contains("fullCalcOnLoad=\"1\""));
        assert!(out.get("xl/theme/theme1.xml").is_some());
        assert!(out.get("docProps/core.xml").is_some());
    }
}

#[quench_test::test]
fn edits_are_recalculated_and_written() {
    let mut x = read(OPENPYXL).unwrap();
    x.engine.set(0, at("B2"), Content::Value(n(10.0)));
    x.engine.set(0, at("A10"), Content::Value(t("added")));
    let new = x.add_sheet("Summary").unwrap();
    x.engine
        .set(new, at("A1"), Content::Formula("Data!D2+Data!D3".into()));
    let money = x.engine.workbook().styles.len();
    let mut style_set = x.engine.workbook().clone();
    let id = style_set.style_id(&Style {
        number_format: "0.000%".into(),
        ..Default::default()
    });
    assert_eq!(id as usize, money);
    // Styles are added through the workbook the engine holds.
    let again = {
        let mut wb = x.engine.workbook().clone();
        let id = wb.style_id(&Style {
            number_format: "0.000%".into(),
            ..Default::default()
        });
        wb.sheets[new].set_style(at("A1"), id);
        let mut y = Xlsx::new(wb);
        // A rebuilt file keeps nothing of the original; the point here is the new style round trip.
        y.engine.recalc_all();
        read(&y.write().unwrap()).unwrap()
    };
    assert_eq!(
        again.engine.workbook().number_format(new, at("A1")),
        "0.000%"
    );
    let back = read(&x.write().unwrap()).unwrap();
    assert_eq!(val(&back, "Data", "D2"), n(15.0));
    assert_eq!(val(&back, "Data", "B4"), n(19.0));
    assert_eq!(val(&back, "Data", "A10"), t("added"));
    assert_eq!(val(&back, "Summary", "A1"), n(17.25));
    assert_eq!(back.engine.workbook().sheets.len(), 3);
    // What was there is still there.
    assert_eq!(val(&back, "Data", "A4"), t("  padded "));
}

#[quench_test::test]
fn styles_added_to_an_existing_file_are_appended() {
    let mut x = read(LIBREOFFICE).unwrap();
    let mut wb = x.engine.workbook().clone();
    let id = wb.style_id(&Style {
        number_format: "[Red]0.0;(0.0)".into(),
        ..Default::default()
    });
    wb.sheets[0].set_style(at("B3"), id);
    x.engine = sheets_calc::Engine::new(wb);
    let back = read(&x.write().unwrap()).unwrap();
    assert_eq!(
        back.engine.workbook().number_format(0, at("B3")),
        "[Red]0.0;(0.0)"
    );
    let e2 = back.engine.workbook().number_format(0, at("E2"));
    assert_eq!(sheets_format::format_value(e2, &n(45351.0)), "2024-02-29"); // the file's own styles are intact
    let out = Package::read(&x.write().unwrap()).unwrap();
    let styles = String::from_utf8(out.get("xl/styles.xml").unwrap().to_vec()).unwrap();
    assert!(styles.contains("[Red]0.0;(0.0)"));
}

#[quench_test::test]
fn borders_font_size_and_family_are_written_and_read_back() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        let line = |kind, color| Some(Line { kind, color });
        let boxed = Border {
            top: line(LineKind::Thin, None),
            bottom: line(LineKind::Double, Some(0x00ff_0000)),
            left: line(LineKind::Dashed, Some(0x0000_00ff)),
            right: line(LineKind::Thick, None),
        };
        let a = wb.style_id(&Style {
            border: boxed,
            font_size: Some(29),
            font_name: Some("Courier New".into()),
            bold: true,
            ..Default::default()
        });
        let b = wb.style_id(&Style {
            font_size: Some(48),
            ..Default::default()
        });
        wb.sheets[0].set_style(at("G5"), a);
        wb.sheets[0].set_style(at("G6"), b);
        x.engine = sheets_calc::Engine::new(wb);
        let back = read(&x.write().unwrap()).unwrap();
        let styles = &back.engine.workbook().styles;
        let got = &styles[back.engine.style(0, at("G5")) as usize];
        assert_eq!(got.border, boxed);
        assert_eq!(
            (got.font_size, got.font_name.as_deref(), got.bold),
            (Some(29), Some("Courier New"), true)
        );
        let big = &styles[back.engine.style(0, at("G6")) as usize];
        assert_eq!(big.font_size, Some(48));
        assert_eq!(big.border, Border::default());
        // The default format shows no size or family of its own.
        let plain = &styles[0];
        assert_eq!((plain.font_size, plain.font_name.as_deref()), (None, None));
    }
}

#[quench_test::test]
fn a_border_from_a_file_is_read_and_survives_other_changes() {
    let x = read(OPENPYXL).unwrap();
    let wb = x.engine.workbook();
    assert!(
        wb.styles.iter().any(|s| !s.border.is_none()),
        "the corpus has a bordered cell"
    );
    let mut y = read(OPENPYXL).unwrap();
    let mut wb = y.engine.workbook().clone();
    let (at_b, st) = wb.sheets[0]
        .styled()
        .find_map(|(c, id)| {
            (!wb.styles[id as usize].border.is_none()).then(|| (c, wb.styles[id as usize].clone()))
        })
        .unwrap();
    let id = wb.style_id(&Style {
        bold: !st.bold,
        ..st.clone()
    });
    wb.sheets[0].set_style(at_b, id);
    y.engine = sheets_calc::Engine::new(wb);
    let back = read(&y.write().unwrap()).unwrap();
    let now = &back.engine.workbook().styles[back.engine.style(0, at_b) as usize];
    assert_eq!(
        now.border, st.border,
        "the border stays when the font changes"
    );
    assert_eq!(now.bold, !st.bold);
}

#[quench_test::test]
fn frozen_panes_merges_and_hidden_rows_and_columns_round_trip() {
    use sheets_model::Range;
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        // The corpus has a merged range that is read into the model now.
        assert!(
            x.engine.workbook().sheets[0].merge_at(at("A8")).is_some(),
            "A8:C8 is merged"
        );
        let mut wb = x.engine.workbook().clone();
        wb.sheets[0].set_freeze(2, 1);
        wb.sheets[0].merge(Range::parse("E10:F11").unwrap());
        wb.sheets[0].set_row_hidden(4, true);
        wb.sheets[0].set_row_hidden(20, true); // a row with no cells at all
        wb.sheets[0].set_col_hidden(6, true);
        wb.sheets[0].set_col_width(9, Some(200.0));
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let back = read(&out).unwrap();
        let s = &back.engine.workbook().sheets[0];
        assert_eq!(s.freeze(), (2, 1));
        assert!(s.merge_at(at("F11")).is_some() && s.merge_at(at("A8")).is_some());
        assert!(s.row_hidden(4) && s.row_hidden(20) && !s.row_hidden(5));
        assert!(s.col_hidden(6) && !s.col_hidden(7));
        assert_eq!(s.col_width(9), Some(200.0));
        let sheet = String::from_utf8(
            Package::read(&out)
                .unwrap()
                .get("xl/worksheets/sheet1.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(sheet.contains("<pane xSplit=\"1\" ySplit=\"2\" topLeftCell=\"B3\" activePane=\"bottomRight\" state=\"frozen\"/>"), "{sheet}");
        // Taking them away takes them out of the file.
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_freeze(0, 0);
        wb.sheets[0].unmerge(at("E10"));
        wb.sheets[0].set_row_hidden(4, false);
        wb.sheets[0].set_col_hidden(6, false);
        y.engine = sheets_calc::Engine::new(wb);
        let again = read(&y.write().unwrap()).unwrap();
        let s = &again.engine.workbook().sheets[0];
        assert_eq!(s.freeze(), (0, 0));
        assert!(s.merge_at(at("E10")).is_none() && s.merge_at(at("A8")).is_some());
        assert!(!s.row_hidden(4) && s.row_hidden(20) && !s.col_hidden(6));
    }
}

#[quench_test::test]
fn an_autofilter_with_its_checked_values_round_trips() {
    use sheets_model::{AutoFilter, Range};
    let mut x = read(OPENPYXL).unwrap();
    let mut wb = x.engine.workbook().clone();
    let allowed = [(
        1_u32,
        ["a".to_string(), "b & c".to_string(), String::new()]
            .into_iter()
            .collect(),
    )]
    .into_iter()
    .collect();
    wb.sheets[0].set_autofilter(Some(AutoFilter {
        range: Range::parse("A1:C9").unwrap(),
        allowed,
    }));
    x.engine = sheets_calc::Engine::new(wb);
    let out = x.write().unwrap();
    let sheet = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/worksheets/sheet1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(sheet.contains("<autoFilter ref=\"A1:C9\"><filterColumn colId=\"1\"><filters blank=\"1\"><filter val=\"a\"/><filter val=\"b &amp; c\"/></filters></filterColumn></autoFilter>"), "{sheet}");
    let back = read(&out).unwrap();
    let f = back.engine.workbook().sheets[0].autofilter().unwrap();
    assert_eq!(f.range, Range::parse("A1:C9").unwrap());
    assert_eq!(
        f.allowed[&1].iter().cloned().collect::<Vec<_>>(),
        ["", "a", "b & c"]
    );
    // Removing it takes it out of the file.
    let mut y = read(&out).unwrap();
    let mut wb = y.engine.workbook().clone();
    wb.sheets[0].set_autofilter(None);
    y.engine = sheets_calc::Engine::new(wb);
    assert!(
        read(&y.write().unwrap()).unwrap().engine.workbook().sheets[0]
            .autofilter()
            .is_none()
    );
}

#[quench_test::test]
fn validations_and_conditional_formats_round_trip() {
    use sheets_model::{
        Allow, Compare, Cond, CondRule, Dxf, NumberKind, Range, Severity, Validation,
    };
    let r = |t: &str| Range::parse(t).unwrap();
    let mut x = read(LIBREOFFICE).unwrap();
    let mut wb = x.engine.workbook().clone();
    let validations = vec![
        Validation {
            range: r("G2:G9"),
            allow: Allow::List(vec!["red".into(), "green & blue".into()]),
            allow_blank: true,
            dropdown: true,
            severity: Severity::Stop,
            title: "Colour".into(),
            message: "Pick one".into(),
        },
        Validation {
            range: r("H2:H9"),
            allow: Allow::ListFrom("Sheet1!$A$1:$A$5".into()),
            allow_blank: false,
            dropdown: false,
            severity: Severity::Warning,
            title: String::new(),
            message: String::new(),
        },
        Validation {
            range: r("I2:I9"),
            allow: Allow::Number {
                kind: NumberKind::Whole,
                op: Compare::Between,
                a: "1".into(),
                b: "10".into(),
            },
            allow_blank: false,
            dropdown: true,
            severity: Severity::Information,
            title: String::new(),
            message: "1 to 10".into(),
        },
        Validation {
            range: r("J2"),
            allow: Allow::Number {
                kind: NumberKind::Length,
                op: Compare::LessOrEqual,
                a: "5".into(),
                b: String::new(),
            },
            allow_blank: true,
            dropdown: true,
            severity: Severity::Stop,
            title: String::new(),
            message: String::new(),
        },
        Validation {
            range: r("K2:K3"),
            allow: Allow::Custom("ISNUMBER(K2)".into()),
            allow_blank: true,
            dropdown: true,
            severity: Severity::Stop,
            title: String::new(),
            message: String::new(),
        },
    ];
    let red = Dxf {
        bold: true,
        italic: false,
        color: Some(0x009C_0006),
        fill: Some(0x00FF_C7CE),
    };
    let rules = vec![
        CondRule {
            range: r("A1:A9"),
            cond: Cond::CellIs {
                op: Compare::Greater,
                a: "5".into(),
                b: String::new(),
            },
            dxf: red.clone(),
            priority: 1,
            stop_if_true: false,
        },
        CondRule {
            range: r("B1:B9"),
            cond: Cond::Contains("a \"q\"".into()),
            dxf: red.clone(),
            priority: 2,
            stop_if_true: true,
        },
        CondRule {
            range: r("C1:C9"),
            cond: Cond::Expression("$A1>2".into()),
            dxf: Dxf {
                italic: true,
                ..Dxf::default()
            },
            priority: 3,
            stop_if_true: false,
        },
        CondRule {
            range: r("D1:D9"),
            cond: Cond::Top {
                n: 3,
                bottom: true,
                percent: false,
            },
            dxf: red.clone(),
            priority: 4,
            stop_if_true: false,
        },
        CondRule {
            range: r("E1:E9"),
            cond: Cond::ColorScale {
                low: 0x00F8_696B,
                mid: Some(0x00FF_EB84),
                high: 0x0063_BE7B,
            },
            dxf: Dxf::default(),
            priority: 5,
            stop_if_true: false,
        },
        CondRule {
            range: r("F1:F9"),
            cond: Cond::DataBar { color: 0x0063_8EC6 },
            dxf: Dxf::default(),
            priority: 6,
            stop_if_true: false,
        },
        CondRule {
            range: r("G1:G9"),
            cond: Cond::Duplicates,
            dxf: red.clone(),
            priority: 7,
            stop_if_true: false,
        },
        CondRule {
            range: r("H1:H9"),
            cond: Cond::AboveAverage { below: true },
            dxf: red.clone(),
            priority: 8,
            stop_if_true: false,
        },
        CondRule {
            range: r("I1:I9"),
            cond: Cond::BeginsWith("ab".into()),
            dxf: red.clone(),
            priority: 9,
            stop_if_true: false,
        },
        CondRule {
            range: r("J1:J9"),
            cond: Cond::Blanks,
            dxf: red,
            priority: 10,
            stop_if_true: false,
        },
    ];
    wb.sheets[0].set_validations(validations.clone());
    wb.sheets[0].set_cond_rules(rules.clone());
    wb.sheets[0].set_cond_raw(vec!["<conditionalFormatting sqref=\"L1:L5\"><cfRule type=\"iconSet\" priority=\"11\"><iconSet iconSet=\"3Arrows\"><cfvo type=\"percent\" val=\"0\"/><cfvo type=\"percent\" val=\"33\"/><cfvo type=\"percent\" val=\"67\"/></iconSet></cfRule></conditionalFormatting>".into()]);
    x.engine = sheets_calc::Engine::new(wb);
    let out = x.write().unwrap();
    let back = read(&out).unwrap();
    let s = &back.engine.workbook().sheets[0];
    assert_eq!(s.validations(), validations.as_slice());
    let got = s.cond_rules();
    assert_eq!(got.len(), rules.len());
    for (a, b) in got.iter().zip(&rules) {
        assert_eq!(
            (&a.range, &a.cond, a.stop_if_true),
            (&b.range, &b.cond, b.stop_if_true)
        );
        if !matches!(b.cond, Cond::ColorScale { .. } | Cond::DataBar { .. }) {
            assert_eq!(a.dxf, b.dxf, "{:?}", b.cond);
        }
    }
    assert_eq!(s.cond_raw().len(), 1, "what is not understood is kept");
    assert!(s.cond_raw()[0].contains("3Arrows"));
    let again = read(&back.write().unwrap()).unwrap();
    assert_eq!(
        again.engine.workbook().sheets[0].cond_rules(),
        s.cond_rules(),
        "writing is stable"
    );
    let styles = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/styles.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        styles.contains("<dxfs count=\"2\">"),
        "two distinct formats were added: {styles}"
    );
}

#[quench_test::test]
fn defined_names_are_read_and_written_beside_the_ones_the_suite_does_not_own() {
    let x = read(OPENPYXL).unwrap();
    assert!(
        x.engine
            .workbook()
            .names
            .iter()
            .any(|(_, r)| r.contains("$D$4")),
        "{:?}",
        x.engine.workbook().names
    );
    let mut y = read(OPENPYXL).unwrap();
    let mut wb = y.engine.workbook().clone();
    wb.set_name("Tax", "0.2");
    wb.set_name("Totals", "Data!$D$4:$D$9");
    y.engine = sheets_calc::Engine::new(wb);
    let out = y.write().unwrap();
    let back = read(&out).unwrap();
    assert_eq!(back.engine.workbook().name("tax"), Some("0.2"));
    assert_eq!(
        back.engine.workbook().name("Totals"),
        Some("Data!$D$4:$D$9")
    );
    let book = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/workbook.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        book.contains("<definedName name=\"Tax\">0.2</definedName>"),
        "{book}"
    );
    // Removing every name leaves none.
    let mut z = read(&out).unwrap();
    let mut wb = z.engine.workbook().clone();
    wb.names.clear();
    z.engine = sheets_calc::Engine::new(wb);
    let none = read(&z.write().unwrap()).unwrap();
    assert_eq!(none.engine.workbook().names, Vec::<(String, String)>::new());
}

#[quench_test::test]
fn notes_on_cells_are_written_as_comments_with_their_drawing_and_read_back() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        wb.sheets[0].set_comment(at("B2"), "first & <second>\nline two");
        wb.sheets[0].set_comment(at("D5"), "another");
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let pkg = Package::read(&out).unwrap();
        let names: Vec<&str> = pkg.names().collect();
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("xl/comments") && n.ends_with(".xml")),
            "{names:?}"
        );
        assert!(names.iter().any(|n| n.ends_with(".vml")), "{names:?}");
        let sheet =
            String::from_utf8(pkg.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        assert!(sheet.contains("<legacyDrawing r:id="), "{sheet}");
        let back = read(&out).unwrap();
        let s = &back.engine.workbook().sheets[0];
        assert_eq!(s.comment(at("B2")), Some("first & <second>\nline two"));
        assert_eq!(s.comment(at("D5")), Some("another"));
        assert_eq!(s.comments().count(), 2);
        // Writing again replaces the parts rather than piling up more.
        let again = Package::read(&back.write().unwrap()).unwrap();
        assert_eq!(
            again
                .names()
                .filter(|n| n.starts_with("xl/comments"))
                .count(),
            1
        );
        assert_eq!(again.names().filter(|n| n.ends_with(".vml")).count(), 1);
        // Removing the notes removes the parts.
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_comment(at("B2"), "");
        wb.sheets[0].set_comment(at("D5"), "");
        y.engine = sheets_calc::Engine::new(wb);
        let none = Package::read(&y.write().unwrap()).unwrap();
        assert!(
            none.names()
                .all(|n| !n.starts_with("xl/comments") && !n.ends_with(".vml")),
            "{:?}",
            none.names().collect::<Vec<_>>()
        );
        let sheet =
            String::from_utf8(none.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        assert!(!sheet.contains("legacyDrawing"));
    }
}

#[quench_test::test]
fn formulas_that_fill_several_cells_are_written_as_array_formulas_and_read_back() {
    let mut x = read(OPENPYXL).unwrap();
    let mut wb = x.engine.workbook().clone();
    for (i, v) in [1.0, 5.0, 3.0, 8.0].into_iter().enumerate() {
        wb.sheets[0].set(
            CellRef::new(30 + u32::try_from(i).unwrap(), 0),
            Content::Value(Value::Number(v)),
        );
    }
    wb.sheets[0].set(
        at("B31"),
        Content::Formula("FILTER(A31:A34,A31:A34>2)".into()),
    );
    wb.sheets[0].set(at("C31"), Content::Formula("SORT(A31:A34,1,-1)".into()));
    wb.sheets[0].set(at("D31"), Content::Formula("SUM(B31:B33)".into()));
    x.engine = sheets_calc::Engine::new(wb);
    assert_eq!(x.engine.value(0, at("B33")), n(8.0));
    let out = x.write().unwrap();
    let sheet = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/worksheets/sheet1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        sheet.contains(
            "<f t=\"array\" ref=\"B31:B33\">_xlfn._xlws.FILTER(A31:A34,A31:A34&gt;2)</f>"
        ),
        "{sheet}"
    );
    assert!(
        sheet.contains("<f t=\"array\" ref=\"C31:C34\">_xlfn._xlws.SORT("),
        "{sheet}"
    );
    assert!(
        sheet.contains("<c r=\"B33\"><v>8</v></c>"),
        "the filled cells hold their values"
    );
    let back = read(&out).unwrap();
    let e = &back.engine;
    assert_eq!(
        e.content(0, at("B31")),
        Some(&Content::Formula("FILTER(A31:A34,A31:A34>2)".into()))
    );
    assert_eq!(
        e.content(0, at("B32")),
        None,
        "a filled cell is not a cell of its own"
    );
    assert_eq!(
        (
            e.value(0, at("B32")),
            e.value(0, at("B33")),
            e.value(0, at("C34"))
        ),
        (n(3.0), n(8.0), n(1.0))
    );
    assert_eq!(e.value(0, at("D31")), n(16.0));
    assert_eq!(
        read(&back.write().unwrap())
            .unwrap()
            .engine
            .value(0, at("C31")),
        n(8.0),
        "writing is stable"
    );
}

#[quench_test::test]
fn outline_levels_of_rows_and_columns_round_trip_with_the_collapsed_marker() {
    let mut x = read(OPENPYXL).unwrap();
    let mut wb = x.engine.workbook().clone();
    for r in 10..13 {
        wb.sheets[0].set_row_level(r, 1);
        wb.sheets[0].set_row_hidden(r, true);
    }
    wb.sheets[0].set_row_level(11, 2);
    wb.sheets[0].set_col_level(8, 1);
    wb.sheets[0].set_col_level(9, 1);
    x.engine = sheets_calc::Engine::new(wb);
    let out = x.write().unwrap();
    let sheet = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/worksheets/sheet1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        sheet.contains("outlineLevelRow=\"2\"") && sheet.contains("outlineLevelCol=\"1\""),
        "{sheet}"
    );
    assert!(
        sheet.contains("<row r=\"14\"")
            && sheet
                .split("<row r=\"14\"")
                .nth(1)
                .unwrap()
                .starts_with(" collapsed=\"1\""),
        "the row under a hidden group says so"
    );
    let back = read(&out).unwrap();
    let s = &back.engine.workbook().sheets[0];
    assert_eq!(
        (
            s.row_level(10),
            s.row_level(11),
            s.row_level(13),
            s.col_level(8),
            s.col_level(9),
            s.col_level(7)
        ),
        (1, 2, 0, 1, 1, 0)
    );
    assert!(s.row_hidden(11));
    let mut y = read(&out).unwrap();
    let mut wb = y.engine.workbook().clone();
    for r in 10..13 {
        wb.sheets[0].set_row_level(r, 0);
    }
    wb.sheets[0].set_col_level(8, 0);
    y.engine = sheets_calc::Engine::new(wb);
    let again = read(&y.write().unwrap()).unwrap();
    let s = &again.engine.workbook().sheets[0];
    assert_eq!((s.row_level(10), s.col_level(8), s.col_level(9)), (0, 0, 1));
}

#[quench_test::test]
fn the_format_of_whole_columns_and_rows_is_written_and_read_back() {
    let mut x = read(OPENPYXL).unwrap();
    let mut wb = x.engine.workbook().clone();
    let green = wb.style_id(&Style {
        fill: Some(0x0000_FF00),
        ..Style::default()
    });
    let bold = wb.style_id(&Style {
        bold: true,
        ..Style::default()
    });
    wb.sheets[0].set_col_style(11, green);
    wb.sheets[0].set_col_style(12, green);
    wb.sheets[0].set_row_style(40, bold);
    x.engine = sheets_calc::Engine::new(wb);
    let out = x.write().unwrap();
    let sheet = String::from_utf8(
        Package::read(&out)
            .unwrap()
            .get("xl/worksheets/sheet1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        sheet.contains("<row r=\"41\" s=\"") && sheet.contains("customFormat=\"1\""),
        "{sheet}"
    );
    let back = read(&out).unwrap();
    let s = &back.engine.workbook().sheets[0];
    assert_eq!(
        (
            s.col_style(11),
            s.col_style(12),
            s.col_style(13),
            s.row_style(40)
        ),
        (green, green, 0, bold)
    );
    assert_eq!(
        back.engine.style(0, at("L99")),
        green,
        "an empty cell of the column shows it"
    );
    let mut y = read(&out).unwrap();
    let mut wb = y.engine.workbook().clone();
    wb.sheets[0].set_col_style(11, 0);
    wb.sheets[0].set_row_style(40, 0);
    y.engine = sheets_calc::Engine::new(wb);
    let again = read(&y.write().unwrap()).unwrap();
    let s = &again.engine.workbook().sheets[0];
    assert_eq!(
        (s.col_style(11), s.col_style(12), s.row_style(40)),
        (0, green, 0)
    );
}

#[quench_test::test]
fn page_setup_with_print_area_and_titles_is_written_and_read_back() {
    use sheets_model::{Orientation, PageSetup, Paper, Range};
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        wb.sheets[0].set_page(PageSetup {
            orientation: Orientation::Landscape,
            paper: Paper::Letter,
            margin_mm: 20,
            fit_width: true,
            gridlines: true,
            repeat_rows: Some((0, 1)),
            print_area: Range::parse("A1:D9"),
        });
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let pkg = Package::read(&out).unwrap();
        let book = String::from_utf8(pkg.get("xl/workbook.xml").unwrap().to_vec()).unwrap();
        assert!(
            book.contains("_xlnm.Print_Area") && book.contains("$A$1:$D$9"),
            "{book}"
        );
        assert!(
            book.contains("_xlnm.Print_Titles") && book.contains("$1:$2"),
            "{book}"
        );
        let sheet =
            String::from_utf8(pkg.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        for want in [
            "orientation=\"landscape\"",
            "paperSize=\"1\"",
            "fitToWidth=\"1\"",
            "fitToHeight=\"0\"",
            "fitToPage=\"1\"",
            "gridLines=\"1\"",
            "left=\"0.787\"",
        ] {
            assert!(sheet.contains(want), "{want} in {sheet}");
        }
        let back = read(&out).unwrap();
        assert_eq!(
            back.engine.workbook().sheets[0].page(),
            &PageSetup {
                orientation: Orientation::Landscape,
                paper: Paper::Letter,
                margin_mm: 20,
                fit_width: true,
                gridlines: true,
                repeat_rows: Some((0, 1)),
                print_area: Range::parse("A1:D9"),
            }
        );
        assert!(
            back.engine
                .workbook()
                .names
                .iter()
                .all(|(n, _)| !n.contains("xlnm")),
            "they are not ordinary names"
        );
        // Taking the settings away takes the names and options away.
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_page(PageSetup {
            gridlines: false,
            ..PageSetup::default()
        });
        y.engine = sheets_calc::Engine::new(wb);
        let plain = y.write().unwrap();
        let book = String::from_utf8(
            Package::read(&plain)
                .unwrap()
                .get("xl/workbook.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!book.contains("Print_Area") && !book.contains("Print_Titles"));
        let again = read(&plain).unwrap();
        assert_eq!(
            again.engine.workbook().sheets[0].page().orientation,
            Orientation::Auto
        );
        assert!(!again.engine.workbook().sheets[0].page().fit_width);
    }
}

#[quench_test::test]
fn charts_are_written_with_their_drawing_and_read_back() {
    use sheets_model::{Chart, ChartKind, Range};
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        for (r, (label, a, b)) in [
            ("Quarter", 0.0, 0.0),
            ("Q1", 10.0, 30.0),
            ("Q2", 20.0, 25.0),
            ("Q3", 15.0, 5.0),
        ]
        .into_iter()
        .enumerate()
        {
            let r = 40 + u32::try_from(r).unwrap();
            let mut cell =
                |c: u32, v: Value| wb.sheets[0].set(CellRef::new(r, c), Content::Value(v));
            cell(0, Value::Text(label.into()));
            if r == 40 {
                cell(1, Value::Text("North".into()));
                cell(2, Value::Text("South".into()));
            } else {
                cell(1, Value::Number(a));
                cell(2, Value::Number(b));
            }
        }
        let data = Range::parse("A41:C44").unwrap();
        let mut column = Chart::new(ChartKind::Column, data);
        column.title = "Sales & \"costs\"".into();
        let mut rows = Chart::new(ChartKind::Line, Range::parse("A41:C44").unwrap());
        rows.rows_as_series = true;
        rows.legend = false;
        rows.at = CellRef::new(45, 0);
        rows.width = 300;
        rows.height = 180;
        let charts: Vec<Chart> = [
            ChartKind::Bar,
            ChartKind::Area,
            ChartKind::Pie,
            ChartKind::Scatter,
        ]
        .into_iter()
        .map(|k| Chart {
            kind: k,
            at: CellRef::new(60, 2),
            ..Chart::new(k, data)
        })
        .collect();
        let mut all = vec![column.clone(), rows.clone()];
        all.extend(charts);
        wb.sheets[0].set_charts(all.clone());
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let pkg = Package::read(&out).unwrap();
        let names: Vec<&str> = pkg.names().collect();
        assert_eq!(
            names
                .iter()
                .filter(|n| n.starts_with("xl/charts/chart"))
                .count(),
            6,
            "{names:?}"
        );
        assert!(names.iter().any(|n| n.starts_with("xl/drawings/drawing")));
        let sheet =
            String::from_utf8(pkg.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        assert!(sheet.contains("<drawing r:id="), "{sheet}");
        let first = String::from_utf8(
            pkg.get(
                names
                    .iter()
                    .find(|n| n.starts_with("xl/charts/chart"))
                    .unwrap(),
            )
            .unwrap()
            .to_vec(),
        )
        .unwrap();
        assert!(
            first.contains("<c:barChart>") && first.contains("<c:f>'Sheet1'!$B$42:$B$44</c:f>")
                || first.contains("$B$42:$B$44"),
            "{first}"
        );
        assert!(
            first.contains("Sales &amp; \"costs\"")
                || first.contains("Sales &amp; &quot;costs&quot;"),
            "{first}"
        );
        let back = read(&out).unwrap();
        let got = back.engine.workbook().sheets[0].charts();
        assert_eq!(got.len(), 6);
        let (a, b) = (&got[0], &got[1]);
        assert_eq!(
            (
                a.kind,
                a.title.as_str(),
                a.data,
                a.rows_as_series,
                a.legend,
                a.at,
                a.width,
                a.height
            ),
            (
                ChartKind::Column,
                "Sales & \"costs\"",
                data,
                false,
                true,
                column.at,
                480,
                300
            )
        );
        assert_eq!(
            (
                b.kind,
                b.data,
                b.rows_as_series,
                b.legend,
                b.at,
                b.width,
                b.height
            ),
            (
                ChartKind::Line,
                data,
                true,
                false,
                CellRef::new(45, 0),
                300,
                180
            )
        );
        assert_eq!(
            got[2..].iter().map(|c| c.kind).collect::<Vec<_>>(),
            [
                ChartKind::Bar,
                ChartKind::Area,
                ChartKind::Pie,
                ChartKind::Scatter
            ]
        );
        // Writing again replaces the parts and does not pile them up.
        let again = Package::read(&back.write().unwrap()).unwrap();
        assert_eq!(
            again
                .names()
                .filter(|n| n.starts_with("xl/charts/chart"))
                .count(),
            6
        );
        assert_eq!(
            again
                .names()
                .filter(|n| n.starts_with("xl/drawings/drawing") && n.ends_with(".xml"))
                .count(),
            1
        );
        // Taking them away takes the parts away.
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_charts(Vec::new());
        y.engine = sheets_calc::Engine::new(wb);
        let none = Package::read(&y.write().unwrap()).unwrap();
        assert!(
            none.names()
                .all(|n| !n.starts_with("xl/charts/") && !n.starts_with("xl/drawings/")),
            "{:?}",
            none.names().collect::<Vec<_>>()
        );
        let sheet =
            String::from_utf8(none.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
        assert!(!sheet.contains("<drawing"));
    }
}

#[quench_test::test]
fn sparklines_are_written_in_the_extension_list_and_read_back_beside_other_extensions() {
    use sheets_model::{Range, SparkKind, Sparkline};
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        let list = vec![
            Sparkline {
                cell: at("H1"),
                data: Range::parse("A1:E1").unwrap(),
                kind: SparkKind::Line,
                color: 0x0037_6092,
            },
            Sparkline {
                cell: at("H2"),
                data: Range::parse("A2:E2").unwrap(),
                kind: SparkKind::Line,
                color: 0x0037_6092,
            },
            Sparkline {
                cell: at("H3"),
                data: Range::parse("A3:E3").unwrap(),
                kind: SparkKind::Column,
                color: 0x00FF_0000,
            },
            Sparkline {
                cell: at("H4"),
                data: Range::parse("A4:E4").unwrap(),
                kind: SparkKind::WinLoss,
                color: 0x0000_8000,
            },
        ];
        wb.sheets[0].set_sparklines(list.clone());
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let sheet = String::from_utf8(
            Package::read(&out)
                .unwrap()
                .get("xl/worksheets/sheet1.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(sheet.contains("<x14:sparklineGroup displayEmptyCellsAs=\"gap\"><x14:colorSeries rgb=\"FF376092\"/>"), "{sheet}");
        assert!(sheet.contains("type=\"column\"") && sheet.contains("type=\"stacked\""));
        assert_eq!(
            sheet.matches("<x14:sparklineGroup ").count(),
            3,
            "two lines share a group"
        );
        assert!(
            sheet.contains("<xm:f>'Data'!$A$1:$E$1</xm:f><xm:sqref>H1</xm:sqref>"),
            "{sheet}"
        );
        let back = read(&out).unwrap();
        let got: Vec<Sparkline> = back.engine.workbook().sheets[0].sparklines().to_vec();
        assert_eq!(got.len(), 4);
        for sp in &list {
            assert!(got.contains(sp), "{sp:?} in {got:?}");
        }
        assert_eq!(
            read(&back.write().unwrap())
                .unwrap()
                .engine
                .workbook()
                .sheets[0]
                .sparklines()
                .len(),
            4,
            "writing is stable"
        );
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_sparklines(Vec::new());
        y.engine = sheets_calc::Engine::new(wb);
        let none = String::from_utf8(
            Package::read(&y.write().unwrap())
                .unwrap()
                .get("xl/worksheets/sheet1.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!none.contains("sparkline"));
    }
}

#[quench_test::test]
fn sheet_protection_and_unlocked_cells_round_trip_and_the_password_hash_matches_other_programs() {
    use sheets_model::Protection;
    // Hashes other programs give for the same passwords.
    for (password, hash) in [("secret", 0xDAA7), ("", 0xCE4B), ("a", 0xCE88)] {
        assert_eq!(Protection::hash(password), hash, "{password:?}");
    }
    assert!(
        Protection {
            password_hash: Some(0xDAA7),
            opaque: false
        }
        .accepts("secret")
    );
    assert!(
        !Protection {
            password_hash: Some(0xDAA7),
            opaque: false
        }
        .accepts("Secret")
    );
    assert!(
        Protection {
            password_hash: None,
            opaque: false
        }
        .accepts("anything")
    );
    assert!(
        !Protection {
            password_hash: None,
            opaque: true
        }
        .accepts(""),
        "a hash we cannot check never lets anyone in"
    );
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        let mut wb = x.engine.workbook().clone();
        let open = wb.style_id(&Style {
            locked: false,
            bold: true,
            ..Style::default()
        });
        wb.sheets[0].set_style(at("B4"), open);
        wb.sheets[0].set_protection(Some(Protection {
            password_hash: Some(Protection::hash("secret")),
            opaque: false,
        }));
        x.engine = sheets_calc::Engine::new(wb);
        let out = x.write().unwrap();
        let sheet = String::from_utf8(
            Package::read(&out)
                .unwrap()
                .get("xl/worksheets/sheet1.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            sheet.contains("<sheetProtection sheet=\"1\"") && sheet.contains("password=\"DAA7\""),
            "{sheet}"
        );
        let styles = String::from_utf8(
            Package::read(&out)
                .unwrap()
                .get("xl/styles.xml")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            styles.contains("<protection locked=\"0\"") && styles.contains("applyProtection=\"1\""),
            "{styles}"
        );
        let back = read(&out).unwrap();
        let p = back.engine.workbook().sheets[0].protection().unwrap();
        assert_eq!(
            p,
            Protection {
                password_hash: Some(0xDAA7),
                opaque: false
            }
        );
        let styles = &back.engine.workbook().styles;
        assert!(
            !styles[back.engine.style(0, at("B4")) as usize].locked
                && styles[back.engine.style(0, at("B2")) as usize].locked,
            "only the chosen cell is unlocked"
        );
        assert!(
            styles[back.engine.style(0, at("B4")) as usize].bold,
            "and it kept its other format"
        );
        let mut y = read(&out).unwrap();
        let mut wb = y.engine.workbook().clone();
        wb.sheets[0].set_protection(None);
        y.engine = sheets_calc::Engine::new(wb);
        assert!(
            read(&y.write().unwrap()).unwrap().engine.workbook().sheets[0]
                .protection()
                .is_none()
        );
    }
    // A protection made with a newer hash is kept and cannot be lifted here.
    let guarded = crafted("", "<row r=\"1\"><c r=\"A1\"><v>1</v></c></row>", "");
    let x = read(&guarded).unwrap();
    assert!(x.engine.workbook().sheets[0].protection().is_none());
}

#[quench_test::test]
fn tables_with_their_filters_round_trip() {
    use sheets_model::{AutoFilter, Table, Workbook};
    let mut wb = Workbook::new();
    wb.sheets.push(sheets_model::Sheet::new("Second"));
    let s = &mut wb.sheets[0];
    for (a, v) in [("A1", "Name"), ("B1", "Qty"), ("A2", "bolt"), ("A3", "nut")] {
        s.set(at(a), Content::Value(Value::Text(v.into())));
    }
    s.set(at("B2"), Content::Value(Value::Number(4.0)));
    s.set(at("B3"), Content::Value(Value::Number(9.0)));
    let mut allowed = std::collections::BTreeMap::new();
    allowed.insert(0, ["bolt".to_string()].into_iter().collect());
    s.set_tables(vec![Table {
        name: "Parts".into(),
        filter: AutoFilter {
            range: sheets_model::Range::parse("A1:B3").unwrap(),
            allowed,
        },
        banded: true,
        style: "TableStyleMedium7".into(),
    }]);
    wb.sheets[1].set(at("A1"), Content::Value(Value::Text("h".into())));
    wb.sheets[1].set(at("A2"), Content::Value(Value::Text("v".into())));
    wb.sheets[1].set_tables(vec![Table {
        name: "Mine".into(),
        filter: AutoFilter {
            range: sheets_model::Range::parse("A1:A2").unwrap(),
            allowed: std::collections::BTreeMap::new(),
        },
        banded: true,
        style: sheets_model::custom_table_style(0x00FF_0000, 0, 0x0000_FF00),
    }]);
    let x = Xlsx::new(wb);
    let out = x.write().unwrap();
    let package = Package::read(&out).unwrap();
    let table = String::from_utf8(package.get("xl/tables/table1.xml").unwrap().to_vec()).unwrap();
    assert!(
        table.contains("displayName=\"Parts\"") && table.contains("ref=\"A1:B3\""),
        "{table}"
    );
    assert!(
        table.contains("<tableColumn id=\"1\" name=\"Name\"")
            && table.contains("showRowStripes=\"1\""),
        "{table}"
    );
    let sheet =
        String::from_utf8(package.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
    assert!(sheet.contains("<tableParts count=\"1\">"), "{sheet}");
    let back = read(&out).unwrap();
    let t = &back.engine.workbook().sheets[0].tables()[0];
    assert_eq!(
        (t.name.as_str(), t.banded, t.range().to_string().as_str()),
        ("Parts", true, "A1:B3")
    );
    assert!(t.filter.allowed[&0].contains("bolt"));
    assert_eq!(t.style, "TableStyleMedium7");
    // A look of our own colours is defined in the styles and comes back as the same colours.
    let styles = String::from_utf8(package.get("xl/styles.xml").unwrap().to_vec()).unwrap();
    assert!(
        styles.contains("<tableStyle name=\"Custom_FF0000_000000_00FF00\"")
            && styles.contains("firstRowStripe"),
        "{styles}"
    );
    assert_eq!(
        back.engine.workbook().sheets[1].tables()[0].style,
        "Custom_FF0000_000000_00FF00"
    );
    // Written again without a sheet's tables, that sheet's parts go.
    let mut wb = back.engine.workbook().clone();
    wb.sheets[0].set_tables(Vec::new());
    wb.sheets[1].set_tables(Vec::new());
    let again = Xlsx::new(wb).write().unwrap();
    let parts = Package::read(&again).unwrap();
    assert!(
        parts.get("xl/tables/table1.xml").is_none() && parts.get("xl/tables/table2.xml").is_none()
    );
}

#[quench_test::test]
fn a_new_workbook_is_a_valid_file() {
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Sales".into();
    let pct = wb.style_id(&Style {
        number_format: "0.0%".into(),
        ..Default::default()
    });
    let date = wb.style_id(&Style {
        number_format: "yyyy-mm-dd".into(),
        ..Default::default()
    });
    wb.sheets[0].set(at("A1"), Content::Value(t("x & <y>")));
    wb.sheets[0].set(at("B1"), Content::Value(n(0.125)));
    wb.sheets[0].set_style(at("B1"), pct);
    wb.sheets[0].set(at("C1"), Content::Value(n(45351.0)));
    wb.sheets[0].set_style(at("C1"), date);
    wb.sheets[0].set(at("D1"), Content::Formula("B1*2&\"!\"".into()));
    wb.sheets[0].set(at("A2"), Content::Value(Value::Bool(true)));
    wb.sheets[0].set(at("B2"), Content::Value(Value::Error(CellError::NA)));
    wb.sheets[0].set(at("C2"), Content::Formula("A2".into()));
    wb.sheets[0].set(at("A3"), Content::Formula("C3+1".into()));
    wb.sheets[0].set(at("C3"), Content::Formula("A3+1".into())); // a cycle is written as 0
    wb.sheets[0].set_style(at("E5"), pct); // a formatted empty cell
    let x = Xlsx::new(wb);
    let bytes = x.write().unwrap();
    let pkg = Package::read(&bytes).unwrap();
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "xl/styles.xml",
        "xl/sharedStrings.xml",
    ] {
        assert!(pkg.get(part).is_some(), "{part}");
    }
    let ct = ContentTypes::parse(pkg.get("[Content_Types].xml").unwrap()).unwrap();
    assert!(
        ct.of("xl/worksheets/sheet1.xml")
            .unwrap()
            .contains("worksheet+xml")
    );
    let back = read(&bytes).unwrap();
    assert_eq!(val(&back, "Sales", "A1"), t("x & <y>"));
    assert_eq!(val(&back, "Sales", "B1"), n(0.125));
    assert_eq!(back.engine.workbook().number_format(0, at("B1")), "0.0%");
    assert_eq!(val(&back, "Sales", "D1"), t("0.25!"));
    assert_eq!(val(&back, "Sales", "A2"), Value::Bool(true));
    assert_eq!(val(&back, "Sales", "B2"), Value::Error(CellError::NA));
    assert_eq!(val(&back, "Sales", "C2"), Value::Bool(true));
    assert_eq!(back.engine.workbook().number_format(0, at("E5")), "0.0%");
    assert_eq!(snapshot(&x)[0].1.len(), snapshot(&back)[0].1.len());
}

fn crafted(workbook_pr: &str, sheet_xml: &str, styles: &str) -> Vec<u8> {
    let mut p = Package::new();
    let mut ct = ContentTypes::default();
    ct.set_default(
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    );
    ct.set_default("xml", "application/xml");
    p.set("[Content_Types].xml", ct.to_bytes());
    let rel = |id: &str, kind: &str, target: &str| Rel {
        id: id.into(),
        kind: kind.into(),
        target: target.into(),
        external: false,
    };
    p.set(
        "_rels/.rels",
        rels::to_bytes(&[rel("rId1", rels::OFFICE_DOCUMENT, "xl/workbook.xml")]),
    );
    p.set(
        "xl/_rels/workbook.xml.rels",
        rels::to_bytes(&[
            rel("rId1", rels::WORKSHEET, "worksheets/sheet1.xml"),
            rel("rId2", rels::STYLES, "styles.xml"),
        ]),
    );
    p.set(
        "xl/workbook.xml",
        format!("<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:rel=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{workbook_pr}<sheets><sheet name=\"S\" sheetId=\"1\" rel:id=\"rId1\"/></sheets></workbook>").into_bytes(),
    );
    p.set("xl/worksheets/sheet1.xml", format!("<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{sheet_xml}</sheetData></worksheet>").into_bytes());
    p.set("xl/styles.xml", format!("<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{styles}</styleSheet>").into_bytes());
    p.write().unwrap()
}

#[quench_test::test]
fn shared_formulas_are_expanded_for_every_cell() {
    let sheet = r#"<row r="1"><c r="A1"><v>1</v></c><c r="B1"><f t="shared" ref="B1:C3" si="0">A1*2+$A$1</f><v>3</v></c><c r="C1"><f t="shared" si="0"/></c></row>
        <row r="2"><c r="A2"><v>5</v></c><c r="B2"><f t="shared" si="0"/></c></row>"#;
    let x = read(&crafted("", sheet, "")).unwrap();
    let f = |a: &str| match x.engine.workbook().sheets[0].get(at(a)) {
        Some(Content::Formula(f)) => f.clone(),
        other => panic!("{a}: {other:?}"),
    };
    assert_eq!(f("B1"), "A1*2+$A$1");
    assert_eq!(f("C1"), "B1*2+$A$1");
    assert_eq!(f("B2"), "A2*2+$A$1");
    assert_eq!(val(&x, "S", "B2"), n(11.0));
}

#[quench_test::test]
fn cell_types_and_missing_addresses() {
    let sheet = r#"<row><c t="inlineStr"><is><t xml:space="preserve"> in </t></is></c><c t="b"><v>1</v></c><c t="e"><v>#N/A</v></c><c t="str"><v>txt</v></c><c t="d"><v>2024-02-29T12:00:00</v></c></row>
        <row r="5"><c r="C5"><v> 2.5 </v></c><c><v>7</v></c></row>"#;
    let x = read(&crafted("", sheet, "")).unwrap();
    assert_eq!(val(&x, "S", "A1"), t(" in "));
    assert_eq!(val(&x, "S", "B1"), Value::Bool(true));
    assert_eq!(val(&x, "S", "C1"), Value::Error(CellError::NA));
    assert_eq!(val(&x, "S", "D1"), t("txt"));
    assert_eq!(val(&x, "S", "E1"), n(45351.5));
    assert_eq!(val(&x, "S", "C5"), n(2.5));
    assert_eq!(val(&x, "S", "D5"), n(7.0)); // no address: the next column
}

#[quench_test::test]
fn the_1904_date_system_is_converted() {
    let styles = r#"<cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14"/></cellXfs>"#;
    let sheet = r#"<row r="1"><c r="A1" s="1"><v>0</v></c><c r="B1"><v>5</v></c></row>"#;
    let x = read(&crafted("<workbookPr date1904=\"1\"/>", sheet, styles)).unwrap();
    assert_eq!(val(&x, "S", "A1"), n(1462.0)); // 1904-01-01 in the 1900 system
    assert_eq!(val(&x, "S", "B1"), n(5.0)); // not a date: untouched
    let out = Package::read(&x.write().unwrap()).unwrap();
    assert!(
        std::str::from_utf8(out.get("xl/workbook.xml").unwrap())
            .unwrap()
            .contains("date1904=\"0\"")
    );
    assert_eq!(
        val(&read(&x.write().unwrap()).unwrap(), "S", "A1"),
        n(1462.0)
    );
}

#[quench_test::test]
fn removing_a_sheet_removes_its_part() {
    let mut x = read(OPENPYXL).unwrap();
    assert!(x.remove_sheet(1));
    assert!(!x.remove_sheet(5));
    let out = Package::read(&x.write().unwrap()).unwrap();
    assert!(out.get("xl/worksheets/sheet2.xml").is_none());
    let back = read(&x.write().unwrap()).unwrap();
    assert_eq!(back.engine.workbook().sheets.len(), 1);
    assert_eq!(val(&back, "Data", "F6"), Value::Error(CellError::Ref)); // it read the sheet that is gone
}

#[quench_test::test]
fn bad_files_are_errors() {
    assert!(read(b"nope").is_err());
    assert!(read(&Package::new().write().unwrap()).is_err());
    let mut p = Package::new();
    p.set(
        "_rels/.rels",
        rels::to_bytes(&[Rel {
            id: "rId1".into(),
            kind: rels::OFFICE_DOCUMENT.into(),
            target: "xl/workbook.xml".into(),
            external: false,
        }]),
    );
    assert!(read(&p.write().unwrap()).is_err()); // no workbook part
    let broken = crafted("", "<row><c r=\"A1\"><v>1</v></c></row", "");
    assert!(read(&broken).is_err());
    let no_sheets = {
        let mut x = Package::read(&crafted("", "", "")).unwrap();
        x.set("xl/workbook.xml", b"<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheets/></workbook>".to_vec());
        x.write().unwrap()
    };
    assert!(matches!(read(&no_sheets), Err(Error::Invalid(_))));
    assert!(
        Error::Invalid("x".into())
            .to_string()
            .contains("not a workbook")
    );
    let mut wb = Workbook::new();
    wb.sheets[0].name = "bad/name".into();
    assert!(Xlsx::new(wb).write().is_err());
    let mut wb = Workbook::new();
    wb.sheets[0].name = "x".repeat(32);
    assert!(Xlsx::new(wb).write().is_err());
}

fn with_cols(cols: &str, rows: &str) -> Vec<u8> {
    let mut p = Package::read(&crafted("", rows, "")).unwrap();
    let sheet = String::from_utf8(p.get("xl/worksheets/sheet1.xml").unwrap().to_vec()).unwrap();
    p.set(
        "xl/worksheets/sheet1.xml",
        sheet
            .replace("<sheetData>", &format!("<cols>{cols}</cols><sheetData>"))
            .into_bytes(),
    );
    p.write().unwrap()
}

fn sheet_xml(bytes: &[u8]) -> String {
    String::from_utf8(
        Package::read(bytes)
            .unwrap()
            .get("xl/worksheets/sheet1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[quench_test::test]
fn column_widths_and_row_heights_are_read_in_pixels() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let x = read(bytes).unwrap();
        let sheet = &x.engine.workbook().sheets[0];
        let a = sheet.col_width(0).expect("column A has a width");
        assert!((a - 140.0).abs() < 2.0, "20 characters are 140 px, got {a}");
        assert_eq!(sheet.col_width(1), None);
        let h = sheet.row_height(6).expect("row 7 has a height");
        assert!((h - 40.0).abs() < 0.5, "30 points are 40 px, got {h}");
    }
}

#[quench_test::test]
fn changed_sizes_are_written_and_the_rest_of_the_columns_are_left_alone() {
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let mut x = read(bytes).unwrap();
        x.engine.set_col_width(0, 2, Some(210.0));
        x.engine.set_row_height(0, 2, Some(60.0));
        let out = x.write().unwrap();
        let back = read(&out).unwrap();
        let sheet = &back.engine.workbook().sheets[0];
        assert!((sheet.col_width(2).unwrap() - 210.0).abs() < 0.01);
        assert!(
            (sheet.col_width(0).unwrap() - 140.0).abs() < 2.0,
            "the width the file had stays"
        );
        assert!((sheet.row_height(2).unwrap() - 60.0).abs() < 0.01);
        assert!((sheet.row_height(6).unwrap() - 40.0).abs() < 0.5);
        assert_eq!(
            back.engine.workbook().sheets[1].col_widths().count(),
            0,
            "other sheets are untouched"
        );
    }
}

#[quench_test::test]
fn resizing_a_column_inside_a_wide_range_splits_it_and_keeps_its_style() {
    let bytes = with_cols(
        "<col min=\"1\" max=\"16384\" width=\"9.14\" style=\"2\"/><col min=\"3\" max=\"3\" width=\"0\" hidden=\"1\"/>",
        "<row r=\"1\"><c r=\"A1\"><v>1</v></c></row>",
    );
    // The sheet-wide width is a default; the hidden column has no width to show.
    let mut x = read(&bytes).unwrap();
    assert_eq!(x.engine.workbook().sheets[0].col_widths().count(), 0);
    x.engine.set_col_width(0, 5, Some(140.0)); // column F
    let xml = sheet_xml(&x.write().unwrap());
    assert!(xml.contains("min=\"1\" max=\"5\""), "{xml}");
    assert!(
        xml.contains("min=\"6\" max=\"6\" width=\"20\" style=\"2\" customWidth=\"1\"")
            || xml.contains("min=\"6\" max=\"6\" width=\"20\""),
        "{xml}"
    );
    assert!(xml.contains("min=\"7\" max=\"16384\""), "{xml}");
    assert_eq!(
        xml.matches("style=\"2\"").count(),
        3,
        "every piece keeps the column style"
    );
    assert!(xml.contains("hidden=\"1\""), "the hidden column is kept");
    let back = read(&x.write().unwrap()).unwrap();
    assert!((back.engine.workbook().sheets[0].col_width(5).unwrap() - 140.0).abs() < 0.01);
    // The five columns before it are a short range now, so they read as widths of their own (64 px, the file's 9.14).
    assert_eq!(back.engine.workbook().sheets[0].col_widths().count(), 6);
    assert!((back.engine.workbook().sheets[0].col_width(0).unwrap() - 64.0).abs() < 0.1);
}

#[quench_test::test]
fn a_width_set_on_a_file_without_columns_makes_the_element() {
    let mut x = read(&crafted(
        "",
        "<row r=\"1\"><c r=\"A1\"><v>1</v></c></row>",
        "",
    ))
    .unwrap();
    x.engine.set_col_width(0, 1, Some(70.0));
    x.engine.set_col_width(0, 2, Some(70.0));
    x.engine.set_col_width(0, 4, Some(35.0));
    let xml = sheet_xml(&x.write().unwrap());
    let cols_at = xml.find("<cols>").expect("a cols element");
    assert!(
        cols_at < xml.find("<sheetData>").unwrap(),
        "cols comes before the data"
    );
    assert_eq!(xml.matches("<col ").count(), 3);
    let back = read(&x.write().unwrap()).unwrap();
    assert!((back.engine.workbook().sheets[0].col_width(4).unwrap() - 35.0).abs() < 0.01);
    // Taking every width away removes the element again.
    let mut y = read(&x.write().unwrap()).unwrap();
    for c in [1, 2, 4] {
        y.engine.set_col_width(0, c, None);
    }
    // (the widths the file had are kept: they are what the file says, and clearing is "default")
    let xml = sheet_xml(&y.write().unwrap());
    assert!(
        xml.contains("<col "),
        "widths the file had stay until they are changed: {xml}"
    );
}

fn style_at(x: &Xlsx, sheet: usize, a1: &str) -> Style {
    let wb = x.engine.workbook();
    wb.styles[wb.sheets[sheet].style(at(a1)) as usize].clone()
}

fn styles_xml(bytes: &[u8]) -> office_fmt::xml::Value {
    office_fmt::xml::parse(
        Package::read(bytes).unwrap().get("xl/styles.xml").unwrap(),
        false,
    )
    .unwrap()
}

#[quench_test::test]
fn fonts_fills_and_alignment_are_read_from_real_files() {
    use sheets_model::Align;
    for bytes in [OPENPYXL, LIBREOFFICE] {
        let x = read(bytes).unwrap();
        let a1 = style_at(&x, 0, "A1");
        assert!(a1.bold && !a1.italic && !a1.underline && a1.color.is_none());
        let b1 = style_at(&x, 0, "B1");
        assert!(b1.bold && b1.italic && b1.underline, "{b1:?}");
        assert_eq!(b1.color, Some(0xFF_0000));
        assert_eq!(style_at(&x, 0, "C1").fill, Some(0xFF_FF00));
        let d1 = style_at(&x, 0, "D1");
        assert_eq!((d1.align, d1.wrap), (Align::Center, true));
        assert_eq!(style_at(&x, 0, "F1").align, Align::Right);
        let plain = style_at(&x, 0, "A2");
        assert!(!plain.bold && plain.fill.is_none() && plain.align == Align::General);
        // Every style says which format of the file it came from.
        let wb = x.engine.workbook();
        assert!(
            wb.styles
                .iter()
                .enumerate()
                .all(|(i, s)| s.base as usize == i)
        );
    }
}

#[quench_test::test]
fn a_changed_format_keeps_its_border_and_theme_colour() {
    let mut x = read(OPENPYXL).unwrap();
    let have = x.engine.workbook().styles.len();
    // E1 has a bottom border the suite does not describe; make it bold.
    let mut bold = style_at(&x, 0, "E1");
    assert_ne!(bold.base, 0, "it is a format of its own");
    bold.bold = true;
    let mut wb = x.engine.workbook().clone();
    let id = wb.style_id(&bold);
    assert_eq!(id as usize, have, "a new format, appended");
    wb.sheets[0].set_style(at("E1"), id);
    // A plain cell made italic keeps the theme colour of the default font.
    let mut italic = style_at(&x, 0, "A2");
    italic.italic = true;
    let italic_id = wb.style_id(&italic);
    wb.sheets[0].set_style(at("A2"), italic_id);
    x.engine = sheets_calc::Engine::new(wb);
    let out = x.write().unwrap();
    let before = styles_xml(OPENPYXL);
    let after = styles_xml(&out);
    let xfs = |root: &office_fmt::xml::Value| {
        office_fmt::xml::elements(office_fmt::xml::child(root, "cellXfs").unwrap(), "xf")
            .cloned()
            .collect::<Vec<_>>()
    };
    let (old, new) = (xfs(&before), xfs(&after));
    assert_eq!(new.len(), old.len() + 2);
    let border =
        |xf: &office_fmt::xml::Value| office_fmt::xml::attr(xf, "borderId").map(str::to_string);
    let base = x.engine.workbook().styles[have].base as usize;
    assert_eq!(
        border(&new[have]),
        border(&old[base]),
        "the border of the format it came from"
    );
    assert_ne!(border(&new[have]).as_deref(), Some("0"));
    let font_xml = office_fmt::xml::write(office_fmt::xml::child(&after, "fonts").unwrap());
    let fonts = String::from_utf8(font_xml).unwrap();
    assert!(
        fonts.contains("<color theme=\"1\"/>") && fonts.contains("<i/>"),
        "an italic font that keeps the theme colour: {fonts}"
    );
    // And it reads back as asked.
    let back = read(&out).unwrap();
    assert!(style_at(&back, 0, "E1").bold);
    assert!(style_at(&back, 0, "A2").italic);
    assert!(!style_at(&back, 0, "A2").bold);
    assert!(
        style_at(&back, 0, "A1").bold,
        "the file's own formats are as they were"
    );
}

#[quench_test::test]
fn new_formats_in_a_new_workbook_write_and_read_back() {
    use sheets_model::Align;
    let mut wb = Workbook::new();
    let fancy = Style {
        number_format: "0.0%".into(),
        bold: true,
        italic: true,
        underline: true,
        color: Some(0x12_34AB),
        fill: Some(0xCC_EEFF),
        align: Align::Center,
        wrap: true,
        border: Border::default(),
        locked: true,
        font_size: Some(26),
        font_name: Some("Georgia".into()),
        base: 0,
    };
    let id = wb.style_id(&fancy);
    let plain_fill = wb.style_id(&Style {
        fill: Some(0xCC_EEFF),
        ..Style::default()
    });
    wb.sheets[0].set(at("A1"), Content::Value(n(0.5)));
    wb.sheets[0].set_style(at("A1"), id);
    wb.sheets[0].set_style(at("B1"), plain_fill);
    let bytes = Xlsx::new(wb).write().unwrap();
    let back = read(&bytes).unwrap();
    assert_eq!(
        style_at(&back, 0, "A1"),
        Style {
            base: style_at(&back, 0, "A1").base,
            ..fancy
        }
    );
    let b1 = style_at(&back, 0, "B1");
    assert_eq!(
        (b1.fill, b1.bold, b1.align),
        (Some(0xCC_EEFF), false, Align::General)
    );
    // The two formats share one fill, and the file has exactly the fills it needs.
    let fills = office_fmt::xml::child(&styles_xml(&bytes), "fills")
        .unwrap()
        .clone();
    assert_eq!(
        office_fmt::xml::elements(&fills, "fill").count(),
        3,
        "none, gray125 and the one colour"
    );
    assert_eq!(office_fmt::xml::attr(&fills, "count"), Some("3"));
}

#[quench_test::test]
fn a_change_to_the_number_format_alone_leaves_font_and_fill_untouched() {
    let mut x = read(OPENPYXL).unwrap();
    let mut s = style_at(&x, 0, "C1"); // yellow fill
    s.number_format = "0.000".into();
    let mut wb = x.engine.workbook().clone();
    let id = wb.style_id(&s);
    wb.sheets[0].set_style(at("C1"), id);
    x.engine = sheets_calc::Engine::new(wb);
    let before = styles_xml(OPENPYXL);
    let after = styles_xml(&x.write().unwrap());
    let count = |r: &office_fmt::xml::Value, g: &str, t: &str| {
        office_fmt::xml::elements(office_fmt::xml::child(r, g).unwrap(), t).count()
    };
    assert_eq!(
        count(&after, "fonts", "font"),
        count(&before, "fonts", "font"),
        "no new font"
    );
    assert_eq!(
        count(&after, "fills", "fill"),
        count(&before, "fills", "fill"),
        "no new fill"
    );
    assert_eq!(
        count(&after, "cellXfs", "xf"),
        count(&before, "cellXfs", "xf") + 1
    );
}

#[quench_test::test]
fn the_cell_scanner_copes_with_everything_xml_allows_there() {
    let rows = "<row r='1' spans='1:3' ht='20' customHeight='1'><!-- a comment -->\
        <c r='A1' t='inlineStr'><is><t>a &amp; b &lt;c&gt; &#65;&#x42;</t></is></c>\
        <c r='B1' t='inlineStr'><is><r><rPr><b/></rPr><t>rich </t></r><r><t>text</t></r><rPh sb='0' eb='1'><t>PHONETIC</t></rPh></is></c>\
        <c r='C1' t='str'><f>A1&amp;\"&lt;\"</f><v><![CDATA[x < y & z]]></v></c>\
        <c r='D1'><v>1</v><extLst><ext uri='x'><y a='1'><z/></y></ext></extLst></c>\
        <c r='E1' s='0'/><c r='F1' t='b'><v>1</v></c></row>\
        <?pi ignored?><row r=\"3\"><c r=\"A3\"><v> 7 </v></c><c><v>8</v></c></row><row r=\"5\" hidden=\"1\"/>";
    let x = read(&crafted("", rows, "")).unwrap();
    assert_eq!(val(&x, "S", "A1"), t("a & b <c> AB"));
    assert_eq!(
        val(&x, "S", "B1"),
        t("rich text"),
        "runs joined, phonetic hints left out"
    );
    assert_eq!(
        val(&x, "S", "C1"),
        t("a & b <c> AB<"),
        "a formula with references in it is read as the formula it is"
    );
    assert_eq!(
        val(&x, "S", "D1"),
        n(1.0),
        "an unknown child element is skipped, nested or not"
    );
    assert_eq!(val(&x, "S", "E1"), Value::Empty);
    assert_eq!(val(&x, "S", "F1"), Value::Bool(true));
    assert_eq!(val(&x, "S", "A3"), n(7.0));
    assert_eq!(
        val(&x, "S", "B3"),
        n(8.0),
        "a cell without an address follows the one before"
    );
    let h = x.engine.workbook().sheets[0].row_height(0).unwrap();
    assert!((h - 20.0 * 4.0 / 3.0).abs() < 0.01);
    // A hidden row with no cells is kept when written.
    let out = sheet_xml(&x.write().unwrap());
    assert!(out.contains("<row r=\"5\" hidden=\"1\"/>"), "{out}");
    assert!(out.contains("ht=\"20\"") && out.contains("customHeight=\"1\""));
}

#[quench_test::test]
fn damaged_sheet_text_is_an_error_not_a_panic_or_a_hang() {
    let cases = [
        "<row r=\"1\"><c r=\"A1\"><v>1</v>",
        "<row r=\"1\"><c r=\"A1\"><v>1</c></row>",
        "<row r=\"1\"><c r=\"A1\"",
        "<row r=\"1\"><!-- never closed",
        "<row r=\"1\"><c r=\"A1\"><v><![CDATA[x</v></c></row>",
        "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>x",
        "<<<<",
        "<row><c><f>",
    ];
    for rows in cases {
        let bytes = crafted("", rows, "");
        assert!(read(&bytes).is_err(), "{rows}");
    }
    // Odd but well-formed input reads.
    for rows in [
        "",
        "<row/>",
        "<row r=\"1\"/>",
        "<row r=\"1\"><c/></row>",
        "<row r=\"0\"><c r=\"A1\"><v>1</v></c></row>",
    ] {
        assert!(read(&crafted("", rows, "")).is_ok(), "{rows}");
    }
}

#[quench_test::test]
fn results_the_file_gave_stand_in_for_functions_we_do_not_have() {
    let rows = r#"<row r="1"><c r="A1"><f>_xlfn.NOSUCHFUNCTION(1,2)</f><v>42</v></c>
        <c r="B1" t="str"><f>FROBNICATE("x")&amp;"!"</f><v>hello</v></c>
        <c r="C1"><f>1+1</f><v>99</v></c>
        <c r="D1" t="e"><f>FILTER(A1:A3,B1:B3)</f><v>#N/A</v></c>
        <c r="E1"><f>A1*2</f><v>84</v></c></row>"#;
    let x = read(&crafted("", rows, "")).unwrap();
    assert_eq!(val(&x, "S", "A1"), n(42.0));
    assert_eq!(val(&x, "S", "B1"), t("hello"));
    assert_eq!(val(&x, "S", "C1"), n(2.0), "we calculate what we can");
    assert_eq!(val(&x, "S", "D1"), Value::Error(CellError::NA));
    assert_eq!(val(&x, "S", "E1"), n(84.0));
    // Written back, the formula is as it was and the value is the one we showed.
    let out = sheet_xml(&x.write().unwrap());
    assert!(out.contains("NOSUCHFUNCTION(1,2)</f><v>42</v>"), "{out}");
    let again = read(&x.write().unwrap()).unwrap();
    assert_eq!(val(&again, "S", "A1"), n(42.0));
    assert_eq!(val(&again, "S", "B1"), t("hello"));
}

/// With `XLSX_ORACLE_OUT=<dir>` set, writes what this crate produces so another program can open it.
#[quench_test::test]
fn files_for_an_external_reader() {
    let Ok(dir) = std::env::var("XLSX_ORACLE_OUT") else {
        return;
    };
    for (name, bytes) in [("openpyxl", OPENPYXL), ("libreoffice", LIBREOFFICE)] {
        let mut x = read(bytes).unwrap();
        x.engine.set(0, at("B2"), Content::Value(n(10.0)));
        std::fs::write(format!("{dir}/{name}_out.xlsx"), x.write().unwrap()).unwrap();
    }
    // Formats made here, for a reader to check that they mean what they say.
    let mut wb = Workbook::new();
    let styled = wb.style_id(&Style {
        bold: true,
        italic: true,
        underline: true,
        color: Some(0xFF_0000),
        fill: Some(0xFF_FF00),
        align: sheets_model::Align::Center,
        ..Style::default()
    });
    let pct = wb.style_id(&Style {
        number_format: "0.0%".into(),
        fill: Some(0x00_FF00),
        ..Style::default()
    });
    wb.sheets[0].set(at("A1"), Content::Value(t("styled")));
    wb.sheets[0].set_style(at("A1"), styled);
    wb.sheets[0].set(at("B1"), Content::Value(n(0.256)));
    wb.sheets[0].set_style(at("B1"), pct);
    std::fs::write(
        format!("{dir}/styled_out.xlsx"),
        Xlsx::new(wb).write().unwrap(),
    )
    .unwrap();
}

quench_test::main!();

#[quench_test::test]
fn damaged_parts_inside_good_workbooks_are_errors_or_workbooks_but_never_a_panic() {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for source in [OPENPYXL, LIBREOFFICE] {
        let package = Package::read(source).unwrap();
        let names: Vec<String> = package
            .names()
            .filter(|n| n.ends_with(".xml") && !n.starts_with('['))
            .map(str::to_string)
            .collect();
        for round in 0..300 {
            let name = &names[(next() as usize) % names.len()];
            let mut bytes = package.get(name).unwrap().to_vec();
            for _ in 0..=(round % 4) {
                let at = (next() as usize) % bytes.len();
                match next() % 4 {
                    0 => bytes[at] = b"<>\"&/= 0\n"[(next() % 9) as usize],
                    1 => bytes.truncate(at.max(1)),
                    2 => {
                        let end = (at + 1 + (next() as usize) % 30).min(bytes.len());
                        bytes.drain(at..end);
                    }
                    _ => bytes.insert(at, next() as u8),
                }
                if bytes.is_empty() {
                    break;
                }
            }
            let mut broken = package.clone();
            broken.set(name, bytes);
            let packed = broken.write().unwrap();
            std::panic::catch_unwind(|| {
                if let Ok(x) = read(&packed) {
                    let _ = x.write();
                }
            })
            .unwrap_or_else(|_| panic!("round {round} in {name} panicked"));
        }
    }
}
