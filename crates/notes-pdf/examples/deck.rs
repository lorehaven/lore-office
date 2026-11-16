#![allow(clippy::unwrap_used, clippy::print_stdout)]
//! `cargo run -p notes-pdf --example deck -- out.pdf [theme]` writes a sample deck.
fn main() {
    let text = "# Welcome\n\nNotes are *plain* markdown files that stay on **your** machine, with `code` and a [link](https://example.com). Long lines wrap to the width of the slide the way they do on screen.\n\n- Link notes with [[double brackets]]\n- Tag them\n  - nested item\n- [x] done\n- [ ] to do\n---\n## Tables and quotes\n\n> [!tip] Present it\n> Slides are split by a line with three dashes.\n\n| Shape | Sides |\n|:--|--:|\n| Triangle | 3 |\n| Square | 4 |\n\n```\nfn main() {}\n```\n\n$$E = mc^2$$\n---\n<!-- layout: center -->\n# The end\n\nThank you\n";
    let slides = notes_md::slides(text);
    let theme = std::env::args().nth(2).unwrap_or_else(|| "default".into());
    let bytes = notes_pdf::deck_pdf("Sample", &theme, (1280, 720), &slides);
    std::fs::write(std::env::args().nth(1).unwrap(), bytes).unwrap();
}
