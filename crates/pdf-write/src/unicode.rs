//! Text the standard fonts cannot show: set in Noto Sans (the font quench-text carries), which is embedded in the file.
//!
//! The whole font is put in the file once a page needs it (compressed), as a CID font with identity encoding, with a
//! `ToUnicode` map so the text can still be searched and copied. Bold is drawn by thickening the glyphs with a thin outline,
//! italic by slanting them; serif and monospaced text in such a string comes out in the same sans face.

use quench_text::Font as Ttf;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

/// Whether the standard fonts (`WinAnsi`) lack a character of `text`.
pub fn needed(text: &str) -> bool {
    text.chars().any(|c| crate::win_ansi(c) == b'?' && c != '?')
}

pub fn font() -> Arc<Ttf> {
    quench_text::noto_sans()
}

/// The glyph for a character: its own, else the font's "missing" box (glyph 0).
pub fn glyph(font: &Ttf, c: char) -> u16 {
    font.glyph(c).unwrap_or(0)
}

/// How wide `text` is at `size` points.
pub fn width(text: &str, size: f64) -> f64 {
    let font = font();
    let units: u32 = text
        .chars()
        .map(|c| u32::from(font.advance(glyph(&font, c))))
        .sum();
    f64::from(units) * size / f64::from(font.units_per_em())
}

/// The glyphs of `text` as a PDF hex string, noting which glyph stands for which character.
pub fn encode(text: &str, used: &mut BTreeMap<u16, char>) -> String {
    let font = font();
    let mut out = String::from("<");
    for c in text.chars() {
        let g = glyph(&font, c);
        // The box stands for any character the font lacks.
        used.entry(g).or_insert(if g == 0 { '\u{fffd}' } else { c });
        let _ = write!(out, "{g:04X}");
    }
    out.push('>');
    out
}

fn zlib(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9c];
    out.extend(quench_archive::deflate::compress(raw));
    out.extend(quench_archive::checksum::adler32(raw).to_be_bytes());
    out
}

/// The objects that make the font, numbered from `first`: the Type0 font (`first`), its CID font, descriptor, file and
/// `ToUnicode` map. Each is the body of an object.
pub fn objects(used: &BTreeMap<u16, char>, first: usize) -> Vec<Vec<u8>> {
    let font = font();
    let per = f64::from(font.units_per_em());
    let thousandth = |v: f64| (v * 1000.0 / per).round() as i64;
    let mut widths = String::new();
    for g in used.keys() {
        let _ = write!(widths, "{g} [{}] ", thousandth(f64::from(font.advance(*g))));
    }
    let (ascent, descent) = (
        thousandth(f64::from(font.ascent())),
        thousandth(f64::from(font.descent())),
    );
    let mut map = String::new();
    let entries: Vec<(&u16, &char)> = used.iter().collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(map, "{} beginbfchar", chunk.len());
        for (g, c) in chunk {
            let mut units = [0u16; 2];
            let mut hex = String::new();
            for u in c.encode_utf16(&mut units).iter() {
                let _ = write!(hex, "{u:04X}");
            }
            let _ = writeln!(map, "<{g:04X}> <{hex}>");
        }
        map.push_str("endbfchar\n");
    }
    let to_unicode = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n{map}endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend"
    );
    let stream = |dict: &str, data: &[u8]| {
        let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        body
    };
    let file = zlib(font.bytes());
    vec![
        format!("<< /Type /Font /Subtype /Type0 /BaseFont /NotoSans /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>", first + 1, first + 4).into_bytes(),
        format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /NotoSans /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 600 /W [{widths}] /CIDToGIDMap /Identity >>", first + 2).into_bytes(),
        format!("<< /Type /FontDescriptor /FontName /NotoSans /Flags 32 /FontBBox [-600 {descent} 1300 {ascent}] /ItalicAngle 0 /Ascent {ascent} /Descent {descent} /CapHeight 714 /StemV 80 /FontFile2 {} 0 R >>", first + 3).into_bytes(),
        stream(&format!("/Filter /FlateDecode /Length1 {}", font.bytes().len()), &file),
        stream("", to_unicode.as_bytes()),
    ]
}

/// The text of a document in the embedded font, for a writer that makes its own page contents.
pub mod public {
    use std::collections::BTreeMap;

    /// Glyphs used so far, and the characters they stand for.
    pub type Used = BTreeMap<u16, char>;

    /// `text` as a PDF hex string of glyphs of the embedded font, noting the glyphs in `used`.
    #[must_use]
    pub fn encode(text: &str, used: &mut Used) -> String {
        super::encode(text, used)
    }

    /// How wide `text` is in the embedded font at `size` points.
    #[must_use]
    pub fn width(text: &str, size: f64) -> f64 {
        super::width(text, size)
    }

    /// The five objects that make the font (Type0 font first), to be numbered from `first`.
    #[must_use]
    pub fn objects(used: &Used, first: usize) -> Vec<Vec<u8>> {
        super::objects(used, first)
    }
}
