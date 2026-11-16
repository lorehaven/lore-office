//! Reading `word/document.xml`.

use crate::{DOCUMENT, Error};
use docs_text::{Align, Cell, Doc, Family, Kind, Picture, Pos, Style};
use office_fmt::Package;
use office_fmt::xml::{self, Value};
use std::rc::Rc;

/// A document read from a file: the text, and the package it came in (kept for writing back).
#[derive(Clone, Debug)]
pub struct Docx {
    pub doc: Doc,
    pub package: Package,
    pub report: Report,
}

/// What reading simplified.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Tables, whose cells became paragraphs.
    pub tables: usize,
    /// Pictures, drawings and objects, which are not shown.
    pub pictures: usize,
}

impl Report {
    /// Whether anything was simplified.
    #[must_use]
    pub const fn is_lossy(&self) -> bool {
        self.tables > 0 || self.pictures > 0
    }
}

fn flag(rpr: &Value, name: &str) -> bool {
    xml::child(rpr, name)
        .is_some_and(|e| !matches!(xml::attr(e, "w:val"), Some("0" | "false" | "none")))
}

/// The kind of typeface a font name stands for.
fn family_of(name: &str) -> Family {
    if let Some(known) = docs_text::typeface(name.trim()) {
        return known.family;
    }
    let n = name.to_lowercase();
    if [
        "courier",
        "consolas",
        "mono",
        "menlo",
        "lucida console",
        "source code",
    ]
    .iter()
    .any(|m| n.contains(m))
    {
        Family::Mono
    } else if [
        "times",
        "georgia",
        "cambria",
        "garamond",
        "serif",
        "palatino",
        "book antiqua",
        "liberation serif",
    ]
    .iter()
    .any(|m| n.contains(m))
        && !n.contains("sans")
    {
        Family::Serif
    } else {
        Family::Sans
    }
}

/// The highlight of a run: a `w:highlight` colour name, or the fill of its shading.
fn highlight_of(rpr: &Value) -> Option<u32> {
    if let Some(name) = xml::child(rpr, "w:highlight").and_then(|e| xml::attr(e, "w:val")) {
        return match name {
            "yellow" => Some(0x00FF_FF00),
            "green" => Some(0x0000_FF00),
            "cyan" => Some(0x0000_FFFF),
            "magenta" => Some(0x00FF_00FF),
            "blue" => Some(0x0000_00FF),
            "red" => Some(0x00FF_0000),
            "darkYellow" => Some(0x0080_8000),
            "lightGray" => Some(0x00C0_C0C0),
            "darkGray" => Some(0x0080_8080),
            "darkBlue" => Some(0x0000_0080),
            "darkCyan" => Some(0x0000_8080),
            "darkGreen" => Some(0x0000_8000),
            "darkMagenta" => Some(0x0080_0080),
            "darkRed" => Some(0x0080_0000),
            _ => None,
        };
    }
    xml::child(rpr, "w:shd")
        .and_then(|e| xml::attr(e, "w:fill"))
        .filter(|v| v.len() == 6)
        .and_then(|v| u32::from_str_radix(v, 16).ok())
}

/// The typeface a run names, when it is not one of the three the editor has by name.
fn font_name_of(run: &Value) -> Option<String> {
    let rpr = xml::child(run, "w:rPr")?;
    let f = xml::child(rpr, "w:rFonts")?;
    let name = xml::attr(f, "w:ascii").or_else(|| xml::attr(f, "w:hAnsi"))?;
    (!matches!(name, "Arial" | "Times New Roman" | "Courier New")).then(|| name.to_string())
}

fn style_of(run: &Value) -> Style {
    xml::child(run, "w:rPr").map_or_else(Style::default, |rpr| Style {
        bold: flag(rpr, "w:b"),
        italic: flag(rpr, "w:i"),
        underline: flag(rpr, "w:u"),
        strike: flag(rpr, "w:strike") || flag(rpr, "w:dstrike"),
        sup: xml::child(rpr, "w:vertAlign").and_then(|e| xml::attr(e, "w:val"))
            == Some("superscript"),
        sub: xml::child(rpr, "w:vertAlign").and_then(|e| xml::attr(e, "w:val"))
            == Some("subscript"),
        highlight: highlight_of(rpr),
        size: xml::child(rpr, "w:sz")
            .and_then(|e| xml::attr(e, "w:val"))
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|s| (2..=1638).contains(s)),
        color: xml::child(rpr, "w:color")
            .and_then(|e| xml::attr(e, "w:val"))
            .filter(|v| v.len() == 6)
            .and_then(|v| u32::from_str_radix(v, 16).ok()),
        family: xml::child(rpr, "w:rFonts")
            .and_then(|e| xml::attr(e, "w:ascii").or_else(|| xml::attr(e, "w:hAnsi")))
            .map(family_of),
        link: None,
        font: None,
    })
}

