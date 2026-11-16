#![allow(clippy::unwrap_used)]
//! `cargo run -p pdf-edit --example highlight -- in.pdf out.pdf`: a yellow highlight, an underline and a note on the first page.
use pdf_edit::{NewMarkup, NewText, all_parts, assemble_full};
use quench_pdf::Document;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sources = [Document::load(&std::fs::read(&args[1]).unwrap()).unwrap()];
    let marks = [
        (
            0,
            NewMarkup {
                kind: "Highlight".into(),
                boxes: vec![(72.0, 700.0, 300.0, 716.0)],
                color: (255, 235, 59),
                contents: String::new(),
            },
        ),
        (
            0,
            NewMarkup {
                kind: "Underline".into(),
                boxes: vec![(72.0, 650.0, 300.0, 664.0)],
                color: (220, 38, 38),
                contents: String::new(),
            },
        ),
        (
            0,
            NewMarkup {
                kind: "Text".into(),
                boxes: vec![(400.0, 700.0, 416.0, 716.0)],
                color: (255, 200, 0),
                contents: "Zażółć".into(),
            },
        ),
    ];
    let texts = [(
        0,
        NewText {
            x: 330.0,
            y: 650.0,
            size: 12.0,
            color: (0, 0, 160),
            text: "Zażółć gęślą jaźń".into(),
        },
    )];
    std::fs::write(
        &args[2],
        assemble_full(&sources, &all_parts(&sources), &marks, &texts).unwrap(),
    )
    .unwrap();
}
