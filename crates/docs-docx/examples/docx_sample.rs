#![allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
//! `cargo run -p docs-docx --example docx_sample -- out.docx`
use docs_text::{Doc, Kind, Mark, Pos};

fn main() {
    let mut d = Doc::from_text(
        "A Title\nSome bold and italic text with caf\u{e9}\nitem one\nitem two\nLast paragraph",
    );
    d.set_kind(0, Kind::Heading(1));
    d.set_kind(2, Kind::Bullet);
    d.set_kind(3, Kind::Bullet);
    d.toggle(Pos::new(1, 5), Pos::new(1, 9), Mark::Bold);
    d.toggle(Pos::new(1, 14), Pos::new(1, 20), Mark::Italic);
    d.set_link(
        Pos::new(4, 0),
        Pos::new(4, 4),
        Some("https://example.org/page"),
    );
    d.insert_table(3, 2, 3);
    for (i, w) in ["Name", "Qty", "Price", "Nut", "4", "1.20"]
        .iter()
        .enumerate()
    {
        d.replace(Pos::new(4 + i, 0), Pos::new(4 + i, 0), w);
    }
    d.toggle(Pos::new(4, 0), Pos::new(4, 4), Mark::Strike);
    d.toggle(Pos::new(1, 0), Pos::new(1, 4), Mark::Highlight);
    d.toggle(Pos::new(1, 5), Pos::new(1, 9), Mark::Superscript);
    d.set_header("Sample report");
    d.set_footer("Page {page} of {pages}");
    d.set_fmt(2, |f| docs_text::ParaFmt { indent: 1440, ..f });
    d.set_fmt(4, |f| docs_text::ParaFmt {
        page_break: true,
        ..f
    });
    let (w, h) = (64_u32, 32_u32);
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|i| [((i % w) * 4) as u8, ((i / w) * 8) as u8, 160, 255])
        .collect();
    d.insert_picture(
        0,
        docs_text::Picture {
            mime: "image/png",
            data: std::rc::Rc::new(quench_image::encode_png(w, h, &rgba)),
            width: 12800,
            height: 6400,
        },
    );
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        docs_docx::write_new(&d).unwrap(),
    )
    .unwrap();
}
