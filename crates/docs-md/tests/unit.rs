#![allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
use docs_md::{from_markdown, to_markdown};
use docs_text::{Doc, Kind, Mark, Pos};

#[quench_test::test]
fn reads_headings_lists_emphasis_links_code_tables_and_joins_lines() {
    let doc = from_markdown(
        "# Title\n\nA **bold** and *italic* and ~~gone~~ and `code` with a [link](https://a.example/x_y) and \\* a star.\nSecond line of it.\n\n- one\n- two\n\n1. first\n2. second\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n\n```\nlet x = 1;\n```\n\n> quoted\n",
    );
    let text: Vec<String> = (0..doc.paras().len()).map(|i| doc.text(i)).collect();
    assert_eq!(text[0], "Title");
    assert_eq!(doc.paras()[0].kind, Kind::Heading(1));
    assert_eq!(
        text[1],
        "A bold and italic and gone and code with a link and * a star. Second line of it."
    );
    assert!(doc.all_marked(Pos::new(1, 2), Pos::new(1, 6), Mark::Bold));
    assert!(doc.all_marked(Pos::new(1, 11), Pos::new(1, 17), Mark::Italic));
    assert!(doc.all_marked(Pos::new(1, 22), Pos::new(1, 26), Mark::Strike));
    assert_eq!(doc.link_at(Pos::new(1, 47)), Some("https://a.example/x_y"));
    assert_eq!(&text[2..6], ["one", "two", "first", "second"]);
    assert_eq!(doc.paras()[2].kind, Kind::Bullet);
    assert_eq!(doc.paras()[4].kind, Kind::Numbered);
    let t = doc.paras().iter().position(|p| p.cell.is_some()).unwrap();
    assert_eq!(doc.table_of(t).map(|x| (x.2, x.3)), Some((3, 2)));
    assert_eq!(&text[t..t + 6], ["A", "B", "1", "2", "3", "4"]);
    assert!(text.contains(&"let x = 1;".to_string()) && text.contains(&"quoted".to_string()));
}

#[quench_test::test]
fn writes_what_it_reads_so_a_document_comes_back_the_same() {
    let md = "# Title\n\nSome **bold**, *italic*, ~~struck~~ and [a link](https://example.org/p) in text.\n\n- one\n- two\n\n1. first\n2. second\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nLast paragraph with a \\* star and a \\_ score.\n";
    let doc = from_markdown(md);
    let out = to_markdown(&doc);
    assert_eq!(out, md);
    let again = from_markdown(&out);
    assert_eq!(again.plain(), doc.plain());
}

#[quench_test::test]
fn an_empty_document_and_odd_input_give_something_sensible() {
    assert_eq!(to_markdown(&Doc::new()), "\n");
    assert_eq!(from_markdown("").plain(), "");
    let d = from_markdown("**unclosed and [broken](link\n\n#nospace\n");
    assert!(d.plain().contains("unclosed") && d.plain().contains("#nospace"));
}

#[quench_test::test]
fn the_web_page_has_the_structure_the_formatting_the_links_and_the_table() {
    let mut doc = from_markdown(
        "# Head & title\n\nA **bold** [link](https://e.org/?a=1&b=2).\n\n- one\n- two\n\n| H | I |\n| --- | --- |\n| 1 | 2 |\n",
    );
    doc.toggle(Pos::new(1, 2), Pos::new(1, 6), Mark::Highlight);
    let html = docs_md::to_html(&doc, "T<1>");
    assert!(html.starts_with("<!doctype html>") && html.contains("<title>T&lt;1&gt;</title>"));
    assert!(html.contains("<h1>Head &amp; title</h1>"), "{html}");
    assert!(html.contains("<b>bold</b>") && html.contains("background:#ffff00"));
    assert!(html.contains("<a href=\"https://e.org/?a=1&amp;b=2\">link</a>"));
    assert!(html.contains("<ul>\n<li>one</li>\n<li>two</li>\n</ul>"));
    assert!(html.contains("<table>") && html.contains("<td style=\"width:50%\">1</td>"));
    assert!(html.ends_with("</body></html>\n"));
}

#[quench_test::test]
fn a_line_break_inside_a_paragraph_is_two_spaces_and_comes_back() {
    let doc = from_markdown("first  \nsecond\\\nthird\n");
    assert_eq!(doc.paras().len(), 1);
    assert_eq!(doc.text(0), "first\u{2028}second\u{2028}third");
    assert_eq!(to_markdown(&doc), "first  \nsecond  \nthird\n");
    assert!(docs_md::to_html(&doc, "t").contains("first<br>second<br>third"));
}

#[quench_test::test]
fn headings_go_down_to_the_sixth_level() {
    let doc = from_markdown("#### four\n\n###### six\n");
    assert_eq!(doc.paras()[0].kind, docs_text::Kind::Heading(4));
    assert_eq!(doc.paras()[1].kind, docs_text::Kind::Heading(6));
    assert_eq!(to_markdown(&doc), "#### four\n\n###### six\n");
    assert!(docs_md::to_html(&doc, "t").contains("<h6>six</h6>"));
}

#[quench_test::test]
fn any_text_is_a_document_and_any_document_is_markdown_and_a_page_without_a_panic() {
    let pieces = [
        "#", "# ", "**", "*", "_", "~~", "`", "[", "](", ")", "|", " | ", "---", "\n", "\n\n",
        "- ", "1. ", "> ", "```", "  \n", "\\", "word", " ", "\u{2028}", "ą", "![", "<", "&",
    ];
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..1500 {
        let mut text = String::new();
        for _ in 0..40 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            text.push_str(pieces[(state % pieces.len() as u64) as usize]);
        }
        let doc = from_markdown(&text);
        let md = to_markdown(&doc);
        let _ = from_markdown(&md);
        let html = docs_md::to_html(&doc, "t");
        assert!(html.ends_with("</html>\n"));
    }
}
quench_test::main!();
