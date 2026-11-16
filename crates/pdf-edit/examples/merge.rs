#![allow(clippy::unwrap_used, clippy::print_stdout)]
//! `cargo run -p pdf-edit --example merge -- out.pdf a.pdf b.pdf ...`: every page of every file, in order.
use quench_pdf::Document;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sources: Vec<Document> = args[2..]
        .iter()
        .map(|p| Document::load(&std::fs::read(p).unwrap()).unwrap())
        .collect();
    let out = pdf_edit::assemble(&sources, &pdf_edit::all_parts(&sources)).unwrap();
    std::fs::write(&args[1], out).unwrap();
}
