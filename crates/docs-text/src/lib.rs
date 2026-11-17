//! Paragraphs of styled text and the operations that edit them.
//!
//! A position is a paragraph and a number of UTF-16 units into it: the units the browser counts in, so what an editable element
//! reports maps onto the model without conversion. Every operation keeps the runs normal: no empty run in a paragraph that has
//! text, neighbours of one style are one run.

/// The kinds of typeface: a document names one of these, and each program draws it with the fonts it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Sans,
    Serif,
    Mono,
}

/// A typeface a person can pick by name: the kind it stands in for where it is not there (a PDF carries three kinds), and the
/// fonts a browser tries for it, the ones with the same metrics last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typeface {
    pub name: &'static str,
    pub family: Family,
    pub css: &'static str,
}

const fn face(name: &'static str, family: Family, css: &'static str) -> Typeface {
    Typeface { name, family, css }
}

/// The popular typefaces, by kind. Each is drawn in the browser by its own name where the computer has it, and by a free
/// font of the same size where it has not.
pub const TYPEFACES: &[Typeface] = &[
    face(
        "Arial",
        Family::Sans,
        "Arial, \"Liberation Sans\", Helvetica, sans-serif",
    ),
    face(
        "Helvetica",
        Family::Sans,
        "Helvetica, Arial, \"Liberation Sans\", sans-serif",
    ),
    face(
        "Verdana",
        Family::Sans,
        "Verdana, \"DejaVu Sans\", Geneva, sans-serif",
    ),
    face(
        "Tahoma",
        Family::Sans,
        "Tahoma, Verdana, \"DejaVu Sans\", sans-serif",
    ),
    face(
        "Trebuchet MS",
        Family::Sans,
        "\"Trebuchet MS\", \"Lucida Grande\", Arial, sans-serif",
    ),
    face(
        "Calibri",
        Family::Sans,
        "Calibri, Carlito, Candara, \"Segoe UI\", sans-serif",
    ),
    face(
        "Segoe UI",
        Family::Sans,
        "\"Segoe UI\", \"Helvetica Neue\", Arial, sans-serif",
    ),
    face(
        "Century Gothic",
        Family::Sans,
        "\"Century Gothic\", \"URW Gothic\", Futura, sans-serif",
    ),
    face(
        "Gill Sans",
        Family::Sans,
        "\"Gill Sans\", \"Gill Sans MT\", Calibri, sans-serif",
    ),
    face(
        "Lucida Sans",
        Family::Sans,
        "\"Lucida Sans\", \"Lucida Grande\", \"Lucida Sans Unicode\", sans-serif",
    ),
    face(
        "Roboto",
        Family::Sans,
        "Roboto, \"Helvetica Neue\", Arial, sans-serif",
    ),
    face(
        "Open Sans",
        Family::Sans,
        "\"Open Sans\", \"Noto Sans\", Arial, sans-serif",
    ),
    face(
        "Comic Sans MS",
        Family::Sans,
        "\"Comic Sans MS\", \"Comic Neue\", cursive, sans-serif",
    ),
    face(
        "Impact",
        Family::Sans,
        "Impact, \"Arial Narrow Bold\", Haettenschweiler, sans-serif",
    ),
    face(
        "Times New Roman",
        Family::Serif,
        "\"Times New Roman\", \"Liberation Serif\", Times, serif",
    ),
    face(
        "Georgia",
        Family::Serif,
        "Georgia, \"DejaVu Serif\", \"Times New Roman\", serif",
    ),
    face(
        "Garamond",
        Family::Serif,
        "Garamond, \"EB Garamond\", \"Palatino Linotype\", serif",
    ),
    face(
        "Palatino Linotype",
        Family::Serif,
        "\"Palatino Linotype\", Palatino, \"Book Antiqua\", \"URW Palladio L\", serif",
    ),
    face(
        "Book Antiqua",
        Family::Serif,
        "\"Book Antiqua\", Palatino, \"Palatino Linotype\", serif",
    ),
    face("Cambria", Family::Serif, "Cambria, Caladea, Georgia, serif"),
    face(
        "Baskerville",
        Family::Serif,
        "Baskerville, \"Baskerville Old Face\", Georgia, serif",
    ),
    face(
        "Bookman Old Style",
        Family::Serif,
        "\"Bookman Old Style\", Bookman, \"URW Bookman\", Georgia, serif",
    ),
    face(
        "Century Schoolbook",
        Family::Serif,
        "\"Century Schoolbook\", \"New Century Schoolbook\", Georgia, serif",
    ),
    face(
        "Rockwell",
        Family::Serif,
        "Rockwell, \"Rockwell Nova\", Georgia, serif",
    ),
    face(
        "Courier New",
        Family::Mono,
        "\"Courier New\", \"Liberation Mono\", Courier, monospace",
    ),
    face(
        "Consolas",
        Family::Mono,
        "Consolas, \"DejaVu Sans Mono\", Menlo, monospace",
    ),
    face(
        "Lucida Console",
        Family::Mono,
        "\"Lucida Console\", \"Lucida Sans Typewriter\", Monaco, monospace",
    ),
    face(
        "Monaco",
        Family::Mono,
        "Monaco, Menlo, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "Menlo",
        Family::Mono,
        "Menlo, Monaco, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "Source Code Pro",
        Family::Mono,
        "\"Source Code Pro\", Consolas, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "Helvetica Neue",
        Family::Sans,
        "\"Helvetica Neue\", Helvetica, Arial, \"Liberation Sans\", sans-serif",
    ),
    face(
        "Arial Narrow",
        Family::Sans,
        "\"Arial Narrow\", \"Liberation Sans Narrow\", Arial, sans-serif",
    ),
    face(
        "Arial Black",
        Family::Sans,
        "\"Arial Black\", \"Arial Bold\", Arial, sans-serif",
    ),
    face(
        "Geneva",
        Family::Sans,
        "Geneva, Verdana, \"DejaVu Sans\", sans-serif",
    ),
    face(
        "Lucida Grande",
        Family::Sans,
        "\"Lucida Grande\", \"Lucida Sans Unicode\", Tahoma, sans-serif",
    ),
    face(
        "Candara",
        Family::Sans,
        "Candara, Calibri, Carlito, sans-serif",
    ),
    face(
        "Corbel",
        Family::Sans,
        "Corbel, Calibri, Carlito, sans-serif",
    ),
    face(
        "Franklin Gothic",
        Family::Sans,
        "\"Franklin Gothic Medium\", \"Franklin Gothic\", \"Libre Franklin\", Arial, sans-serif",
    ),
    face(
        "Futura",
        Family::Sans,
        "Futura, \"Century Gothic\", \"URW Gothic\", sans-serif",
    ),
    face(
        "Avenir",
        Family::Sans,
        "Avenir, \"Avenir Next\", Nunito, \"Helvetica Neue\", sans-serif",
    ),
    face(
        "Optima",
        Family::Sans,
        "Optima, Candara, \"URW Classico\", sans-serif",
    ),
    face(
        "Myriad Pro",
        Family::Sans,
        "\"Myriad Pro\", Myriad, \"Segoe UI\", sans-serif",
    ),
    face(
        "Frutiger",
        Family::Sans,
        "Frutiger, \"Frutiger Linotype\", \"Segoe UI\", sans-serif",
    ),
    face(
        "Lato",
        Family::Sans,
        "Lato, Carlito, \"Helvetica Neue\", Arial, sans-serif",
    ),
    face(
        "Montserrat",
        Family::Sans,
        "Montserrat, \"Century Gothic\", Arial, sans-serif",
    ),
    face(
        "Poppins",
        Family::Sans,
        "Poppins, \"Century Gothic\", Arial, sans-serif",
    ),
    face(
        "Inter",
        Family::Sans,
        "Inter, \"Segoe UI\", Roboto, Arial, sans-serif",
    ),
    face(
        "Noto Sans",
        Family::Sans,
        "\"Noto Sans\", \"Open Sans\", Arial, sans-serif",
    ),
    face(
        "Source Sans Pro",
        Family::Sans,
        "\"Source Sans Pro\", \"Source Sans 3\", \"Segoe UI\", sans-serif",
    ),
    face(
        "Ubuntu",
        Family::Sans,
        "Ubuntu, Cantarell, \"Segoe UI\", sans-serif",
    ),
    face(
        "Nunito",
        Family::Sans,
        "Nunito, \"Varela Round\", \"Segoe UI\", sans-serif",
    ),
    face(
        "Raleway",
        Family::Sans,
        "Raleway, \"Century Gothic\", Arial, sans-serif",
    ),
    face(
        "Fira Sans",
        Family::Sans,
        "\"Fira Sans\", \"Segoe UI\", Arial, sans-serif",
    ),
    face(
        "Cantarell",
        Family::Sans,
        "Cantarell, Ubuntu, \"Segoe UI\", sans-serif",
    ),
    face(
        "DejaVu Sans",
        Family::Sans,
        "\"DejaVu Sans\", Verdana, sans-serif",
    ),
    face(
        "Liberation Sans",
        Family::Sans,
        "\"Liberation Sans\", Arial, Helvetica, sans-serif",
    ),
    face(
        "Segoe Print",
        Family::Sans,
        "\"Segoe Print\", \"Comic Sans MS\", \"Comic Neue\", cursive",
    ),
    face(
        "Trebuchet",
        Family::Sans,
        "Trebuchet, \"Trebuchet MS\", Arial, sans-serif",
    ),
    face(
        "Palatino",
        Family::Serif,
        "Palatino, \"Palatino Linotype\", \"Book Antiqua\", \"URW Palladio L\", serif",
    ),
    face(
        "Times",
        Family::Serif,
        "Times, \"Times New Roman\", \"Liberation Serif\", serif",
    ),
    face(
        "Constantia",
        Family::Serif,
        "Constantia, Cambria, Caladea, Georgia, serif",
    ),
    face(
        "Perpetua",
        Family::Serif,
        "Perpetua, \"Palatino Linotype\", Georgia, serif",
    ),
    face(
        "Hoefler Text",
        Family::Serif,
        "\"Hoefler Text\", \"Baskerville Old Face\", Georgia, serif",
    ),
    face(
        "Didot",
        Family::Serif,
        "Didot, \"Bodoni MT\", \"Playfair Display\", Georgia, serif",
    ),
    face(
        "Bodoni",
        Family::Serif,
        "Bodoni, \"Bodoni MT\", \"Bodoni 72\", Didot, Georgia, serif",
    ),
    face(
        "Minion Pro",
        Family::Serif,
        "\"Minion Pro\", Minion, \"Palatino Linotype\", Georgia, serif",
    ),
    face(
        "Noto Serif",
        Family::Serif,
        "\"Noto Serif\", \"DejaVu Serif\", Georgia, serif",
    ),
    face(
        "Merriweather",
        Family::Serif,
        "Merriweather, Georgia, \"Times New Roman\", serif",
    ),
    face(
        "Playfair Display",
        Family::Serif,
        "\"Playfair Display\", Didot, Georgia, serif",
    ),
    face(
        "Lora",
        Family::Serif,
        "Lora, Georgia, \"Times New Roman\", serif",
    ),
    face(
        "EB Garamond",
        Family::Serif,
        "\"EB Garamond\", Garamond, \"Palatino Linotype\", serif",
    ),
    face(
        "Cormorant Garamond",
        Family::Serif,
        "\"Cormorant Garamond\", Garamond, \"EB Garamond\", serif",
    ),
    face(
        "Libre Baskerville",
        Family::Serif,
        "\"Libre Baskerville\", Baskerville, Georgia, serif",
    ),
    face(
        "Source Serif Pro",
        Family::Serif,
        "\"Source Serif Pro\", \"Source Serif 4\", Georgia, serif",
    ),
    face(
        "DejaVu Serif",
        Family::Serif,
        "\"DejaVu Serif\", Georgia, serif",
    ),
    face(
        "Liberation Serif",
        Family::Serif,
        "\"Liberation Serif\", \"Times New Roman\", Times, serif",
    ),
    face(
        "Andale Mono",
        Family::Mono,
        "\"Andale Mono\", Monaco, Menlo, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "Cascadia Code",
        Family::Mono,
        "\"Cascadia Code\", \"Cascadia Mono\", Consolas, monospace",
    ),
    face(
        "Fira Code",
        Family::Mono,
        "\"Fira Code\", \"Fira Mono\", Consolas, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "JetBrains Mono",
        Family::Mono,
        "\"JetBrains Mono\", \"Fira Code\", Consolas, monospace",
    ),
    face(
        "Inconsolata",
        Family::Mono,
        "Inconsolata, Consolas, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "Ubuntu Mono",
        Family::Mono,
        "\"Ubuntu Mono\", Consolas, \"DejaVu Sans Mono\", monospace",
    ),
    face(
        "SF Mono",
        Family::Mono,
        "\"SF Mono\", SFMono-Regular, Menlo, Monaco, monospace",
    ),
    face(
        "DejaVu Sans Mono",
        Family::Mono,
        "\"DejaVu Sans Mono\", Menlo, Consolas, monospace",
    ),
    face(
        "Liberation Mono",
        Family::Mono,
        "\"Liberation Mono\", \"Courier New\", Courier, monospace",
    ),
    face(
        "Courier",
        Family::Mono,
        "Courier, \"Courier New\", \"Liberation Mono\", monospace",
    ),
];

