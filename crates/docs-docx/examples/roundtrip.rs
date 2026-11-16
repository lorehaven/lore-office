#![allow(clippy::unwrap_used, clippy::print_stdout)]
//! `cargo run -p docs-docx --example roundtrip -- in.docx out.docx`: read a file, write it back over its own package, and say what
//! was simplified.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let read = docs_docx::read(&std::fs::read(&args[1]).unwrap()).unwrap();
    println!(
        "{} paragraphs, {} table(s) flattened, {} picture(s) lost",
        read.doc.paras().len(),
        read.report.tables,
        read.report.pictures
    );
    std::fs::write(
        &args[2],
        docs_docx::write(&read.doc, &read.package).unwrap(),
    )
    .unwrap();
}
