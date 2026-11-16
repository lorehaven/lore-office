#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::cast_possible_truncation,
    clippy::missing_const_for_fn
)]
use pdf_edit::{Error, Part, all_parts, assemble};
use pdf_write::{Document as Writer, Font, Page};
use quench_pdf::{Document, extract_pages};

fn file(texts: &[&str]) -> Vec<u8> {
    let mut doc = Writer::new();
    for t in texts {
        let mut page = Page::new(300.0, 400.0);
        page.text(20.0, 50.0, t, Font::Helvetica, 14.0, 0);
        page.image(
            20.0,
            100.0,
            40.0,
            20.0,
            2,
            1,
            &[255, 0, 0, 255, 0, 0, 255, 128],
        );
        doc.add(page);
    }
    doc.finish()
}

#[quench_test::test]
fn pages_of_several_files_are_put_together_in_order_with_their_pictures() {
    let a = Document::load(&file(&["one", "two", "three"])).unwrap();
    let b = Document::load(&file(&["four", "five"])).unwrap();
    let sources = [a, b];
    let out = assemble(&sources, &all_parts(&sources)).unwrap();
    assert_eq!(
        extract_pages(&out).unwrap(),
        ["one", "two", "three", "four", "five"]
    );
    let doc = Document::load(&out).unwrap();
    assert_eq!(doc.page_count(), 5);
    // The picture (and its mask) came along: the drawing has it.
    assert!(doc.render_page(4).unwrap().svg.contains("<image"));
    // Reorder, extract, delete: all are a choice of parts.
    let parts = [
        Part {
            source: 1,
            page: 1,
            turn: 0,
        },
        Part {
            source: 0,
            page: 0,
            turn: 0,
        },
        Part {
            source: 1,
            page: 1,
            turn: 0,
        },
    ];
    assert_eq!(
        extract_pages(&assemble(&sources, &parts).unwrap()).unwrap(),
        ["five", "one", "five"]
    );
}

#[quench_test::test]
fn pages_are_turned_by_quarter_turns_added_to_their_own() {
    let a = [Document::load(&file(&["x"])).unwrap()];
    let turned = |turn| {
        let out = assemble(
            &a,
            &[Part {
                source: 0,
                page: 0,
                turn,
            }],
        )
        .unwrap();
        let page = Document::load(&out).unwrap().render_page(0).unwrap();
        (page.width, page.height)
    };
    assert_eq!(turned(0), (300.0, 400.0));
    assert_eq!(turned(90), (400.0, 300.0));
    assert_eq!(turned(-90), (400.0, 300.0));
    assert_eq!(turned(180), (300.0, 400.0));
    // Turning a turned file again adds up.
    let once = assemble(
        &a,
        &[Part {
            source: 0,
            page: 0,
            turn: 90,
        }],
    )
    .unwrap();
    let again = [Document::load(&once).unwrap()];
    let out = assemble(
        &again,
        &[Part {
            source: 0,
            page: 0,
            turn: 90,
        }],
    )
    .unwrap();
    let page = Document::load(&out).unwrap().render_page(0).unwrap();
    assert_eq!((page.width, page.height), (300.0, 400.0));
}

#[quench_test::test]
fn mistakes_are_errors_and_a_file_written_again_still_reads() {
    let a = [Document::load(&file(&["x"])).unwrap()];
    assert_eq!(assemble(&a, &[]), Err(Error::Empty));
    assert_eq!(
        assemble(
            &a,
            &[Part {
                source: 0,
                page: 3,
                turn: 0
            }]
        ),
        Err(Error::NoSuchPage { source: 0, page: 3 })
    );
    assert_eq!(
        assemble(
            &a,
            &[Part {
                source: 2,
                page: 0,
                turn: 0
            }]
        ),
        Err(Error::NoSuchPage { source: 2, page: 0 })
    );
    // What was made can be the source of the next.
    let first = assemble(&a, &all_parts(&a)).unwrap();
    let second = assemble(
        &[Document::load(&first).unwrap()],
        &[Part {
            source: 0,
            page: 0,
            turn: 0,
        }],
    )
    .unwrap();
    assert_eq!(extract_pages(&second).unwrap(), ["x"]);
}

quench_test::main!();

#[quench_test::test]
fn markup_is_added_to_pages_and_read_and_drawn_back_and_kept_through_further_changes() {
    use pdf_edit::{NewMarkup, assemble_marked};
    let a = [Document::load(&file(&["one", "two"])).unwrap()];
    let parts = all_parts(&a);
    let marks = [
        (
            1,
            NewMarkup {
                kind: "Highlight".into(),
                boxes: vec![(20.0, 340.0, 120.0, 355.0)],
                color: (255, 235, 59),
                contents: String::new(),
            },
        ),
        (
            1,
            NewMarkup {
                kind: "Text".into(),
                boxes: vec![(150.0, 300.0, 170.0, 320.0)],
                color: (255, 200, 0),
                contents: "Zażółć note".into(),
            },
        ),
    ];
    let out = assemble_marked(&a, &parts, &marks).unwrap();
    let doc = Document::load(&out).unwrap();
    assert!(doc.markup_of(0).is_empty());
    let got = doc.markup_of(1);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].kind, "Highlight");
    assert_eq!(got[0].boxes, [(20.0, 340.0, 120.0, 355.0)]);
    assert_eq!(got[0].color, (255, 235, 59));
    assert_eq!(got[1].contents, "Zażółć note");
    let svg = doc.render_page(1).unwrap().svg;
    assert!(
        svg.contains("mix-blend-mode:multiply") && svg.contains("#ffeb3b"),
        "{svg}"
    );
    // Taking the pages through the editor again keeps what was put on them, and adds to it.
    let again = assemble_marked(
        &[Document::load(&out).unwrap()],
        &all_parts(&[Document::load(&out).unwrap()]),
        &[(
            1,
            NewMarkup {
                kind: "Underline".into(),
                boxes: vec![(20.0, 300.0, 80.0, 312.0)],
                color: (255, 0, 0),
                contents: String::new(),
            },
        )],
    )
    .unwrap();
    assert_eq!(Document::load(&again).unwrap().markup_of(1).len(), 3);
    assert_eq!(extract_pages(&again).unwrap(), ["one", "two"]);
}

