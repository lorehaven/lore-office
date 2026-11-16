#![allow(clippy::unwrap_used)]
//! `cargo run --release -p docs-print --example long_pdf -- out.pdf`: a PDF of about 330 pages, to time readers on.
use docs_text::Doc;

fn main() {
    let line = "Zażółć gęślą jaźń. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt. ";
    let d = Doc::from_text(&vec![line.repeat(3); 3000].join("\n"));
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        docs_print::to_pdf(&d, &docs_print::Setup::default(), "long"),
    )
    .unwrap();
}