/// The text a run holds: its `w:t`, tabs and breaks.
/// What the runs of a paragraph hold besides text: pictures that can be shown (a relationship id and the size, in points,
/// the file gives them), and the number of drawings, pictures and objects that cannot.
#[derive(Default)]
struct Found {
    lost: usize,
    /// A page break inside a run.
    page_break: bool,
    pictures: Vec<(String, Option<(f64, f64)>)>,
}

fn descendant<'a>(e: &'a Value, name: &str) -> Option<&'a Value> {
    xml::children(e).iter().find_map(|k| {
        if xml::tag(k) == Some(name) {
            Some(k)
        } else {
            descendant(k, name)
        }
    })
}

fn drawing(d: &Value) -> Option<(String, Option<(f64, f64)>)> {
    let rid = xml::attr_local(descendant(d, "a:blip")?, "embed")?.to_string();
    let size = descendant(d, "wp:extent").and_then(|e| {
        let cx: f64 = xml::attr(e, "cx")?.parse().ok()?;
        let cy: f64 = xml::attr(e, "cy")?.parse().ok()?;
        Some((cx / 12700.0, cy / 12700.0))
    });
    Some((rid, size))
}

fn run_text(run: &Value, found: &mut Found) -> String {
    let mut out = String::new();
    for k in xml::children(run) {
        match xml::tag(k) {
            Some("w:t") => out.push_str(&xml::text(k)),
            Some("w:tab") => out.push('\t'),
            Some("w:br") if xml::attr(k, "w:type") == Some("page") => found.page_break = true,
            Some("w:br" | "w:cr") => out.push(docs_text::LINE_BREAK),
            Some("w:noBreakHyphen") => out.push('\u{2011}'),
            Some("w:drawing") => match drawing(k) {
                Some(d) => found.pictures.push(d),
                None => found.lost += 1,
            },
            Some("w:pict" | "w:object") => found.lost += 1,
            _ => {}
        }
    }
    out
}

/// The runs under an element, however deeply wrapped (hyperlinks, smart tags, fields).
fn collect_runs(
    e: &Value,
    out: &mut Vec<(String, Style, Option<String>, Option<String>)>,
    found: &mut Found,
    links: &[(String, String)],
    link: Option<&str>,
) {
    for k in xml::children(e) {
        match xml::tag(k) {
            Some("w:r") => out.push((
                run_text(k, found),
                style_of(k),
                link.map(str::to_string),
                font_name_of(k),
            )),
            Some("w:hyperlink") => {
                let target = xml::attr(k, "r:id")
                    .and_then(|id| links.iter().find(|(i, _)| i == id))
                    .map(|(_, t)| t.clone())
                    .or_else(|| xml::attr(k, "w:anchor").map(|a| format!("#{a}")));
                collect_runs(k, out, found, links, target.as_deref().or(link));
            }
            Some("w:smartTag" | "w:sdt" | "w:sdtContent" | "w:ins" | "w:fldSimple") => {
                collect_runs(k, out, found, links, link);
            }
            _ => {}
        }
    }
}

/// Whether each list (`numId`) of a file is a bullet list.
fn bullet_lists(package: &Package) -> Vec<(String, bool)> {
    let Some(root) = package
        .get("word/numbering.xml")
        .and_then(|b| xml::parse(b, false).ok())
    else {
        return Vec::new();
    };
    let formats: Vec<(String, String)> = xml::elements(&root, "w:abstractNum")
        .filter_map(|a| {
            let id = xml::attr(a, "w:abstractNumId")?.to_string();
            let fmt = xml::elements(a, "w:lvl")
                .next()
                .and_then(|l| xml::child(l, "w:numFmt"))
                .and_then(|f| xml::attr(f, "w:val"))?
                .to_string();
            Some((id, fmt))
        })
        .collect();
    xml::elements(&root, "w:num")
        .filter_map(|n| {
            let id = xml::attr(n, "w:numId")?.to_string();
            let abs = xml::child(n, "w:abstractNumId").and_then(|a| xml::attr(a, "w:val"))?;
            let fmt = formats
                .iter()
                .find(|(a, _)| a == abs)
                .map(|(_, f)| f.as_str())?;
            Some((id, matches!(fmt, "bullet" | "none")))
        })
        .collect()
}

