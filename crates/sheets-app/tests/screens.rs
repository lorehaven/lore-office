#![allow(clippy::suboptimal_flops, clippy::missing_const_for_fn)]
#![allow(
    clippy::assert_is_empty,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::manual_assert,
    clippy::float_cmp,
    clippy::default_trait_access,
    clippy::needless_pass_by_value
)]
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
use quench_ui::event::{ScrollEvent, Size};
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;
use sheets_app::*;
use sheets_model::CellRef;
use std::rc::Rc;

struct Screen {
    r: Rc<TestRenderer>,
    state: SheetState,
    _scope: Scope,
}

fn open(rows: u32) -> Screen {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let ((state, _), scope) = create_root(|| {
        let state = SheetState::new(demo_workbook(rows));
        let mounted = mount(&r, &root, sheet_view(&state));
        (state, mounted)
    });
    Screen {
        r,
        state,
        _scope: scope,
    }
}

impl Screen {
    fn one(&self, attr: &str, value: &str) -> NodeId {
        let all = self.r.find_by_attr(attr, value);
        assert_eq!(all.len(), 1, "{attr}={value}: {} nodes", all.len());
        all[0]
    }

    fn cells(&self) -> Vec<NodeId> {
        self.r.find_by_attr("role", "gridcell")
    }

    fn cell(&self, row: u32, col: u32) -> Option<NodeId> {
        let (rr, cc) = ((row + 1).to_string(), (col + 1).to_string());
        self.cells().into_iter().find(|n| {
            self.r.attr(*n, "aria-rowindex").as_deref() == Some(&rr)
                && self.r.attr(*n, "aria-colindex").as_deref() == Some(&cc)
        })
    }

    fn text(&self, row: u32, col: u32) -> String {
        self.r.text_of(
            self.cell(row, col)
                .unwrap_or_else(|| panic!("no cell {row},{col}")),
        )
    }

    fn scroll(&self, top: f64, left: f64) {
        self.r.fire(
            self.one("role", "grid"),
            &Event::Scroll(ScrollEvent { top, left }),
        );
    }

    fn key(&self, key: &str) {
        self.r.fire(
            self.one("role", "grid"),
            &Event::KeyDown(KeyEvent::new(key)),
        );
    }

    fn bar(&self) -> NodeId {
        self.one("class", "formula")
    }

    fn selected(&self) -> CellRef {
        self.state.selected.get_untracked()
    }
}

/// The file as text with the (compressed) page drawings read out, so what is drawn can be searched.
fn inflated(bytes: &[u8]) -> String {
    let mut out = String::from_utf8_lossy(bytes).to_string();
    if let Ok(doc) = quench_pdf::Document::load(bytes) {
        for p in doc.pages() {
            out.push('\n');
            out.push_str(&String::from_utf8_lossy(&doc.page_contents(&p)));
        }
    }
    out
}

#[quench_test::test]
fn only_the_cells_in_view_are_built() {
    let s = open(2000);
    let n = s.cells().len();
    // The assumed 800x480 view: 20 rows and 9 columns plus two beyond each edge.
    assert!((200..=400).contains(&n), "{n} cells");
    assert!(
        n < 1000,
        "the sheet has {} cells",
        u64::from(ROWS) * u64::from(COLS)
    );
    assert_eq!(s.text(0, 0), "Id");
    assert_eq!(s.text(1, 1), "item 1");
    assert_eq!(s.text(2, 2), "3.00"); // =A3*1.5 under #,##0.00
    assert_eq!(s.text(2, 3), "even");
    assert_eq!(s.text(3, 4), "9.00"); // 1.5 + 3 + 4.5, the running total
    assert_eq!(s.text(1, 5), "item 1 (odd)");
    assert!(s.cell(0, 30).is_none(), "far columns are not built");
    assert!(s.cell(500, 0).is_none(), "far rows are not built");
    // Headers follow the same window.
    assert!(s.r.text().contains('Z') || s.r.text().contains('I'));
}

#[quench_test::test]
fn scrolling_moves_the_window_and_keeps_the_node_count() {
    let s = open(2000);
    let before = s.r.live_nodes();
    let first = s.cell(5, 1).unwrap();
    s.scroll(ROW_H * 6.0, 0.0);
    // One row up is gone, the cell that stays is the same element.
    assert_eq!(
        s.cell(5, 1),
        Some(first),
        "cells in both windows keep their elements"
    );
    s.scroll(ROW_H * 1000.0, COL_W * 20.0);
    assert!(s.cell(5, 1).is_none());
    assert_eq!(s.text(1000, 21), "");
    assert!(s.cell(1000, 21).is_some());
    // A window away from the edges has more cells than the one at the corner (it has margin on every side), but it is
    // the same size wherever it is, and nothing is left behind by scrolling.
    let middle = s.r.live_nodes();
    assert!(
        middle > before && middle < before * 2,
        "{before} nodes at the corner, {middle} in the middle"
    );
    for step in 1..=20 {
        s.scroll(
            7.0f64.mul_add(f64::from(step), 1000.0) * ROW_H,
            3.0f64.mul_add(f64::from(step), 20.0) * COL_W,
        );
    }
    let after = s.r.live_nodes();
    assert!(
        after.abs_diff(middle) < middle / 10,
        "{middle} nodes, then {after} after 20 scrolls"
    );
    // Back at the top the first cells are rebuilt with their values.
    s.scroll(0.0, 0.0);
    assert_eq!(s.text(1, 1), "item 1");
    // The end of the sheet.
    s.scroll(f64::from(ROWS) * ROW_H, f64::from(COLS) * COL_W);
    assert!(s.cell(ROWS - 1, COLS - 1).is_some());
    assert!(s.cells().len() < 1000);
}

#[quench_test::test]
fn a_resize_changes_how_many_cells_are_built() {
    let s = open(100);
    let before = s.cells().len();
    s.r.fire(
        s.one("role", "grid"),
        &Event::Resize(Size {
            width: 1600.0,
            height: 960.0,
        }),
    );
    assert!(
        s.cells().len() > before * 3,
        "{} cells after, {before} before",
        s.cells().len()
    );
    assert!(s.state.page_rows.get_untracked() >= 40);
}

#[quench_test::test]
fn clicking_selects_and_shows_what_is_in_the_cell() {
    let s = open(100);
    assert_eq!(
        s.r.attr(s.one("class", "name"), "value")
            .unwrap_or_default(),
        "A1"
    );
    s.r.fire(
        s.cell(2, 2).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    assert_eq!(s.selected(), CellRef::new(2, 2));
    assert_eq!(
        s.r.attr(s.one("class", "name"), "value")
            .unwrap_or_default(),
        "C3"
    );
    assert_eq!(s.r.attr(s.bar(), "value").as_deref(), Some("=A3*1.5"));
    s.r.fire(
        s.cell(2, 1).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    assert_eq!(s.r.attr(s.bar(), "value").as_deref(), Some("item 2"));
    // The cursor is where the selection is.
    let cursor = s.one("class", "cursor");
    assert_eq!(s.r.style_of(cursor, "left").as_deref(), Some("96px"));
    assert_eq!(s.r.style_of(cursor, "top").as_deref(), Some("48px"));
}

#[quench_test::test]
fn the_keyboard_moves_the_cursor_within_the_sheet() {
    let s = open(100);
    s.key("ArrowDown");
    s.key("ArrowRight");
    s.key("ArrowRight");
    assert_eq!(s.selected(), CellRef::new(1, 2));
    s.key("ArrowUp");
    s.key("ArrowUp");
    s.key("ArrowUp"); // stops at the edge
    assert_eq!(s.selected(), CellRef::new(0, 2));
    s.key("Tab");
    assert_eq!(s.selected(), CellRef::new(0, 3));
    s.key("Enter");
    assert_eq!(s.selected(), CellRef::new(1, 3));
    s.key("PageDown");
    assert_eq!(s.selected().row, 1 + s.state.page_rows.get_untracked());
    s.key("PageUp");
    assert_eq!(s.selected(), CellRef::new(1, 3));
    s.key("Home");
    assert_eq!(s.selected(), CellRef::new(1, 0));
    s.state.select(CellRef::new(5, COLS - 2));
    for _ in 0..3 {
        s.key("ArrowRight");
    }
    assert_eq!(
        s.selected(),
        CellRef::new(5, COLS - 1),
        "stops at the last column"
    );
    s.state.select(CellRef::new(ROWS - 2, 0));
    for _ in 0..3 {
        s.key("ArrowDown");
    }
    assert_eq!(s.selected().row, ROWS - 1, "stops at the last row");
}

#[quench_test::test]
fn typing_replaces_a_cell_and_enter_commits_and_recalculates() {
    let s = open(100);
    s.r.fire(
        s.cell(2, 0).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    ); // A3 = 2
    assert_eq!(s.text(2, 2), "3.00");
    s.key("1");
    assert!(s.state.editing.get_untracked());
    // The bar is emptied and takes focus; the browser types the character into it.
    assert_eq!(s.r.attr(s.bar(), "value").as_deref(), Some(""));
    s.r.fire(s.bar(), &Event::Input("1".into()));
    s.r.fire(s.bar(), &Event::Input("10".into()));
    s.r.fire(s.bar(), &Event::KeyDown(KeyEvent::new("Enter")));
    assert!(!s.state.editing.get_untracked());
    assert_eq!(s.selected(), CellRef::new(3, 0)); // moved down
    assert_eq!(s.text(2, 0), "10");
    assert_eq!(s.text(2, 2), "15.00"); // C3 = A3*1.5, recalculated
    assert_eq!(s.text(2, 3), "even");
    // The running total below follows too: it was 1.5 + 3 + 4.5 and is now 1.5 + 15 + ...
    assert_eq!(s.text(3, 4), "21.00");
}

#[quench_test::test]
fn escape_drops_an_edit_and_a_click_keeps_it() {
    let s = open(100);
    s.r.fire(
        s.cell(1, 0).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    s.key("9");
    s.r.fire(s.bar(), &Event::Input("999".into()));
    s.r.fire(s.bar(), &Event::KeyDown(KeyEvent::new("Escape")));
    assert_eq!(s.text(1, 0), "1");
    assert_eq!(s.r.attr(s.bar(), "value").as_deref(), Some("1"));
    // Typing then clicking another cell commits, as a spreadsheet does.
    s.key("7");
    s.r.fire(s.bar(), &Event::Input("77".into()));
    s.r.fire(
        s.cell(5, 5).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    assert_eq!(s.text(1, 0), "77");
    assert_eq!(s.selected(), CellRef::new(5, 5));
}

#[quench_test::test]
fn formulas_text_and_clearing() {
    let s = open(10);
    s.r.fire(
        s.cell(0, 7).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    ); // H1
    s.key("=");
    s.r.fire(s.bar(), &Event::Input("=SUM(A2:A5)&\" total\"".into()));
    s.r.fire(s.bar(), &Event::KeyDown(KeyEvent::new("Tab")));
    assert_eq!(s.text(0, 7), "10 total");
    assert_eq!(s.selected(), CellRef::new(0, 8)); // Tab moves right
    s.r.fire(
        s.cell(0, 7).unwrap(),
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    assert_eq!(
        s.r.attr(s.bar(), "value").as_deref(),
        Some("=SUM(A2:A5)&\" total\"")
    );
    s.key("Delete");
    assert_eq!(s.text(0, 7), "");
    // A formula with a mistake shows an error in the cell, in red.
    s.key("=");
    s.r.fire(s.bar(), &Event::Input("=1/0".into()));
    s.r.fire(s.bar(), &Event::KeyDown(KeyEvent::new("Enter")));
    assert_eq!(s.text(0, 7), "#DIV/0!");
    assert_eq!(
        s.r.attr(s.cell(0, 7).unwrap(), "class").as_deref(),
        Some("cell err")
    );
    // Numbers are right-aligned by class.
    assert_eq!(
        s.r.attr(s.cell(1, 0).unwrap(), "class").as_deref(),
        Some("cell num")
    );
}

#[quench_test::test]
fn what_is_typed_means_what_it_says() {
    use sheets_model::{Content, Value};
    let cases = [
        ("", Content::Value(Value::Empty)),
        ("42", Content::Value(Value::Number(42.0))),
        ("-1.5e3", Content::Value(Value::Number(-1500.0))),
        ("12%", Content::Value(Value::Number(0.12))),
        ("TRUE", Content::Value(Value::Bool(true))),
        ("true", Content::Value(Value::Bool(true))),
        (
            "#N/A",
            Content::Value(Value::Error(sheets_model::CellError::NA)),
        ),
        ("=A1+1", Content::Formula("A1+1".into())),
        ("=", Content::Value(Value::Text("=".into()))),
        ("'42", Content::Value(Value::Text("42".into()))),
        ("hello", Content::Value(Value::Text("hello".into()))),
        ("1 2", Content::Value(Value::Text("1 2".into()))),
        ("inf", Content::Value(Value::Text("inf".into()))),
        ("1e", Content::Value(Value::Text("1e".into()))),
    ];
    for (text, want) in cases {
        assert_eq!(parse_input(text), want, "{text:?}");
    }
    // Editing shows what typing would need to give the same content back.
    for content in [
        Content::Value(Value::Number(0.1)),
        Content::Value(Value::Text("42".into())),
        Content::Value(Value::Text("=1+1".into())),
        Content::Value(Value::Text("TRUE".into())),
        Content::Value(Value::Text("plain".into())),
        Content::Formula("SUM(A1:A3)".into()),
        Content::Value(Value::Bool(false)),
    ] {
        assert_eq!(
            parse_input(&edit_text(Some(&content))),
            content,
            "{content:?}"
        );
    }
    assert_eq!(edit_text(None), "");
}

#[quench_test::test]
fn a_cell_in_view_updates_when_a_cell_it_reads_changes() {
    let s = open(10);
    let c3 = s.cell(2, 2).unwrap();
    s.r.clear_ops();
    s.state.set_text(CellRef::new(2, 0), "100");
    assert_eq!(s.r.text_of(c3), "150.00");
    // Only the cells that depend on A3 were touched: not the hundreds in view.
    let all = s.r.ops();
    let ops = all.len();
    assert!(
        ops < 60,
        "{ops} operations for one edit: {:?}",
        &all[..12.min(ops)]
    );
}

fn button_labelled(s: &Screen, label: &str) -> NodeId {
    let found: Vec<NodeId> =
        s.r.find_by_tag("button")
            .into_iter()
            .filter(|b| s.r.text_of(*b) == label)
            .collect();
    assert_eq!(found.len(), 1, "{label}: {} buttons", found.len());
    found[0]
}

/// Open the File menu and choose what starts with `label`.
fn choose(s: &Screen, label: &str) {
    s.r.fire(button_labelled(s, "File"), &Event::Click);
    click_item(s, label);
    assert!(
        s.r.find_by_attr("role", "menu").is_empty(),
        "choosing closes the menu"
    );
}

/// The same for a choice inside a submenu of the File menu.
fn choose_in(s: &Screen, submenu: &str, label: &str) {
    s.r.fire(button_labelled(s, "File"), &Event::Click);
    click_item(s, submenu);
    click_item(s, label);
    assert!(
        s.r.find_by_attr("role", "menu").is_empty(),
        "choosing closes the menu"
    );
}

fn click_item(s: &Screen, label: &str) {
    let items: Vec<NodeId> =
        s.r.find_by_attr("role", "menuitem")
            .into_iter()
            .filter(|n| s.r.text_of(*n).starts_with(label))
            .collect();
    assert_eq!(items.len(), 1, "{label}: {} menu items", items.len());
    s.r.fire(items[0], &Event::Click);
}

fn with_platform() -> Rc<quench_ui::platform::ManualPlatform> {
    let manual = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(manual.clone());
    manual
}

fn picked(name: &str, bytes: Vec<u8>) -> quench_ui::platform::PickedFile {
    quench_ui::platform::PickedFile {
        name: name.into(),
        mime: String::new(),
        bytes,
    }
}

fn small_xlsx() -> Vec<u8> {
    let mut wb = sheets_model::Workbook::new();
    wb.sheets[0].name = "Budget".into();
    wb.sheets[0].set(
        CellRef::new(0, 0),
        sheets_model::Content::Value(sheets_model::Value::Text("Total".into())),
    );
    wb.sheets[0].set(
        CellRef::new(0, 1),
        sheets_model::Content::Formula("SUM(B2:B3)".into()),
    );
    wb.sheets[0].set(
        CellRef::new(1, 1),
        sheets_model::Content::Value(sheets_model::Value::Number(40.0)),
    );
    wb.sheets[0].set(
        CellRef::new(2, 1),
        sheets_model::Content::Value(sheets_model::Value::Number(2.0)),
    );
    let mut x = sheets_xlsx::Xlsx::new(wb);
    x.add_sheet("Notes");
    let notes = x.engine.sheet_index("Notes").unwrap();
    x.engine.set(
        notes,
        CellRef::new(0, 0),
        sheets_model::Content::Value(sheets_model::Value::Text("second sheet".into())),
    );
    x.write().unwrap()
}

#[quench_test::test]
fn open_asks_for_a_file_and_shows_what_was_picked() {
    let manual = with_platform();
    let s = open(10);
    choose(&s, "Open\u{2026}");
    let asked = manual.pending_picks();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].0.contains(".xlsx") && asked[0].0.contains(".csv"));
    assert!(!asked[0].1, "one file");
    assert!(manual.answer_pick(vec![picked("budget.xlsx", small_xlsx())]));
    assert_eq!(s.text(0, 0), "Total");
    assert_eq!(s.text(0, 1), "42");
    assert_eq!(
        s.r.attr(s.one("class", "file-name file-name-input"), "value")
            .as_deref(),
        Some("budget.xlsx")
    );
    assert_eq!(s.r.text_of(s.one("role", "status")), "Opened budget.xlsx");
    assert_eq!(s.selected(), CellRef::new(0, 0));
    assert_eq!(s.state.sheet_names(), ["Budget", "Notes"]);
}

#[quench_test::test]
fn a_csv_opens_as_a_sheet_named_after_the_file() {
    let manual = with_platform();
    let s = open(3);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked(
        "prices.csv",
        b"item,price
bolt,1.5
nut,0.25
"
        .to_vec(),
    )]);
    assert_eq!(s.text(1, 0), "bolt");
    assert_eq!(s.text(2, 1), "0.25");
    assert_eq!(s.state.sheet_names(), ["prices"]);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("t.tsv", b"a\tb\n1\t2\n".to_vec())]);
    assert_eq!(s.text(1, 1), "2");
}

#[quench_test::test]
fn a_file_that_cannot_be_read_is_reported_and_changes_nothing() {
    let manual = with_platform();
    let s = open(3);
    for (name, bytes) in [
        ("broken.xlsx", b"not a zip".to_vec()),
        ("photo.png", vec![1, 2, 3]),
    ] {
        choose(&s, "Open\u{2026}");
        manual.answer_pick(vec![picked(name, bytes)]);
        assert!(
            s.r.text_of(s.one("role", "status"))
                .starts_with(&format!("Could not open {name}")),
            "{name}"
        );
        assert_eq!(s.text(0, 0), "Id", "the workbook is as it was");
    }
    // Cancelling the picker is quiet.
    choose(&s, "Open\u{2026}");
    manual.answer_pick(Vec::new());
    assert_eq!(s.text(0, 0), "Id");
}