/// The typeface of this name, if it is one of [`TYPEFACES`] (names are compared without regard to case).
#[must_use]
pub fn typeface(name: &str) -> Option<&'static Typeface> {
    TYPEFACES.iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

/// Character styles. `None` for the size, colour and typeface means the paragraph's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Raised and lowered text (at most one is on).
    pub sup: bool,
    pub sub: bool,
    /// The colour behind the text (`0xRRGGBB`), for a highlight.
    pub highlight: Option<u32>,
    /// In half points (24 is 12 pt).
    pub size: Option<u16>,
    /// `0xRRGGBB`.
    pub color: Option<u32>,
    pub family: Option<Family>,
    /// The typeface by name (a number in the document's list of them, [`Doc::font_name`]); `family` is the kind it stands in for
    /// where the name is not known.
    pub font: Option<u32>,
    /// A link: the number of its address in the document's list of them ([`Doc::url`]).
    pub link: Option<u32>,
}

/// The colour a highlight is made in.
pub const HIGHLIGHT: u32 = 0x00FF_FF00;

/// How the lines of a paragraph sit between the margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// One of the styles, for toggling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strike,
    Superscript,
    Subscript,
    Highlight,
}

impl Style {
    #[must_use]
    pub const fn has(self, mark: Mark) -> bool {
        match mark {
            Mark::Bold => self.bold,
            Mark::Italic => self.italic,
            Mark::Underline => self.underline,
            Mark::Strike => self.strike,
            Mark::Superscript => self.sup,
            Mark::Subscript => self.sub,
            Mark::Highlight => self.highlight.is_some(),
        }
    }

