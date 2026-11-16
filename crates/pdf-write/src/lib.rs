//! Writing PDF files.
//!
//! A [`Document`] is a list of [`Page`]s; a page is drawn with y measured from the top, in points (1/72 inch).
//!
//! ```
//! use pdf_write::{Document, Font, Page, A4};
//! let mut page = Page::new(A4.0, A4.1);
//! page.text(72.0, 72.0, "Hello", Font::Helvetica, 12.0, 0x000000);
//! let mut doc = Document::new();
//! doc.title("Greeting");
//! doc.add(page);
//! let bytes = doc.finish();
//! assert!(bytes.starts_with(b"%PDF-1.4"));
//! ```

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::fmt::Write as _;

mod metrics;
mod unicode;

pub use unicode::public as embedded;

pub use metrics::text_width;

/// A4 in points: width, height.
pub const A4: (f64, f64) = (595.28, 841.89);
/// US Letter in points: width, height.
pub const LETTER: (f64, f64) = (612.0, 792.0);

/// The fonts every viewer has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    Times,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
}

/// Sans serif, serif or fixed width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Sans,
    Serif,
    Mono,
}

impl Font {
    /// The font of a family with the given weight and slant.
    #[must_use]
    pub const fn in_family(family: Family, bold: bool, italic: bool) -> Self {
        match (family, bold, italic) {
            (Family::Sans, false, false) => Self::Helvetica,
            (Family::Sans, true, false) => Self::HelveticaBold,
            (Family::Sans, false, true) => Self::HelveticaOblique,
            (Family::Sans, true, true) => Self::HelveticaBoldOblique,
            (Family::Serif, false, false) => Self::Times,
            (Family::Serif, true, false) => Self::TimesBold,
            (Family::Serif, false, true) => Self::TimesItalic,
            (Family::Serif, true, true) => Self::TimesBoldItalic,
            (Family::Mono, false, false) => Self::Courier,
            (Family::Mono, true, false) => Self::CourierBold,
            (Family::Mono, false, true) => Self::CourierOblique,
            (Family::Mono, true, true) => Self::CourierBoldOblique,
        }
    }

    /// Sans serif with the given weight and slant.
    #[must_use]
    pub const fn styled(bold: bool, italic: bool) -> Self {
        Self::in_family(Family::Sans, bold, italic)
    }

    const ALL: [Self; 12] = [
        Self::Helvetica,
        Self::HelveticaBold,
        Self::HelveticaOblique,
        Self::HelveticaBoldOblique,
        Self::Times,
        Self::TimesBold,
        Self::TimesItalic,
        Self::TimesBoldItalic,
        Self::Courier,
        Self::CourierBold,
        Self::CourierOblique,
        Self::CourierBoldOblique,
    ];

    const fn base_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::Times => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }

    const fn family(self) -> Family {
        match self {
            Self::Helvetica
            | Self::HelveticaBold
            | Self::HelveticaOblique
            | Self::HelveticaBoldOblique => Family::Sans,
            Self::Times | Self::TimesBold | Self::TimesItalic | Self::TimesBoldItalic => {
                Family::Serif
            }
            _ => Family::Mono,
        }
    }

    const fn italic(self) -> bool {
        matches!(
            self,
            Self::HelveticaOblique
                | Self::HelveticaBoldOblique
                | Self::TimesItalic
                | Self::TimesBoldItalic
                | Self::CourierOblique
                | Self::CourierBoldOblique
        )
    }

    const fn bold(self) -> bool {
        matches!(
            self,
            Self::HelveticaBold
                | Self::HelveticaBoldOblique
                | Self::TimesBold
                | Self::TimesBoldItalic
                | Self::CourierBold
                | Self::CourierBoldOblique
        )
    }
}

