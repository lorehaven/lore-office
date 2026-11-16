#![allow(clippy::unwrap_used, clippy::print_stdout)]
//! `cargo run --release -p pdf-edit --example time -- file.pdf`: how long loading, text, a drawn page and page operations take.
use std::time::Instant;

fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let t = Instant::now();
    let doc = quench_pdf::Document::load(&bytes).unwrap();
    println!("load: {:?}, {} pages", t.elapsed(), doc.page_count());
    let t = Instant::now();
    let text = quench_pdf::extract_pages(&bytes).unwrap();
    println!(
        "text of all pages: {:?} ({} chars)",
        t.elapsed(),
        text.iter().map(String::len).sum::<usize>()
    );
    let t = Instant::now();
    let page = doc.render_page(100).unwrap();
    println!(
        "draw page 101: {:?} ({} KiB svg, {} runs)",
        t.elapsed(),
        page.svg.len() / 1024,
        page.runs.len()
    );
    let t = Instant::now();
    let out = pdf_edit::assemble(
        std::slice::from_ref(&doc),
        &pdf_edit::all_parts(std::slice::from_ref(&doc)),
    )
    .unwrap();
    println!(
        "reassemble all: {:?} ({} KiB)",
        t.elapsed(),
        out.len() / 1024
    );
}
