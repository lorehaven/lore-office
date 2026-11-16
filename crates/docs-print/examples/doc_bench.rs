#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::cast_precision_loss)]
//! `cargo run --release -p docs-print --example bench`: how long a long document takes to lay out and write.
use docs_text::{Doc, Mark, Pos};
use std::time::Instant;

fn main() {
    let line = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. ";
    let mut d = Doc::from_text(&vec![line.repeat(3); 3000].join("\n"));
    for i in (0..3000).step_by(7) {
        d.toggle(Pos::new(i, 5), Pos::new(i, 40), Mark::Bold);
    }
    let t = Instant::now();
    let laid = docs_print::layout(&d, &docs_print::Setup::default());
    println!("layout: {} pages in {:?}", laid.pages.len(), t.elapsed());
    let t = Instant::now();
    let pdf = docs_print::to_pdf(&d, &docs_print::Setup::default(), "x");
    println!("pdf: {} KiB in {:?}", pdf.len() / 1024, t.elapsed());
    let t = Instant::now();
    let docx = docs_docx::write_new(&d).unwrap();
    println!("docx: {} KiB in {:?}", docx.len() / 1024, t.elapsed());
    let t = Instant::now();
    let back = docs_docx::read(&docx).unwrap();
    println!(
        "read: {} paragraphs in {:?}",
        back.doc.paras().len(),
        t.elapsed()
    );
    let t = Instant::now();
    let md = docs_md::to_markdown(&d);
    let _ = docs_md::from_markdown(&md);
    println!("markdown round trip in {:?}", t.elapsed());
}