    #[must_use]
    pub const fn with(mut self, mark: Mark, on: bool) -> Self {
        match mark {
            Mark::Bold => self.bold = on,
            Mark::Italic => self.italic = on,
            Mark::Underline => self.underline = on,
            Mark::Strike => self.strike = on,
            Mark::Superscript => {
                self.sup = on;
                self.sub &= !on;
            }
            Mark::Subscript => {
                self.sub = on;
                self.sup &= !on;
            }
            Mark::Highlight => {
                self.highlight = if on { Some(HIGHLIGHT) } else { None };
            }
        }
        self
    }
}

/// A stretch of text in one style.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

/// What a paragraph is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Paragraph,
    /// 1 to 3.
    Heading(u8),
    Bullet,
    /// A numbered list item (the numbers count the items of a run of them).
    Numbered,
}

/// Where a paragraph sits in a table: the cell holds just that paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub table: u32,
    pub row: u16,
    pub col: u16,
}

/// A picture: the file as it came (PNG, JPEG or GIF) and the size it is shown at.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Picture {
    /// `image/png`, `image/jpeg` or `image/gif`.
    pub mime: &'static str,
    pub data: std::rc::Rc<Vec<u8>>,
    /// The size shown, in hundredths of a point (a whole number so pictures can be compared and hashed).
    pub width: u32,
    pub height: u32,
}

impl Picture {
    /// The size in points.
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        (
            f64::from(self.width) / 100.0,
            f64::from(self.height) / 100.0,
        )
    }

    /// The file extension that goes with the type.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self.mime {
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            _ => "png",
        }
    }
}

/// The character a line break inside a paragraph (Shift+Enter) is stored as.
pub const LINE_BREAK: char = '\u{2028}';

/// How a paragraph is spaced and indented (the defaults are zero: as the document's body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParaFmt {
    /// The left indent, in twentieths of a point.
    pub indent: u16,
    /// The extra indent of the first line (negative: a hanging indent), in twentieths of a point.
    pub first: i16,
    /// The space after the paragraph in twentieths of a point; `None` for the default.
    pub after: Option<u16>,
    /// Line spacing in 240ths of a line (240 is single, 360 one and a half); 0 for the default.
    pub line: u16,
    /// Whether the paragraph starts a new page.
    pub page_break: bool,
    /// How deep a list item is nested (0 to 8).
    pub level: u8,
}

/// How a table is laid out beyond its cells.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TableFmt {
    /// The width of each column in thousandths of the table's width (empty: all equal).
    pub widths: Vec<u16>,
    /// Whether the first row is a header (set in bold, and repeated at the top of each page a table runs over).
    pub header: bool,
}

/// A paragraph. Its `id` stays with it while it is edited, so the screen can tell which ones changed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Para {
    pub id: u32,
    pub kind: Kind,
    pub align: Align,
    /// The table cell the paragraph is the content of, if it is. A table is the paragraphs of its cells, row by row.
    pub cell: Option<Cell>,
    /// A picture paragraph shows picture number `picture` (see [`Doc::picture`]) and has no text.
    pub picture: Option<u32>,
    /// Indents, spacing and page break.
    pub fmt: ParaFmt,
    pub runs: Vec<Run>,
}

/// A place in the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub para: usize,
    /// UTF-16 units from the start of the paragraph.
    pub offset: usize,
}

impl Pos {
    #[must_use]
    pub const fn new(para: usize, offset: usize) -> Self {
        Self { para, offset }
    }
}

/// The byte offset of a UTF-16 offset in `text` (the end of the text when past it).
fn byte_at(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

fn units(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// A document: paragraphs, at least one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    paras: Vec<Para>,
    next_id: u32,
    links: Vec<String>,
    pictures: Vec<Picture>,
    fonts: Vec<String>,
    header: String,
    footer: String,
    paper: PaperSize,
    tables: Vec<(u32, TableFmt)>,
}

/// The paper sizes a document can be set for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PaperSize {
    #[default]
    A4,
    A3,
    A5,
    Letter,
    Legal,
}

impl PaperSize {
    /// Every size, in the order a menu lists them.
    pub const ALL: [Self; 5] = [Self::A4, Self::A3, Self::A5, Self::Letter, Self::Legal];

    /// Width and height in twentieths of a point (what a Word file keeps).
    #[must_use]
    pub const fn twips(self) -> (u32, u32) {
        match self {
            Self::A4 => (11_906, 16_838),
            Self::A3 => (16_838, 23_811),
            Self::A5 => (8_391, 11_906),
            Self::Letter => (12_240, 15_840),
            Self::Legal => (12_240, 20_160),
        }
    }

