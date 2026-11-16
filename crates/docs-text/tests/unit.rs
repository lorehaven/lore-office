#![allow(
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::redundant_closure_for_method_calls
)]
use docs_text::{Align, Doc, Family, Kind, Mark, Pos, Style};

fn p(para: usize, offset: usize) -> Pos {
    Pos::new(para, offset)
}

#[quench_test::test]
fn typing_inserts_and_enter_splits_a_paragraph() {
    let mut d = Doc::new();
    let mut caret = d.replace(p(0, 0), p(0, 0), "Hello world");
    assert_eq!((caret, d.plain().as_str()), (p(0, 11), "Hello world"));
    caret = d.replace(p(0, 5), p(0, 5), "\n");
    assert_eq!((caret, d.plain().as_str()), (p(1, 0), "Hello\n world"));
    assert_eq!(d.paras().len(), 2);
    assert_ne!(d.paras()[0].id, d.paras()[1].id);
    // Several lines at once (a paste).
    caret = d.replace(p(1, 1), p(1, 1), "a\nb\nc");
    assert_eq!(
        (caret, d.plain().as_str()),
        (p(3, 1), "Hello\n a\nb\ncworld")
    );
}

#[quench_test::test]
fn deleting_inside_across_and_joining_paragraphs() {
    let mut d = Doc::from_text("one\ntwo\nthree");
    assert_eq!(d.paras().len(), 3);
    // Backspace at the start of a paragraph is a delete from the end of the one before.
    let caret = d.delete(p(0, 3), p(1, 0));
    assert_eq!((caret, d.plain().as_str()), (p(0, 3), "onetwo\nthree"));
    // A range across paragraphs.
    let caret = d.delete(p(0, 2), p(1, 2));
    assert_eq!((caret, d.plain().as_str()), (p(0, 2), "onree"));
    assert_eq!(d.paras().len(), 1);
    // Backwards ranges mean the same.
    let mut e = Doc::from_text("abcdef");
    e.delete(p(0, 4), p(0, 1));
    assert_eq!(e.plain(), "aef");
    assert_eq!(e.slice(p(0, 2), p(0, 0)), "ae");
}

#[quench_test::test]
fn positions_count_utf16_units() {
    let mut d = Doc::from_text("a\u{e9}\u{1F600}b");
    assert_eq!(d.len_of(0), 5, "the emoji is two units");
    d.delete(p(0, 2), p(0, 4));
    assert_eq!(d.plain(), "a\u{e9}b");
    d.replace(p(0, 1), p(0, 1), "\u{1F600}");
    assert_eq!(d.plain(), "a\u{1F600}\u{e9}b");
    assert_eq!(d.clamp(p(9, 99)), p(0, 5));
}

#[quench_test::test]
fn marks_toggle_over_ranges_and_runs_stay_normal() {
    let mut d = Doc::from_text("hello world\nsecond");
    assert!(d.toggle(p(0, 6), p(0, 11), Mark::Bold));
    assert_eq!(d.paras()[0].runs.len(), 2);
    assert!(d.paras()[0].runs[1].style.bold && d.paras()[0].runs[1].text == "world");
    assert!(
        d.all_marked(p(0, 7), p(0, 9), Mark::Bold) && !d.all_marked(p(0, 4), p(0, 8), Mark::Bold)
    );
    // Partly marked: it is turned on for all; marked everywhere: it is turned off.
    assert!(d.toggle(p(0, 4), p(0, 8), Mark::Bold));
    assert!(d.all_marked(p(0, 4), p(0, 11), Mark::Bold));
    assert!(!d.toggle(p(0, 4), p(0, 11), Mark::Bold));
    assert_eq!(d.paras()[0].runs.len(), 1, "back to one run");
    // Across paragraphs.
    assert!(d.toggle(p(0, 6), p(1, 3), Mark::Italic));
    assert!(d.paras()[1].runs[0].style.italic && d.paras()[1].runs[0].text == "sec");
    // Typing continues the style of the text before it.
    let mut e = Doc::from_text("ab");
    e.toggle(p(0, 0), p(0, 2), Mark::Underline);
    e.replace(p(0, 2), p(0, 2), "c");
    assert_eq!(e.paras()[0].runs.len(), 1);
    assert!(e.style_at(p(0, 3)).underline);
}