#[quench_test::test]
fn saving_offers_a_file_that_reads_back_with_the_edits() {
    let manual = with_platform();
    let s = open(5);
    s.state.set_text(CellRef::new(1, 0), "100");
    choose_in(&s, "Save\u{2026}", "Excel workbook");
    let saved = manual.downloads();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "Untitled.xlsx");
    assert!(saved[0].1.contains("spreadsheetml.sheet"));
    let back = sheets_xlsx::read(&saved[0].2).unwrap();
    assert_eq!(
        back.engine.value(0, CellRef::new(1, 0)),
        sheets_model::Value::Number(100.0)
    );
    assert_eq!(
        back.engine.value(0, CellRef::new(1, 2)),
        sheets_model::Value::Number(150.0),
        "formulas recalculated"
    );
    assert_eq!(s.r.text_of(s.one("role", "status")), "Saved Untitled.xlsx");
    // The csv is what the sheet shows.
    choose_in(&s, "Save\u{2026}", "CSV");
    let csv = String::from_utf8(manual.downloads()[1].2.clone()).unwrap();
    assert!(
        csv.starts_with("Id,Item,Price,Parity,Running total,Label\n100,item 1,150.00,even,"),
        "{csv}"
    );
    assert_eq!(manual.downloads()[1].0, "Untitled.csv");
}

#[quench_test::test]
fn a_file_saved_after_opening_keeps_its_name_and_what_the_suite_does_not_know() {
    let manual = with_platform();
    let s = open(3);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("budget.xlsx", small_xlsx())]);
    s.state.set_text(CellRef::new(1, 1), "58");
    choose_in(&s, "Save\u{2026}", "Excel workbook");
    assert_eq!(manual.downloads()[0].0, "budget.xlsx");
    let back = sheets_xlsx::read(&manual.downloads()[0].2).unwrap();
    assert_eq!(
        back.engine.value(0, CellRef::new(0, 1)),
        sheets_model::Value::Number(60.0)
    );
    assert_eq!(back.engine.sheet_index("Notes"), Some(1));
}

#[quench_test::test]
fn sheet_tabs_switch_the_sheet_and_add_new_ones() {
    let manual = with_platform();
    let s = open(3);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("budget.xlsx", small_xlsx())]);
    let tabs: Vec<String> =
        s.r.find_by_attr("role", "tab")
            .into_iter()
            .map(|t| s.r.text_of(t))
            .collect();
    assert_eq!(tabs, ["Budget", "Notes"], "the + button is not a tab");
    s.r.fire(button_labelled(&s, "Notes"), &Event::Click);
    assert_eq!(s.state.active.get_untracked(), 1);
    assert_eq!(s.text(0, 0), "second sheet");
    assert_eq!(
        s.r.attr(s.one("class", "name"), "value")
            .unwrap_or_default(),
        "A1"
    );
    assert_eq!(
        s.r.attr(button_labelled(&s, "Notes"), "class").as_deref(),
        Some("tab active")
    );
    s.r.fire(button_labelled(&s, "+"), &Event::Click);
    assert_eq!(s.state.sheet_names(), ["Budget", "Notes", "Sheet3"]);
    assert_eq!(s.state.active.get_untracked(), 2);
    s.state.set_text(CellRef::new(0, 0), "mine");
    assert_eq!(s.text(0, 0), "mine");
    s.r.fire(button_labelled(&s, "Budget"), &Event::Click);
    assert_eq!(s.text(0, 0), "Total", "each sheet has its own cells");
    s.state.switch_sheet(99); // not a sheet: ignored
    assert_eq!(s.state.active.get_untracked(), 0);
    // New starts again from nothing.
    choose(&s, "New");
    assert_eq!(s.state.sheet_names(), ["Sheet1"]);
    assert_eq!(
        s.r.attr(s.one("class", "file-name file-name-input"), "value")
            .as_deref(),
        Some("Untitled.xlsx")
    );
}

const fn pointer() -> Event {
    Event::PointerDown(PointerEvent {
        x: 0.0,
        y: 0.0,
        pointer_id: 1,
    })
}

impl Screen {
    fn press(&self, row: u32, col: u32) {
        self.r.fire(self.cell(row, col).unwrap(), &pointer());
    }

    fn over(&self, row: u32, col: u32) {
        self.r.fire(
            self.cell(row, col).unwrap(),
            &Event::PointerMove(PointerEvent {
                x: 0.0,
                y: 0.0,
                pointer_id: 1,
            }),
        );
    }

    fn release(&self) {
        self.r.fire(
            self.one("role", "grid"),
            &Event::PointerUp(PointerEvent {
                x: 0.0,
                y: 0.0,
                pointer_id: 1,
            }),
        );
    }

    fn chord(&self, key: &str, ctrl: bool, shift: bool) {
        let mut k = KeyEvent::new(key);
        (k.ctrl, k.shift) = (ctrl, shift);
        self.r.fire(self.one("role", "grid"), &Event::KeyDown(k));
    }

    fn copy(&self) -> Option<quench_ui::ClipboardData> {
        let _ = quench_ui::take_clipboard_request();
        self.r.fire(self.one("role", "grid"), &Event::Copy);
        quench_ui::take_clipboard_request()
    }

    fn cut(&self) -> Option<quench_ui::ClipboardData> {
        let _ = quench_ui::take_clipboard_request();
        self.r.fire(self.one("role", "grid"), &Event::Cut);
        quench_ui::take_clipboard_request()
    }

    fn paste(&self, text: &str) {
        self.r.fire(
            self.one("role", "grid"),
            &Event::Paste(PasteEvent {
                text: text.into(),
                html: String::new(),
            }),
        );
    }

    fn range(&self) -> String {
        self.state.selection().to_string()
    }
}

fn r(a: &str) -> sheets_model::Range {
    sheets_model::Range::parse(a).unwrap()
}

#[quench_test::test]
fn shift_and_the_arrows_stretch_the_selection_from_the_active_cell() {
    let s = open(20);
    s.press(1, 1); // B2
    assert_eq!(s.range(), "B2");
    s.chord("ArrowDown", false, true);
    s.chord("ArrowRight", false, true);
    s.chord("ArrowRight", false, true);
    assert_eq!(s.range(), "B2:D3");
    assert_eq!(s.selected(), CellRef::new(1, 1), "the active cell stays");
    assert_eq!(
        s.r.attr(s.one("class", "name"), "value")
            .unwrap_or_default(),
        "B2"
    );
    let overlay = s.one("class", "range");
    assert_eq!(s.r.attr(overlay, "data-multi").as_deref(), Some("true"));
    assert_eq!(s.r.style_of(overlay, "left").as_deref(), Some("96px"));
    assert_eq!(s.r.style_of(overlay, "top").as_deref(), Some("24px"));
    assert_eq!(s.r.style_of(overlay, "width").as_deref(), Some("288px"));
    assert_eq!(s.r.style_of(overlay, "height").as_deref(), Some("48px"));
    s.chord("ArrowUp", false, true);
    s.chord("ArrowUp", false, true);
    s.chord("ArrowUp", false, true);
    assert_eq!(
        s.range(),
        "B1:D2",
        "stretching back past the active cell turns the rectangle over"
    );
    s.key("ArrowDown"); // a plain arrow moves the active cell and drops the rest
    assert_eq!(s.range(), "B3");
    assert_eq!(
        s.r.attr(s.one("class", "range"), "data-multi").as_deref(),
        Some("false")
    );
}

#[quench_test::test]
fn dragging_with_the_pointer_selects_a_rectangle_until_it_is_released() {
    let s = open(20);
    s.press(2, 1);
    s.over(3, 2);
    s.over(4, 3);
    assert_eq!(s.range(), "B3:D5");
    s.release();
    s.over(8, 8); // the pointer is up: moving does nothing
    assert_eq!(s.range(), "B3:D5");
    // Holding shift while clicking stretches instead of starting again.
    s.chord("Shift", false, true);
    s.press(6, 5);
    assert_eq!(s.range(), "B3:F7");
    s.r.fire(s.one("role", "grid"), &Event::KeyUp(KeyEvent::new("Shift")));
    s.release();
    s.press(0, 0);
    assert_eq!(
        s.range(),
        "A1",
        "without shift a click starts a new selection"
    );
}

#[quench_test::test]
fn a_header_selects_its_whole_column_or_row_and_ctrl_a_everything() {
    let s = open(20);
    let col = s.r.find_by_attr("class", "colhdr")[2]; // C
    s.r.fire(col, &Event::Click);
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::new(CellRef::new(0, 2), CellRef::new(ROWS - 1, 2))
    );
    let row = s.r.find_by_attr("class", "rowhdr")[4]; // row 5
    s.r.fire(row, &Event::Click);
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::new(CellRef::new(4, 0), CellRef::new(4, COLS - 1))
    );
    s.chord("a", true, false);
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::new(CellRef::new(0, 0), CellRef::new(ROWS - 1, COLS - 1))
    );
}

#[quench_test::test]
fn selecting_a_whole_column_does_not_send_the_view_to_its_far_end() {
    let s = open(20);
    s.state.select(CellRef::new(4, 3));
    let col = s.r.find_by_attr("class", "colhdr")[3];
    s.r.fire(col, &Event::PointerDown(at_x(0.0)));
    s.r.fire(col, &Event::Click);
    assert_eq!(s.state.selection().end.row, ROWS - 1);
    assert_eq!(
        s.state.focus_at.get_untracked(),
        CellRef::new(0, 3),
        "focus (and the scrolling after it) stays at the top"
    );
    // Stretching with the keyboard follows the moving corner as before.
    s.state.select(CellRef::new(4, 3));
    s.chord("ArrowDown", false, true);
    assert_eq!(s.state.focus_at.get_untracked(), CellRef::new(5, 3));
}

#[quench_test::test]
fn copying_gives_what_the_cells_show_as_tab_separated_text_and_html() {
    let s = open(20);
    s.state.select_range(r("A2:C3"));
    let got = s.copy().expect("something was copied");
    assert_eq!(got.text, "1\titem 1\t1.50\n2\titem 2\t3.00\n");
    assert!(
        got.html
            .starts_with("<table><tr><td>1</td><td>item 1</td><td>1.50</td></tr>"),
        "{}",
        got.html
    );
    // Nothing to copy from an empty corner: the browser's own copy runs.
    s.state.select_range(r("Z100:AA101"));
    assert!(s.copy().is_none());
    // Special characters survive both forms.
    s.state.set_text(CellRef::new(30, 0), "say \"hi\"\tthere");
    s.state.set_text(CellRef::new(30, 1), "a<b & c");
    s.state.select_range(r("A31:B31"));
    let got = s.copy().unwrap();
    assert_eq!(got.text, "\"say \"\"hi\"\"\tthere\"\ta<b & c\n");
    assert!(got.html.contains("a&lt;b &amp; c"));
}

#[quench_test::test]
fn pasting_text_from_elsewhere_writes_values_and_formulas_at_the_selection() {
    let s = open(20);
    s.state.select(CellRef::new(0, 7)); // H1
    s.paste("a\t1\n\"x\ty\"\t=1+1\r\n");
    assert_eq!(s.text(0, 7), "a");
    assert_eq!(s.text(0, 8), "1");
    assert_eq!(s.text(1, 7), "x\ty");
    assert_eq!(s.text(1, 8), "2");
    assert!(
        s.r.attr(s.cell(0, 8).unwrap(), "class")
            .unwrap()
            .contains("num"),
        "numbers are numbers"
    );
    assert_eq!(s.range(), "H1:I2", "what was pasted is selected");
    // Text typed into a formatted cell takes the cell's format.
    s.state.select(CellRef::new(1, 2)); // C2 has #,##0.00
    s.paste("7");
    assert_eq!(s.text(1, 2), "7.00");
    // Pasting nothing does nothing.
    s.paste("");
}

#[quench_test::test]
fn pasting_what_was_copied_here_moves_formulas_and_keeps_formats() {
    let s = open(20);
    s.state.select(CellRef::new(1, 2)); // C2 = A2*1.5, 1.50
    let got = s.copy().unwrap();
    s.state.select(CellRef::new(9, 2)); // C10
    s.paste(&got.text);
    assert_eq!(
        s.text(9, 2),
        "13.50",
        "=A10*1.5 with A10 = 9, formatted like the source"
    );
    assert_eq!(s.state.edit_text(CellRef::new(9, 2)), "=A10*1.5");
    // One copied cell fills a bigger selection.
    s.state.select(CellRef::new(1, 1)); // B2 "item 1"
    let got = s.copy().unwrap();
    s.state.select_range(r("F2:F4"));
    s.paste(&got.text);
    assert_eq!(
        (s.text(1, 5), s.text(2, 5), s.text(3, 5)),
        ("item 1".into(), "item 1".into(), "item 1".into())
    );
    assert_eq!(s.range(), "F2:F4");
    // A formula copied sideways moves its references the other way.
    s.state.select(CellRef::new(1, 2));
    let got = s.copy().unwrap();
    s.state.select(CellRef::new(1, 8)); // I2
    s.paste(&got.text);
    assert_eq!(s.state.edit_text(CellRef::new(1, 8)), "=G2*1.5");
}

#[quench_test::test]
fn cut_moves_the_cells_when_pasted_and_not_before() {
    let s = open(20);
    s.state.select_range(r("A2:B2"));
    let got = s.cut().unwrap();
    assert_eq!(
        s.text(1, 0),
        "1",
        "a cut leaves the cells until they are pasted"
    );
    s.state.select(CellRef::new(1, 7)); // H2
    s.paste(&got.text);
    assert_eq!(
        (s.text(1, 7), s.text(1, 8)),
        ("1".to_string(), "item 1".to_string())
    );
    assert_eq!(
        (s.text(1, 0), s.text(1, 1)),
        (String::new(), String::new()),
        "now they are gone from where they were"
    );
    // The formula that read A2 now reads nothing, and the pasted cells are one undo step.
    s.state.undo();
    assert_eq!(
        (s.text(1, 0), s.text(1, 1)),
        ("1".to_string(), "item 1".to_string())
    );
    assert_eq!((s.text(1, 7), s.text(1, 8)), (String::new(), String::new()));
    s.state.redo();
    assert_eq!(s.text(1, 7), "1");
    assert_eq!(s.text(1, 0), "");
}

#[quench_test::test]
fn undo_and_redo_walk_back_and_forward_through_edits() {
    let s = open(5);
    let undo = button_labelled(&s, "Undo");
    let redo = button_labelled(&s, "Redo");
    assert!(s.r.attr(undo, "disabled").is_some(), "nothing to undo yet");
    assert!(s.r.attr(redo, "disabled").is_some());
    s.state.set_text(CellRef::new(1, 0), "100");
    s.state.set_text(CellRef::new(1, 0), "200");
    assert!(s.r.attr(undo, "disabled").is_none());
    assert_eq!(s.text(1, 2), "300.00");
    s.chord("z", true, false);
    assert_eq!(s.text(1, 0), "100");
    assert_eq!(s.text(1, 2), "150.00", "what depends on the cell follows");
    assert!(s.r.attr(redo, "disabled").is_none());
    s.chord("z", true, false);
    assert_eq!(s.text(1, 0), "1");
    s.chord("z", true, false); // nothing left: quiet
    assert!(s.r.attr(undo, "disabled").is_some());
    s.chord("y", true, false);
    assert_eq!(s.text(1, 0), "100");
    s.chord("z", true, true); // ctrl+shift+z is redo too
    assert_eq!(s.text(1, 0), "200");
    // A new edit ends the redo line.
    s.chord("z", true, false);
    s.state.set_text(CellRef::new(1, 0), "7");
    assert!(s.r.attr(button_labelled(&s, "Redo"), "disabled").is_some());
    s.r.fire(button_labelled(&s, "Undo"), &Event::Click);
    assert_eq!(s.text(1, 0), "100");
}

#[quench_test::test]
fn deleting_a_selection_empties_every_cell_in_one_undoable_step() {
    let s = open(5);
    s.state.select_range(r("A2:B3"));
    s.key("Delete");
    assert_eq!(
        (s.text(1, 0), s.text(1, 1), s.text(2, 0), s.text(2, 1)),
        (String::new(), String::new(), String::new(), String::new())
    );
    assert_eq!(
        s.text(1, 2),
        "0.00",
        "the formula that read the deleted cell sees nothing"
    );
    assert_eq!(s.text(3, 0), "3", "cells outside it are untouched");
    s.chord("z", true, false);
    assert_eq!(
        (s.text(1, 0), s.text(2, 1)),
        ("1".to_string(), "item 2".to_string())
    );
    assert_eq!(s.text(1, 2), "1.50");
}

#[quench_test::test]
fn undo_goes_back_to_the_sheet_where_the_change_was() {
    let manual = with_platform();
    let s = open(3);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("budget.xlsx", small_xlsx())]);
    s.state.set_text(CellRef::new(5, 5), "x"); // on Budget
    s.r.fire(button_labelled(&s, "Notes"), &Event::Click);
    s.chord("z", true, false);
    assert_eq!(
        s.state.active.get_untracked(),
        0,
        "the sheet the change was on is shown again"
    );
    assert_eq!(s.selected(), CellRef::new(5, 5));
    assert_eq!(s.text(5, 5), "");
    // Opening a file starts a new history.
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("budget.xlsx", small_xlsx())]);
    assert!(s.r.attr(button_labelled(&s, "Undo"), "disabled").is_some());
}

#[quench_test::test]
fn typed_numbers_may_have_thousands_separators_and_a_dollar_sign() {
    use sheets_model::{Content, Value};
    let n = |x: f64| Content::Value(Value::Number(x));
    let t = |x: &str| Content::Value(Value::Text(x.into()));
    for (text, want) in [
        ("1,234.5", n(1234.5)),
        ("$1,234", n(1234.0)),
        ("12,345,678", n(12_345_678.0)),
        ("-1,000", n(-1000.0)),
        ("$5", n(5.0)),
        ("1,23", t("1,23")),
        ("1,2345", t("1,2345")),
        (",5", t(",5")),
        ("1,000,", t("1,000,")),
    ] {
        assert_eq!(parse_input(text), want, "{text}");
    }
}

fn at_x(x: f64) -> PointerEvent {
    PointerEvent {
        x,
        y: 0.0,
        pointer_id: 1,
    }
}

fn at_y(y: f64) -> PointerEvent {
    PointerEvent {
        x: 0.0,
        y,
        pointer_id: 1,
    }
}

impl Screen {
    fn col_handle(&self, col: usize) -> NodeId {
        self.r.find_by_attr("class", "hdr-handle col")[col]
    }

    fn row_handle(&self, row: usize) -> NodeId {
        self.r.find_by_attr("class", "hdr-handle row")[row]
    }

    fn sheet_root(&self) -> NodeId {
        self.one("class", "sheet")
    }

    fn col_size(&self, col: u32) -> f64 {
        self.state.axes.get_untracked().cols.size(col)
    }

    fn row_size(&self, row: u32) -> f64 {
        self.state.axes.get_untracked().rows.size(row)
    }

    /// A key pressed in the cell editor: it reaches the editor, then bubbles up to the grid the editor is in.
    fn editor_key(&self, key: &str) {
        let editor = self.editor().expect("the cell editor is open");
        self.r.fire(editor, &Event::KeyDown(KeyEvent::new(key)));
        self.r.fire(
            self.one("role", "grid"),
            &Event::KeyDown(KeyEvent::new(key)),
        );
    }

    fn editor(&self) -> Option<NodeId> {
        self.r.find_by_attr("class", "cell-editor").first().copied()
    }
}

#[quench_test::test]
fn the_axis_finds_positions_from_the_sizes_that_differ() {
    let a = Axis::new(24.0, 100, [(2, 100.0), (50, 10.0)]);
    assert_eq!(
        (a.size(0), a.size(2), a.size(50), a.size(99)),
        (24.0, 100.0, 10.0, 24.0)
    );
    assert_eq!((a.offset(0), a.offset(2), a.offset(3)), (0.0, 48.0, 148.0));
    assert_eq!(a.offset(50), 148.0 + 24.0 * 47.0);
    assert_eq!(a.total(), 24.0 * 98.0 + 100.0 + 10.0);
    assert_eq!(
        (
            a.index_at(-5.0),
            a.index_at(0.0),
            a.index_at(47.9),
            a.index_at(48.0)
        ),
        (0, 0, 1, 2)
    );
    assert_eq!((a.index_at(147.9), a.index_at(148.0)), (2, 3));
    assert_eq!(a.index_at(1e9), 99, "past the end is the last item");
    // Every item is found at its own middle, wherever the exceptions are.
    let b = Axis::new(
        30.0,
        500,
        (0..500)
            .filter(|i| i % 7 == 3)
            .map(|i| (i, 20.0 + f64::from(i % 5) * 11.0)),
    );
    for i in 0..500 {
        assert_eq!(b.index_at(b.offset(i) + b.size(i) / 2.0), i, "item {i}");
        assert!((b.offset(i + 1) - b.offset(i) - b.size(i)).abs() < 1e-9);
    }
    // Later duplicates win, nonsense and out-of-range entries are ignored.
    let c = Axis::new(
        10.0,
        5,
        [(1, 50.0), (1, 70.0), (9, 5.0), (2, f64::NAN), (3, -4.0)],
    );
    assert_eq!(
        (c.size(1), c.size(2), c.size(3), c.count()),
        (70.0, 10.0, 10.0, 5)
    );
    assert_eq!(Axis::new(10.0, 0, []).index_at(5.0), 0);
}