/// One page and what is drawn on it.
#[derive(Clone, Debug)]
pub struct Page {
    width: f64,
    height: f64,
    content: String,
    images: Vec<Img>,
    /// Boxes that open an address: (x, y, width, height, address), measured from the top left.
    links: Vec<(f64, f64, f64, f64, String)>,
    /// Glyphs of the embedded font used on the page, and the characters they stand for.
    used: std::collections::BTreeMap<u16, char>,
}

/// A picture on a page: its pixels as zlib-compressed RGB, and its alpha (compressed) when it is not opaque.
#[derive(Clone, Debug)]
struct Img {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

fn zlib(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9c];
    out.extend(quench_archive::deflate::compress(raw));
    out.extend(quench_archive::checksum::adler32(raw).to_be_bytes());
    out
}

fn num(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn rgb(c: u32) -> String {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    format!(
        "{} {} {}",
        num(f64::from(r) / 255.0),
        num(f64::from(g) / 255.0),
        num(f64::from(b) / 255.0)
    )
}

impl Page {
    #[must_use]
    pub const fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            content: String::new(),
            images: Vec::new(),
            links: Vec::new(),
            used: std::collections::BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    /// Draw a picture of `width` by `height` pixels (`rgba`, 4 bytes a pixel, straight alpha) in the box whose top left is
    /// (`x`, `y`) and which is `w` by `h` points. Pixels that do not add up to the size are ignored.
    #[allow(clippy::too_many_arguments)]
    pub fn image(&mut self, x: f64, y: f64, w: f64, h: f64, width: u32, height: u32, rgba: &[u8]) {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
        let mut alpha = Vec::with_capacity(rgba.len() / 4);
        #[allow(clippy::chunks_exact_to_as_chunks)]
        for px in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
            alpha.push(px[3]);
        }
        let see_through = alpha.iter().any(|a| *a != 255);
        self.images.push(Img {
            width,
            height,
            rgb: zlib(&rgb),
            alpha: see_through.then(|| zlib(&alpha)),
        });
        let _ = writeln!(
            self.content,
            "q {} 0 0 {} {} {} cm /Im{} Do Q",
            num(w),
            num(h),
            num(x),
            num(self.height - y - h),
            self.images.len()
        );
    }

    /// Make the box with its top left at (`x`, `y`) a link to `address` (an `https:` or `mailto:` one, usually).
    pub fn link(&mut self, x: f64, y: f64, w: f64, h: f64, address: &str) {
        self.links.push((x, y, w, h, address.to_string()));
    }

    /// Fill the rectangle with its top left at (`x`, `y`).
    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: u32) {
        let _ = writeln!(
            self.content,
            "{} rg {} {} {} {} re f",
            rgb(color),
            num(x),
            num(self.height - y - h),
            num(w),
            num(h)
        );
    }

    /// A straight line of the given width.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, color: u32) {
        let _ = writeln!(
            self.content,
            "{} RG {} w {} {} m {} {} l S",
            rgb(color),
            num(width),
            num(x1),
            num(self.height - y1),
            num(x2),
            num(self.height - y2)
        );
    }

    /// A filled shape through the points (the first is joined to the last).
    pub fn fill_polygon(&mut self, points: &[(f64, f64)], color: u32) {
        let Some(((x0, y0), rest)) = points.split_first() else {
            return;
        };
        let mut path = format!("{} rg {} {} m", rgb(color), num(*x0), num(self.height - y0));
        for (x, y) in rest {
            let _ = write!(path, " {} {} l", num(*x), num(self.height - y));
        }
        let _ = writeln!(self.content, "{path} h f");
    }

    /// A line through the points.
    pub fn polyline(&mut self, points: &[(f64, f64)], width: f64, color: u32) {
        let Some(((x0, y0), rest)) = points.split_first() else {
            return;
        };
        let mut path = format!(
            "{} RG {} w 1 j {} {} m",
            rgb(color),
            num(width),
            num(*x0),
            num(self.height - y0)
        );
        for (x, y) in rest {
            let _ = write!(path, " {} {} l", num(*x), num(self.height - y));
        }
        let _ = writeln!(self.content, "{path} S");
    }

    /// Text with its baseline at `y` starting at `x`. Characters the fonts lack are drawn as `?`.
    pub fn text(&mut self, x: f64, y: f64, text: &str, font: Font, size: f64, color: u32) {
        if text.is_empty() {
            return;
        }
        if unicode::needed(text) {
            self.text_unicode(x, y, text, font, size, color);
            return;
        }
        let _ = writeln!(
            self.content,
            "BT {} rg /F{} {} Tf {} {} Td ({}) Tj ET",
            rgb(color),
            font.index() + 1,
            num(size),
            num(x),
            num(self.height - y),
            escape(text)
        );
    }

    /// Text with characters the standard fonts lack, in the embedded font: bold and italic as `font` says.
    fn text_unicode(&mut self, x: f64, y: f64, text: &str, font: Font, size: f64, color: u32) {
        let hex = unicode::encode(text, &mut self.used);
        let top = self.height - y;
        let place = if font.italic() {
            format!("1 0 0.2126 1 {} {} Tm", num(x), num(top))
        } else {
            format!("{} {} Td", num(x), num(top))
        };
        // Bold is the glyphs thickened by a thin outline in the same colour.
        let weight = if font.bold() {
            format!("2 Tr {} RG {} w ", rgb(color), num(size * 0.028))
        } else {
            String::new()
        };
        let _ = writeln!(
            self.content,
            "BT {} rg {weight}/FU {} Tf {place} {hex} Tj ET",
            rgb(color),
            num(size)
        );
    }

    /// Draw what `inside` draws, but only within the rectangle.
    pub fn clipped(&mut self, x: f64, y: f64, w: f64, h: f64, inside: impl FnOnce(&mut Self)) {
        let _ = writeln!(
            self.content,
            "q {} {} {} {} re W n",
            num(x),
            num(self.height - y - h),
            num(w),
            num(h)
        );
        inside(self);
        self.content.push_str("Q\n");
    }
}

