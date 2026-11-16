//! A deck as a PDF: one page per slide.
//!
//! Each slide is laid out on the stage (the same pixel size the screen uses, so line breaks and proportions match what is
//! shown), then drawn on a page of that size in points. The layout is a plain top-to-bottom flow of the markdown blocks:
//! headings, paragraphs with bold, italic, code and links, lists (also nested and tasks), code blocks, quotes and callouts,
//! tables and formulas. A slide that is longer than the page is cut at the bottom, as it is on screen.
//! The layout `center` and `title` centre the slide's text vertically; `two-col` is set in one column.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::missing_const_for_fn,
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    clippy::suboptimal_flops,
    clippy::match_same_arms,
    clippy::semicolon_if_nothing_returned
)]

mod flow;
mod look;

pub use look::{Look, look};

use notes_md::Slide;
use pdf_write::{Document, Page};

/// Pixels of the stage are drawn as 3/4 of a point.
const PT: f64 = 0.75;

/// The deck as a PDF. `stage` is the slide's size in pixels (as `notes-app` gives it), `theme` a theme name.
#[must_use]
pub fn deck_pdf(title: &str, theme: &str, stage: (u32, u32), slides: &[Slide]) -> Vec<u8> {
    let look = look(theme);
    let (w, h) = (f64::from(stage.0), f64::from(stage.1));
    let mut doc = Document::new();
    doc.title(title);
    let total = slides.len();
    for (n, slide) in slides.iter().enumerate() {
        let mut page = Page::new(w * PT, h * PT);
        page.fill_rect(0.0, 0.0, w * PT, h * PT, look.background);
        let flowed = flow::slide(&slide.markdown, slide.layout.as_deref(), (w, h), &look);
        page.clipped(0.0, 0.0, w * PT, h * PT, |p| flow::paint(p, &flowed, PT));
        // The number of the slide, in the corner.
        let label = format!("{} / {total}", n + 1);
        let width = pdf_write::text_width(&label, pdf_write::Font::Helvetica, 14.0);
        page.text(
            w * PT - width - 24.0,
            h * PT - 16.0,
            &label,
            pdf_write::Font::Helvetica,
            14.0,
            look.muted,
        );
        doc.add(page);
    }
    doc.finish()
}