#[quench_test::test]
fn dragging_a_column_border_resizes_it_and_everything_after_it_moves() {
    let s = open(20);
    assert_eq!(s.col_size(1), COL_W);
    let total = s.state.axes.get_untracked().cols.total();
    s.r.fire(s.col_handle(1), &Event::PointerDown(at_x(500.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(530.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(560.0)));
    assert_eq!(
        s.col_size(1),
        COL_W + 60.0,
        "it follows the pointer, wherever it is over the screen"
    );
    assert_eq!(s.state.axes.get_untracked().cols.total(), total + 60.0);
    // The grid's tracks, the cursor and the sizer all follow.
    let window = s.one("class", "window");
    assert!(
        s.r.style_of(window, "grid-template-columns")
            .unwrap()
            .starts_with("96px 156px 96px"),
        "{:?}",
        s.r.style_of(window, "grid-template-columns")
    );
    s.state.select(CellRef::new(0, 2));
    assert_eq!(
        s.r.style_of(s.one("class", "cursor"), "left").as_deref(),
        Some("252px"),
        "96 + 156"
    );
    assert_eq!(
        s.r.style_of(s.one("class", "sizer"), "width").unwrap(),
        format!("{}px", total + 60.0)
    );
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_x(560.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(900.0)));
    assert_eq!(s.col_size(1), 156.0, "released: moving does nothing");
    // The click that ends a drag does not select the column; the next real click does.
    s.state.select(CellRef::new(3, 3));
    let header = s.r.find_by_attr("class", "colhdr")[1];
    s.r.fire(header, &Event::Click);
    assert_eq!(s.selected(), CellRef::new(3, 3));
    s.r.fire(header, &Event::PointerDown(at_x(0.0)));
    s.r.fire(header, &Event::Click);
    assert_eq!(s.state.selection().start.col, 1);
    // It is kept in the workbook, and stops at the limits.
    assert_eq!(
        s.state.with_engine(|e| e.workbook().sheets[0].col_width(1)),
        Some(156.0)
    );
    s.r.fire(s.col_handle(1), &Event::PointerDown(at_x(500.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(-5000.0)));
    assert_eq!(s.col_size(1), MIN_COL_W);
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(5000.0)));
    assert_eq!(s.col_size(1), MAX_COL_W);
}

#[quench_test::test]
fn dragging_a_row_border_resizes_the_row() {
    let s = open(20);
    s.r.fire(s.row_handle(2), &Event::PointerDown(at_y(100.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_y(130.0)));
    assert_eq!(
        (s.row_size(1), s.row_size(2), s.row_size(3)),
        (ROW_H, ROW_H + 30.0, ROW_H)
    );
    let window = s.one("class", "window");
    assert!(
        s.r.style_of(window, "grid-template-rows")
            .unwrap()
            .starts_with("24px 24px 54px 24px")
    );
    s.state.select(CellRef::new(3, 0));
    assert_eq!(
        s.r.style_of(s.one("class", "cursor"), "top").as_deref(),
        Some("102px"),
        "24 + 24 + 54"
    );
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_y(130.0)));
    assert_eq!(
        s.state
            .with_engine(|e| e.workbook().sheets[0].row_height(2)),
        Some(54.0)
    );
}

#[quench_test::test]
fn a_double_click_on_a_border_fits_the_column_and_resets_the_row() {
    let s = open(20);
    s.r.fire(s.col_handle(1), &Event::DoubleClick); // B holds "item 20" at the longest, 7 characters
    assert_eq!(s.col_size(1), 69.0);
    s.r.fire(s.col_handle(5), &Event::DoubleClick); // F: "item 20 (even)"
    assert_eq!(s.col_size(5), (14.0f64 * 7.5 + 16.0).round());
    s.r.fire(s.col_handle(9), &Event::DoubleClick); // J is empty: the default again
    assert_eq!(s.col_size(9), COL_W);
    s.r.fire(s.row_handle(3), &Event::PointerDown(at_y(10.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_y(40.0)));
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_y(40.0)));
    assert_eq!(s.row_size(3), ROW_H + 30.0);
    s.r.fire(s.row_handle(3), &Event::DoubleClick);
    assert_eq!(s.row_size(3), ROW_H);
}

#[quench_test::test]
fn typing_in_the_grid_edits_in_the_cell_itself() {
    let s = open(20);
    assert!(s.editor().is_none(), "no editor until editing starts");
    s.state.select(CellRef::new(1, 1)); // B2
    s.key("x");
    let editor = s.editor().expect("the editor is over the cell");
    assert_eq!(
        s.r.attr(editor, "value").as_deref(),
        Some(""),
        "a typed character starts with an empty field"
    );
    assert_eq!(s.r.style_of(editor, "left").as_deref(), Some("96px"));
    assert_eq!(s.r.style_of(editor, "top").as_deref(), Some("24px"));
    s.r.fire(editor, &Event::Input("xyz".into()));
    assert_eq!(
        s.r.attr(s.bar(), "value").as_deref(),
        Some("xyz"),
        "the formula bar shows the same text"
    );
    s.editor_key("Enter");
    assert!(s.editor().is_none(), "committing removes it");
    assert_eq!(s.text(1, 1), "xyz");
    assert_eq!(s.selected(), CellRef::new(2, 1));
    // F2 and a double click edit what is there; Escape drops the change.
    s.state.select(CellRef::new(1, 1));
    s.key("F2");
    let editor = s.editor().unwrap();
    assert_eq!(s.r.attr(editor, "value").as_deref(), Some("xyz"));
    s.r.fire(editor, &Event::Input("changed".into()));
    s.editor_key("Escape");
    assert!(s.editor().is_none());
    assert_eq!(s.text(1, 1), "xyz");
    s.r.fire(s.cell(3, 0).unwrap(), &Event::DoubleClick);
    assert_eq!(s.r.attr(s.editor().unwrap(), "value").as_deref(), Some("3"));
    // Tab commits and moves right.
    s.r.fire(s.editor().unwrap(), &Event::Input("33".into()));
    s.editor_key("Tab");
    assert_eq!(
        (s.text(3, 0), s.selected()),
        ("33".to_string(), CellRef::new(3, 1))
    );
    // Typing in the formula bar instead does not open the editor.
    s.state.select(CellRef::new(5, 5));
    s.r.fire(s.bar(), &Event::Input("from the bar".into()));
    assert!(s.editor().is_none());
    s.r.fire(s.bar(), &Event::KeyDown(KeyEvent::new("Enter")));
    assert_eq!(s.text(5, 5), "from the bar");
}

#[quench_test::test]
fn keys_typed_in_the_cell_editor_do_not_reach_the_grid_behind_it() {
    let s = open(20);
    s.state.select(CellRef::new(1, 1));
    s.key("h");
    let editor = s.editor().unwrap();
    // The browser types the character, then the next keys arrive at the editor and bubble up to the grid.
    s.r.fire(editor, &Event::Input("h".into()));
    for (key, typed) in [("e", "he"), ("l", "hel"), ("p", "help")] {
        s.r.fire(s.one("role", "grid"), &Event::KeyDown(KeyEvent::new(key)));
        s.r.fire(editor, &Event::Input(typed.into()));
    }
    assert_eq!(
        s.state.draft.get_untracked(),
        "help",
        "the grid did not start the edit over"
    );
    assert_eq!(
        s.selected(),
        CellRef::new(1, 1),
        "arrows in the editor move the caret, not the cell"
    );
    s.r.fire(
        s.one("role", "grid"),
        &Event::KeyDown(KeyEvent::new("ArrowDown")),
    );
    assert_eq!(s.selected(), CellRef::new(1, 1));
    // Enter commits once, even though it also bubbles up to the grid.
    s.editor_key("Enter");
    assert_eq!(
        (s.text(1, 1), s.selected()),
        ("help".to_string(), CellRef::new(2, 1)),
        "one step down, not two"
    );
    // The next key in the grid is the grid's again.
    s.key("ArrowDown");
    assert_eq!(s.selected(), CellRef::new(3, 1));
    // Copy and paste in the editor act on its text, not on the cells.
    s.key("x");
    let before = s.state.edit_text(CellRef::new(3, 1));
    s.paste("zzz");
    assert_eq!(
        s.state.edit_text(CellRef::new(3, 1)),
        before,
        "a paste while typing is the field's"
    );
    assert!(s.copy().is_none());
    s.state.cancel_edit();
}

#[quench_test::test]
fn the_editor_sits_over_the_cell_after_columns_and_rows_were_resized() {
    let s = open(20);
    s.r.fire(s.col_handle(0), &Event::PointerDown(at_x(0.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(100.0)));
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_x(100.0)));
    s.r.fire(s.row_handle(0), &Event::PointerDown(at_y(0.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_y(16.0)));
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_y(16.0)));
    s.state.select(CellRef::new(1, 1));
    s.key("F2");
    let editor = s.editor().unwrap();
    assert_eq!(
        s.r.style_of(editor, "left").as_deref(),
        Some("196px"),
        "A is 196 wide now"
    );
    assert_eq!(
        s.r.style_of(editor, "top").as_deref(),
        Some("40px"),
        "row 1 is 40 high now"
    );
    assert_eq!(s.r.style_of(editor, "width").as_deref(), Some("96px"));
    s.state.cancel_edit();
}

fn with_sizes() -> Vec<u8> {
    let mut x = sheets_xlsx::Xlsx::new(sheets_model::Workbook::new());
    x.engine.set(
        0,
        CellRef::new(0, 0),
        sheets_model::Content::Value(sheets_model::Value::Text("wide".into())),
    );
    x.engine.set_col_width(0, 0, Some(200.0));
    x.engine.set_row_height(0, 1, Some(48.0));
    x.add_sheet("Plain");
    x.write().unwrap()
}

#[quench_test::test]
fn sizes_come_with_a_file_go_back_into_it_and_belong_to_their_sheet() {
    let manual = with_platform();
    let s = open(3);
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("sized.xlsx", with_sizes())]);
    assert_eq!(
        (s.col_size(0), s.row_size(1), s.col_size(1)),
        (200.0, 48.0, COL_W)
    );
    s.r.fire(button_labelled(&s, "Plain"), &Event::Click);
    assert_eq!(
        (s.col_size(0), s.row_size(1)),
        (COL_W, ROW_H),
        "each sheet has its own sizes"
    );
    s.r.fire(button_labelled(&s, "Sheet1"), &Event::Click);
    assert_eq!(s.col_size(0), 200.0);
    // Resize, save, read the saved file.
    s.r.fire(s.col_handle(2), &Event::PointerDown(at_x(0.0)));
    s.r.fire(s.sheet_root(), &Event::PointerMove(at_x(44.0)));
    s.r.fire(s.sheet_root(), &Event::PointerUp(at_x(44.0)));
    choose_in(&s, "Save\u{2026}", "Excel workbook");
    let back = sheets_xlsx::read(&manual.downloads()[0].2).unwrap();
    let sheet = &back.engine.workbook().sheets[0];
    assert!((sheet.col_width(0).unwrap() - 200.0).abs() < 0.01);
    assert!((sheet.col_width(2).unwrap() - 140.0).abs() < 0.01);
    assert!((sheet.row_height(1).unwrap() - 48.0).abs() < 0.01);
}

impl Screen {
    fn class_of(&self, row: u32, col: u32) -> String {
        self.r.attr(self.cell(row, col).unwrap(), "class").unwrap()
    }

    fn colour_input(&self, label: &str) -> NodeId {
        self.one("aria-label", label)
    }
}

#[quench_test::test]
fn decimal_places_step_through_formats() {
    for (from, delta, to) in [
        ("0.00", 1, "0.000"),
        ("0.00", -1, "0.0"),
        ("0.0", -1, "0"),
        ("0", -1, "0"),
        ("0", 1, "0.0"),
        ("0", 2, "0.00"),
        ("#,##0", 1, "#,##0.0"),
        ("#,##0.00", -2, "#,##0"),
        ("0%", 1, "0.0%"),
        ("0.00%", -1, "0.0%"),
        ("$#,##0.00", 1, "$#,##0.000"),
        ("0.00E+00", 1, "0.000E+00"),
        ("General", 1, "0.0"),
        ("General", -1, "General"),
        ("", 1, "0.0"),
        ("yyyy-mm-dd", 1, "yyyy-mm-dd"),
        ("@", -1, "@"),
        ("0.00;[Red]-0.00", 1, "0.000;[Red]-0.000"),
        ("\"x\"0.0", 1, "\"x\"0.00"),
        ("0.0\" units\"", -1, "0\" units\""),
    ] {
        assert_eq!(more_decimals(from, delta), to, "{from} {delta:+}");
    }
    assert_eq!(parse_color("#ff8000"), Some(0xFF_8000));
    assert_eq!(
        (
            parse_color("ff8000"),
            parse_color("#ff80"),
            parse_color("#gg0000")
        ),
        (None, None, None)
    );
    assert_eq!(color_css(0x00_00FF), "#0000ff");
    assert_eq!(color_css(0xAB_CDEF), "#abcdef");
}

#[quench_test::test]
fn bold_italic_underline_toggle_for_the_whole_selection_and_undo_together() {
    let s = open(10);
    s.state.select_range(r("B2:C3"));
    assert_eq!(s.class_of(1, 1), "cell");
    s.r.fire(button_labelled(&s, "B"), &Event::Click);
    for (row, col) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
        assert!(
            s.class_of(row, col).split(' ').any(|c| c == "b"),
            "{row},{col}: {}",
            s.class_of(row, col)
        );
    }
    assert_eq!(
        s.class_of(1, 0),
        "cell num",
        "cells outside it are untouched"
    );
    assert_eq!(
        s.r.attr(button_labelled(&s, "B"), "aria-pressed")
            .as_deref(),
        Some("true")
    );
    assert!(
        s.r.attr(button_labelled(&s, "B"), "class")
            .unwrap()
            .contains("on")
    );
    s.r.fire(button_labelled(&s, "I"), &Event::Click);
    s.r.fire(button_labelled(&s, "U"), &Event::Click);
    assert!(s.class_of(1, 1).contains("b i u"), "{}", s.class_of(1, 1));
    // The same button again turns it off everywhere (going by the active cell).
    s.r.fire(button_labelled(&s, "B"), &Event::Click);
    assert!(!s.class_of(2, 2).split(' ').any(|c| c == "b"));
    assert_eq!(
        s.r.attr(button_labelled(&s, "B"), "aria-pressed")
            .as_deref(),
        Some("false")
    );
    // Each press is one undo step.
    s.chord("z", true, false);
    assert!(s.class_of(2, 2).split(' ').any(|c| c == "b"));
    s.chord("z", true, false);
    s.chord("z", true, false);
    s.chord("z", true, false);
    assert_eq!(s.class_of(1, 1), "cell");
    s.chord("y", true, false);
    assert!(s.class_of(1, 1).split(' ').any(|c| c == "b"));
}

#[quench_test::test]
fn alignment_and_wrap_buttons_choose_and_clear() {
    let s = open(10);
    s.state.select(CellRef::new(1, 1));
    s.r.fire(button_labelled(&s, "Center"), &Event::Click);
    assert!(s.class_of(1, 1).contains(" ac"));
    assert_eq!(
        s.r.attr(button_labelled(&s, "Center"), "aria-pressed")
            .as_deref(),
        Some("true")
    );
    s.r.fire(button_labelled(&s, "Right"), &Event::Click);
    assert!(s.class_of(1, 1).contains(" ar") && !s.class_of(1, 1).contains(" ac"));
    s.r.fire(button_labelled(&s, "Right"), &Event::Click);
    assert!(
        !s.class_of(1, 1).contains(" ar"),
        "choosing the alignment it has turns it off"
    );
    s.r.fire(button_labelled(&s, "Left"), &Event::Click);
    assert!(s.class_of(1, 1).contains(" al"));
    s.r.fire(button_labelled(&s, "Wrap"), &Event::Click);
    assert!(s.class_of(1, 1).contains(" wrap"));
}

#[quench_test::test]
fn colours_come_from_the_colour_inputs_and_can_be_removed() {
    let s = open(10);
    s.state.select_range(r("A2:A3"));
    s.r.fire(
        s.colour_input("Text colour"),
        &Event::Change("#ff0000".into()),
    );
    s.r.fire(
        s.colour_input("Fill colour"),
        &Event::Change("#ffff00".into()),
    );
    for row in [1, 2] {
        let c = s.cell(row, 0).unwrap();
        assert_eq!(s.r.style_of(c, "color").as_deref(), Some("#ff0000"));
        assert_eq!(
            s.r.style_of(c, "background-color").as_deref(),
            Some("#ffff00")
        );
    }
    assert_eq!(
        s.r.style_of(s.cell(3, 0).unwrap(), "background-color"),
        None,
        "outside the selection"
    );
    // The inputs show the active cell's colours.
    assert_eq!(
        s.r.attr(s.colour_input("Fill colour"), "value").as_deref(),
        Some("#ffff00")
    );
    s.r.fire(button_labelled(&s, "No fill"), &Event::Click);
    assert_eq!(
        s.r.style_of(s.cell(1, 0).unwrap(), "background-color"),
        None
    );
    assert_eq!(
        s.r.style_of(s.cell(1, 0).unwrap(), "color").as_deref(),
        Some("#ff0000"),
        "the text colour stays"
    );
    // Something that is not a colour is ignored.
    s.r.fire(s.colour_input("Text colour"), &Event::Change("red".into()));
    assert_eq!(
        s.r.style_of(s.cell(1, 0).unwrap(), "color"),
        None,
        "no colour means the default again"
    );
}

#[quench_test::test]
fn the_number_format_list_and_the_decimal_buttons_change_what_numbers_show() {
    let s = open(10);
    s.state.select_range(r("C2:C3"));
    assert_eq!(
        (s.text(1, 2), s.text(2, 2)),
        ("1.50".to_string(), "3.00".to_string())
    );
    let list = s.one("class", "format-list");
    s.r.fire(list, &Event::Change("0.00%".into()));
    assert_eq!(
        (s.text(1, 2), s.text(2, 2)),
        ("150.00%".to_string(), "300.00%".to_string())
    );
    s.r.fire(button_labelled(&s, ".0-"), &Event::Click);
    assert_eq!(s.text(1, 2), "150.0%");
    s.r.fire(button_labelled(&s, ".0+"), &Event::Click);
    s.r.fire(button_labelled(&s, ".0+"), &Event::Click);
    assert_eq!(s.text(2, 2), "300.000%");
    // The list shows the format of the active cell; one it does not offer appears as its own entry.
    let options = s.r.children(s.one("aria-label", "Number format"));
    assert_eq!(options.len(), PRESETS.len() + 1);
    let selected: Vec<String> = options
        .iter()
        .filter(|o| s.r.attr(**o, "selected").is_some())
        .map(|o| s.r.text_of(*o))
        .collect();
    assert_eq!(selected, ["0.000%"]);
    s.r.fire(list, &Event::Change("General".into()));
    assert_eq!(s.text(1, 2), "1.5");
    let selected: Vec<String> =
        s.r.find_by_tag("option")
            .iter()
            .filter(|o| s.r.attr(**o, "selected").is_some())
            .map(|o| s.r.text_of(*o))
            .filter(|t| !t.contains("en-US"))
            .collect();
    assert_eq!(selected, ["General"]);
    // Text and dates are not numbers with places.
    s.r.fire(list, &Event::Change("yyyy-mm-dd".into()));
    s.r.fire(button_labelled(&s, ".0+"), &Event::Click);
    assert_eq!(s.state.active_style().number_format, "yyyy-mm-dd");
}