#[quench_test::test]
fn kinds_and_composition_sync() {
    let mut d = Doc::from_text("title\nbody");
    d.set_kind(0, Kind::Heading(1));
    d.set_kind(1, Kind::Bullet);
    // Splitting a heading keeps its kind on the first part and makes the rest a plain paragraph.
    d.replace(p(0, 2), p(0, 2), "\n");
    assert_eq!(
        (d.paras()[0].kind, d.paras()[1].kind, d.paras()[2].kind),
        (Kind::Heading(1), Kind::Paragraph, Kind::Bullet)
    );
    d.set_para_text(1, "composed \u{4e2d}\u{6587}");
    assert_eq!(d.text(1), "composed \u{4e2d}\u{6587}");
    // Only what differs is replaced: the style of the rest stays.
    let mut e = Doc::from_text("hello world");
    e.toggle(p(0, 6), p(0, 11), Mark::Bold);
    let caret = e.set_para_text(0, "hello \u{4e2d} world");
    assert_eq!(e.text(0), "hello \u{4e2d} world");
    assert_eq!(caret, p(0, 8));
    assert!(
        e.all_marked(p(0, 8), p(0, 14), Mark::Bold),
        "world stays bold"
    );
    assert_eq!(e.paras()[0].runs.len(), 2);
    assert_eq!(d.plain().lines().count(), 3);
}

#[quench_test::test]
fn sizes_colours_typefaces_alignment_and_lists() {
    let mut d = Doc::from_text("hello world\nitem");
    d.restyle(p(0, 0), p(0, 5), |s| Style {
        size: Some(48),
        color: Some(0x00FF_0000),
        family: Some(Family::Serif),
        ..s
    });
    assert_eq!(d.paras()[0].runs.len(), 2);
    assert_eq!(d.common_style(p(0, 1), p(0, 4)).unwrap().size, Some(48));
    assert_eq!(
        d.common_style(p(0, 0), p(0, 11)),
        None,
        "a range with two styles has no common one"
    );
    assert_eq!(
        d.style_at(p(0, 5)).color,
        Some(0x00FF_0000),
        "typing after styled text goes on in its style"
    );
    d.set_align(0, Align::Center);
    d.set_kind(1, Kind::Numbered);
    // Enter in a list item makes another item and keeps the alignment; in a heading it makes a plain paragraph.
    d.replace(p(1, 4), p(1, 4), "\n");
    assert_eq!(d.paras()[2].kind, Kind::Numbered);
    d.replace(p(0, 11), p(0, 11), "\n");
    assert_eq!(
        (d.paras()[1].kind, d.paras()[1].align),
        (Kind::Paragraph, Align::Center)
    );
    d.set_kind(0, Kind::Heading(2));
    d.replace(p(0, 11), p(0, 11), "\n");
    assert_eq!(d.paras()[1].kind, Kind::Paragraph);
}

quench_test::main!();

#[quench_test::test]
fn a_link_covers_text_survives_editing_and_comes_off() {
    let mut d = Doc::from_text("one two three");
    d.set_link(Pos::new(0, 4), Pos::new(0, 7), Some("https://a.example"));
    assert_eq!(d.link_at(Pos::new(0, 5)), Some("https://a.example"));
    assert_eq!(d.link_at(Pos::new(0, 2)), None);
    // The same address is one link number.
    d.set_link(Pos::new(0, 8), Pos::new(0, 13), Some("https://a.example"));
    let ids: std::collections::HashSet<_> = d.paras()[0]
        .runs
        .iter()
        .filter_map(|r| r.style.link)
        .collect();
    assert_eq!(ids.len(), 1);
    d.set_link(Pos::new(0, 4), Pos::new(0, 7), None);
    assert_eq!(d.link_at(Pos::new(0, 5)), None);
    assert_eq!(d.link_at(Pos::new(0, 10)), Some("https://a.example"));
}