    /// The size whose width and height (twentieths of a point) are nearest these, if any is close.
    #[must_use]
    pub fn from_twips(width: u32, height: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| {
            let (w, h) = p.twips();
            w.abs_diff(width) < 200 && h.abs_diff(height) < 200
        })
    }

    /// The name shown to the person.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A5 => "A5",
            Self::Letter => "US Letter",
            Self::Legal => "US Legal",
        }
    }
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

impl Para {
    /// Whether two paragraphs say the same, whatever their ids (the id only tells paragraphs apart on one screen).
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        (
            &self.kind,
            &self.align,
            &self.cell,
            &self.picture,
            &self.fmt,
            &self.runs,
        ) == (
            &other.kind,
            &other.align,
            &other.cell,
            &other.picture,
            &other.fmt,
            &other.runs,
        )
    }
}

impl Doc {
    /// Whether everything but the paragraphs is alike in both: the lists the paragraphs point into, the header and footer, the
    /// paper and the tables.
    #[must_use]
    pub fn same_apart_from_paras(&self, other: &Self) -> bool {
        (
            &self.links,
            &self.pictures,
            &self.fonts,
            &self.header,
            &self.footer,
            self.paper,
            &self.tables,
        ) == (
            &other.links,
            &other.pictures,
            &other.fonts,
            &other.header,
            &other.footer,
            other.paper,
            &other.tables,
        )
    }

    /// Replace `del` paragraphs from `at` by `paras` (they get ids of this document); the document keeps at least one.
    pub fn splice_paras(&mut self, at: usize, del: usize, paras: Vec<Para>) {
        let at = at.min(self.paras.len());
        let end = (at + del).min(self.paras.len());
        let made: Vec<Para> = paras
            .into_iter()
            .map(|mut p| {
                p.id = self.next_id;
                self.next_id += 1;
                p
            })
            .collect();
        self.paras.splice(at..end, made);
        if self.paras.is_empty() {
            let id = self.next_id;
            self.next_id += 1;
            self.paras.push(Para {
                id,
                kind: Kind::Paragraph,
                align: Align::Left,
                cell: None,
                picture: None,
                fmt: ParaFmt::default(),
                runs: vec![Run {
                    text: String::new(),
                    style: Style::default(),
                }],
            });
        }
    }