#[quench_test::test]
fn each_selected_cell_steps_its_decimals_from_its_own_format() {
    let s = open(10);
    s.state.select_range(r("B2:C2")); // "item 1" in General, 1.50 under #,##0.00
    s.r.fire(button_labelled(&s, ".0+"), &Event::Click);
    assert_eq!(
        s.text(1, 2),
        "1.500",
        "#,##0.00 became #,##0.000 for the cell that had it"
    );
    s.state.select_range(r("C2:E2"));
    s.r.fire(button_labelled(&s, ".0-"), &Event::Click);
    assert_eq!(s.text(1, 2), "1.50");
    assert_eq!(
        s.text(1, 4),
        "1.5",
        "the running total went from two places to one"
    );
    assert_eq!(s.text(1, 3), "odd", "text is left alone");
}

#[quench_test::test]
fn formatting_a_whole_column_formats_the_cells_that_exist() {
    let s = open(20);
    let count = |s: &Screen| {
        s.state
            .with_engine(|e| e.workbook().sheets[0].styled().count())
    };
    let before = count(&s); // the demo already formats its price and total columns
    s.r.fire(s.r.find_by_attr("class", "colhdr")[1], &Event::Click); // column B
    s.r.fire(button_labelled(&s, "B"), &Event::Click);
    assert_eq!(
        count(&s) - before,
        21,
        "the header and the twenty rows, not a million cells"
    );
    assert!(s.class_of(0, 1).split(' ').any(|c| c == "b"));
    assert!(s.class_of(20, 1).split(' ').any(|c| c == "b"));
    // An empty block is formatted cell by cell.
    s.state.select_range(r("H30:I31"));
    s.r.fire(button_labelled(&s, "I"), &Event::Click);
    assert_eq!(count(&s) - before, 21 + 4);
}

#[quench_test::test]
fn clear_returns_to_the_plain_format_and_paste_carries_formats_along() {
    let s = open(10);
    s.state.select_range(r("B2:B2"));
    s.r.fire(button_labelled(&s, "B"), &Event::Click);
    s.r.fire(
        s.colour_input("Fill colour"),
        &Event::Change("#00ff00".into()),
    );
    s.state.select(CellRef::new(1, 1));
    let got = s.copy().unwrap();
    s.state.select(CellRef::new(8, 7));
    s.paste(&got.text);
    assert!(
        s.class_of(8, 7).split(' ').any(|c| c == "b"),
        "the bold came with the copy"
    );
    assert_eq!(
        s.r.style_of(s.cell(8, 7).unwrap(), "background-color")
            .as_deref(),
        Some("#00ff00")
    );
    s.state.select(CellRef::new(1, 1));
    s.r.fire(button_labelled(&s, "Clear"), &Event::Click);
    assert_eq!(s.class_of(1, 1), "cell");
    assert_eq!(
        s.r.style_of(s.cell(1, 1).unwrap(), "background-color"),
        None
    );
}

#[quench_test::test]
fn formats_go_into_the_saved_file_and_come_back_with_an_opened_one() {
    let manual = with_platform();
    let s = open(5);
    s.state.select_range(r("A1:B1"));
    s.r.fire(button_labelled(&s, "B"), &Event::Click);
    s.r.fire(button_labelled(&s, "Center"), &Event::Click);
    s.r.fire(
        s.colour_input("Fill colour"),
        &Event::Change("#ffcc00".into()),
    );
    choose_in(&s, "Save\u{2026}", "Excel workbook");
    let saved = manual.downloads()[0].2.clone();
    let back = sheets_xlsx::read(&saved).unwrap();
    let wb = back.engine.workbook();
    let style = &wb.styles[wb.sheets[0].style(CellRef::new(0, 1)) as usize];
    assert!(style.bold);
    assert_eq!(style.fill, Some(0xFF_CC00));
    assert_eq!(style.align, sheets_model::Align::Center);
    // Opening that file shows the formats.
    choose(&s, "New");
    choose(&s, "Open\u{2026}");
    manual.answer_pick(vec![picked("fmt.xlsx", saved)]);
    assert!(
        s.class_of(0, 0).contains(" b") && s.class_of(0, 0).contains(" ac"),
        "{}",
        s.class_of(0, 0)
    );
    assert_eq!(
        s.r.style_of(s.cell(0, 1).unwrap(), "background-color")
            .as_deref(),
        Some("#ffcc00")
    );
    assert_eq!(s.class_of(2, 0), "cell num", "other cells are plain");
}

fn blank() -> Screen {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let ((state, _), scope) = create_root(|| {
        let state = SheetState::new(sheets_model::Workbook::new());
        let mounted = mount(&r, &root, sheet_view(&state));
        (state, mounted)
    });
    Screen {
        r,
        state,
        _scope: scope,
    }
}

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

#[quench_test::test]
fn find_marks_cells_counts_them_and_steps_through_them() {
    let s = blank();
    for (c, t) in [
        ("A1", "apple"),
        ("B2", "Pineapple"),
        ("C3", "pear"),
        ("A5", "=\"apple\"&1"),
        ("D1", "42"),
    ] {
        s.state.set_text(at(c), t);
    }
    s.chord("f", true, false);
    assert!(s.state.find_open.get_untracked());
    assert!(
        !s.r.find_by_attr("role", "search").is_empty(),
        "the bar is on screen"
    );
    s.state.find_query.set("APPLE".to_string());
    assert_eq!(
        s.state.find_count(),
        Some(3),
        "the formula's own text matches too"
    );
    s.state.match_case.set(true);
    assert_eq!(s.state.find_count(), Some(0));
    s.state.match_case.set(false);
    s.state.whole_cell.set(true);
    assert_eq!(s.state.find_count(), Some(1));
    s.state.whole_cell.set(false);
    s.state.select(at("A1"));
    assert!(s.state.find_next(true));
    assert_eq!(s.state.selected.get_untracked(), at("B2"));
    s.state.find_next(true);
    assert_eq!(s.state.selected.get_untracked(), at("A5"));
    s.state.find_next(true);
    assert_eq!(s.state.selected.get_untracked(), at("A1"), "wraps round");
    s.state.find_next(false);
    assert_eq!(s.state.selected.get_untracked(), at("A5"), "and backwards");
    assert!(
        s.cells()
            .iter()
            .any(|n| s.r.attr(*n, "class").is_some_and(|c| c.contains("found")))
    );
    s.state.find_query.set("nothing like it".to_string());
    assert!(!s.state.find_next(true));
    s.key("Escape");
    assert!(!s.state.find_open.get_untracked());
}

#[quench_test::test]
fn replace_changes_one_cell_or_all_of_them_in_one_undo_step() {
    let s = blank();
    for (c, t) in [
        ("A1", "cat"),
        ("A2", "Concatenate cat cat"),
        ("A3", "=LEN(\"cat\")"),
        ("A4", "7"),
    ] {
        s.state.set_text(at(c), t);
    }
    s.state.find_query.set("cat".to_string());
    s.state.replace_with.set("dog".to_string());
    s.state.select(at("A1"));
    s.state.replace_current();
    assert_eq!(s.state.edit_text(at("A1")), "dog");
    assert_eq!(
        s.state.selected.get_untracked(),
        at("A2"),
        "goes on to the next match"
    );
    assert_eq!(s.state.replace_all(), 2);
    assert_eq!(s.state.edit_text(at("A2")), "Condogenate dog dog");
    assert_eq!(s.state.edit_text(at("A3")), "=LEN(\"dog\")");
    assert_eq!(s.state.display(at("A3")), "3");
    assert_eq!(s.state.edit_text(at("A4")), "7");
    s.state.undo();
    assert_eq!(s.state.edit_text(at("A2")), "Concatenate cat cat");
    assert_eq!(s.state.edit_text(at("A3")), "=LEN(\"cat\")");
    assert_eq!(
        s.state.edit_text(at("A1")),
        "dog",
        "one step undoes all the replacing"
    );
    s.state.whole_cell.set(true);
    s.state.find_query.set("7".to_string());
    s.state.replace_with.set("seven".to_string());
    s.state.replace_all();
    assert_eq!(s.state.display(at("A4")), "seven");
    s.state.find_query.set(String::new());
    assert_eq!(s.state.replace_all(), 0, "an empty search replaces nothing");
}

#[quench_test::test]
fn ctrl_arrows_jump_over_blocks_and_ctrl_end_finds_the_last_cell() {
    let s = blank();
    for r in 0..3 {
        s.state.set_text(CellRef::new(r, 0), "x");
    }
    s.state.set_text(CellRef::new(6, 0), "y");
    s.state.set_text(CellRef::new(8, 2), "z");
    s.state.select(CellRef::new(0, 0));
    s.chord("ArrowDown", true, false);
    assert_eq!(
        s.state.selected.get_untracked(),
        CellRef::new(2, 0),
        "to the end of the block"
    );
    s.chord("ArrowDown", true, false);
    assert_eq!(
        s.state.selected.get_untracked(),
        CellRef::new(6, 0),
        "across the gap"
    );
    s.chord("ArrowDown", true, false);
    assert_eq!(
        s.state.selected.get_untracked(),
        CellRef::new(1_048_575, 0),
        "to the edge"
    );
    s.chord("ArrowUp", true, false);
    assert_eq!(s.state.selected.get_untracked(), CellRef::new(6, 0));
    s.chord("ArrowRight", true, true);
    assert_eq!(
        s.state.selection().end.col,
        16_383,
        "shift stretches the selection"
    );
    s.chord("End", true, false);
    assert_eq!(s.state.selected.get_untracked(), CellRef::new(8, 2));
}

#[quench_test::test]
fn shortcuts_format_fill_and_open_the_find_bar() {
    let s = blank();
    s.state.set_text(at("A1"), "5");
    s.state.set_text(at("B1"), "=A1*2");
    s.state.select(at("A1"));
    s.chord("b", true, false);
    assert!(s.state.look(at("A1")).bold);
    s.chord("i", true, false);
    s.chord("u", true, false);
    let st = s.state.look(at("A1"));
    assert!(st.italic && st.underline);
    s.chord("b", true, false);
    assert!(!s.state.look(at("A1")).bold, "again takes it off");
    // Fill down and right.
    s.state.select(at("B1"));
    s.state.extend_to(at("B4"));
    s.chord("d", true, false);
    assert_eq!(
        s.state.edit_text(at("B3")),
        "=A3*2",
        "formulas move as in a copy"
    );
    s.state.set_text(at("A7"), "=A1+1");
    s.state.select(at("A7"));
    s.state.extend_to(at("C7"));
    s.chord("r", true, false);
    assert_eq!(s.state.edit_text(at("C7")), "=C1+1");
    assert_eq!(
        s.state.edit_text(at("A7")),
        "=A1+1",
        "the first cell is the source"
    );
    s.state.select(at("A1"));
    s.chord("h", true, false);
    assert!(s.state.replace_first.get_untracked());
    assert_eq!(
        s.state.find_query.get_untracked(),
        "5",
        "a single selected cell's text is the starting search"
    );
}

#[quench_test::test]
fn sheet_tabs_rename_and_delete_with_formulas_following() {
    let s = blank();
    s.state.add_sheet();
    s.state.switch_sheet(1);
    s.state.set_text(at("A1"), "9");
    s.state.switch_sheet(0);
    s.state.set_text(at("A1"), "=Sheet2!A1+1");
    assert_eq!(s.state.sheet_names(), ["Sheet1", "Sheet2"]);
    s.state.begin_rename(1);
    assert!(
        !s.r.find_by_attr("aria-label", "Sheet name").is_empty(),
        "a field takes the tab's place"
    );
    s.state.rename_draft.set("Budget 2027".to_string());
    s.state.commit_rename();
    assert_eq!(s.state.sheet_names(), ["Sheet1", "Budget 2027"]);
    assert_eq!(s.state.edit_text(at("A1")), "='Budget 2027'!A1+1");
    assert_eq!(s.state.display(at("A1")), "10");
    // Names that will not do keep the field open and say why.
    for (bad, why) in [
        ("sheet1", "already"),
        ("", "needs a name"),
        ("a/b", "cannot contain"),
        ("x".repeat(32).as_str(), "at most 31"),
    ] {
        s.state.begin_rename(1);
        s.state.rename_draft.set(bad.to_string());
        s.state.commit_rename();
        assert!(s.state.renaming.get_untracked().is_some(), "{bad:?}");
        assert!(
            s.state.status.get_untracked().contains(why),
            "{bad:?}: {}",
            s.state.status.get_untracked()
        );
        s.state.cancel_rename();
    }
    assert_eq!(s.state.sheet_names(), ["Sheet1", "Budget 2027"]);
    // Delete asks first.
    s.state.ask_delete(1);
    assert_eq!(s.state.pending_delete.get_untracked(), Some(1));
    s.state.cancel_delete();
    assert_eq!(s.state.sheet_names().len(), 2);
    s.state.switch_sheet(1);
    s.state.ask_delete(1);
    s.state.confirm_delete();
    assert_eq!(s.state.sheet_names(), ["Sheet1"]);
    assert_eq!(
        s.state.active.get_untracked(),
        0,
        "moves to a sheet that is left"
    );
    s.state.ask_delete(0);
    assert_eq!(
        s.state.pending_delete.get_untracked(),
        None,
        "the last sheet stays"
    );
}

#[quench_test::test]
fn the_clock_feeds_now_and_today_and_a_saved_file_comes_back() {
    let s = blank();
    s.state.set_clock(|| 45_351.75);
    s.state.set_text(at("A1"), "=TODAY()");
    s.state.set_text(at("A2"), "=NOW()-A1");
    assert_eq!(s.state.edit_text(at("A1")), "=TODAY()");
    assert_eq!(s.state.display(at("A2")), "0.75");
    s.state.set_text(at("A3"), "hello");
    let bytes = s.state.xlsx_bytes().unwrap();
    let t = blank();
    t.state.set_clock(|| 45_352.25);
    t.state.load_bytes("kept.xlsx", &bytes).unwrap();
    assert_eq!(t.state.name.get_untracked(), "kept.xlsx");
    assert_eq!(t.state.display(at("A3")), "hello");
    assert_eq!(
        t.state.display(at("A2")),
        "0.25",
        "volatile cells are calculated again at the new time"
    );
    assert!(t.state.load_bytes("bad.xlsx", b"nonsense").is_err());
}

#[quench_test::test]
fn the_file_menu_lists_save_and_export_as_submenus_of_formats() {
    let manual = with_platform();
    let s = blank();
    s.state.set_text(at("A1"), "alpha");
    s.state.set_text(at("B1"), "7");
    s.r.fire(button_labelled(&s, "File"), &Event::Click);
    let names = |s: &Screen| -> Vec<String> {
        s.r.find_by_attr("role", "menuitem")
            .into_iter()
            .map(|n| s.r.text_of(n))
            .collect()
    };
    assert_eq!(
        names(&s),
        [
            "New",
            "Open\u{2026}Ctrl+O",
            "Save\u{2026}\u{25b8}",
            "Export\u{2026}\u{25b8}",
            "Page setup\u{2026}",
            "Print\u{2026}Ctrl+P"
        ]
    );
    click_item(&s, "Save\u{2026}");
    let inner = names(&s);
    assert!(
        inner
            .iter()
            .any(|n| n.starts_with("Excel workbook (.xlsx)"))
    );
    assert!(
        inner.iter().any(|n| n.starts_with("CSV"))
            && inner
                .iter()
                .any(|n| n.starts_with("Tab separated (.tsv)") || n.starts_with("Tab separated"))
    );
    click_item(&s, "Save\u{2026}"); // again folds it
    assert_eq!(names(&s).len(), 6);
    s.r.fire(
        s.r.find_by_attr("role", "menu")[0],
        &Event::KeyDown(KeyEvent::new("Escape")),
    );
    assert!(
        s.r.find_by_attr("role", "menu").is_empty(),
        "escape closes it all"
    );

    choose_in(&s, "Save\u{2026}", "Tab separated");
    choose_in(&s, "Export\u{2026}", "PDF");
    let seen = manual.downloads();
    assert_eq!(seen[0].0, "Untitled.tsv");
    assert_eq!(String::from_utf8(seen[0].2.clone()).unwrap(), "alpha\t7\n");
    assert_eq!(seen[1].0, "Untitled.pdf");
    assert_eq!(seen[1].1, "application/pdf");
    assert!(seen[1].2.starts_with(b"%PDF-"));
    assert_eq!(
        s.r.text_of(s.one("role", "status")),
        "Exported Untitled.pdf"
    );
}

#[quench_test::test]
fn print_shows_the_pages_in_a_new_tab_and_ctrl_p_does_the_same() {
    let manual = with_platform();
    let s = blank();
    s.state.set_text(at("A1"), "printed");
    choose(&s, "Print\u{2026}");
    s.state.add_sheet();
    s.state.switch_sheet(1);
    s.state.set_text(at("A1"), "second");
    s.chord("p", true, false);
    let seen = manual.downloads();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|d| d.0.is_empty() && d.1 == "application/pdf"),
        "no name: shown, not saved"
    );
    assert!(inflated(&seen[0].2).contains("printed"));
    assert!(
        inflated(&seen[1].2).contains("second"),
        "the sheet on screen"
    );
    assert!(
        s.r.text_of(s.one("role", "status"))
            .contains("print them from there")
    );
}