#[quench_test::test]
fn text_is_set_on_a_page_after_its_own_contents_in_the_embedded_font_and_reads_back() {
    use pdf_edit::{NewText, assemble_full};
    let a = [Document::load(&file(&["one", "two"])).unwrap()];
    let parts = all_parts(&a);
    let texts = [
        (
            0,
            NewText {
                x: 100.0,
                y: 200.0,
                size: 12.0,
                color: (0, 0, 120),
                text: "Zażółć gęślą".into(),
            },
        ),
        (
            0,
            NewText {
                x: 100.0,
                y: 180.0,
                size: 10.0,
                color: (0, 0, 0),
                text: "second".into(),
            },
        ),
    ];
    let out = assemble_full(&a, &parts, &[], &texts).unwrap();
    let pages = extract_pages(&out).unwrap();
    assert!(
        pages[0].contains("one")
            && pages[0].contains("Zażółć gęślą")
            && pages[0].contains("second"),
        "{:?}",
        pages[0]
    );
    assert_eq!(pages[1], "two");
    let page = Document::load(&out).unwrap().render_page(0).unwrap();
    assert!(
        page.svg.contains("font-size=\"12\"")
            && page.runs.iter().any(|r| r.text.contains("Zażółć")),
        "{:?}",
        page.runs
    );
    let run = page
        .runs
        .iter()
        .find(|r| r.text.contains("Zażółć"))
        .unwrap();
    assert!(
        (run.x - 100.0).abs() < 0.5 && (run.y - 200.0).abs() < 0.5
            || (run.y - (400.0 - 200.0)).abs() < 0.5,
        "{run:?}"
    );
    // Written again, with more.
    let again = assemble_full(
        &[Document::load(&out).unwrap()],
        &all_parts(&[Document::load(&out).unwrap()]),
        &[],
        &[(
            1,
            NewText {
                x: 50.0,
                y: 50.0,
                size: 11.0,
                color: (0, 0, 0),
                text: "later".into(),
            },
        )],
    )
    .unwrap();
    let both = extract_pages(&again).unwrap();
    assert!(both[0].contains("Zażółć") && both[1].contains("later") && both[1].contains("two"));
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[quench_test::test]
fn damaged_pdfs_are_read_drawn_and_edited_or_refused_but_never_a_panic() {
    use pdf_edit::{NewMarkup, NewText, assemble_full};
    let base = file(&["one", "Zażółć two", "three"]);
    let marked = {
        let d = [Document::load(&base).unwrap()];
        assemble_full(
            &d,
            &all_parts(&d),
            &[(
                0,
                NewMarkup {
                    kind: "Highlight".into(),
                    boxes: vec![(10.0, 10.0, 90.0, 30.0)],
                    color: (255, 255, 0),
                    contents: "c".into(),
                },
            )],
            &[(
                1,
                NewText {
                    x: 20.0,
                    y: 20.0,
                    size: 10.0,
                    color: (0, 0, 0),
                    text: "ąę".into(),
                },
            )],
        )
        .unwrap()
    };
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    std::panic::set_hook(Box::new(|info| eprintln!("PANIC {info}")));
    for source in [&base, &marked] {
        for round in 0..250 {
            let mut bytes = source.clone();
            for _ in 0..=(round % 6) {
                let at = (rng.next() as usize) % bytes.len();
                match rng.next() % 4 {
                    0 => bytes[at] = b"<>[]()/ 0123456789\n"[(rng.next() % 19) as usize],
                    1 => bytes.truncate(at.max(8)),
                    2 => {
                        let end = (at + 1 + (rng.next() as usize) % 20).min(bytes.len());
                        bytes.drain(at..end);
                    }
                    _ => bytes.insert(at, rng.next() as u8),
                }
            }
            std::panic::catch_unwind(|| {
                let _ = extract_pages(&bytes);
                if let Ok(doc) = Document::load(&bytes) {
                    for i in 0..doc.page_count().min(8) {
                        if let Some(page) = doc.render_page(i) {
                            assert!(page.svg.ends_with("</svg>"));
                        }
                        let _ = doc.markup_of(i);
                    }
                    let _ = doc.outline();
                    let docs = [doc];
                    let _ = assemble_full(&docs, &all_parts(&docs), &[], &[]);
                }
            })
            .unwrap_or_else(|_| panic!("round {round} panicked"));
        }
    }
    let _ = std::panic::take_hook();
}
