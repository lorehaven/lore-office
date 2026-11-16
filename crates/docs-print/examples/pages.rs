#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_possible_truncation
)]
//! `cargo run -p docs-print --example pages -- out.pdf`
use docs_text::{Align, Doc, Family, Kind, Mark, Pos, Style};

fn main() {
    let body = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";
    let mut d = Doc::from_text(&format!(
        "A document title\nIntroduction\n{body}\n{body}\nbullet one\nbullet two with a longer line that wraps around to the next line when it is long enough\nfirst step\nsecond step\nCentred line\n{body}"
    ));
    d.set_kind(0, Kind::Heading(1));
    d.set_kind(1, Kind::Heading(2));
    d.set_align(0, Align::Center);
    d.set_align(3, Align::Justify);
    d.set_align(8, Align::Center);
    d.set_kind(4, Kind::Bullet);
    d.set_kind(5, Kind::Bullet);
    d.set_kind(6, Kind::Numbered);
    d.set_kind(7, Kind::Numbered);
    d.insert_table(2, 2, 3);
    for (i, w) in ["Name", "Qty", "Price", "Nut", "4", "1.20"]
        .iter()
        .enumerate()
    {
        d.replace(Pos::new(3 + i, 0), Pos::new(3 + i, 0), w);
    }
    let (w, h) = (48_u32, 24_u32);
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|i| [((i % w) * 5) as u8, 60, 190, 255])
        .collect();
    d.insert_picture(
        0,
        docs_text::Picture {
            mime: "image/png",
            data: std::rc::Rc::new(quench_image::encode_png(w, h, &rgba)),
            width: 14400,
            height: 7200,
        },
    );
    d.toggle(Pos::new(2, 6), Pos::new(2, 17), Mark::Bold);
    d.toggle(Pos::new(2, 18), Pos::new(2, 23), Mark::Italic);
    d.toggle(Pos::new(2, 30), Pos::new(2, 40), Mark::Underline);
    d.restyle(Pos::new(3, 0), Pos::new(3, 5), |s| Style {
        size: Some(32),
        color: Some(0x00C0_0000),
        family: Some(Family::Sans),
        ..s
    });
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        docs_print::to_pdf(&d, &docs_print::Setup::default(), "Sample"),
    )
    .unwrap();
}