#[quench_test::test]
fn a_table_is_cells_that_edit_alone_and_grows_and_shrinks_by_rows_and_columns() {
    let mut d = Doc::from_text("before\nafter");
    let at = d.insert_table(0, 2, 3);
    assert_eq!(at, Pos::new(1, 0));
    assert_eq!(d.paras().len(), 8);
    assert_eq!(d.table_of(3), Some((1, 6, 2, 3)));
    assert_eq!(d.cell_at(0), None);
    assert_eq!(d.cell_at(7), None);
    // Typing goes into the cell; a line break in pasted text becomes a space; cells are not joined or split.
    d.replace(Pos::new(2, 0), Pos::new(2, 0), "a\nb");
    assert_eq!(d.text(2), "a b");
    assert_eq!(d.paras().len(), 8);
    d.replace(Pos::new(2, 0), Pos::new(3, 0), "");
    assert_eq!(d.text(2), "a b");
    d.replace(Pos::new(0, 6), Pos::new(1, 0), "");
    assert_eq!(
        d.paras().len(),
        8,
        "the end of a paragraph does not join the first cell"
    );
    d.replace(Pos::new(6, 0), Pos::new(7, 0), "");
    assert_eq!(
        d.paras().len(),
        8,
        "nor does the paragraph after join the last cell"
    );
    // Rows and columns.
    let row = d.add_row(2).unwrap();
    assert_eq!(row, Pos::new(4, 0));
    assert_eq!(d.table_of(1), Some((1, 9, 3, 3)));
    let col = d.add_col(2).unwrap();
    assert_eq!(d.table_of(1).map(|t| (t.2, t.3)), Some((3, 4)));
    assert_eq!(col, Pos::new(3, 0));
    assert_eq!(d.text(2), "a b", "the cell kept its text");
    assert_eq!(d.cell_at(3).map(|c| (c.row, c.col)), Some((0, 2)));
    d.delete_col(3).unwrap();
    d.delete_row(5).unwrap();
    assert_eq!(d.table_of(1).map(|t| (t.2, t.3)), Some((2, 3)));
    assert_eq!(d.text(2), "a b");
    // Removing the last row or column removes the table, and the text around it stays.
    d.delete_table(1).unwrap();
    assert_eq!(d.plain(), "before\nafter");
    // Two tables one after the other keep a paragraph between them.
    let mut d = Doc::new();
    d.insert_table(0, 1, 1);
    d.insert_table(0, 1, 1);
    assert_eq!(d.paras().iter().filter(|p| p.cell.is_none()).count(), 3);
}

fn pic(w: f64, h: f64) -> docs_text::Picture {
    docs_text::Picture {
        mime: "image/png",
        data: std::rc::Rc::new(vec![1, 2, 3]),
        width: (w * 100.0) as u32,
        height: (h * 100.0) as u32,
    }
}

#[quench_test::test]
fn a_picture_has_a_paragraph_of_its_own_that_takes_no_text_and_enter_goes_past() {
    let mut d = Doc::from_text("one\ntwo");
    let at = d.insert_picture(0, pic(200.0, 100.0));
    assert_eq!(at, Pos::new(1, 0));
    assert_eq!(d.paras().len(), 3);
    assert_eq!(d.paras()[1].picture, Some(0));
    assert_eq!(d.picture(0).map(|p| p.size()), Some((200.0, 100.0)));
    // Typing in it does nothing; Enter starts a paragraph after it.
    d.replace(Pos::new(1, 0), Pos::new(1, 0), "x");
    assert_eq!(d.plain(), "one\n\ntwo");
    let caret = d.replace(Pos::new(1, 0), Pos::new(1, 0), "\n");
    assert_eq!((caret, d.paras().len()), (Pos::new(2, 0), 4));
    assert_eq!(d.paras()[1].picture, Some(0));
    // Backspace from the paragraph after it takes the picture out.
    d.replace(Pos::new(1, 0), Pos::new(2, 0), "");
    assert!(d.paras().iter().all(|p| p.picture.is_none()));
    // A picture at the end of a document gets a paragraph after it.
    let mut d = Doc::new();
    d.insert_picture(0, pic(10.0, 10.0));
    assert_eq!(d.paras().len(), 3);
    // Scaling keeps the shape and stops at the width given.
    d.scale_picture(0, 2.0, 15.0);
    assert_eq!(d.picture(0).map(|p| p.size()), Some((15.0, 15.0)));
}