fn kind_of(p: &Value, lists: &[(String, bool)]) -> Kind {
    let Some(ppr) = xml::child(p, "w:pPr") else {
        return Kind::Paragraph;
    };
    let style = xml::child(ppr, "w:pStyle")
        .and_then(|s| xml::attr(s, "w:val"))
        .map(str::to_lowercase);
    if let Some(style) = &style {
        if style == "title" {
            return Kind::Heading(1);
        }
        if let Some(n) = style
            .strip_prefix("heading")
            .and_then(|n| n.trim().parse::<u8>().ok())
        {
            return Kind::Heading(n.clamp(1, 6));
        }
    }
    if let Some(num) = xml::child(ppr, "w:numPr") {
        let id = xml::child(num, "w:numId")
            .and_then(|n| xml::attr(n, "w:val"))
            .unwrap_or("");
        if id == "0" {
            return Kind::Paragraph;
        }
        return match lists.iter().find(|(n, _)| n == id) {
            Some((_, false)) => Kind::Numbered,
            _ => Kind::Bullet,
        };
    }
    match style.as_deref() {
        Some(s) if s.contains("listnumber") => Kind::Numbered,
        Some(s) if s.contains("listbullet") || s.contains("listparagraph") => Kind::Bullet,
        _ => Kind::Paragraph,
    }
}

/// A twips attribute as a number of twentieths of a point (a twip is that).
fn twips(e: &Value, name: &str) -> Option<i64> {
    xml::attr(e, name).and_then(|v| v.parse().ok())
}

fn fmt_of(p: &Value) -> docs_text::ParaFmt {
    let mut f = docs_text::ParaFmt::default();
    let Some(ppr) = xml::child(p, "w:pPr") else {
        return f;
    };
    // A list item's indent is the list's own (the numbering gives it), not the paragraph's.
    let listed = xml::child(ppr, "w:numPr").is_some();
    if !listed && let Some(ind) = xml::child(ppr, "w:ind") {
        let left = twips(ind, "w:left")
            .or_else(|| twips(ind, "w:start"))
            .unwrap_or(0);
        f.indent = u16::try_from(left.clamp(0, 20000)).unwrap_or(0);
        let first = twips(ind, "w:firstLine")
            .or_else(|| twips(ind, "w:hanging").map(|h| -h))
            .unwrap_or(0);
        f.first = i16::try_from(first.clamp(-20000, 20000)).unwrap_or(0);
    }
    if let Some(sp) = xml::child(ppr, "w:spacing") {
        f.after = twips(sp, "w:after").and_then(|v| u16::try_from(v.clamp(0, 20000)).ok());
        // Only proportional spacing ("auto") is kept; exact and at-least heights are the file's own.
        if matches!(xml::attr(sp, "w:lineRule"), None | Some("auto")) {
            f.line = twips(sp, "w:line")
                .and_then(|v| u16::try_from(v.clamp(0, 2000)).ok())
                .unwrap_or(0);
        }
    }
    f.level = xml::child(ppr, "w:numPr")
        .and_then(|n| xml::child(n, "w:ilvl"))
        .and_then(|l| xml::attr(l, "w:val"))
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(0, |v| v.min(8));
    f.page_break = xml::child(ppr, "w:pageBreakBefore")
        .is_some_and(|e| !matches!(xml::attr(e, "w:val"), Some("0" | "false")));
    f
}

fn align_of(p: &Value) -> Align {
    match xml::child(p, "w:pPr")
        .and_then(|ppr| xml::child(ppr, "w:jc"))
        .and_then(|j| xml::attr(j, "w:val"))
    {
        Some("center") => Align::Center,
        Some("right" | "end") => Align::Right,
        Some("both" | "distribute") => Align::Justify,
        _ => Align::Left,
    }
}