#[quench_test::test]
fn rows_and_columns_are_inserted_and_deleted_with_formulas_following_and_undo() {
    let s = blank();
    for (c, t) in [("A1", "1"), ("A2", "2"), ("A3", "3"), ("B1", "=SUM(A1:A3)")] {
        s.state.set_text(at(c), t);
    }
    s.state.select(at("A2"));
    s.state.extend_to(at("A3")); // two rows
    s.state.insert_selected(sheets_model::Axis::Row);
    assert_eq!(s.state.edit_text(at("A4")), "2");
    assert_eq!(s.state.edit_text(at("A2")), "");
    assert_eq!(s.state.edit_text(at("B1")), "=SUM(A1:A5)");
    assert_eq!(s.state.display(at("B1")), "6");
    assert_eq!(s.r.text_of(s.one("role", "status")), "Inserted 2 rows");
    s.state.undo();
    assert_eq!(s.state.edit_text(at("A2")), "2");
    assert_eq!(s.state.edit_text(at("B1")), "=SUM(A1:A3)");
    s.state.redo();
    assert_eq!(s.state.edit_text(at("A4")), "2");
    s.state.undo();
    // Delete the middle row, then the column the sum is in.
    s.state.select(at("A2"));
    s.state.delete_selected(sheets_model::Axis::Row);
    assert_eq!(s.state.edit_text(at("B1")), "=SUM(A1:A2)");
    assert_eq!(s.state.display(at("B1")), "4");
    s.state.undo();
    assert_eq!(
        s.state.display(at("B1")),
        "6",
        "undo brings back the row and the range"
    );
    s.state.select(at("B1"));
    s.state.insert_selected(sheets_model::Axis::Col);
    assert_eq!(s.state.edit_text(at("C1")), "=SUM(A1:A3)");
    s.state.undo();
    // The shortcuts pick rows, or columns when whole columns are selected.
    s.state.select(at("A1"));
    s.chord("-", true, false);
    assert_eq!(s.state.edit_text(at("A1")), "2", "Ctrl+- deletes the row");
    s.state.undo();
    s.chord("+", true, true);
    assert_eq!(s.state.edit_text(at("A2")), "1", "Ctrl++ inserts one");
    s.state.undo();
    // The Cells menus offer all four.
    s.r.fire(button_labelled(&s, "Insert"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 2);
}

#[quench_test::test]
fn borders_go_around_the_selection_or_inside_it_and_show_on_the_cells() {
    use sheets_model::LineKind;
    let s = blank();
    for c in ["A1", "B1", "A2", "B2"] {
        s.state.set_text(at(c), "x");
    }
    s.state.select(at("A1"));
    s.state.extend_to(at("B2"));
    s.state.set_borders(BorderMode::Outside);
    let b = |c: &str| s.state.look(at(c)).border;
    assert!(b("A1").top.is_some() && b("A1").left.is_some());
    assert!(
        b("A1").bottom.is_none() && b("A1").right.is_none(),
        "inside edges stay open"
    );
    assert!(b("B2").bottom.is_some() && b("B2").right.is_some() && b("B2").top.is_none());
    // The element carries the lines as CSS.
    let node = s.cell(0, 0).unwrap();
    assert_eq!(
        s.r.style_of(node, "border-color").as_deref(),
        Some("var(--office-fg) var(--office-grid) var(--office-grid) var(--office-fg)"),
        "top and left drawn, the open sides keep the grid line"
    );
    s.state.set_borders(BorderMode::All);
    assert!(
        b("B1").top.is_some()
            && b("B1").bottom.is_some()
            && b("B1").left.is_some()
            && b("B1").right.is_some()
    );
    s.state.set_borders(BorderMode::ThickOutside);
    assert_eq!(b("A1").top.unwrap().kind, LineKind::Thick);
    assert_eq!(
        b("A1").bottom.unwrap().kind,
        LineKind::Thin,
        "the inside stays as it was"
    );
    s.state.select(at("A2"));
    s.state.set_borders(BorderMode::DoubleBottom);
    assert_eq!(b("A2").bottom.unwrap().kind, LineKind::Double);
    s.state.set_borders(BorderMode::None);
    assert!(b("A2").is_none());
    s.state.undo();
    assert_eq!(
        b("A2").bottom.unwrap().kind,
        LineKind::Double,
        "one undo step each"
    );
    // The menu offers them.
    s.r.fire(button_labelled(&s, "Borders"), &Event::Click);
    assert!(s.r.find_by_attr("role", "menuitem").len() >= 9);
}

#[quench_test::test]
fn font_size_and_family_apply_to_the_selection_and_rows_grow_to_fit() {
    let s = blank();
    s.state.set_text(at("A1"), "big");
    s.state.set_text(at("A2"), "small");
    s.state.select(at("A1"));
    s.state.set_font_size(Some(48));
    s.state.set_font_name(Some("Courier New"));
    let look = s.state.look(at("A1"));
    assert_eq!(
        (look.size, look.font.as_deref()),
        (Some(48), Some("Courier New"))
    );
    let node = s.cell(0, 0).unwrap();
    assert_eq!(s.r.style_of(node, "font-size").as_deref(), Some("32px"));
    assert!(
        s.r.style_of(node, "font-family")
            .unwrap()
            .starts_with("'Courier New', 'Liberation Mono'")
    );
    let grown = s.state.axes.get().rows.size(0);
    assert!(grown > ROW_H, "row 1 grew to {grown}");
    assert!(
        (s.state.axes.get().rows.size(1) - ROW_H).abs() < f64::EPSILON,
        "other rows did not"
    );
    assert_eq!(s.state.look(at("A2")).size, None);
    s.state.set_font_size(None);
    s.state.set_font_name(None);
    assert_eq!(s.state.look(at("A1")).font, None);
    // The lists are on the ribbon.
    assert_eq!(s.r.find_by_attr("class", "format-list font-list").len(), 1);
    assert_eq!(s.r.find_by_attr("aria-label", "Font size").len(), 1);
}

fn count_class(s: &Screen, class: &str) -> usize {
    s.r.find_by_attr("class", class).len()
}

#[quench_test::test]
fn merged_cells_show_as_one_select_as_one_and_undo() {
    let s = blank();
    for (c, t) in [
        ("A1", "keep"),
        ("B1", "lost"),
        ("A2", "gone"),
        ("B2", "also"),
    ] {
        s.state.set_text(at(c), t);
    }
    s.state.select(at("A1"));
    s.state.extend_to(at("B2"));
    s.state.merge_selection();
    assert_eq!(s.state.edit_text(at("A1")), "keep");
    assert_eq!(
        s.state.edit_text(at("B1")),
        "",
        "what the other cells held is dropped"
    );
    assert_eq!(
        s.state.merge_span(at("B2")),
        Some(sheets_model::Range::parse("A1:B2").unwrap())
    );
    assert_eq!(count_class(&s, "merged"), 1, "one drawn over its cells");
    // Clicking any part selects the whole.
    s.state.select(at("B2"));
    assert_eq!(
        (
            s.state.selected.get_untracked(),
            s.state.range_end.get_untracked()
        ),
        (at("A1"), at("B2"))
    );
    s.state.move_by(0, 1);
    assert_eq!(
        s.state.selected.get_untracked(),
        at("C1"),
        "moving right leaves the merged cell"
    );
    s.state.select(at("A1"));
    s.state.move_by(1, 0);
    assert_eq!(
        s.state.selected.get_untracked(),
        at("A3"),
        "and moving down"
    );
    s.state.undo();
    assert!(s.state.merge_span(at("A1")).is_none());
    assert_eq!(
        s.state.edit_text(at("B1")),
        "lost",
        "undo brings the cells back"
    );
    assert_eq!(count_class(&s, "merged"), 0);
    s.state.redo();
    s.state.select(at("A1"));
    s.state.unmerge_selection();
    assert!(s.state.merge_span(at("A1")).is_none());
    // A merge follows rows inserted above it (it is two rows tall, so two are inserted).
    s.state.undo();
    s.state.select(at("A1"));
    s.state.insert_selected(sheets_model::Axis::Row);
    assert_eq!(
        s.state.merge_span(at("A3")),
        Some(sheets_model::Range::parse("A3:B4").unwrap())
    );
}

#[quench_test::test]
fn hidden_rows_and_columns_take_no_room_and_are_stepped_over() {
    let s = blank();
    s.state.select(at("B2"));
    s.state.hide_selected(sheets_model::Axis::Row);
    let axes = s.state.axes.get();
    assert!(axes.rows.size(1) == 0.0 && (axes.rows.size(2) - ROW_H).abs() < f64::EPSILON);
    assert_eq!(axes.rows.offset(2) - axes.rows.offset(1), 0.0);
    assert_ne!(
        s.state.selected.get_untracked(),
        at("B2"),
        "the cursor left the hidden row"
    );
    s.state.select(at("B1"));
    s.state.move_by(1, 0);
    assert_eq!(
        s.state.selected.get_untracked(),
        at("B3"),
        "down steps over the hidden row"
    );
    s.state.move_by(-1, 0);
    assert_eq!(s.state.selected.get_untracked(), at("B1"));
    s.state.select(at("C1"));
    s.state.hide_selected(sheets_model::Axis::Col);
    assert_eq!(s.state.axes.get().cols.size(2), 0.0);
    s.state.select(at("B1"));
    s.state.move_by(0, 1);
    assert_eq!(s.state.selected.get_untracked(), at("D1"));
    // Showing: select around and unhide.
    s.state.select(at("A1"));
    s.state.extend_to(at("A3"));
    s.state.unhide_around(sheets_model::Axis::Row);
    assert!((s.state.axes.get().rows.size(1) - ROW_H).abs() < f64::EPSILON);
    s.state.undo();
    assert!(
        s.state.axes.get().rows.size(1) == 0.0,
        "undo hides it again"
    );
    // The same rows hide whole selections and the last ones cannot go.
    s.state.select(at("A1"));
    s.state.select_all();
    s.state.hide_selected(sheets_model::Axis::Row);
    assert!(
        s.state.axes.get().rows.size(0) > 0.0,
        "not every row can be hidden"
    );
}

#[quench_test::test]
fn frozen_panes_stay_while_the_rest_scrolls() {
    let s = open(50);
    assert_eq!(
        count_class(&s, "pane pane-top") + count_class(&s, "pane pane-left"),
        0
    );
    s.state.select(at("C3"));
    s.state.freeze_at_cursor();
    assert_eq!(s.state.axes.get().frozen, (2, 2));
    for class in ["pane pane-top", "pane pane-left", "pane pane-corner"] {
        assert_eq!(count_class(&s, class), 1, "{class}");
    }
    assert_eq!(
        count_class(&s, "hdr-window pinned"),
        2,
        "the first headers are pinned too"
    );
    // The frozen cells are in the panes as well as under them.
    let first: Vec<_> = s
        .cells()
        .into_iter()
        .filter(|n| {
            s.r.attr(*n, "aria-rowindex").as_deref() == Some("1")
                && s.r.attr(*n, "aria-colindex").as_deref() == Some("1")
        })
        .collect();
    assert!(
        first.len() >= 3,
        "A1 in the main grid, the top pane and the corner: {}",
        first.len()
    );
    s.scroll(2000.0, 0.0);
    assert!(s.cells().len() > 20);
    let top_rows: std::collections::BTreeSet<_> =
        s.r.find_by_attr("class", "pane-box top")
            .iter()
            .flat_map(|b| s.r.children(*b))
            .collect();
    assert!(!top_rows.is_empty());
    s.state.freeze(1, 0);
    assert_eq!(count_class(&s, "pane pane-left"), 0);
    assert_eq!(count_class(&s, "pane pane-top"), 1);
    s.state.undo();
    assert_eq!(s.state.axes.get().frozen, (2, 2));
    s.state.freeze(0, 0);
    assert_eq!(count_class(&s, "pane pane-top"), 0);
    // And through the menu.
    s.r.fire(button_labelled(&s, "Freeze"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 4);
}

#[quench_test::test]
fn a_file_dropped_on_the_page_is_opened() {
    let _manual = with_platform();
    let s = blank();
    s.state.accept_drops();
    quench_ui::platform::deliver_drop(vec![quench_ui::platform::PickedFile {
        name: "dropped.csv".into(),
        mime: "text/csv".into(),
        bytes: b"a,b\n1,2\n".to_vec(),
    }]);
    assert_eq!(s.state.name.get_untracked(), "dropped.csv");
    assert_eq!(s.state.display(at("B2")), "2");
    assert_eq!(s.r.text_of(s.one("role", "status")), "Opened dropped.csv");
}

#[quench_test::test]
fn wide_numbers_show_hashes_text_borrows_empty_neighbours_and_wrapped_rows_grow() {
    let s = blank();
    s.state.set_text(at("A1"), "123456789012");
    s.state.select(at("A1"));
    s.state.set_number_format("0.000000000");
    s.state
        .set_text(at("A2"), "a long label that is wider than its cell");
    s.state
        .set_text(at("A3"), "another long label that is wider than its cell");
    s.state.set_text(at("B3"), "blocks");
    s.state.set_text(at("A4"), "short");
    let look = |c: &str| s.state.look(at(c));
    assert!(
        look("A1").text.chars().all(|c| c == '#') && !look("A1").text.is_empty(),
        "{}",
        look("A1").text
    );
    assert_eq!(
        s.state.display(at("A1")),
        "123456789012.000000000",
        "the value is still all there"
    );
    assert!(
        look("A2").spill > 100,
        "free cells to the right are used: {}",
        look("A2").spill
    );
    assert!(look("A2").class().contains("spill"));
    assert!(
        look("A3").spill == 0 || look("A3").spill < 4,
        "a cell with something in it stops the text"
    );
    assert_eq!(look("A4").spill, 0);
    let node = s.cell(1, 0).unwrap();
    assert!(
        s.r.style_of(node, "clip-path")
            .unwrap()
            .starts_with("inset(0 -")
    );
    // Widening the column shows the number.
    s.state.set_text(at("C1"), "x");
    // Wrapped text makes its row taller; narrower columns make it taller still.
    s.state.select(at("A2"));
    let before = s.state.axes.get().rows.size(1);
    s.state.toggle_wrap();
    let after = s.state.axes.get().rows.size(1);
    assert!(after > before, "{before} -> {after}");
    assert!(
        (s.state.axes.get().rows.size(0) - ROW_H).abs() < f64::EPSILON,
        "other rows stay"
    );
    s.state.toggle_wrap();
    assert!(
        (s.state.axes.get().rows.size(1) - ROW_H).abs() < f64::EPSILON,
        "and it goes back"
    );
}

fn table(s: &Screen) {
    for (r, (name, qty)) in [
        ("Name", "Qty"),
        ("pear", "3"),
        ("Apple", "10"),
        ("fig", "2"),
        ("apple", "7"),
    ]
    .into_iter()
    .enumerate()
    {
        s.state
            .set_text(CellRef::new(u32::try_from(r).unwrap(), 0), name);
        s.state
            .set_text(CellRef::new(u32::try_from(r).unwrap(), 1), qty);
    }
    s.state.set_text(at("C2"), "=B2*2");
    s.state.set_text(at("C3"), "=B3*2");
    s.state.set_text(at("C4"), "=B4*2");
    s.state.set_text(at("C5"), "=B5*2");
}

#[quench_test::test]
fn sorting_keeps_the_heading_moves_whole_rows_and_formulas_follow() {
    let s = blank();
    table(&s);
    s.state.select(at("B3"));
    s.state.sort_selection(true);
    let col = |c: u32| {
        (0..5)
            .map(|r| s.state.edit_text(CellRef::new(r, c)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        col(1),
        ["Qty", "2", "3", "7", "10"],
        "numbers ascend under the heading"
    );
    assert_eq!(col(0), ["Name", "fig", "pear", "apple", "Apple"]);
    assert_eq!(
        col(2),
        ["", "=B2*2", "=B3*2", "=B4*2", "=B5*2"],
        "formulas still read their own row"
    );
    assert_eq!(s.state.display(at("C5")), "20");
    s.state.sort_selection(false);
    assert_eq!(col(1), ["Qty", "10", "7", "3", "2"]);
    s.state.undo();
    assert_eq!(col(1), ["Qty", "2", "3", "7", "10"], "one undo step");
    // By the text column, case ignored, equal keys keep their order.
    s.state.select(at("A2"));
    s.state.sort_selection(true);
    assert_eq!(col(0), ["Name", "apple", "Apple", "fig", "pear"]);
}

#[quench_test::test]
fn a_filter_hides_rows_by_the_values_ticked_and_shows_them_again() {
    let s = blank();
    table(&s);
    s.state.select(at("A1"));
    s.state.toggle_filter();
    let f = s.state.autofilter().unwrap();
    assert_eq!(f.range, sheets_model::Range::parse("A1:C5").unwrap());
    assert_eq!(count_class(&s, "filter-btn"), 3, "an arrow on each heading");
    // Open the list for the first column: its values are listed, all ticked.
    s.r.fire(s.r.find_by_attr("class", "filter-btn")[0], &Event::Click);
    assert_eq!(s.state.filter_open.get_untracked(), Some(0));
    let (values, more) = s.state.filter_values(0);
    assert_eq!(values, ["Apple", "apple", "fig", "pear"], "{more}");
    assert_eq!(s.r.find_by_attr("class", "check filter-item").len(), 4);
    s.state.toggle_filter_value("fig");
    s.state.toggle_filter_value("pear");
    s.state.apply_filter();
    assert!(s.state.filter_open.get_untracked().is_none());
    let axes = s.state.axes.get();
    let rows = &axes.rows;
    assert_eq!(
        (
            rows.size(1) == 0.0,
            rows.size(2) > 0.0,
            rows.size(3) == 0.0,
            rows.size(4) > 0.0
        ),
        (true, true, true, true)
    );
    assert_eq!(s.r.text_of(s.one("role", "status")), "");
    assert_eq!(
        count_class(&s, "filter-btn on"),
        1,
        "the filtered column is marked"
    );
    // A second column narrows it more.
    s.state
        .set_filter_column(1, Some(std::iter::once("10".to_string()).collect()));
    assert!(s.state.axes.get().rows.size(4) == 0.0 && s.state.axes.get().rows.size(2) > 0.0);
    s.state.undo();
    assert!(s.state.axes.get().rows.size(4) > 0.0, "undo");
    s.state.clear_filters();
    assert!((1..5).all(|r| s.state.axes.get().rows.size(r) > 0.0));
    s.state.toggle_filter();
    assert!(s.state.autofilter().is_none());
    assert_eq!(count_class(&s, "filter-btn"), 0);
    // The menus are there.
    s.r.fire(button_labelled(&s, "Sort"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 2);
}

fn fill(s: &Screen, from: &str, to: &str) -> sheets_model::Range {
    s.state
        .select_range(sheets_model::Range::parse(from).unwrap());
    s.state
        .autofill(sheets_model::Range::parse(from).unwrap(), at(to))
        .unwrap()
}

#[quench_test::test]
fn autofill_continues_numbers_dates_names_and_formulas() {
    let s = blank();
    for (c, t) in [
        ("A1", "1"),
        ("A2", "3"),
        ("B1", "Mon"),
        ("C1", "Jan"),
        ("D1", "Item 7"),
        ("E1", "x"),
        ("E2", "y"),
        ("F1", "5"),
        ("G1", "=A1*2"),
    ] {
        s.state.set_text(at(c), t);
    }
    let col = |c: &str, rows: u32| {
        (0..rows)
            .map(|r| {
                s.state
                    .edit_text(CellRef::new(r, sheets_model::col_index(c).unwrap()))
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fill(&s, "A1:A2", "A5"),
        sheets_model::Range::parse("A1:A5").unwrap()
    );
    assert_eq!(
        col("A", 5),
        ["1", "3", "5", "7", "9"],
        "a step from the first two numbers"
    );
    fill(&s, "B1:B1", "B8");
    assert_eq!(
        col("B", 8),
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun", "Mon"]
    );
    fill(&s, "C1:C1", "C3");
    assert_eq!(col("C", 3), ["Jan", "Feb", "Mar"]);
    fill(&s, "D1:D1", "D3");
    assert_eq!(col("D", 3), ["Item 7", "Item 8", "Item 9"]);
    fill(&s, "E1:E2", "E6");
    assert_eq!(
        col("E", 6),
        ["x", "y", "x", "y", "x", "y"],
        "a pattern repeats"
    );
    fill(&s, "F1:F1", "F3");
    assert_eq!(col("F", 3), ["5", "5", "5"], "one number is copied");
    fill(&s, "G1:G1", "G3");
    assert_eq!(
        col("G", 3),
        ["=A1*2", "=A2*2", "=A3*2"],
        "formulas move as in a copy"
    );
    s.state.undo();
    assert_eq!(
        col("G", 3),
        ["=A1*2", "", ""],
        "one undo step for the whole fill"
    );
    // Upwards, and to the right.
    s.state.set_text(at("A10"), "4");
    s.state.set_text(at("A11"), "5");
    s.state.set_text(at("H1"), "Q3");
    assert_eq!(
        fill(&s, "A10:A11", "A8"),
        sheets_model::Range::parse("A8:A11").unwrap()
    );
    assert_eq!(
        col("A", 11)[7..].to_vec(),
        ["2", "3", "4", "5"],
        "up runs the series backwards"
    );
    fill(&s, "H1:H1", "L1");
    let row: Vec<String> = ["H", "I", "J", "K", "L"]
        .iter()
        .map(|c| {
            s.state
                .edit_text(CellRef::new(0, sheets_model::col_index(c).unwrap()))
        })
        .collect();
    assert_eq!(row, ["Q3", "Q4", "Q5", "Q6", "Q7"]);
    s.state
        .select_range(sheets_model::Range::parse("A1:A2").unwrap());
    assert!(
        s.state
            .autofill(sheets_model::Range::parse("A1:A2").unwrap(), at("A2"))
            .is_none(),
        "inside the source: nothing"
    );
}

#[quench_test::test]
fn dragging_the_fill_handle_fills_where_the_pointer_goes() {
    let s = blank();
    s.state.set_text(at("A1"), "1");
    s.state.set_text(at("A2"), "2");
    s.state
        .select_range(sheets_model::Range::parse("A1:A2").unwrap());
    let handle = s.r.find_by_attr("class", "fill-handle");
    assert!(
        !handle.is_empty(),
        "the corner of the selection has a handle"
    );
    s.r.fire(handle[0], &pointer());
    s.over(5, 0);
    assert_eq!(
        count_class(&s, "fill-preview"),
        1,
        "a preview of where it would fill"
    );
    s.release();
    assert_eq!(s.state.edit_text(at("A6")), "6");
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("A1:A6").unwrap()
    );
    assert_eq!(count_class(&s, "fill-preview"), 0);
    // Dragging that goes nowhere fills nothing.
    s.r.fire(s.r.find_by_attr("class", "fill-handle")[0], &pointer());
    s.release();
    assert_eq!(s.state.edit_text(at("A7")), "");
}

fn fill_dialog(s: &Screen, id: DialogId, values: &[&str]) {
    s.state.open_dialog(id);
    assert_eq!(count_class(s, "dialog"), 1, "the form is on screen");
    for (i, v) in values.iter().enumerate() {
        if !v.is_empty() {
            s.state.set_field(i, v);
        }
    }
    s.state.submit_dialog();
}

#[quench_test::test]
fn a_drop_down_list_rule_refuses_other_entries_and_offers_its_items() {
    let s = blank();
    s.state
        .select_range(sheets_model::Range::parse("B2:B4").unwrap());
    fill_dialog(&s, DialogId::ValidList, &["red, green , blue"]);
    assert!(s.state.dialog.get_untracked().is_none(), "the form closed");
    s.state.select(at("B3"));
    assert_eq!(s.state.dropdown_items().unwrap(), ["red", "green", "blue"]);
    s.state.set_text(at("B3"), "purple");
    assert_eq!(
        s.state.edit_text(at("B3")),
        "",
        "refused entries are taken back"
    );
    assert!(
        s.r.text_of(s.one("role", "status"))
            .starts_with("Not entered.")
    );
    s.state.set_text(at("B3"), "GREEN");
    assert_eq!(s.state.edit_text(at("B3")), "GREEN", "case does not matter");
    // The arrow and the list in the cell.
    assert_eq!(count_class(&s, "dv-btn"), 1);
    s.r.fire(s.r.find_by_attr("class", "dv-btn")[0], &Event::Click);
    let items = s.r.find_by_attr("class", "menu-item dv-item");
    assert_eq!(items.len(), 3);
    s.r.fire(items[2], &Event::Click);
    assert_eq!(s.state.edit_text(at("B3")), "blue");
    assert_eq!(count_class(&s, "dv-list"), 0, "choosing closes the list");
    // Outside the range there is no rule; clearing removes it.
    s.state.set_text(at("C1"), "anything");
    assert_eq!(s.state.edit_text(at("C1")), "anything");
    s.state
        .select_range(sheets_model::Range::parse("B2:B4").unwrap());
    s.state.clear_validation();
    s.state.set_text(at("B2"), "purple");
    assert_eq!(s.state.edit_text(at("B2")), "purple");
    s.state.undo();
    s.state.undo();
    s.state.undo();
    // A list from cells.
    s.state.set_text(at("E1"), "yes");
    s.state.set_text(at("E2"), "no");
    s.state.select(at("D1"));
    fill_dialog(&s, DialogId::ValidList, &["$E$1:$E$2"]);
    assert_eq!(s.state.dropdown_items().unwrap(), ["yes", "no"]);
    s.state.set_text(at("D1"), "maybe");
    assert_eq!(s.state.edit_text(at("D1")), "");
}

#[quench_test::test]
fn number_rules_check_whole_numbers_ranges_lengths_and_formulas_and_can_only_warn() {
    let s = blank();
    s.state.select(at("A1"));
    fill_dialog(
        &s,
        DialogId::ValidNumber,
        &["whole", "between", "1", "10", "stop", "1 to 10 please"],
    );
    for (text, kept) in [
        ("5", true),
        ("10", true),
        ("11", false),
        ("2.5", false),
        ("abc", false),
        ("0", false),
    ] {
        s.state.set_text(at("A1"), "");
        s.state.set_text(at("A1"), text);
        assert_eq!(s.state.edit_text(at("A1")) == text, kept, "{text}");
    }
    assert!(
        s.r.text_of(s.one("role", "status"))
            .contains("1 to 10 please")
    );
    // A text length rule that only warns keeps the entry.
    s.state.select(at("B1"));
    fill_dialog(
        &s,
        DialogId::ValidNumber,
        &[
            "length",
            "lessThanOrEqual",
            "3",
            "",
            "warning",
            "Short is better",
        ],
    );
    s.state.set_text(at("B1"), "toolong");
    assert_eq!(s.state.edit_text(at("B1")), "toolong");
    assert!(
        s.r.text_of(s.one("role", "status"))
            .contains("Short is better")
    );
    // A formula rule.
    s.state
        .select_range(sheets_model::Range::parse("C1:C3").unwrap());
    fill_dialog(
        &s,
        DialogId::ValidCustom,
        &["=COUNTIF($C$1:$C$3,C1)=0", "No repeats"],
    );
    s.state.set_text(at("C1"), "x");
    s.state.set_text(at("C2"), "y");
    s.state.set_text(at("C3"), "x");
    assert_eq!(s.state.edit_text(at("C3")), "", "a repeat is refused");
    // Mistakes in the form keep it open and say what is wrong.
    s.state.open_dialog(DialogId::ValidNumber);
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("Enter the value")
    );
    assert_eq!(
        s.r.find_by_attr("role", "alert").len(),
        1,
        "the message is on screen"
    );
    s.state.cancel_dialog();
    assert!(s.state.dialog.get_untracked().is_none());
}

#[quench_test::test]
fn conditional_formats_colour_cells_scales_and_bars_follow_the_numbers() {
    let s = blank();
    for (r, v) in [10, 50, 150, 200, 90].into_iter().enumerate() {
        s.state
            .set_text(CellRef::new(u32::try_from(r).unwrap(), 0), &v.to_string());
    }
    s.state
        .select_range(sheets_model::Range::parse("A1:A5").unwrap());
    fill_dialog(
        &s,
        DialogId::CondCompare,
        &["greaterThan", "100", "", "#ffc7ce", "#9c0006", "1"],
    );
    let look = |c: &str| s.state.look(at(c));
    assert_eq!(
        (look("A3").fill, look("A3").color, look("A3").bold),
        (Some(0x00FF_C7CE), Some(0x009C_0006), true)
    );
    assert_eq!(look("A1").fill, None);
    s.state.set_text(at("A1"), "300");
    assert_eq!(look("A1").fill, Some(0x00FF_C7CE), "follows the values");
    let node = s.cell(2, 0).unwrap();
    assert_eq!(
        s.r.style_of(node, "background-color").as_deref(),
        Some("#ffc7ce")
    );
    // The top three, and a formula, and one of each quick kind.
    s.state
        .select_range(sheets_model::Range::parse("A1:A5").unwrap());
    s.state.clear_cond(false);
    assert_eq!(look("A3").fill, None);
    fill_dialog(
        &s,
        DialogId::CondTop,
        &["top", "2", "", "#00ff00", "#000000", ""],
    );
    let marked: Vec<bool> = (1..=5)
        .map(|r| look(&format!("A{r}")).fill.is_some())
        .collect();
    assert_eq!(
        marked,
        [true, false, false, true, false],
        "300 and 200 are the largest two"
    );
    s.state.clear_cond(true);
    fill_dialog(
        &s,
        DialogId::CondExpr,
        &["=$A1>=150", "#0000ff", "#ffffff", ""],
    );
    assert!(
        look("A1").fill == Some(0x0000_00FF)
            && look("A2").fill.is_none()
            && look("A4").fill.is_some()
    );
    s.state.clear_cond(true);
    s.state
        .quick_cond(sheets_model::Cond::AboveAverage { below: false });
    assert_eq!(
        marked_rows(&s),
        [true, false, false, true, false],
        "the average is 158"
    );
    s.state.clear_cond(true);
    // A colour scale shades from the smallest to the largest; a data bar fills by the size.
    fill_dialog(
        &s,
        DialogId::CondScale,
        &["2", "#000000", "#000000", "#ffffff"],
    );
    assert_eq!(
        look("A2").fill,
        Some(0),
        "the smallest value (50) takes the first colour"
    );
    assert_eq!(
        look("A1").fill,
        Some(0x00FF_FFFF),
        "the largest (300) the last"
    );
    assert!(look("A5").fill.unwrap() > 0 && look("A5").fill.unwrap() < 0x00FF_FFFF);
    s.state.clear_cond(true);
    fill_dialog(&s, DialogId::CondBar, &["#638ec6"]);
    assert_eq!(look("A1").bar, Some((100, 0x0063_8EC6)));
    assert_eq!(look("A4").bar.unwrap().0, 67, "200 of 300");
    assert_eq!(look("A1").fill, None, "a bar is not a fill");
    let bar_node = s.cell(3, 0).unwrap();
    assert!(
        s.r.style_of(bar_node, "background-image")
            .unwrap()
            .contains("linear-gradient")
    );
}

fn marked_rows(s: &Screen) -> Vec<bool> {
    (1..=5)
        .map(|r| s.state.look(at(&format!("A{r}"))).fill.is_some())
        .collect()
}

#[quench_test::test]
fn names_can_be_defined_used_and_deleted_and_notes_can_be_added() {
    let s = blank();
    for r in 0..3 {
        s.state.set_text(CellRef::new(r, 0), &(r + 1).to_string());
    }
    s.state
        .select_range(sheets_model::Range::parse("A1:A3").unwrap());
    assert_eq!(s.state.selection_reference(), "Sheet1!$A$1:$A$3");
    s.state.open_dialog(DialogId::DefineName);
    assert_eq!(
        s.state.dialog.get_untracked().unwrap().fields[1].value,
        "Sheet1!$A$1:$A$3",
        "refers to the selection"
    );
    s.state.set_field(0, "Sales");
    s.state.submit_dialog();
    s.state.set_text(at("C1"), "=SUM(Sales)*2");
    assert_eq!(s.state.display(at("C1")), "12");
    // A bad name keeps the form open.
    s.state.open_dialog(DialogId::DefineName);
    s.state.set_field(0, "A1");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("starts with a letter")
    );
    s.state.cancel_dialog();
    fill_dialog(&s, DialogId::DeleteName, &["Sales"]);
    assert_eq!(s.state.display(at("C1")), "#NAME?");
    // Notes.
    s.state.select(at("B2"));
    fill_dialog(&s, DialogId::Comment, &["Check this later"]);
    assert_eq!(
        s.state.look(at("B2")).note.as_deref(),
        Some("Check this later")
    );
    assert!(s.state.look(at("B2")).class().contains("note"));
    assert_eq!(
        s.r.attr(s.cell(1, 1).unwrap(), "title").as_deref(),
        Some("Check this later")
    );
    s.state.undo();
    assert!(s.state.look(at("B2")).note.is_none(), "undo removes it");
    s.state.redo();
    fill_dialog(&s, DialogId::Comment, &[" "]);
    assert!(
        s.state.look(at("B2")).note.is_none(),
        "empty text removes it"
    );
}

#[quench_test::test]
fn typing_a_formula_suggests_functions_and_names_and_shows_the_arguments() {
    let s = blank();
    s.state.set_text(at("A1"), "1");
    s.state
        .select_range(sheets_model::Range::parse("A1:A1").unwrap());
    s.state.open_dialog(DialogId::DefineName);
    s.state.set_field(0, "Sales2027");
    s.state.submit_dialog();
    s.state.select(at("C1"));
    s.state.begin_edit(Some(""));
    let type_text = |t: &str| {
        s.r.fire(s.one("class", "formula"), &Event::Input(t.to_string()))
    };
    type_text("=sum");
    assert_eq!(
        s.state.suggestion_list(),
        ["SUMIF", "SUMIFS", "SUMPRODUCT", "SUMSQ"]
    );
    assert_eq!(s.r.find_by_attr("role", "option").len(), 4);
    // The arrows pick and Tab takes the pick.
    let field = s.one("class", "formula");
    s.r.fire(field, &Event::KeyDown(KeyEvent::new("ArrowDown")));
    assert_eq!(s.state.suggest_index.get_untracked(), 1);
    s.r.fire(field, &Event::KeyDown(KeyEvent::new("Tab")));
    assert_eq!(s.state.draft.get_untracked(), "=SUMIFS(");
    assert!(
        s.state.editing.get_untracked(),
        "Tab took the name and did not leave the cell"
    );
    // Inside the bracket the arguments are shown.
    let panel = s.r.text_of(s.one("class", "suggest-hint"));
    assert!(
        panel.starts_with("SUMIFS(sum_range, criteria_range1") && panel.ends_with("(argument 1)"),
        "{panel}"
    );
    type_text("=SUMIFS(A1:A9,B1:B9,");
    assert!(
        s.r.text_of(s.one("class", "suggest-hint"))
            .ends_with("(argument 3)")
    );
    // Names are suggested too, and a click on a suggestion takes it.
    type_text("=sale");
    assert_eq!(s.state.suggestion_list(), ["Sales2027"]);
    s.r.fire(
        s.r.find_by_attr("role", "option")[0],
        &Event::PointerDown(PointerEvent {
            x: 0.0,
            y: 0.0,
            pointer_id: 1,
        }),
    );
    assert_eq!(s.state.draft.get_untracked(), "=Sales2027");
    // Nothing is suggested inside a text, or for something that is not a formula.
    type_text("=\"su");
    assert!(s.state.suggestion_list().is_empty());
    type_text("su");
    assert!(s.state.suggestion_list().is_empty());
    type_text("=SUM(");
    assert!(s.state.suggestion_list().is_empty());
    s.state.cancel_edit();
}

#[quench_test::test]
fn the_name_box_jumps_to_cells_ranges_names_and_sheets() {
    let s = blank();
    s.state.add_sheet();
    s.state.switch_sheet(0);
    let box_ = s.one("class", "name");
    let go = |text: &str| {
        s.r.fire(box_, &Event::Input(text.to_string()));
        s.r.fire(box_, &Event::KeyDown(KeyEvent::new("Enter")));
    };
    go("C5");
    assert_eq!(s.state.selected.get_untracked(), at("C5"));
    go("b2:d4");
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("B2:D4").unwrap()
    );
    s.state
        .select_range(sheets_model::Range::parse("E1:E3").unwrap());
    s.state.open_dialog(DialogId::DefineName);
    s.state.set_field(0, "Block");
    s.state.submit_dialog();
    s.state.select(at("A1"));
    go("Block");
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("E1:E3").unwrap()
    );
    go("Sheet2!$B$7");
    assert_eq!(
        (
            s.state.active.get_untracked(),
            s.state.selected.get_untracked()
        ),
        (1, at("B7"))
    );
    go("Nowhere");
    assert!(
        s.r.text_of(s.one("role", "status"))
            .contains("Cannot find Nowhere")
    );
    go("Sheet9!A1");
    assert!(
        s.r.text_of(s.one("role", "status"))
            .contains("no sheet called Sheet9")
    );
}

#[quench_test::test]
fn find_can_use_patterns_and_look_through_every_sheet() {
    let s = blank();
    for (c, t) in [("A1", "order 123"), ("A2", "item 7"), ("A3", "ab-12-cd")] {
        s.state.set_text(at(c), t);
    }
    s.state.add_sheet();
    s.state.switch_sheet(1);
    s.state.set_text(at("B2"), "order 456");
    s.state.set_text(at("C3"), "plain");
    s.state.switch_sheet(0);
    s.state.select(at("A1"));
    s.chord("f", true, false);
    s.state.find_query.set(r"order \d+".to_string());
    assert_eq!(
        s.state.find_count(),
        Some(0),
        "literally, that text is nowhere"
    );
    s.state.find_regex.set(true);
    assert_eq!(
        s.state.find_count(),
        Some(1),
        "as a pattern it is in one cell on this sheet"
    );
    s.state.find_all_sheets.set(true);
    assert_eq!(s.state.find_count(), Some(2), "and two in all");
    assert!(s.state.find_next(true));
    assert_eq!(
        (
            s.state.active.get_untracked(),
            s.state.selected.get_untracked()
        ),
        (1, at("B2")),
        "the search went on to the next sheet"
    );
    assert!(s.state.find_next(true));
    assert_eq!(
        (
            s.state.active.get_untracked(),
            s.state.selected.get_untracked()
        ),
        (0, at("A1")),
        "and round to the first again"
    );
    assert!(s.state.find_next(false));
    assert_eq!(
        s.state.active.get_untracked(),
        1,
        "backwards goes the other way"
    );
    // Case, whole cell and a bad pattern.
    s.state.find_query.set("ORDER".to_string());
    assert_eq!(s.state.find_count(), Some(2), "case is ignored");
    s.state.match_case.set(true);
    assert_eq!(s.state.find_count(), Some(0));
    s.state.match_case.set(false);
    s.state.find_query.set(r"\d+".to_string());
    s.state.whole_cell.set(true);
    assert_eq!(s.state.find_count(), Some(0), "no cell is only digits");
    s.state.whole_cell.set(false);
    s.state.find_query.set("(unclosed".to_string());
    assert_eq!(s.state.find_count(), Some(0));
    assert!(!s.state.find_problem.get_untracked().is_empty());
    assert_eq!(
        s.r.text_of(s.one("class", "find-count")),
        "Not a valid pattern"
    );
    assert!(!s.state.find_next(true));
    // Replace with groups across sheets in one undo step.
    s.state.find_query.set(r"(\w+) (\d+)".to_string());
    s.state.replace_with.set("$2:$1".to_string());
    s.state.switch_sheet(0);
    assert_eq!(s.state.replace_all(), 3);
    assert_eq!(s.state.edit_text(at("A1")), "123:order");
    assert_eq!(s.state.edit_text(at("A2")), "7:item");
    s.state.switch_sheet(1);
    assert_eq!(s.state.edit_text(at("B2")), "456:order");
    s.state.undo();
    s.state.switch_sheet(1);
    assert_eq!(
        s.state.edit_text(at("B2")),
        "order 456",
        "all of it is one step"
    );
    s.state.switch_sheet(0);
    assert_eq!(s.state.edit_text(at("A1")), "order 123");
}

#[quench_test::test]
fn paste_special_pastes_values_formulas_formats_or_swapped() {
    let s = blank();
    s.state.set_text(at("A1"), "2");
    s.state.set_text(at("B1"), "=A1*10");
    s.state.set_text(at("A2"), "text");
    s.state
        .select_range(sheets_model::Range::parse("A1:B2").unwrap());
    s.state.set_bold_for_test();
    s.state.paste_special(PasteMode::Values);
    assert_eq!(
        s.r.text_of(s.one("role", "status")),
        "Copy some cells here first"
    );
    assert!(s.state.copy(false).is_some());
    s.state.select(at("D1"));
    s.state.paste_special(PasteMode::Values);
    assert_eq!(
        s.state.edit_text(at("E1")),
        "20",
        "the formula became the number it shows"
    );
    assert!(
        !s.state.look(at("D1")).bold,
        "and the format was not carried"
    );
    s.state.select(at("D4"));
    s.state.paste_special(PasteMode::Formulas);
    assert_eq!(
        s.state.edit_text(at("E4")),
        "=D4*10",
        "formulas move as in a copy"
    );
    assert!(!s.state.look(at("D4")).bold);
    s.state.select(at("G1"));
    s.state.set_text(at("G1"), "keep");
    s.state.paste_special(PasteMode::Formats);
    assert_eq!(
        s.state.edit_text(at("G1")),
        "keep",
        "contents are left alone"
    );
    assert!(s.state.look(at("G1")).bold, "and the format arrives");
    s.state.select(at("J1"));
    s.state.paste_special(PasteMode::Transpose);
    assert_eq!(
        [
            s.state.edit_text(at("J1")),
            s.state.edit_text(at("K1")),
            s.state.edit_text(at("J2")),
            s.state.edit_text(at("K2"))
        ],
        ["2", "text", "=J1*10", ""],
        "rows and columns swapped"
    );
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("J1:K2").unwrap()
    );
    s.state.undo();
    assert_eq!(s.state.edit_text(at("J1")), "", "one undo step");
    s.r.fire(button_labelled(&s, "Paste special"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 4);
}

#[quench_test::test]
fn rows_and_columns_can_be_grouped_collapsed_expanded_and_ungrouped() {
    let s = blank();
    s.state
        .select_range(sheets_model::Range::parse("A2:C4").unwrap());
    s.state.group_selected(true);
    s.state
        .select_range(sheets_model::Range::parse("A3:A3").unwrap());
    s.state.group_selected(true);
    assert_eq!(
        (
            s.state.level_of(true, 0),
            s.state.level_of(true, 1),
            s.state.level_of(true, 2),
            s.state.level_of(true, 4)
        ),
        (0, 1, 2, 0)
    );
    assert_eq!(
        count_class(&s, "rowhdr lvl lvl2"),
        1,
        "the headers show the level"
    );
    s.state
        .select_range(sheets_model::Range::parse("B1:C1").unwrap());
    s.state.group_selected(false);
    assert_eq!(
        (
            s.state.level_of(false, 0),
            s.state.level_of(false, 1),
            s.state.level_of(false, 2)
        ),
        (0, 1, 1)
    );
    s.state.select(at("A1"));
    s.state.collapse_groups(true);
    let axes = s.state.axes.get();
    assert!(
        (1..4).all(|r| axes.rows.size(r) == 0.0)
            && axes.rows.size(0) > 0.0
            && axes.rows.size(4) > 0.0
    );
    s.state.expand_groups(true);
    assert!((1..4).all(|r| s.state.axes.get().rows.size(r) > 0.0));
    s.state.collapse_groups(false);
    assert!(s.state.axes.get().cols.size(1) == 0.0 && s.state.axes.get().cols.size(2) == 0.0);
    s.state.undo();
    s.state
        .select_range(sheets_model::Range::parse("A2:A4").unwrap());
    s.state.ungroup_selected(true);
    assert_eq!(
        (s.state.level_of(true, 1), s.state.level_of(true, 2)),
        (0, 1),
        "one level out"
    );
    s.state.ungroup_selected(true);
    assert_eq!(s.state.level_of(true, 2), 0);
    s.r.fire(button_labelled(&s, "Group"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 8);
}

#[quench_test::test]
fn formatting_whole_columns_and_rows_gives_empty_cells_the_format_and_new_cells_start_with_it() {
    let s = blank();
    s.state.set_text(at("B2"), "x");
    s.state
        .select_range(sheets_model::Range::parse("B1:B1048576").unwrap());
    s.state.toggle_bold();
    s.state.format_selection(|st| st.fill = Some(0x00FF_FF00));
    assert_eq!(
        s.state.look(at("B900")).fill,
        Some(0x00FF_FF00),
        "a cell nobody touched"
    );
    assert!(s.state.look(at("B2")).bold);
    assert_eq!(s.state.look(at("C900")).fill, None);
    // A cell typed there starts with the format.
    s.state.set_text(at("B5000"), "typed");
    assert!(
        s.state.look(at("B5000")).bold,
        "{:?}",
        s.state.look(at("B5000"))
    );
    // Whole rows too, and a cell's own format beats the row's.
    s.state
        .select_range(sheets_model::Range::parse("A3:XFD3").unwrap());
    s.state.format_selection(|st| st.italic = true);
    assert!(s.state.look(at("Z3")).italic && !s.state.look(at("Z4")).italic);
    assert!(
        s.state.look(at("B3")).italic && s.state.look(at("B3")).bold,
        "the row's italic on the column's cell"
    );
    s.state.undo();
    assert!(!s.state.look(at("Z3")).italic, "undone with the row");
    // Rows and columns inserted keep their formats with them.
    s.state.select(at("A1"));
    s.state.insert_selected(sheets_model::Axis::Col);
    assert_eq!(
        s.state.look(at("C900")).fill,
        Some(0x00FF_FF00),
        "the column moved right"
    );
    assert_eq!(s.state.look(at("B900")).fill, None);
}

#[quench_test::test]
fn page_setup_is_asked_for_in_a_form_and_changes_what_is_printed() {
    let manual = with_platform();
    let s = blank();
    for r in 1..=3 {
        s.state.set_text(at(&format!("A{r}")), &format!("line {r}"));
    }
    choose(&s, "Page setup\u{2026}");
    let d = s.state.dialog.get_untracked().unwrap();
    assert_eq!(
        (d.fields[0].value.as_str(), d.fields[2].value.as_str()),
        ("a4", "13")
    );
    for (i, v) in ["letter", "landscape", "25", "", "", "1:1", "A2:A2"]
        .iter()
        .enumerate()
    {
        if !v.is_empty() {
            s.state.set_field(i, v);
        }
    }
    s.state.toggle_field(4); // lines off
    s.state.submit_dialog();
    assert!(s.state.dialog.get_untracked().is_none());
    s.state.print();
    let pdf = inflated(&manual.downloads()[0].2);
    assert!(pdf.contains("/MediaBox [0 0 792 612]"), "Letter, sideways");
    assert!(
        pdf.contains("line 2") && pdf.contains("line 1") && !pdf.contains("(line 3)"),
        "area A2 with the row 1 repeated"
    );
    assert_eq!(pdf.matches(" 0.25 w ").count(), 0, "no lines between cells");
    // Mistakes are explained and keep the form open.
    s.state.open_dialog(DialogId::PageSetup);
    s.state.set_field(2, "wide");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("millimetres")
    );
    s.state.set_field(2, "10");
    s.state.set_field(5, "x:y");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("Rows to repeat")
    );
    s.state.cancel_dialog();
    // The settings are kept with the workbook and undone with it.
    s.state.undo();
    s.state.open_dialog(DialogId::PageSetup);
    assert_eq!(
        s.state.dialog.get_untracked().unwrap().fields[0].value,
        "a4",
        "undone"
    );
    s.state.cancel_dialog();
}

#[quench_test::test]
fn charts_are_made_from_the_selection_drawn_moved_changed_and_removed() {
    let s = blank();
    for (r, (q, a)) in [
        ("Quarter", "Sales"),
        ("Q1", "10"),
        ("Q2", "20"),
        ("Q3", "30"),
    ]
    .into_iter()
    .enumerate()
    {
        s.state
            .set_text(CellRef::new(u32::try_from(r).unwrap(), 0), q);
        s.state
            .set_text(CellRef::new(u32::try_from(r).unwrap(), 1), a);
    }
    s.state.select(at("A1"));
    s.state.new_chart_dialog(sheets_model::ChartKind::Column);
    let d = s.state.dialog.get_untracked().unwrap();
    assert_eq!(
        (
            d.fields[0].value.as_str(),
            d.fields[2].value.as_str(),
            d.fields[7].value.as_str()
        ),
        ("column", "A1:B4", "C1"),
        "the block around the cursor, put beside it"
    );
    s.state.set_field(1, "Sales by quarter");
    s.state.submit_dialog();
    assert_eq!(s.state.charts().len(), 1);
    assert_eq!(
        s.r.find_by_attr("role", "img").len(),
        1,
        "a chart is on the sheet"
    );
    assert_eq!(
        count_class(&s, "chart picked"),
        1,
        "and it is the picked one"
    );
    let node = s.r.find_by_attr("role", "img")[0];
    let src = s.r.attr(s.r.children(node)[0], "src").unwrap();
    assert!(
        src.starts_with("data:image/svg+xml;utf8,%3Csvg")
            && src.contains("Sales by quarter")
            && !src.contains('#')
            && !src.contains('<'),
        "{}",
        &src[..80]
    );
    // It follows the cells.
    s.state.set_text(at("B2"), "40");
    let after =
        s.r.attr(s.r.children(s.r.find_by_attr("role", "img")[0])[0], "src")
            .unwrap();
    assert_ne!(src, after, "the picture changed with the number");
    // Carrying it moves it to the cell under its new corner, in one undo step.
    let pointer = |x: f64, y: f64| PointerEvent {
        x,
        y,
        pointer_id: 1,
    };
    let chart = s.r.find_by_attr("role", "img")[0];
    s.r.fire(chart, &Event::PointerDown(pointer(500.0, 100.0)));
    assert_eq!(s.state.chart_selected.get_untracked(), Some(0));
    s.state
        .chart_pointer(500.0 + 96.0 * 2.0, 100.0 + 24.0 * 3.0);
    assert_eq!(s.state.chart_drop.get_untracked(), Some(at("E4")));
    assert_eq!(
        count_class(&s, "chart-ghost"),
        1,
        "a preview of where it will land"
    );
    s.release();
    assert_eq!(s.state.charts()[0].at, at("E4"));
    assert!(s.state.chart_drop.get_untracked().is_none());
    s.state.undo();
    assert_eq!(s.state.charts()[0].at, at("C1"));
    s.state.redo();
    // Double click changes it; the form is filled in.
    s.r.fire(s.r.find_by_attr("role", "img")[0], &Event::DoubleClick);
    let d = s.state.dialog.get_untracked().unwrap();
    assert!(
        d.title.starts_with("Change")
            && d.fields[1].value == "Sales by quarter"
            && d.fields[7].value == "E4"
    );
    s.state.set_field(0, "pie");
    s.state.set_field(5, "300");
    s.state.submit_dialog();
    let c = s.state.charts();
    assert_eq!(
        (c.len(), c[0].kind, c[0].width),
        (1, sheets_model::ChartKind::Pie, 300),
        "changed, not added"
    );
    // A bad form stays open.
    s.state.edit_chart_dialog();
    s.state.set_field(2, "nonsense");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("A1:C5")
    );
    s.state.cancel_dialog();
    // The Delete key removes the picked chart; clicking a cell lets go of it.
    s.state.press(at("A1"));
    assert!(s.state.chart_selected.get_untracked().is_none());
    s.state.select_chart(0);
    s.chord("Delete", false, false);
    assert!(s.state.charts().is_empty());
    assert_eq!(
        s.state.edit_text(at("A1")),
        "Quarter",
        "and not the cell under the cursor"
    );
    s.r.fire(button_labelled(&s, "Chart"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 8);
}

#[quench_test::test]
fn sparklines_are_drawn_in_cells_from_a_row_of_numbers() {
    let s = blank();
    for (r, row) in [[1, 3, 2, 5], [4, 4, 1, 0], [2, 2, 2, 9]]
        .into_iter()
        .enumerate()
    {
        for (c, v) in row.into_iter().enumerate() {
            s.state.set_text(
                CellRef::new(u32::try_from(r).unwrap(), u32::try_from(c).unwrap()),
                &v.to_string(),
            );
        }
    }
    s.state
        .select_range(sheets_model::Range::parse("A1:D3").unwrap());
    s.state
        .new_sparkline_dialog(sheets_model::SparkKind::Column);
    let d = s.state.dialog.get_untracked().unwrap();
    assert_eq!(
        (d.fields[1].value.as_str(), d.fields[2].value.as_str()),
        ("A1:D3", "E1:E3"),
        "one cell for each row, beside the data"
    );
    s.state.submit_dialog();
    for r in 1..=3 {
        assert!(s.state.look(at(&format!("E{r}"))).spark.is_some());
    }
    assert!(s.state.look(at("D1")).spark.is_none());
    let node = s.cell(0, 4).unwrap();
    let image = s.r.style_of(node, "background-image").unwrap();
    assert!(
        image.starts_with("url(\"data:image/svg+xml;utf8,%3Csvg") && image.contains("%3Crect"),
        "{}",
        &image[..60]
    );
    // It follows the numbers.
    s.state.set_text(at("D1"), "50");
    assert_ne!(
        s.r.style_of(s.cell(0, 4).unwrap(), "background-image")
            .unwrap(),
        image
    );
    // One sparkline over all the cells of a row, and the form refuses a place that does not fit.
    s.state.select(at("H1"));
    s.state.new_sparkline_dialog(sheets_model::SparkKind::Line);
    s.state.set_field(1, "A1:D1");
    s.state.set_field(2, "H1");
    s.state.submit_dialog();
    assert!(s.state.look(at("H1")).spark.is_some());
    s.state.new_sparkline_dialog(sheets_model::SparkKind::Line);
    s.state.set_field(1, "A1:D3");
    s.state.set_field(2, "H5:H6");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("as many cells")
    );
    s.state.cancel_dialog();
    // Remove, and undo.
    s.state
        .select_range(sheets_model::Range::parse("E1:E3").unwrap());
    s.state.clear_sparklines();
    assert!(s.state.look(at("E2")).spark.is_none() && s.state.look(at("H1")).spark.is_some());
    s.state.undo();
    assert!(s.state.look(at("E2")).spark.is_some());
    s.r.fire(button_labelled(&s, "Sparkline"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 4);
}

#[quench_test::test]
fn a_summary_table_of_live_formulas_adds_up_one_column_by_the_values_of_others() {
    let s = blank();
    let rows = [
        ("Region", "Product", "Sales"),
        ("North", "Nut", "10"),
        ("South", "Bolt", "20"),
        ("North", "Bolt", "5"),
        ("south", "Nut", "7"),
        ("North", "Nut", "3"),
    ];
    for (r, (a, b, c)) in rows.into_iter().enumerate() {
        let r = u32::try_from(r).unwrap();
        s.state.set_text(CellRef::new(r, 0), a);
        s.state.set_text(CellRef::new(r, 1), b);
        s.state.set_text(CellRef::new(r, 2), c);
    }
    s.state.select(at("A2"));
    s.state.open_pivot_dialog();
    let d = s.state.dialog.get_untracked().unwrap();
    assert_eq!(
        (d.fields[0].value.as_str(), d.fields[5].value.as_str()),
        ("A1:C6", "E1")
    );
    // Sales by region and product.
    s.state.set_field(1, "0");
    s.state.set_field(2, "1");
    s.state.set_field(3, "2");
    s.state.submit_dialog();
    let at_ = |c: &str| s.state.display(at(c));
    assert_eq!(
        [at_("E1"), at_("F1"), at_("G1"), at_("H1")],
        ["Sum of Sales", "Bolt", "Nut", "Total"]
    );
    assert_eq!(
        [at_("E2"), at_("F2"), at_("G2"), at_("H2")],
        ["North", "5", "13", "18"]
    );
    assert_eq!(
        [at_("E3"), at_("F3"), at_("G3"), at_("H3")],
        ["South", "20", "7", "27"],
        "south and South are one"
    );
    assert_eq!(
        [at_("E4"), at_("F4"), at_("G4"), at_("H4")],
        ["Total", "25", "20", "45"]
    );
    assert!(s.state.look(at("E1")).bold && !s.state.look(at("F2")).bold);
    assert!(
        s.state.edit_text(at("F2")).starts_with("=IFERROR(SUMIFS("),
        "{}",
        s.state.edit_text(at("F2"))
    );
    // It stays up to date.
    s.state.set_text(at("C2"), "100");
    assert_eq!(
        (at_("G2"), at_("H2"), at_("H4")),
        ("103".to_string(), "108".to_string(), "135".to_string())
    );
    s.state.undo();
    s.state.undo();
    assert_eq!(at_("E1"), "", "the whole table is one undo step");
    // Counting and averaging, with no columns across.
    s.state.select(at("A2"));
    s.state.open_pivot_dialog();
    s.state.set_field(1, "0");
    s.state.set_field(3, "2");
    s.state.set_field(4, "count");
    s.state.set_field(5, "J1");
    s.state.submit_dialog();
    assert_eq!(
        [
            at_("J1"),
            at_("K1"),
            at_("J2"),
            at_("K2"),
            at_("K3"),
            at_("K4")
        ],
        ["Count of Sales", "Total", "North", "3", "2", "5"]
    );
    s.state.select(at("A2"));
    s.state.open_pivot_dialog();
    s.state.set_field(4, "average");
    s.state.set_field(5, "M1");
    s.state.submit_dialog();
    assert_eq!(
        [at_("M2"), at_("N2"), at_("N3")],
        ["North", "6", "13.5"].map(String::from),
        "{:?}",
        at_("N2")
    );
    // Mistakes are explained.
    s.state.open_pivot_dialog();
    s.state.set_field(0, "nope");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("A1:D100")
    );
    s.state.cancel_dialog();
}

