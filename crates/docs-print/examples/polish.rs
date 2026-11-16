#![allow(clippy::unwrap_used)]
//! `cargo run -p docs-print --example polish -- out.pdf`: text with Polish letters, bold and italic, over two pages with a link.
use docs_text::{Doc, Kind, Mark, Pos};

fn main() {
    let line = "Zażółć gęślą jaźń. Pchnąć w tę łódź jeża lub ośm skrzyń fig. ";
    let mut d = Doc::from_text(&format!(
        "Zażółć gęślą jaźń\n{}\n{}",
        line.repeat(8),
        line.repeat(40)
    ));
    d.set_kind(0, Kind::Heading(1));
    d.toggle(Pos::new(1, 0), Pos::new(1, 20), Mark::Bold);
    d.toggle(Pos::new(1, 21), Pos::new(1, 40), Mark::Italic);
    d.set_link(Pos::new(2, 0), Pos::new(2, 6), Some("https://example.org/"));
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        docs_print::to_pdf(&d, &docs_print::Setup::default(), "Polski"),
    )
    .unwrap();
}