#[allow(clippy::too_many_arguments)]
fn paragraph(
    doc: &mut Doc,
    first: &mut bool,
    ps: &[&Value],
    cell: Option<Cell>,
    report: &mut Report,
    lists: &[(String, bool)],
    links: &[(String, String)],
    media: &[(String, Rc<Vec<u8>>)],
) {
    let mut runs = Vec::new();
    let mut found = Found::default();
    // The paragraphs of a cell are one, with a space between.
    for (n, p) in ps.iter().enumerate() {
        if n > 0 {
            runs.push((" ".to_string(), Style::default(), None, None));
        }
        collect_runs(p, &mut runs, &mut found, links, None);
    }
    let p = ps[0];
    let at = if *first { 0 } else { doc.append_para() };
    *first = false;
    // Pictures become paragraphs of their own (a cell holds none); what cannot be shown is reported.
    let mut pictures = Vec::new();
    report.pictures += found.lost;
    for (rid, size) in &found.pictures {
        let made = media
            .iter()
            .find(|(id, _)| id == rid)
            .and_then(|(_, data)| picture_of(data, *size));
        match made {
            Some(p) if cell.is_none() => pictures.push(p),
            _ => report.pictures += 1,
        }
    }
    let only_pictures = !pictures.is_empty() && runs.iter().all(|(t, ..)| t.is_empty());
    for (text, style, link, font) in runs {
        let start = Pos::new(at, doc.len_of(at));
        doc.replace(start, start, &text);
        let end = Pos::new(at, doc.len_of(at));
        // What was inserted may be less than the run's text (a carriage return is dropped, a picture takes none).
        if end.offset > start.offset {
            // Typed text takes the style of what precedes it, so each mark is set to what this run says.
            let begin = start;
            for (wanted, mark) in [
                (style.bold, docs_text::Mark::Bold),
                (style.italic, docs_text::Mark::Italic),
                (style.underline, docs_text::Mark::Underline),
                (style.strike, docs_text::Mark::Strike),
                (style.sup, docs_text::Mark::Superscript),
                (style.sub, docs_text::Mark::Subscript),
            ] {
                if doc.all_marked(begin, end, mark) != wanted {
                    doc.toggle(begin, end, mark);
                }
            }
            let (size, color, family, highlight) =
                (style.size, style.color, style.family, style.highlight);
            doc.restyle(begin, end, |s| Style {
                size,
                color,
                family,
                highlight,
                ..s
            });
            doc.set_link(begin, end, link.as_deref());
            // Typed text takes the style before it, so the typeface is set to what this run says, also when it names none.
            let id = font.map(|name| doc.font_id(&name));
            doc.restyle(begin, end, |s| Style { font: id, ..s });
        }
    }
    doc.set_kind(at, kind_of(p, lists));
    doc.set_align(at, align_of(p));
    let mut fmt = fmt_of(p);
    fmt.page_break |= found.page_break;
    doc.set_fmt(at, |_| fmt);
    doc.set_cell(at, cell);
    let mut pictures = pictures.into_iter();
    if only_pictures && let Some(p) = pictures.next() {
        doc.set_picture(at, p);
    }
    for p in pictures {
        let n = doc.append_para();
        doc.set_picture(n, p);
        doc.set_align(n, align_of(ps[0]));
    }
}