#[quench_test::test]
fn a_protected_sheet_refuses_changes_to_locked_cells_until_it_is_unprotected_with_the_password() {
    let s = blank();
    s.state.set_text(at("A1"), "locked");
    s.state.set_text(at("B1"), "open");
    s.state.select(at("B1"));
    s.state.set_locked(false);
    s.state.open_dialog(DialogId::Protect);
    s.state.set_field(0, "secret");
    s.state.set_field(1, "different");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("not the same")
    );
    s.state.set_field(1, "secret");
    s.state.submit_dialog();
    assert!(s.state.protection().is_some());
    // Locked cells refuse changes by every route; the unlocked one takes them.
    s.state.set_text(at("A1"), "changed");
    assert_eq!(s.state.edit_text(at("A1")), "locked");
    assert!(s.r.text_of(s.one("role", "status")).contains("protected"));
    s.state.select(at("A1"));
    s.state.clear_selected();
    assert_eq!(s.state.edit_text(at("A1")), "locked");
    s.state.insert_selected(sheets_model::Axis::Row);
    assert_eq!(
        s.state.edit_text(at("A1")),
        "locked",
        "rows cannot be moved"
    );
    s.state.merge_selection();
    s.state.toggle_bold();
    assert!(!s.state.look(at("A1")).bold);
    s.state.set_text(at("B1"), "edited");
    assert_eq!(s.state.edit_text(at("B1")), "edited");
    assert!(!s.state.look(at("B1")).bold);
    assert_eq!(s.state.edit_text(at("A1")), "locked");
    // The wrong password does not unprotect; the right one does.
    s.state.open_dialog(DialogId::Unprotect);
    s.state.set_field(0, "guess");
    s.state.submit_dialog();
    assert!(
        s.state
            .dialog
            .get_untracked()
            .unwrap()
            .error
            .contains("not the password")
    );
    assert!(s.state.protection().is_some());
    s.state.set_field(0, "secret");
    s.state.submit_dialog();
    assert!(s.state.protection().is_none());
    s.state.set_text(at("A1"), "changed");
    assert_eq!(s.state.edit_text(at("A1")), "changed");
    // Without a password it is lifted at once.
    s.state.open_dialog(DialogId::Protect);
    s.state.submit_dialog();
    assert!(s.state.protection().is_some());
    s.state.open_dialog(DialogId::Unprotect);
    assert!(s.state.protection().is_none() && s.state.dialog.get_untracked().is_none());
    s.r.fire(button_labelled(&s, "Protect"), &Event::Click);
    assert_eq!(s.r.find_by_attr("role", "menuitem").len(), 4);
}