/// The byte a character has in `WinAnsi` (`Windows-1252`), or `?`.
const fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '\u{20ac}' => 0x80,
        '\u{2026}' => 0x85,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{2122}' => 0x99,
        '\t' => b' ',
        _ => b'?',
    }
}

/// The text as a PDF string body: `WinAnsi` bytes, with `(`, `)` and `\` escaped and the rest of the high bytes as octal.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match win_ansi(c) {
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\\' => out.push_str("\\\\"),
            b @ 0x20..=0x7e => out.push(char::from(b)),
            b => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out
}

/// A PDF file being put together.
#[derive(Clone, Debug, Default)]
pub struct Document {
    title: String,
    pages: Vec<Page>,
}

impl Document {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The title the viewer shows.
    pub fn title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    pub fn add(&mut self, page: Page) {
        self.pages.push(page);
    }

    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The file. A document with no pages gets one blank A4 page, since a PDF needs at least one.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn finish(&self) -> Vec<u8> {
        let blank = [Page::new(A4.0, A4.1)];
        let pages: &[Page] = if self.pages.is_empty() {
            &blank
        } else {
            &self.pages
        };
        // Objects: 1 catalog, 2 page tree, 3 info, 4..=15 fonts, then for each page its page object, its content, and the
        // objects of its pictures (a picture, and its alpha when it has one).
        let first_page = 16;
        let mut page_ids = Vec::new();
        let mut next = first_page;
        for page in pages {
            page_ids.push(next);
            next += 2
                + page.links.len()
                + page
                    .images
                    .iter()
                    .map(|i| 1 + usize::from(i.alpha.is_some()))
                    .sum::<usize>();
        }
        // The embedded font, when any page uses it, comes after the pages.
        let mut used = std::collections::BTreeMap::new();
        for page in pages {
            used.extend(page.used.iter().map(|(g, c)| (*g, *c)));
        }
        let unicode_font = (!used.is_empty()).then_some(next);
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::new();
        let object = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        object(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
        let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
        object(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                kids.join(" "),
                pages.len()
            )
            .as_bytes(),
        );
        object(
            &mut out,
            &mut offsets,
            format!(
                "<< /Title ({}) /Producer (lore-office) >>",
                escape(&self.title)
            )
            .as_bytes(),
        );
        for font in Font::ALL {
            object(
                &mut out,
                &mut offsets,
                format!(
                    "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                    font.base_name()
                )
                .as_bytes(),
            );
        }
        let mut fonts = String::new();
        for i in 1..=12 {
            let _ = write!(fonts, "/F{i} {} 0 R ", i + 3);
        }
        if let Some(at) = unicode_font {
            let _ = write!(fonts, "/FU {at} 0 R ");
        }
        for (page, id) in pages.iter().zip(&page_ids) {
            let content = id + 1;
            // The pictures' objects follow the content.
            let mut at = id + 2;
            let mut pictures = String::new();
            let mut ids = Vec::new();
            for (n, img) in page.images.iter().enumerate() {
                let _ = write!(pictures, "/Im{} {at} 0 R ", n + 1);
                ids.push(at);
                at += 1 + usize::from(img.alpha.is_some());
            }
            // The link annotations follow the pictures.
            let annots: Vec<usize> = (0..page.links.len()).map(|k| at + k).collect();
            let annots_entry = if annots.is_empty() {
                String::new()
            } else {
                format!(
                    " /Annots [{}]",
                    annots
                        .iter()
                        .map(|a| format!("{a} 0 R"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            object(
                &mut out,
                &mut offsets,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {fonts} >> /XObject << {pictures} >> >> /Contents {content} 0 R{annots_entry} >>",
                    num(page.width),
                    num(page.height)
                )
                .as_bytes(),
            );
            // The drawing is compressed (it is text, and shrinks to a fifth).
            let packed = zlib(page.content.as_bytes());
            let mut body = format!(
                "<< /Length {} /Filter /FlateDecode >>\nstream\n",
                packed.len()
            )
            .into_bytes();
            body.extend_from_slice(&packed);
            body.extend_from_slice(b"\nendstream");
            object(&mut out, &mut offsets, &body);
            for (img, at) in page.images.iter().zip(ids) {
                let mask = img
                    .alpha
                    .as_ref()
                    .map_or(String::new(), |_| format!(" /SMask {} 0 R", at + 1));
                let mut body = format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {}{mask} >>\nstream\n",
                    img.width,
                    img.height,
                    img.rgb.len()
                )
                .into_bytes();
                body.extend_from_slice(&img.rgb);
                body.extend_from_slice(b"\nendstream");
                object(&mut out, &mut offsets, &body);
                if let Some(alpha) = &img.alpha {
                    let mut body = format!(
                        "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                        img.width,
                        img.height,
                        alpha.len()
                    )
                    .into_bytes();
                    body.extend_from_slice(alpha);
                    body.extend_from_slice(b"\nendstream");
                    object(&mut out, &mut offsets, &body);
                }
            }
            for ((x, y, w, h, address), n) in page.links.iter().zip(annots) {
                let _ = n;
                object(
                    &mut out,
                    &mut offsets,
                    format!(
                        "<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI ({}) >> >>",
                        num(*x),
                        num(page.height - y - h),
                        num(x + w),
                        num(page.height - y),
                        escape(address)
                    )
                    .as_bytes(),
                );
            }
        }
        if let Some(at) = unicode_font {
            for body in unicode::objects(&used, at) {
                object(&mut out, &mut offsets, &body);
            }
        }
        let xref = out.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
        for o in &offsets {
            let _ = writeln!(table, "{o:010} 00000 n ");
        }
        out.extend_from_slice(table.as_bytes());
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                offsets.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}