    /// An empty document: one empty paragraph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            paras: vec![Para {
                id: 1,
                kind: Kind::Paragraph,
                align: Align::Left,
                cell: None,
                picture: None,
                fmt: ParaFmt::default(),
                runs: Vec::new(),
            }],
            next_id: 2,
            links: Vec::new(),
            pictures: Vec::new(),
            fonts: Vec::new(),
            header: String::new(),
            footer: String::new(),
            paper: PaperSize::A4,
            tables: Vec::new(),
        }
    }

    /// A document of plain text, a paragraph for each line.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut doc = Self::new();
        doc.replace(Pos::default(), Pos::default(), text);
        doc
    }

    #[must_use]
    pub fn paras(&self) -> &[Para] {
        &self.paras
    }

    /// The text of a paragraph.
    #[must_use]
    pub fn text(&self, para: usize) -> String {
        self.paras
            .get(para)
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect())
            .unwrap_or_default()
    }

    /// The length of a paragraph in UTF-16 units.
    #[must_use]
    pub fn len_of(&self, para: usize) -> usize {
        self.paras
            .get(para)
            .map_or(0, |p| p.runs.iter().map(|r| units(&r.text)).sum())
    }

    /// The whole text, paragraphs joined by line breaks.
    #[must_use]
    pub fn plain(&self) -> String {
        (0..self.paras.len())
            .map(|i| self.text(i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A position clamped into the document.
    #[must_use]
    pub fn clamp(&self, pos: Pos) -> Pos {
        let para = pos.para.min(self.paras.len() - 1);
        Pos {
            para,
            offset: pos.offset.min(self.len_of(para)),
        }
    }

    /// The style typing at `pos` would have: that of the text before it, else of the text after it.
    #[must_use]
    pub fn style_at(&self, pos: Pos) -> Style {
        let pos = self.clamp(pos);
        let runs = &self.paras[pos.para].runs;
        let mut at = 0;
        for r in runs {
            let end = at + units(&r.text);
            if pos.offset <= end && (pos.offset > at || at == 0) {
                return r.style;
            }
            at = end;
        }
        runs.last().map_or_else(Style::default, |r| r.style)
    }

    fn normalize(para: &mut Para) {
        para.runs.retain(|r| !r.text.is_empty());
        let mut out: Vec<Run> = Vec::new();
        for r in para.runs.drain(..) {
            match out.last_mut() {
                Some(last) if last.style == r.style => last.text.push_str(&r.text),
                _ => out.push(r),
            }
        }
        para.runs = out;
    }

    /// The runs of a paragraph cut at `offset`: before and after.
    fn cut(runs: &[Run], offset: usize) -> (Vec<Run>, Vec<Run>) {
        let (mut before, mut after) = (Vec::new(), Vec::new());
        let mut at = 0;
        for r in runs {
            let len = units(&r.text);
            if at + len <= offset {
                before.push(r.clone());
            } else if at >= offset {
                after.push(r.clone());
            } else {
                let b = byte_at(&r.text, offset - at);
                before.push(Run {
                    text: r.text[..b].to_string(),
                    style: r.style,
                });
                after.push(Run {
                    text: r.text[b..].to_string(),
                    style: r.style,
                });
            }
            at += len;
        }
        (before, after)
    }

    /// Replace everything from `start` to `end` with `text` (line breaks in it start new paragraphs); returns the position
    /// after what was inserted. The inserted text has the style at `start`. A range that crosses paragraphs joins them.
    pub fn replace(&mut self, start: Pos, end: Pos, text: &str) -> Pos {
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        // A picture paragraph holds no text: Enter starts a paragraph after it, and nothing else is typed into it.
        if self.paras[start.para].picture.is_some() && start.para == end.para {
            if text == "\n" {
                let p = self.plain_para();
                self.paras.insert(start.para + 1, p);
                return Pos::new(start.para + 1, 0);
            }
            return start;
        }
        // Edits stay inside one cell (and keep to its one line of text): nothing joins or splits cells.
        let cell = self.paras[start.para].cell;
        if cell != self.paras[end.para].cell {
            return start;
        }
        let flat;
        let text = if cell.is_some() && text.contains(['\n', '\r']) {
            flat = text.replace(['\r', '\n'], " ");
            flat.as_str()
        } else {
            text
        };
        let style = self.style_at(start);
        let (head, _) = Self::cut(&self.paras[start.para].runs, start.offset);
        let (_, tail) = Self::cut(&self.paras[end.para].runs, end.offset);
        let kind = self.paras[start.para].kind;
        // A list goes on in the next paragraph; a heading is followed by a plain one.
        let next_kind = if matches!(kind, Kind::Bullet | Kind::Numbered) {
            kind
        } else {
            Kind::Paragraph
        };
        let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
        let first = lines.next().unwrap_or_default();
        let mut made: Vec<Para> = Vec::new();
        let align = self.paras[start.para].align;
        let mut current = Para {
            id: self.paras[start.para].id,
            kind,
            align,
            cell,
            picture: None,
            fmt: self.paras[start.para].fmt,
            runs: head,
        };
        current.runs.push(Run {
            text: first.to_string(),
            style,
        });
        for line in lines {
            made.push(std::mem::replace(
                &mut current,
                Para {
                    id: self.next_id,
                    kind: next_kind,
                    align,
                    cell,
                    picture: None,
                    fmt: ParaFmt {
                        page_break: false,
                        ..self.paras[start.para].fmt
                    },
                    runs: vec![Run {
                        text: line.to_string(),
                        style,
                    }],
                },
            ));
            self.next_id += 1;
        }
        let caret = Pos {
            para: start.para + made.len(),
            offset: current.runs.iter().map(|r| units(&r.text)).sum(),
        };
        current.runs.extend(tail);
        made.push(current);
        for p in &mut made {
            Self::normalize(p);
        }
        self.paras.splice(start.para..=end.para, made);
        caret
    }

    // ---- typefaces ----

    /// The name a font number stands for.
    #[must_use]
    pub fn font_name(&self, font: u32) -> Option<&str> {
        self.fonts.get(font as usize).map(String::as_str)
    }

    /// The number for a typeface name (a new one when the document has not got it yet).
    pub fn font_id(&mut self, name: &str) -> u32 {
        let at = self
            .fonts
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                self.fonts.push(name.to_string());
                self.fonts.len() - 1
            });
        u32::try_from(at).unwrap_or(0)
    }

    // ---- lists ----

    /// The marker of each list item (`None` for the other paragraphs): a bullet for each level, and numbers that count on
    /// within a run of list items, in the style of the level (1. a. i. 1. a. i. ...).
    #[must_use]
    pub fn list_markers(&self) -> Vec<Option<String>> {
        const BULLETS: [&str; 3] = ["\u{2022}", "\u{25e6}", "\u{25aa}"];
        fn letters(n: u32) -> String {
            let mut n = n;
            let mut out = String::new();
            while n > 0 {
                n -= 1;
                out.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
            }
            out
        }
        fn roman(mut n: u32) -> String {
            let mut out = String::new();
            for (v, r) in [
                (1000, "m"),
                (900, "cm"),
                (500, "d"),
                (400, "cd"),
                (100, "c"),
                (90, "xc"),
                (50, "l"),
                (40, "xl"),
                (10, "x"),
                (9, "ix"),
                (5, "v"),
                (4, "iv"),
                (1, "i"),
            ] {
                while n >= v {
                    out.push_str(r);
                    n -= v;
                }
            }
            out
        }
        let mut counters = [0_u32; 9];
        let mut previous: Option<Kind> = None;
        let mut out = Vec::with_capacity(self.paras.len());
        for p in &self.paras {
            let level = usize::from(p.fmt.level.min(8));
            match p.kind {
                Kind::Bullet | Kind::Numbered => {
                    // A run of items of one kind counts on; another kind (or a paragraph between) starts again.
                    if previous != Some(p.kind) && level == 0 {
                        counters = [0; 9];
                    }
                    previous = Some(p.kind);
                    counters[level] += 1;
                    for c in counters.iter_mut().skip(level + 1) {
                        *c = 0;
                    }
                    out.push(Some(if p.kind == Kind::Bullet {
                        BULLETS[level % 3].to_string()
                    } else {
                        match level % 3 {
                            0 => format!("{}.", counters[level]),
                            1 => format!("{}.", letters(counters[level])),
                            _ => format!("{}.", roman(counters[level])),
                        }
                    }));
                }
                _ => {
                    previous = None;
                    out.push(None);
                }
            }
        }
        out
    }

    // ---- page setup ----

    /// The text at the top of every page (`{page}` and `{pages}` stand for the numbers; lines are separated by `\n`).
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }

    /// The text at the foot of every page (empty: just the page number).
    #[must_use]
    pub fn footer(&self) -> &str {
        &self.footer
    }

    pub fn set_header(&mut self, text: &str) {
        self.header = text.to_string();
    }

    pub fn set_footer(&mut self, text: &str) {
        self.footer = text.to_string();
    }

    /// The paper the document is set for (A4 unless it was changed).
    #[must_use]
    pub const fn paper(&self) -> PaperSize {
        self.paper
    }

    pub const fn set_paper(&mut self, paper: PaperSize) {
        self.paper = paper;
    }

    // ---- pictures ----

    /// The picture a picture paragraph shows.
    #[must_use]
    pub fn picture(&self, number: u32) -> Option<&Picture> {
        self.pictures.get(number as usize)
    }

    /// Put a picture in a paragraph of its own after paragraph `after` (after the whole table when that is in one); a plain
    /// paragraph is kept after it to type in. Returns the picture's paragraph.
    pub fn insert_picture(&mut self, after: usize, picture: Picture) -> Pos {
        let after = self
            .table_of(after)
            .map_or_else(|| after.min(self.paras.len() - 1), |t| t.1);
        let number = u32::try_from(self.pictures.len()).unwrap_or(0);
        self.pictures.push(picture);
        let mut made = self.plain_para();
        made.picture = Some(number);
        let at = after + 1;
        let mut new = vec![made];
        if self
            .paras
            .get(at)
            .is_none_or(|p| p.cell.is_some() || p.picture.is_some())
        {
            new.push(self.plain_para());
        }
        self.paras.splice(at..at, new);
        Pos::new(at, 0)
    }

    /// Make a paragraph a picture paragraph. (For readers.)
    pub fn set_picture(&mut self, para: usize, picture: Picture) {
        let number = u32::try_from(self.pictures.len()).unwrap_or(0);
        self.pictures.push(picture);
        if let Some(p) = self.paras.get_mut(para) {
            p.picture = Some(number);
            p.runs.clear();
        }
    }

    /// Show a picture at `factor` times its size (never wider than `max_width` points).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn scale_picture(&mut self, number: u32, factor: f64, max_width: f64) {
        if let Some(p) = self.pictures.get_mut(number as usize) {
            let (w, h) = p.size();
            let factor = if w * factor > max_width {
                max_width / w
            } else {
                factor
            };
            let to = |v: f64| (v * factor * 100.0).round().max(1000.0) as u32;
            (p.width, p.height) = (to(w), to(h));
        }
    }

    // ---- tables ----

    /// Add an empty plain paragraph at the end; returns its number. (For readers building a document.)
    pub fn append_para(&mut self) -> usize {
        let p = self.plain_para();
        self.paras.push(p);
        self.paras.len() - 1
    }

    /// Make a paragraph the content of a table cell (or with `None` a plain paragraph again). (For readers.)
    pub fn set_cell(&mut self, para: usize, cell: Option<Cell>) {
        if let Some(p) = self.paras.get_mut(para) {
            p.cell = cell;
        }
    }

    /// A table number no table has.
    #[must_use]
    pub fn new_table_id(&self) -> u32 {
        self.paras
            .iter()
            .filter_map(|p| p.cell)
            .map(|c| c.table)
            .max()
            .map_or(1, |t| t + 1)
    }

    /// The cell a paragraph is in.
    #[must_use]
    pub fn cell_at(&self, para: usize) -> Option<Cell> {
        self.paras.get(para).and_then(|p| p.cell)
    }

    /// The first and last paragraph of the table a paragraph is in, with its number of rows and of columns.
    #[must_use]
    pub fn table_of(&self, para: usize) -> Option<(usize, usize, usize, usize)> {
        let t = self.cell_at(para)?.table;
        let first = self
            .paras
            .iter()
            .position(|p| p.cell.is_some_and(|c| c.table == t))?;
        let last = self
            .paras
            .iter()
            .rposition(|p| p.cell.is_some_and(|c| c.table == t))?;
        let (mut rows, mut cols) = (0, 0);
        for c in self.paras[first..=last].iter().filter_map(|p| p.cell) {
            rows = rows.max(usize::from(c.row) + 1);
            cols = cols.max(usize::from(c.col) + 1);
        }
        Some((first, last, rows, cols))
    }

    /// The number of columns of a table, by its number.
    #[must_use]
    pub fn table_cols(&self, table: u32) -> usize {
        self.paras
            .iter()
            .filter_map(|p| p.cell)
            .filter(|c| c.table == table)
            .map(|c| usize::from(c.col) + 1)
            .max()
            .unwrap_or(0)
    }

    fn table_fmt(&self, table: u32) -> Option<&TableFmt> {
        self.tables.iter().find(|t| t.0 == table).map(|t| &t.1)
    }

    fn table_fmt_mut(&mut self, table: u32) -> &mut TableFmt {
        if let Some(at) = self.tables.iter().position(|t| t.0 == table) {
            return &mut self.tables[at].1;
        }
        self.tables.push((table, TableFmt::default()));
        let last = self.tables.len() - 1;
        &mut self.tables[last].1
    }

    /// The widths of the columns of a table in thousandths of its width (they add up to 1000).
    #[must_use]
    pub fn table_widths(&self, table: u32) -> Vec<u16> {
        let cols = self.table_cols(table).max(1);
        match self.table_fmt(table) {
            Some(f) if f.widths.len() == cols && f.widths.iter().all(|w| *w > 0) => {
                f.widths.clone()
            }
            _ => {
                let each = 1000 / u16::try_from(cols).unwrap_or(1);
                let mut v = vec![each; cols];
                v[cols - 1] += 1000 - each * u16::try_from(cols).unwrap_or(1);
                v
            }
        }
    }

    /// Make column `col` of the table `by` thousandths wider (narrower when negative), taking from or giving to its right
    /// neighbour (the left one for the last column); no column goes below 5 percent.
    pub fn resize_column(&mut self, table: u32, col: usize, by: i32) {
        let mut w: Vec<i32> = self
            .table_widths(table)
            .into_iter()
            .map(i32::from)
            .collect();
        if w.len() < 2 || col >= w.len() {
            return;
        }
        let other = if col + 1 < w.len() { col + 1 } else { col - 1 };
        let by = by.clamp(50 - w[col], w[other] - 50);
        w[col] += by;
        w[other] -= by;
        self.table_fmt_mut(table).widths = w
            .into_iter()
            .map(|v| u16::try_from(v).unwrap_or(50))
            .collect();
    }

    /// Set the widths of a table's columns (thousandths; they are scaled to add up to 1000). (For readers.)
    pub fn set_table_widths(&mut self, table: u32, widths: &[u32]) {
        let total: u32 = widths.iter().sum();
        if total == 0 || widths.len() != self.table_cols(table) {
            return;
        }
        let mut scaled: Vec<u16> = widths
            .iter()
            .map(|w| u16::try_from((w * 1000 / total).max(1)).unwrap_or(1))
            .collect();
        let sum: i32 = scaled.iter().map(|w| i32::from(*w)).sum();
        if let Some(last) = scaled.last_mut() {
            *last = u16::try_from((i32::from(*last) + 1000 - sum).max(1)).unwrap_or(1);
        }
        self.table_fmt_mut(table).widths = scaled;
    }

    /// Whether the first row of the table is a header row.
    #[must_use]
    pub fn table_header(&self, table: u32) -> bool {
        self.table_fmt(table).is_some_and(|f| f.header)
    }

    /// Record that the first row is a header (without changing how it is set). (For readers.)
    pub fn mark_table_header(&mut self, table: u32, on: bool) {
        self.table_fmt_mut(table).header = on;
    }

    /// Make the first row of the table a header row (set in bold), or an ordinary one again.
    pub fn set_table_header(&mut self, table: u32, on: bool) {
        self.table_fmt_mut(table).header = on;
        let mut spans = Vec::new();
        for (i, p) in self.paras.iter().enumerate() {
            if p.cell.is_some_and(|c| c.table == table && c.row == 0) {
                spans.push(i);
            }
        }
        for i in spans {
            let end = Pos::new(i, self.len_of(i));
            self.restyle(Pos::new(i, 0), end, |s| Style { bold: on, ..s });
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    fn blank_cell(&mut self, cell: Cell) -> Para {
        let id = self.next_id;
        self.next_id += 1;
        Para {
            id,
            kind: Kind::Paragraph,
            align: Align::Left,
            cell: Some(cell),
            picture: None,
            fmt: ParaFmt::default(),
            runs: Vec::new(),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    fn plain_para(&mut self) -> Para {
        let id = self.next_id;
        self.next_id += 1;
        Para {
            id,
            kind: Kind::Paragraph,
            align: Align::Left,
            cell: None,
            picture: None,
            fmt: ParaFmt::default(),
            runs: Vec::new(),
        }
    }

    /// Put a table of empty cells after paragraph `after` (after the whole table when that is in one); a plain paragraph is
    /// kept after it to type in. Returns the start of the first cell.
    pub fn insert_table(&mut self, after: usize, rows: usize, cols: usize) -> Pos {
        let (rows, cols) = (rows.clamp(1, 200), cols.clamp(1, 8));
        let after = self
            .table_of(after)
            .map_or_else(|| after.min(self.paras.len() - 1), |t| t.1);
        let table = self
            .paras
            .iter()
            .filter_map(|p| p.cell)
            .map(|c| c.table)
            .max()
            .map_or(1, |t| t + 1);
        let mut made = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let cell = Cell {
                    table,
                    row: u16::try_from(r).unwrap_or(0),
                    col: u16::try_from(c).unwrap_or(0),
                };
                made.push(self.blank_cell(cell));
            }
        }
        if self.paras.get(after + 1).is_none_or(|p| p.cell.is_some()) {
            made.push(self.plain_para());
        }
        let at = after + 1;
        self.paras.splice(at..at, made);
        Pos::new(after + 1, 0)
    }

    /// The cells of the table around paragraph `para` as rows, and what is before and after it.
    fn grid_of(&mut self, para: usize) -> Option<(usize, usize, u32, Vec<Vec<Para>>)> {
        let (first, last, rows, cols) = self.table_of(para)?;
        let table = self.paras[first].cell?.table;
        let mut grid: Vec<Vec<Option<Para>>> = vec![vec![None; cols]; rows];
        for p in self.paras.drain(first..=last) {
            if let Some(c) = p.cell {
                grid[usize::from(c.row)][usize::from(c.col)] = Some(p);
            }
        }
        let mut out = Vec::new();
        for (r, row) in grid.into_iter().enumerate() {
            let mut cells = Vec::new();
            for (c, p) in row.into_iter().enumerate() {
                let cell = Cell {
                    table,
                    row: u16::try_from(r).unwrap_or(0),
                    col: u16::try_from(c).unwrap_or(0),
                };
                cells.push(p.unwrap_or_else(|| self.blank_cell(cell)));
            }
            out.push(cells);
        }
        Some((first, last, table, out))
    }

    fn put_grid(&mut self, first: usize, table: u32, grid: Vec<Vec<Para>>) {
        let mut made = Vec::new();
        for (r, row) in grid.into_iter().enumerate() {
            for (c, mut p) in row.into_iter().enumerate() {
                p.cell = Some(Cell {
                    table,
                    row: u16::try_from(r).unwrap_or(0),
                    col: u16::try_from(c).unwrap_or(0),
                });
                made.push(p);
            }
        }
        self.paras.splice(first..first, made);
    }

    /// A row below the one `para` is in; returns the start of its first cell.
    pub fn add_row(&mut self, para: usize) -> Option<Pos> {
        let row = usize::from(self.cell_at(para)?.row);
        let (first, _, table, mut grid) = self.grid_of(para)?;
        let cols = grid[0].len();
        let blank: Vec<Para> = (0..cols)
            .map(|c| {
                self.blank_cell(Cell {
                    table,
                    row: 0,
                    col: u16::try_from(c).unwrap_or(0),
                })
            })
            .collect();
        grid.insert(row + 1, blank);
        let at = first + (row + 1) * cols;
        self.put_grid(first, table, grid);
        Some(Pos::new(at, 0))
    }

    /// A column right of the one `para` is in; returns the start of the new cell in the same row.
    pub fn add_col(&mut self, para: usize) -> Option<Pos> {
        let c = self.cell_at(para)?;
        let (row, col) = (usize::from(c.row), usize::from(c.col));
        let (first, _, table, mut grid) = self.grid_of(para)?;
        self.tables.retain(|t| t.0 != table || t.1.header);
        self.table_fmt_mut(table).widths.clear();
        let blanks: Vec<Para> = (0..grid.len())
            .map(|_| {
                self.blank_cell(Cell {
                    table,
                    row: 0,
                    col: 0,
                })
            })
            .collect();
        for (r, blank) in grid.iter_mut().zip(blanks) {
            r.insert(col + 1, blank);
        }
        let cols = grid[0].len();
        self.put_grid(first, table, grid);
        Some(Pos::new(first + row * cols + col + 1, 0))
    }

    /// Take the row of `para` out (the whole table when it is the last row); returns where the caret goes.
    pub fn delete_row(&mut self, para: usize) -> Option<Pos> {
        let row = usize::from(self.cell_at(para)?.row);
        let (first, _, table, mut grid) = self.grid_of(para)?;
        if grid.len() == 1 {
            self.tables.retain(|t| t.0 != table);
            return Some(self.after_table_removed(first));
        }
        let cols = grid[0].len();
        grid.remove(row);
        let r = row.min(grid.len() - 1);
        self.put_grid(first, table, grid);
        Some(Pos::new(first + r * cols, 0))
    }

    /// Take the column of `para` out (the whole table when it is the last column).
    pub fn delete_col(&mut self, para: usize) -> Option<Pos> {
        let c = self.cell_at(para)?;
        let (row, col) = (usize::from(c.row), usize::from(c.col));
        let (first, _, table, mut grid) = self.grid_of(para)?;
        if grid[0].len() == 1 {
            self.tables.retain(|t| t.0 != table);
            return Some(self.after_table_removed(first));
        }
        self.table_fmt_mut(table).widths.clear();
        for r in &mut grid {
            r.remove(col);
        }
        let cols = grid[0].len();
        let col = col.min(cols - 1);
        self.put_grid(first, table, grid);
        Some(Pos::new(first + row * cols + col, 0))
    }

    /// Take the whole table out.
    pub fn delete_table(&mut self, para: usize) -> Option<Pos> {
        let (first, _, table, _) = self.grid_of(para)?;
        self.tables.retain(|t| t.0 != table);
        Some(self.after_table_removed(first))
    }

    /// The table has just been taken out at `first`: keep a paragraph in the document and say where the caret goes.
    fn after_table_removed(&mut self, first: usize) -> Pos {
        if self.paras.is_empty() {
            let p = self.plain_para();
            self.paras.push(p);
        }
        Pos::new(first.min(self.paras.len() - 1), 0)
    }

    /// Delete a range.
    pub fn delete(&mut self, start: Pos, end: Pos) -> Pos {
        self.replace(start, end, "")
    }

    /// Whether every character of the range has the mark.
    #[must_use]
    pub fn all_marked(&self, start: Pos, end: Pos, mark: Mark) -> bool {
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        for p in start.para..=end.para {
            let (from, to) = (
                if p == start.para { start.offset } else { 0 },
                if p == end.para {
                    end.offset
                } else {
                    self.len_of(p)
                },
            );
            let mut at = 0;
            for r in &self.paras[p].runs {
                let len = units(&r.text);
                if at < to && at + len > from && !r.style.has(mark) {
                    return false;
                }
                at += len;
            }
        }
        true
    }

    /// Turn a mark on over the range, or off when the whole range has it. Returns whether it is on now.
    pub fn toggle(&mut self, start: Pos, end: Pos, mark: Mark) -> bool {
        let on = !self.all_marked(start, end, mark);
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        for p in start.para..=end.para {
            let (from, to) = (
                if p == start.para { start.offset } else { 0 },
                if p == end.para {
                    end.offset
                } else {
                    self.len_of(p)
                },
            );
            if from >= to {
                continue;
            }
            let (before, rest) = Self::cut(&self.paras[p].runs, from);
            let (middle, after) = Self::cut(&rest, to - from);
            let mut runs = before;
            runs.extend(middle.into_iter().map(|r| Run {
                style: r.style.with(mark, on),
                ..r
            }));
            runs.extend(after);
            self.paras[p].runs = runs;
            Self::normalize(&mut self.paras[p]);
        }
        on
    }

    /// The address a link number stands for.
    #[must_use]
    pub fn url(&self, link: u32) -> Option<&str> {
        self.links.get(link as usize).map(String::as_str)
    }

    /// The number for an address (a new one when the document has not got it yet).
    pub fn link_id(&mut self, url: &str) -> u32 {
        let at = self.links.iter().position(|u| u == url).unwrap_or_else(|| {
            self.links.push(url.to_string());
            self.links.len() - 1
        });
        u32::try_from(at).unwrap_or(0)
    }

    /// Make the text of the range a link to `url`, or with `None` take the link off it.
    pub fn set_link(&mut self, start: Pos, end: Pos, url: Option<&str>) {
        let id = url.map(|u| self.link_id(u));
        self.restyle(start, end, |s| Style { link: id, ..s });
    }

    /// The address of the link the position is in or right after, if any.
    #[must_use]
    pub fn link_at(&self, pos: Pos) -> Option<&str> {
        self.style_at(pos).link.and_then(|l| self.url(l))
    }

    /// Change the style of everything in the range with `f`.
    pub fn restyle(&mut self, start: Pos, end: Pos, f: impl Fn(Style) -> Style) {
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        for p in start.para..=end.para {
            let (from, to) = (
                if p == start.para { start.offset } else { 0 },
                if p == end.para {
                    end.offset
                } else {
                    self.len_of(p)
                },
            );
            if from >= to {
                continue;
            }
            let (before, rest) = Self::cut(&self.paras[p].runs, from);
            let (middle, after) = Self::cut(&rest, to - from);
            let mut runs = before;
            runs.extend(middle.into_iter().map(|r| Run {
                style: f(r.style),
                ..r
            }));
            runs.extend(after);
            self.paras[p].runs = runs;
            Self::normalize(&mut self.paras[p]);
        }
    }

    /// Whether the style of every character of the range is the same: that style, or `None` where they differ.
    #[must_use]
    pub fn common_style(&self, start: Pos, end: Pos) -> Option<Style> {
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        if start == end {
            return Some(self.style_at(start));
        }
        let mut found: Option<Style> = None;
        for p in start.para..=end.para {
            let (from, to) = (
                if p == start.para { start.offset } else { 0 },
                if p == end.para {
                    end.offset
                } else {
                    self.len_of(p)
                },
            );
            let mut at = 0;
            for r in &self.paras[p].runs {
                let len = units(&r.text);
                if at < to && at + len > from {
                    match found {
                        None => found = Some(r.style),
                        Some(f) if f != r.style => return None,
                        _ => {}
                    }
                }
                at += len;
            }
        }
        found.or_else(|| Some(self.style_at(start)))
    }

    /// Give a paragraph another alignment.
    pub fn set_align(&mut self, para: usize, align: Align) {
        if let Some(p) = self.paras.get_mut(para) {
            p.align = align;
        }
    }

    /// Change how a paragraph is spaced and indented.
    pub fn set_fmt(&mut self, para: usize, f: impl FnOnce(ParaFmt) -> ParaFmt) {
        if let Some(p) = self.paras.get_mut(para) {
            p.fmt = f(p.fmt);
        }
    }

    /// Give a paragraph another kind.
    pub fn set_kind(&mut self, para: usize, kind: Kind) {
        if let Some(p) = self.paras.get_mut(para) {
            p.kind = kind;
        }
    }

    /// Make the text of a paragraph what the browser made of it (after an input method composed). Only what differs is
    /// replaced, so the styles of the rest stay; returns the position after what changed.
    pub fn set_para_text(&mut self, para: usize, text: &str) -> Pos {
        if para >= self.paras.len() {
            return self.clamp(Pos::new(para, 0));
        }
        let old: Vec<u16> = self.text(para).encode_utf16().collect();
        let new: Vec<u16> = text.encode_utf16().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let inserted = String::from_utf16_lossy(&new[prefix..new.len() - suffix]);
        let (from, to) = (Pos::new(para, prefix), Pos::new(para, old.len() - suffix));
        // A split surrogate pair at the cut is moved out of the middle so the pieces stay whole.
        if inserted.contains('\u{FFFD}') && !text.contains('\u{FFFD}') {
            self.replace(Pos::new(para, 0), Pos::new(para, old.len()), text);
            return Pos::new(para, new.len());
        }
        self.replace(from, to, &inserted)
    }

    /// The text of a range, paragraphs joined by line breaks.
    #[must_use]
    pub fn slice(&self, start: Pos, end: Pos) -> String {
        let (start, end) = (self.clamp(start), self.clamp(end));
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut out = Vec::new();
        for p in start.para..=end.para {
            let text = self.text(p);
            let (from, to) = (
                if p == start.para { start.offset } else { 0 },
                if p == end.para {
                    end.offset
                } else {
                    units(&text)
                },
            );
            out.push(text[byte_at(&text, from)..byte_at(&text, to)].to_string());
        }
        out.join("\n")
    }
}