#[quench_test::test]
fn a_big_file_is_announced_before_it_is_read() {
    use std::fmt::Write as _;
    let manual = with_platform();
    let s = blank();
    // A csv of a few hundred thousand bytes.
    let mut text = String::new();
    for r in 0..30_000 {
        let _ = writeln!(text, "{r},label {r},{}", r * 3);
    }
    let file = quench_ui::platform::PickedFile {
        name: "big.csv".into(),
        mime: "text/csv".into(),
        bytes: text.into_bytes(),
    };
    assert!(file.bytes.len() > 400_000);
    s.state.load_file(&file);
    assert!(
        s.r.text_of(s.one("role", "status"))
            .starts_with("Opening big.csv"),
        "said first"
    );
    assert_eq!(s.state.display(at("A1")), "", "and not read yet");
    manual.advance(100.0);
    assert_eq!(s.state.name.get_untracked(), "big.csv");
    assert_eq!(s.state.display(at("C30000")), "89997");
    assert_eq!(s.r.text_of(s.one("role", "status")), "Opened big.csv");
}

#[quench_test::test]
fn opendocument_and_old_excel_files_open_and_ods_is_offered_for_saving() {
    for (name, bytes) in [
        (
            "book.ods",
            &include_bytes!("../../../corpus/libreoffice_basic.ods")[..],
        ),
        (
            "book.xls",
            &include_bytes!("../../../corpus/libreoffice_basic.xls")[..],
        ),
    ] {
        let s = blank();
        let file = quench_ui::platform::PickedFile {
            name: name.into(),
            mime: String::new(),
            bytes: bytes.to_vec(),
        };
        s.state.load_file(&file);
        assert_eq!(s.state.display(at("A2")), "bolt", "{name}");
        assert!(s.state.display(at("D2")).ends_with("6.00"), "{name}");
    }
}