#[quench_test::test]
fn table_columns_have_widths_that_add_up_and_a_header_row_is_bold() {
    let mut d = Doc::new();
    d.insert_table(0, 3, 3);
    let t = d.cell_at(1).unwrap().table;
    assert_eq!(d.table_widths(t), [333, 333, 334]);
    d.resize_column(t, 0, 100);
    assert_eq!(d.table_widths(t), [433, 233, 334]);
    d.resize_column(t, 2, 100);
    assert_eq!(d.table_widths(t), [433, 133, 434]);
    // Never below five percent, and always one thousand in all.
    d.resize_column(t, 0, 10_000);
    assert_eq!(
        d.table_widths(t).iter().map(|w| u32::from(*w)).sum::<u32>(),
        1000
    );
    assert!(d.table_widths(t).iter().all(|w| *w >= 50));
    d.set_table_widths(t, &[1, 1, 2]);
    assert_eq!(d.table_widths(t), [250, 250, 500]);
    // A column added or taken away starts the widths over.
    d.add_col(2);
    assert_eq!(d.table_widths(t).len(), 4);
    d.replace(Pos::new(1, 0), Pos::new(1, 0), "Name");
    d.set_table_header(t, true);
    assert!(d.table_header(t) && d.all_marked(Pos::new(1, 0), Pos::new(1, 4), Mark::Bold));
    assert!(!d.all_marked(Pos::new(6, 0), Pos::new(6, 0), Mark::Bold) || d.text(6).is_empty());
    d.set_table_header(t, false);
    assert!(!d.all_marked(Pos::new(1, 0), Pos::new(1, 4), Mark::Bold));
}

#[quench_test::test]
fn list_markers_count_on_by_level_and_start_again_after_other_paragraphs() {
    let mut d = Doc::from_text("a\nb\nc\nd\nplain\ne\nf");
    for i in [0, 1, 2, 3, 5] {
        d.set_kind(i, Kind::Numbered);
    }
    d.set_kind(6, Kind::Bullet);
    d.set_fmt(1, |f| docs_text::ParaFmt { level: 1, ..f });
    d.set_fmt(2, |f| docs_text::ParaFmt { level: 1, ..f });
    d.set_fmt(3, |f| docs_text::ParaFmt { level: 2, ..f });
    let m: Vec<Option<String>> = d.list_markers();
    let shown: Vec<&str> = m.iter().map(|x| x.as_deref().unwrap_or("-")).collect();
    assert_eq!(shown, ["1.", "a.", "b.", "i.", "-", "1.", "\u{2022}"]);
}

#[quench_test::test]
fn the_typeface_list_has_no_name_twice_and_each_has_a_stack_ending_in_a_generic_family() {
    let mut seen = std::collections::HashSet::new();
    for t in docs_text::TYPEFACES {
        assert!(
            seen.insert(t.name.to_lowercase()),
            "{} is listed twice",
            t.name
        );
        assert!(
            ["sans-serif", "serif", "monospace", "cursive"]
                .iter()
                .any(|g| t.css.ends_with(g)),
            "{}",
            t.name
        );
        assert_eq!(
            docs_text::typeface(t.name).map(|f| f.family),
            Some(t.family)
        );
    }
    assert!(docs_text::TYPEFACES.len() >= 80);
}
