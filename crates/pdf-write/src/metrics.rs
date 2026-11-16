//! Widths of the standard fonts' characters (Adobe's metrics for Helvetica, in 1/1000 em), for fitting and aligning text.

use crate::{Family, Font};

/// Helvetica, characters 32 (space) to 126.
const REGULAR: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Helvetica-Bold, the same range.
const BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Times-Roman.
const SERIF: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611,
    556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722,
    722, 611, 333, 278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500,
    278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
];

const fn width_of(c: char, family: Family, bold: bool) -> u16 {
    if matches!(family, Family::Mono) {
        return 600;
    }
    let table = match (family, bold) {
        (Family::Serif, _) => &SERIF,
        (_, true) => &BOLD,
        _ => &REGULAR,
    };
    match c {
        ' '..='~' => table[c as usize - 32],
        // Latin-1 letters and symbols vary; an average keeps layout close.
        '\u{a0}'..='\u{ff}' | '\u{2022}' | '\u{2013}' | '\u{20ac}' => 556,
        '\u{2014}' | '\u{2026}' | '\u{2122}' => 1000,
        '\u{2018}' | '\u{2019}' => 222,
        '\u{201c}' | '\u{201d}' => 333,
        '\t' => table[0],
        _ => table['?' as usize - 32],
    }
}

/// How wide `text` is in `font` at `size` points.
#[must_use]
pub fn text_width(text: &str, font: Font, size: f64) -> f64 {
    // What the standard fonts lack is set in the embedded font, whose widths are its own.
    if crate::unicode::needed(text) {
        return crate::unicode::width(text, size);
    }
    let (family, bold) = (font.family(), font.bold());
    let units: u32 = text
        .chars()
        .map(|c| u32::from(width_of(c, family, bold)))
        .sum();
    f64::from(units) * size / 1000.0
}