#[quench_test::test]
fn the_ribbon_is_two_lines_tabs_over_the_tools_of_the_chosen_one() {
    let s = blank();
    let with_class = |tag: &str, class: &str| -> Vec<NodeId> {
        s.r.find_by_tag(tag)
            .into_iter()
            .filter(|n| {
                s.r.attr(*n, "class")
                    .is_some_and(|c| c.split(' ').any(|x| x == class))
            })
            .collect()
    };
    let has = |n: NodeId, class: &str| {
        s.r.attr(n, "class")
            .is_some_and(|c| c.split(' ').any(|x| x == class))
    };
    let tabs = with_class("button", "ribbon-tab");
    let names: Vec<String> = tabs.iter().map(|t| s.r.text_of(*t)).collect();
    assert_eq!(names, ["Home", "Number", "Cells", "Layout", "Data"]);
    let panes = with_class("div", "groups");
    let open = |panes: &[NodeId]| panes.iter().filter(|p| !has(**p, "hidden")).count();
    assert_eq!(open(&panes), 1, "the first tab shows to begin with");
    assert!(!has(panes[0], "hidden"));
    s.r.fire(tabs[4], &Event::Click);
    assert!(!has(panes[4], "hidden") && has(panes[0], "hidden"));
    assert_eq!(open(&panes), 1);
}

#[quench_test::test]
fn the_secondary_button_on_cells_opens_a_menu_of_what_can_be_done_with_them() {
    let s = blank();
    s.state.set_text(at("A1"), "1");
    s.state.set_text(at("B2"), "2");
    s.state
        .select_range(sheets_model::Range::parse("A1:B2").unwrap());
    let cell = s.cell(1, 1).unwrap();
    let pointer = quench_ui::PointerEvent {
        x: 300.0,
        y: 200.0,
        pointer_id: 1,
    };
    s.r.fire(cell, &Event::PointerDown(pointer));
    s.r.fire(cell, &Event::ContextMenu(pointer));
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("A1:B2").unwrap(),
        "pressing inside the selection keeps it"
    );
    assert_eq!(s.state.context.get_untracked(), Some((300.0, 200.0)));
    let items: Vec<String> =
        s.r.find_by_attr("role", "menuitem")
            .into_iter()
            .map(|n| s.r.text_of(n))
            .collect();
    for wanted in [
        "Cut",
        "Copy",
        "Paste",
        "Paste special",
        "Insert",
        "Delete",
        "Clear contents",
        "Merge cells",
        "Sort",
        "Note\u{2026}",
    ] {
        assert!(
            items.iter().any(|i| i.starts_with(wanted)),
            "{wanted} in {items:?}"
        );
    }
    // A choice does its thing and closes the menu.
    let clear =
        s.r.find_by_attr("role", "menuitem")
            .into_iter()
            .find(|n| s.r.text_of(*n).starts_with("Clear contents"))
            .unwrap();
    s.r.fire(clear, &Event::Click);
    assert!(s.state.context.get_untracked().is_none());
    assert_eq!(s.state.edit_text(at("A1")), "");
    assert!(
        s.r.find_by_attr("role", "menuitem")
            .iter()
            .all(|n| s.r.text_of(*n) != "Cut"),
        "closed"
    );
    // Outside the selection the cell becomes the selection.
    let far = s.cell(3, 3).unwrap();
    let p = quench_ui::PointerEvent {
        x: 10.0,
        y: 10.0,
        pointer_id: 1,
    };
    s.r.fire(far, &Event::PointerDown(p));
    s.r.fire(far, &Event::ContextMenu(p));
    assert_eq!(
        s.state.selection(),
        sheets_model::Range::parse("D4").unwrap()
    );
}

#[quench_test::test]
fn a_table_has_its_own_arrows_sorts_grows_and_turns_back_into_cells() {
    let s = blank();
    for (a, v) in [
        ("A1", "Name"),
        ("B1", ""),
        ("A2", "b"),
        ("A3", "c"),
        ("A4", "a"),
    ] {
        s.state.set_text(at(a), v);
    }
    for (a, v) in [("B2", "2"), ("B3", "3"), ("B4", "1")] {
        s.state.set_text(at(a), v);
    }
    s.state.select(at("A1"));
    s.state.insert_table();
    let tables = || s.state.engine_tables();
    assert_eq!(tables().len(), 1);
    assert_eq!(tables()[0].name, "Table1");
    assert_eq!(
        s.state.display(at("B1")),
        "Column2",
        "an empty heading gets a name"
    );
    let filter_buttons = || {
        s.r.find_by_tag("button")
            .into_iter()
            .filter(|b| s.r.attr(*b, "aria-label").as_deref() == Some("Filter"))
            .count()
    };
    assert_eq!(filter_buttons(), 2, "an arrow on each heading");
    // The look: plain by default (the theme colours the band by class), coloured when chosen.
    assert_eq!(s.state.look(at("A1")).fill, None);
    assert!(s.state.look(at("A1")).class().contains("tbl-head") && s.state.look(at("A1")).bold);
    assert!(s.state.look(at("A3")).class().contains("tbl-band"));
    assert!(!s.state.look(at("A2")).class().contains("tbl-band"));
    s.state.set_table_style("TableStyleMedium2");
    assert_eq!(s.state.look(at("A1")).fill, Some(0x0044_72C4));
    assert_eq!(s.state.look(at("A3")).fill, Some(0x00D9_E1F2));
    assert_eq!(s.state.look(at("A2")).fill, None);
    // Your own colours, by the dialog.
    s.state.open_dialog(DialogId::TableLook);
    s.state.set_field(0, "#ff0000");
    s.state.set_field(1, "#000000");
    s.state.set_field(2, "#00ff00");
    s.state.submit_dialog();
    assert_eq!(s.state.look(at("A1")).fill, Some(0x00FF_0000));
    assert_eq!(s.state.look(at("A1")).color, Some(0));
    assert_eq!(s.state.look(at("A3")).fill, Some(0x0000_FF00));
    assert_eq!(
        s.state.engine_tables()[0].style,
        "Custom_FF0000_000000_00FF00"
    );
    s.state.set_table_style("None");
    assert_eq!(s.state.look(at("A1")).fill, None);
    assert!(!s.state.look(at("A1")).bold && !s.state.look(at("A3")).class().contains("tbl-"));
    s.state.set_table_style("");
    // Sorting from the arrow's list.
    s.state.open_filter(Some(0), 0);
    s.state.sort_filter_column(true);
    assert_eq!(
        ["A2", "A3", "A4"].map(|a| s.state.display(at(a))),
        ["a", "b", "c"]
    );
    assert_eq!(s.state.display(at("B2")), "1", "whole rows move");
    s.state.open_filter(Some(0), 1);
    s.state.sort_filter_column(false);
    assert_eq!(
        ["B2", "B3", "B4"].map(|a| s.state.display(at(a))),
        ["3", "2", "1"]
    );
    // Filtering by a value hides rows of this table.
    s.state.open_filter(Some(0), 0);
    s.state.toggle_filter_value("b");
    s.state.apply_filter();
    assert!(s.state.engine_tables()[0].filter.allowed.contains_key(&0));
    // Typing under the table grows it.
    s.state.set_text(at("A5"), "d");
    assert_eq!(tables()[0].range().to_string(), "A1:B5");
    s.state.set_text(at("A7"), "far");
    assert_eq!(tables()[0].range().to_string(), "A1:B5", "a gap does not");
    // A name must be sound and unused.
    assert!(s.state.rename_table("Parts").is_ok());
    assert_eq!(tables()[0].name, "Parts");
    assert!(s.state.rename_table("1bad").is_err() && s.state.rename_table("A1").is_err());
    // Back to cells.
    s.state.remove_table();
    assert!(tables().is_empty());
    assert_eq!(filter_buttons(), 0);
    assert!(!s.state.look(at("A1")).class().contains("tbl-head"));
    // One undo step brings it back.
    s.state.undo();
    assert_eq!(tables().len(), 1);
}

#[quench_test::test]
fn the_screen_speaks_the_language_picked_and_every_catalogue_has_the_same_texts() {
    let s = blank();
    let labels = |s: &Screen| -> Vec<String> {
        s.r.find_by_tag("button")
            .into_iter()
            .map(|b| s.r.text_of(b))
            .collect()
    };
    assert!(labels(&s).iter().any(|l| l == "Home"));
    s.state.set_language("de-DE");
    let german = labels(&s);
    assert!(
        german.iter().any(|l| l == "Start") && german.iter().any(|l| l == "Daten"),
        "{german:?}"
    );
    assert!(!german.iter().any(|l| l == "Home"));
    // A menu in the new language, and a tooltip.
    s.state.set_language("pl-PL");
    assert!(labels(&s).iter().any(|l| l == "Dane"));
    assert_eq!(sheets_app::i18n::tr("Bold"), "Pogrubienie");
    assert_eq!(
        sheets_app::i18n::tr("A text nobody translated"),
        "A text nobody translated"
    );
    // The base language finds its locale; an unknown one is English.
    s.state.set_language("fr");
    assert_eq!(sheets_app::i18n::tr("Bold"), "Gras");
    s.state.set_language("xx-XX");
    assert_eq!(sheets_app::i18n::tr("Bold"), "Bold");
    // The catalogues name the same texts.
    let keys = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with('#') && l.contains(" = "))
            .map(|l| l.split(" = ").next().unwrap_or_default().to_string())
            .collect()
    };
    let english = keys(include_str!("../i18n/en-US.ftl"));
    for (name, text) in [
        ("de", include_str!("../i18n/de-DE.ftl")),
        ("es", include_str!("../i18n/es-ES.ftl")),
        ("fr", include_str!("../i18n/fr-FR.ftl")),
        ("pl", include_str!("../i18n/pl-PL.ftl")),
    ] {
        assert_eq!(keys(text), english, "{name} has the same keys as English");
    }
}

#[quench_test::test]
fn the_secondary_button_on_a_heading_selects_it_and_offers_what_suits_rows_and_columns() {
    let s = blank();
    s.state.set_text(at("B2"), "x");
    let headers = |class: &str| -> Vec<NodeId> {
        s.r.find_by_tag("div")
            .into_iter()
            .filter(|n| {
                s.r.attr(*n, "class")
                    .is_some_and(|c| c.split(' ').next() == Some(class))
            })
            .collect()
    };
    let col_b = headers("colhdr")
        .into_iter()
        .find(|n| s.r.text_of(*n) == "B")
        .unwrap();
    let p = quench_ui::PointerEvent {
        x: 100.0,
        y: 20.0,
        pointer_id: 1,
    };
    s.r.fire(col_b, &Event::ContextMenu(p));
    assert_eq!(s.state.selection().start, at("B1"));
    assert_eq!(s.state.selection().end.row, 1_048_575);
    let items = || -> Vec<String> {
        s.r.find_by_attr("role", "menuitem")
            .into_iter()
            .map(|n| s.r.text_of(n))
            .collect()
    };
    assert!(
        items().iter().any(|i| i.starts_with("Insert"))
            && items().iter().any(|i| i.starts_with("Hide columns"))
    );
    assert!(!items().iter().any(|i| i.starts_with("Merge")));
    s.state.context.set(None);
    let row_2 = headers("rowhdr")
        .into_iter()
        .find(|n| s.r.text_of(*n) == "2")
        .unwrap();
    s.r.fire(
        row_2,
        &Event::ContextMenu(quench_ui::PointerEvent {
            x: 10.0,
            y: 60.0,
            pointer_id: 1,
        }),
    );
    assert!(
        items().iter().any(|i| i.starts_with("Insert"))
            && items().iter().any(|i| i.starts_with("Show hidden rows"))
    );
}

#[quench_test::test]
fn dialogs_and_messages_follow_the_language_too() {
    let s = blank();
    s.state.set_language("de-DE");
    s.state.select(at("A1"));
    s.state.open_dialog(DialogId::TableName);
    assert_eq!(
        s.r.text_of(s.one("role", "status")),
        "Der Cursor ist nicht in einer Tabelle"
    );
    s.state.open_dialog(DialogId::DefineName);
    let d = s.state.dialog.get_untracked().unwrap();
    assert_eq!(d.title, "Namen festlegen");
    assert_eq!(d.fields[0].label, "Name");
    assert_eq!(d.fields[1].label, "Bezieht sich auf");
}

quench_test::main!();

#[quench_test::test]
fn a_drag_that_leaves_the_grid_keeps_growing_the_selection_until_it_is_released() {
    let manual = with_platform();
    let s = open(200);
    s.state.pointer_down(CellRef::new(3, 1));
    s.state.pointer_over(CellRef::new(20, 2));
    // The move reaches the screen after the cell (and the cell saw it, so nothing is pulled).
    s.state.drag_moved(|_| (1, 0));
    // The pointer is past the foot of the grid: the cell it was last over is the one at the edge.
    s.state.drag_moved(|_| (1, 0));
    let first = s.state.range_end.get_untracked().row;
    manual.advance(500.0);
    let later = s.state.range_end.get_untracked().row;
    assert!(later > first + 3, "{first} -> {later}");
    // Back over a cell: the pull stops.
    s.state.pointer_over(CellRef::new(25, 2));
    s.state.drag_moved(|_| (1, 0));
    let held = s.state.range_end.get_untracked().row;
    manual.advance(500.0);
    assert_eq!(s.state.range_end.get_untracked().row, held);
    // Released: nothing moves any more.
    s.state.drag_moved(|_| (1, 0));
    s.state.pointer_up();
    let stopped = s.state.range_end.get_untracked().row;
    manual.advance(500.0);
    assert_eq!(s.state.range_end.get_untracked().row, stopped);
}

#[quench_test::test]
fn typing_a_new_name_in_the_ribbon_renames_the_document_and_keeps_the_extension() {
    let s = open(3);
    let field = s.one("class", "file-name file-name-input");
    s.r.fire(field, &Event::Change("Budget 2027".into()));
    assert_eq!(s.state.name.get_untracked(), "Budget 2027.xlsx");
    s.r.fire(field, &Event::Change("   ".into()));
    assert_eq!(s.state.name.get_untracked(), "Budget 2027.xlsx");
}

fn ops(change: Option<office_cloud_ui::Change>) -> Vec<u8> {
    match change {
        Some(office_cloud_ui::Change::Ops(bytes)) => bytes,
        other => panic!("not a change of cells: {other:?}"),
    }
}

#[quench_test::test]
fn what_is_typed_is_taken_as_cells_and_anything_else_as_the_whole_file() {
    let s = open(5);
    assert!(s.state.take_changes().is_none(), "nothing yet");
    s.state.set_text(at("B2"), "hello");
    s.state.set_text(at("C3"), "=1+2");
    s.state.set_text(at("B2"), "again");
    let taken = ops(s.state.take_changes());
    let written = sheets_app::sync::decode(&taken).unwrap();
    assert_eq!(
        written.len(),
        2,
        "the last write to a cell is the one that counts"
    );
    assert!(
        written.iter().any(|w| w.at == at("B2")
            && w.content
                == Some(sheets_model::Content::Value(sheets_model::Value::Text(
                    "again".into()
                )))),
        "{written:?}"
    );
    assert!(written.iter().any(
        |w| w.at == at("C3") && w.content == Some(sheets_model::Content::Formula("1+2".into()))
    ));
    assert!(s.state.take_changes().is_none(), "taken once");
    // Undo is a change of cells too: the last step put B2 back to what it was.
    s.state.undo();
    let undone = sheets_app::sync::decode(&ops(s.state.take_changes())).unwrap();
    assert_eq!(undone.len(), 1, "{undone:?}");
    assert_eq!(undone[0].at, at("B2"));
    assert_eq!(
        undone[0].content,
        Some(sheets_model::Content::Value(sheets_model::Value::Text(
            "hello".into()
        )))
    );
    // A sheet added is not a change of cells: the whole file is what says it.
    s.state.add_sheet();
    assert_eq!(s.state.take_changes(), Some(office_cloud_ui::Change::Whole));
    assert!(s.state.take_changes().is_none());
}

#[quench_test::test]
fn another_pages_cells_are_written_except_the_ones_this_page_changed_and_has_not_sent() {
    let (a, b) = (open(5), open(5));
    a.state.set_text(at("A1"), "from a");
    a.state.set_text(at("A2"), "2");
    let theirs = ops(a.state.take_changes());
    b.state.set_text(at("A1"), "mine");
    b.state.apply_remote(&theirs).unwrap();
    assert_eq!(b.state.edit_text(at("A2")), "2", "their cell is written");
    assert_eq!(
        b.state.edit_text(at("A1")),
        "mine",
        "mine waits to be sent, and comes after theirs"
    );
    // What was written for others is not a change of this page: only its own is sent.
    let mine = sheets_app::sync::decode(&ops(b.state.take_changes())).unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].at, at("A1"));
    assert!(b.state.apply_remote(b"not json").is_err());
}

#[quench_test::test]
fn a_cell_change_keeps_what_is_in_the_cell_and_how_it_looks_whatever_the_tables_of_the_pages() {
    use sheets_app::sync::{Written, decode, encode};
    use sheets_model::{Align, Content, Line, LineKind, Style, Value};
    let style = Style {
        number_format: "#,##0.00".into(),
        bold: true,
        italic: true,
        color: Some(0x00FF_0000),
        fill: Some(0x00FF_FF00),
        align: Align::Right,
        wrap: true,
        font_size: Some(24),
        font_name: Some("Calibri".into()),
        border: sheets_model::Border {
            top: Some(Line {
                kind: LineKind::Double,
                color: Some(0x0000_00FF),
            }),
            ..sheets_model::Border::default()
        },
        ..Style::default()
    };
    let cells = [
        Content::Value(Value::Number(1.5)),
        Content::Value(Value::Number(-2e30)),
        Content::Value(Value::Text("zażółć \"quoted\" \n line".into())),
        Content::Value(Value::Bool(true)),
        Content::Value(Value::Error(sheets_model::CellError::Div0)),
        Content::Formula("SUM(A1:A3)".into()),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, content)| Written {
        sheet: 1,
        at: CellRef::new(u32::try_from(i).unwrap(), 2),
        content: Some(content),
        style: style.clone(),
    })
    .chain([Written {
        sheet: 0,
        at: CellRef::new(9, 9),
        content: None,
        style: Style::default(),
    }])
    .collect::<Vec<_>>();
    assert_eq!(decode(&encode(&cells)).unwrap(), cells);
    assert!(decode(b"{}").is_err());
    assert!(decode(br#"{"cells":[{"r":1}]}"#).is_err());
}

#[quench_test::test]
fn zoom_steps_up_and_down_and_back_to_one_hundred() {
    let s = open(5);
    let state = s.state;
    assert_eq!(state.zoom.get_untracked(), 100);
    state.zoom_by(1);
    assert_eq!(state.zoom.get_untracked(), 110);
    state.zoom_by(-1);
    state.zoom_by(-1);
    assert_eq!(state.zoom.get_untracked(), 90);
    state.zoom_by(0);
    assert_eq!(state.zoom.get_untracked(), 100);
    for _ in 0..20 {
        state.zoom_by(1);
    }
    assert_eq!(state.zoom.get_untracked(), 300);
}