/// A picture from the bytes of a file, at the size the document gives it (in points) or else its pixels at 96 to the inch.
fn picture_of(data: &Rc<Vec<u8>>, size: Option<(f64, f64)>) -> Option<Picture> {
    let mime = if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if data.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else {
        return None;
    };
    let (w, h) = if let Some(s) = size.filter(|(w, h)| *w >= 1.0 && *h >= 1.0) {
        s
    } else {
        let px = quench_image::decode(data).ok()?;
        (f64::from(px.width) * 0.75, f64::from(px.height) * 0.75)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let hundredths = |v: f64| (v * 100.0).round().clamp(100.0, 4_000_000.0) as u32;
    Some(Picture {
        mime,
        data: data.clone(),
        width: hundredths(w),
        height: hundredths(h),
    })
}

#[allow(clippy::too_many_arguments)]
fn blocks(
    doc: &mut Doc,
    first: &mut bool,
    parent: &Value,
    report: &mut Report,
    lists: &[(String, bool)],
    links: &[(String, String)],
    media: &[(String, Rc<Vec<u8>>)],
) {
    for b in xml::children(parent) {
        match xml::tag(b) {
            Some("w:p") => paragraph(doc, first, &[b], None, report, lists, links, media),
            Some("w:tbl") => {
                let rows: Vec<&Value> = xml::elements(b, "w:tr").collect();
                // Each row as a list of grid columns: a merged cell fills the columns it covers, the first holding the content
                // and the others empty.
                let mut merged = false;
                let expanded: Vec<Vec<Option<&Value>>> = rows
                    .iter()
                    .map(|r| {
                        let mut cols: Vec<Option<&Value>> = Vec::new();
                        for tc in xml::elements(r, "w:tc") {
                            let pr = xml::child(tc, "w:tcPr");
                            let span = pr
                                .and_then(|p| xml::child(p, "w:gridSpan"))
                                .and_then(|g| xml::attr(g, "w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .clamp(1, 8);
                            let continued = pr
                                .and_then(|p| xml::child(p, "w:vMerge"))
                                .is_some_and(|m| !matches!(xml::attr(m, "w:val"), Some("restart")));
                            merged |= span > 1
                                || pr.is_some_and(|p| {
                                    xml::child(p, "w:vMerge").is_some()
                                        || xml::child(p, "w:hMerge").is_some()
                                });
                            cols.push(if continued { None } else { Some(tc) });
                            cols.extend(std::iter::repeat_n(None, span - 1));
                        }
                        cols
                    })
                    .collect();
                let width = expanded.iter().map(Vec::len).max().unwrap_or(0);
                // A grid of up to 8 columns without a table inside a cell is kept as a table.
                let plain = !rows.is_empty()
                    && rows.len() <= 200
                    && (1..=8).contains(&width)
                    && rows.iter().all(|r| {
                        xml::elements(r, "w:tc").all(|c| {
                            xml::elements(c, "w:p").next().is_some()
                                && xml::elements(c, "w:tbl").next().is_none()
                        })
                    });
                if plain {
                    let table = doc.new_table_id();
                    let grid: Vec<u32> = xml::child(b, "w:tblGrid")
                        .map(|g| {
                            xml::elements(g, "w:gridCol")
                                .filter_map(|c| twips(c, "w:w"))
                                .map(|v| u32::try_from(v.max(1)).unwrap_or(1))
                                .collect()
                        })
                        .unwrap_or_default();
                    let header = xml::child(rows[0], "w:trPr")
                        .is_some_and(|p| xml::child(p, "w:tblHeader").is_some());
                    for (r, row) in expanded.iter().enumerate() {
                        for c in 0..width {
                            let cell = Cell {
                                table,
                                row: u16::try_from(r).unwrap_or(0),
                                col: u16::try_from(c).unwrap_or(0),
                            };
                            if let Some(tc) = row.get(c).copied().flatten() {
                                let ps: Vec<&Value> = xml::elements(tc, "w:p").collect();
                                paragraph(doc, first, &ps, Some(cell), report, lists, links, media);
                            } else {
                                let at = if *first { 0 } else { doc.append_para() };
                                *first = false;
                                doc.set_cell(at, Some(cell));
                            }
                        }
                    }
                    doc.set_table_widths(table, &grid);
                    doc.mark_table_header(table, header);
                    if merged {
                        // The merged cells are split in the grid: reported, but the table and its text are kept.
                        report.tables += 1;
                    }
                    continue;
                }
                report.tables += 1;
                // Any other table: its cells become paragraphs, a row's cells joined by tabs.
                for row in rows {
                    let cells: Vec<String> = xml::elements(row, "w:tc")
                        .map(|c| {
                            xml::elements(c, "w:p")
                                .map(|p| {
                                    let mut runs = Vec::new();
                                    let mut found = Found::default();
                                    collect_runs(p, &mut runs, &mut found, links, None);
                                    report.pictures += found.lost + found.pictures.len();
                                    runs.into_iter().map(|(t, ..)| t).collect::<String>()
                                })
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect();
                    let at = if *first { 0 } else { doc.append_para() };
                    *first = false;
                    doc.replace(Pos::new(at, 0), Pos::new(at, 0), &cells.join("\t"));
                }
            }
            Some("w:sdt") => {
                if let Some(content) = xml::child(b, "w:sdtContent") {
                    blocks(doc, first, content, report, lists, links, media);
                }
            }
            _ => {}
        }
    }
}

/// Read a `.docx`.
///
/// # Errors
///
/// [`Error::Invalid`] when the bytes are not a package with a readable `word/document.xml`.
pub fn read(bytes: &[u8]) -> Result<Docx, Error> {
    let package = Package::read(bytes)?;
    let part = package
        .get(DOCUMENT)
        .ok_or_else(|| Error::Invalid("word/document.xml is missing".into()))?;
    let root = xml::parse(part, true)?;
    let body = xml::child(&root, "w:body")
        .ok_or_else(|| Error::Invalid("the document has no body".into()))?;
    let mut doc = Doc::new();
    let (mut first, mut report) = (true, Report::default());
    let lists = bullet_lists(&package);
    let links: Vec<(String, String)> = package
        .get(&office_fmt::rels::rels_part(DOCUMENT))
        .and_then(|b| office_fmt::rels::parse(b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.external && r.kind.ends_with("/hyperlink"))
        .map(|r| (r.id, r.target))
        .collect();
    let media: Vec<(String, Rc<Vec<u8>>)> = office_fmt::rels::parse(
        package
            .get(&office_fmt::rels::rels_part(DOCUMENT))
            .unwrap_or_default(),
    )
    .unwrap_or_default()
    .into_iter()
    .filter(|r| !r.external && r.kind.ends_with("/image"))
    .filter_map(|r| {
        let path = r
            .target
            .strip_prefix('/')
            .map_or_else(|| format!("word/{}", r.target), str::to_string);
        package.get(&path).map(|b| (r.id, Rc::new(b.to_vec())))
    })
    .collect();
    blocks(
        &mut doc,
        &mut first,
        body,
        &mut report,
        &lists,
        &links,
        &media,
    );
    if let Some(section) = xml::child(body, "w:sectPr") {
        let rels = office_fmt::rels::parse(
            package
                .get(&office_fmt::rels::rels_part(DOCUMENT))
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        let part_of = |tag: &str| {
            let r = xml::elements(section, tag)
                .find(|e| matches!(xml::attr(e, "w:type"), None | Some("default")))?;
            let id = xml::attr_local(r, "id")?;
            let target = &rels.iter().find(|x| x.id == id)?.target;
            let path = target
                .strip_prefix('/')
                .map_or_else(|| format!("word/{target}"), str::to_string);
            package.get(&path).map(part_text)
        };
        if let Some(t) = part_of("w:headerReference") {
            doc.set_header(&t);
        }
        if let Some(t) = part_of("w:footerReference") {
            doc.set_footer(&t);
        }
        if let Some(size) = xml::child(section, "w:pgSz") {
            let w = u32::try_from(twips(size, "w:w").unwrap_or(0)).unwrap_or(0);
            let h = u32::try_from(twips(size, "w:h").unwrap_or(0)).unwrap_or(0);
            // Turned on its side, a sheet is the same paper.
            let (w, h) = (w.min(h), w.max(h));
            doc.set_paper(docs_text::PaperSize::from_twips(w, h).unwrap_or_default());
        }
    }
    Ok(Docx {
        doc,
        package,
        report,
    })
}

/// The text of a header or footer part, with `{page}` and `{pages}` for its page-number fields.
fn part_text(bytes: &[u8]) -> String {
    fn walk(e: &Value, out: &mut String, skipping: &mut bool) {
        for k in xml::children(e) {
            match xml::tag(k) {
                Some("w:t") if !*skipping => out.push_str(&xml::text(k)),
                Some("w:tab") if !*skipping => out.push(' '),
                Some("w:fldSimple") => {
                    let instr = xml::attr(k, "w:instr").unwrap_or("");
                    if instr.contains("NUMPAGES") {
                        out.push_str("{pages}");
                    } else if instr.contains("PAGE") {
                        out.push_str("{page}");
                    }
                }
                Some("w:instrText") => {
                    let t = xml::text(k);
                    if t.contains("NUMPAGES") {
                        out.push_str("{pages}");
                    } else if t.contains("PAGE") {
                        out.push_str("{page}");
                    }
                }
                Some("w:fldChar") => match xml::attr(k, "w:fldCharType") {
                    Some("separate") => *skipping = true,
                    Some("end") => *skipping = false,
                    _ => {}
                },
                Some(_) => walk(k, out, skipping),
                None => {}
            }
        }
    }
    let Ok(root) = xml::parse(bytes, true) else {
        return String::new();
    };
    let mut lines = Vec::new();
    for p in xml::elements(&root, "w:p") {
        let (mut line, mut skipping) = (String::new(), false);
        walk(p, &mut line, &mut skipping);
        lines.push(line.trim().to_string());
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}
