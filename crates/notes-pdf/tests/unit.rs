#![allow(clippy::unwrap_used)]
use notes_md::slides;
use notes_pdf::{deck_pdf, look};

fn pages(bytes: &[u8]) -> Vec<String> {
    quench_pdf::extract_pages(bytes).unwrap()
}

#[quench_test::test]
fn each_slide_is_a_page_with_its_text_and_a_number() {
    let deck = slides(
        "# First title\n\nA paragraph with **bold**, *italic*, `code` and a [[Link|shown link]].\n\n- one\n- two\n  - nested\n1. first\n2. second\n\n- [x] done\n- [ ] todo\n---\n## Second\n\n> [!warning] Careful\n> text of the callout\n\n| Shape | Sides |\n|:--|--:|\n| Triangle | 3 |\n\n```\nlet x = 1;\n```\n\n$$E = mc^2$$\n---\nCaf\u{e9} \u{201c}quoted\u{201d} and \u{4e2d}\n\n???\nnot on the slide\n",
    );
    assert_eq!(deck.len(), 3);
    let bytes = deck_pdf("Deck", "default", (1280, 720), &deck);
    let out = pages(&bytes);
    assert_eq!(out.len(), 3);
    for want in [
        "First title",
        "A paragraph with",
        "bold",
        "shown link",
        "one",
        "nested",
        "first",
        "[x]",
        "[ ]",
        "1 / 3",
    ] {
        assert!(out[0].contains(want), "{want} in {:?}", out[0]);
    }
    for want in [
        "Second",
        "Careful",
        "text of the callout",
        "Shape",
        "Triangle",
        "let x = 1;",
        "E = mc^2",
        "2 / 3",
    ] {
        assert!(out[1].contains(want), "{want} in {:?}", out[1]);
    }
    assert!(
        out[2].contains("Caf\u{e9}") && out[2].contains('\u{fffd}'),
        "what even the embedded font lacks is a box that reads as the replacement character: {:?}",
        out[2]
    );
    assert!(
        !out.iter().any(|p| p.contains("not on the slide")),
        "speaker notes stay off the page"
    );
    assert!(!out[0].contains("[["), "links show what they say");
}

#[quench_test::test]
fn the_page_has_the_size_of_the_stage_and_the_colours_of_the_theme() {
    let deck = slides("# Title\n");
    let wide = deck_pdf("T", "night", (1280, 720), &deck);
    let square = deck_pdf("T", "paper", (900, 900), &deck);
    let text = String::from_utf8_lossy(&wide);
    assert!(
        text.contains("/MediaBox [0 0 960 540]"),
        "{}",
        &text[..text.len().min(600)]
    );
    assert!(String::from_utf8_lossy(&square).contains("/MediaBox [0 0 675 675]"));
    assert_ne!(look("night").background, look("default").background);
    assert_eq!(look("nonsense"), look("default"));
}

#[quench_test::test]
fn long_text_wraps_and_a_slide_longer_than_the_page_does_not_break_the_file() {
    let long = "word ".repeat(400);
    let deck = slides(&format!("# T\n\n{long}\n\n{}", "- item\n".repeat(80)));
    let out = pages(&deck_pdf("T", "contrast", (1024, 768), &deck));
    assert_eq!(out.len(), 1);
    assert!(out[0].matches("word").count() > 10);
    // Centred layouts and an empty deck.
    let centred = slides("<!-- layout: center -->\n# Middle\n\nsome text\n");
    assert!(pages(&deck_pdf("T", "default", (1280, 720), &centred))[0].contains("Middle"));
    assert_eq!(
        pages(&deck_pdf("T", "default", (1280, 720), &slides(""))).len(),
        1
    );
}

quench_test::main!();
