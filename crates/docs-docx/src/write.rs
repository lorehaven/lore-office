//! Writing a `.docx`: the model as `word/document.xml`, in a package that is kept or made.

use crate::{DOCUMENT, Error, MAIN_NS, REL_NS};
use docs_text::{Align, Doc, Family, Kind, Para};
use office_fmt::xml::{self, Value};
use office_fmt::{ContentTypes, Package};

const CT_DOCUMENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const CT_HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const CT_FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const CT_NUMBERING: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const REL_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const REL_NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";

fn run_xml(text: &str, style: docs_text::Style, doc: &Doc) -> Value {
    // The order of a run's properties is part of the format.
    let mut props = Vec::new();
    if let Some(f) = style.family {
        let generic = match f {
            Family::Sans => "Arial",
            Family::Serif => "Times New Roman",
            Family::Mono => "Courier New",
        };
        let name = style.font.and_then(|n| doc.font_name(n)).unwrap_or(generic);
        props.push(xml::el(
            "w:rFonts",
            &[("w:ascii", name), ("w:hAnsi", name), ("w:cs", name)],
            Vec::new(),
        ));
    }
    if style.bold {
        props.push(xml::el("w:b", &[], Vec::new()));
    }
    if style.italic {
        props.push(xml::el("w:i", &[], Vec::new()));
    }
    if style.strike {
        props.push(xml::el("w:strike", &[], Vec::new()));
    }
    if let Some(c) = style.color {
        props.push(xml::el(
            "w:color",
            &[("w:val", &format!("{c:06X}"))],
            Vec::new(),
        ));
    }
    if let Some(sz) = style.size {
        props.push(xml::el("w:sz", &[("w:val", &sz.to_string())], Vec::new()));
        props.push(xml::el("w:szCs", &[("w:val", &sz.to_string())], Vec::new()));
    }
    if style.underline {
        props.push(xml::el("w:u", &[("w:val", "single")], Vec::new()));
    }
    if let Some(h) = style.highlight {
        props.push(xml::el(
            "w:shd",
            &[
                ("w:val", "clear"),
                ("w:color", "auto"),
                ("w:fill", &format!("{h:06X}")),
            ],
            Vec::new(),
        ));
    }
    if style.sup || style.sub {
        props.push(xml::el(
            "w:vertAlign",
            &[(
                "w:val",
                if style.sup {
                    "superscript"
                } else {
                    "subscript"
                },
            )],
            Vec::new(),
        ));
    }
    let mut kids = Vec::new();
    if !props.is_empty() {
        kids.push(xml::el("w:rPr", &[], props));
    }
    // Tabs and line breaks are elements of their own between the pieces of text.
    let mut piece = String::new();
    let flush = |piece: &mut String, kids: &mut Vec<Value>| {
        if !piece.is_empty() {
            kids.push(xml::el(
                "w:t",
                &[("xml:space", "preserve")],
                vec![xml::txt(piece)],
            ));
            piece.clear();
        }
    };
    for c in text.chars() {
        match c {
            '\t' => {
                flush(&mut piece, &mut kids);
                kids.push(xml::el("w:tab", &[], Vec::new()));
            }
            docs_text::LINE_BREAK => {
                flush(&mut piece, &mut kids);
                kids.push(xml::el("w:br", &[], Vec::new()));
            }
            c => piece.push(c),
        }
    }
    flush(&mut piece, &mut kids);
    xml::el("w:r", &[], kids)
}

/// An inline picture in a run. (The EMU, 12700 to the point, is the unit of drawings.)
#[allow(clippy::cast_possible_truncation)]
fn drawing_xml(pic: &docs_text::Picture, rid: &str, n: u32) -> Value {
    let (w, h) = pic.size();
    let (cx, cy) = (
        ((w * 12700.0).round() as i64).to_string(),
        ((h * 12700.0).round() as i64).to_string(),
    );
    let id = (n + 1).to_string();
    let name = format!("Picture {id}");
    let a = "http://schemas.openxmlformats.org/drawingml/2006/main";
    let pc = "http://schemas.openxmlformats.org/drawingml/2006/picture";
    let blip = xml::el("a:blip", &[("r:embed", rid)], Vec::new());
    let inline = xml::el(
        "wp:inline",
        &[
            (
                "xmlns:wp",
                "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
            ),
            ("distT", "0"),
            ("distB", "0"),
            ("distL", "0"),
            ("distR", "0"),
        ],
        vec![
            xml::el("wp:extent", &[("cx", &cx), ("cy", &cy)], Vec::new()),
            xml::el("wp:docPr", &[("id", &id), ("name", &name)], Vec::new()),
            xml::el(
                "a:graphic",
                &[("xmlns:a", a)],
                vec![xml::el(
                    "a:graphicData",
                    &[("uri", pc)],
                    vec![xml::el(
                        "pic:pic",
                        &[("xmlns:pic", pc)],
                        vec![
                            xml::el(
                                "pic:nvPicPr",
                                &[],
                                vec![
                                    xml::el(
                                        "pic:cNvPr",
                                        &[("id", &id), ("name", &name)],
                                        Vec::new(),
                                    ),
                                    xml::el("pic:cNvPicPr", &[], Vec::new()),
                                ],
                            ),
                            xml::el(
                                "pic:blipFill",
                                &[],
                                vec![
                                    blip,
                                    xml::el(
                                        "a:stretch",
                                        &[],
                                        vec![xml::el("a:fillRect", &[], Vec::new())],
                                    ),
                                ],
                            ),
                            xml::el(
                                "pic:spPr",
                                &[],
                                vec![
                                    xml::el(
                                        "a:xfrm",
                                        &[],
                                        vec![
                                            xml::el("a:off", &[("x", "0"), ("y", "0")], Vec::new()),
                                            xml::el(
                                                "a:ext",
                                                &[("cx", &cx), ("cy", &cy)],
                                                Vec::new(),
                                            ),
                                        ],
                                    ),
                                    xml::el(
                                        "a:prstGeom",
                                        &[("prst", "rect")],
                                        vec![xml::el("a:avLst", &[], Vec::new())],
                                    ),
                                ],
                            ),
                        ],
                    )],
                )],
            ),
        ],
    );
    xml::el("w:r", &[], vec![xml::el("w:drawing", &[], vec![inline])])
}

fn num_pr(id: &str, level: u8) -> Value {
    xml::el(
        "w:numPr",
        &[],
        vec![
            xml::el(
                "w:ilvl",
                &[("w:val", &level.min(8).to_string())],
                Vec::new(),
            ),
            xml::el("w:numId", &[("w:val", id)], Vec::new()),
        ],
    )
}

/// A paragraph; `list` is the `numId` of the list it belongs to, for the kinds that are list items.
fn paragraph_xml(
    doc: &Doc,
    p: &Para,
    list: Option<&str>,
    links: &[(u32, String)],
    pics: &[(u32, String)],
) -> Value {
    let mut props = Vec::new();
    match p.kind {
        Kind::Paragraph => {}
        Kind::Heading(n) => props.push(xml::el(
            "w:pStyle",
            &[("w:val", &format!("Heading{n}"))],
            Vec::new(),
        )),
        Kind::Bullet | Kind::Numbered => {
            props.push(xml::el(
                "w:pStyle",
                &[("w:val", "ListParagraph")],
                Vec::new(),
            ));
            if let Some(id) = list {
                props.push(num_pr(id, p.fmt.level));
            }
        }
    }
    let f = p.fmt;
    if f.page_break {
        // After the style, before the list: the order of the schema.
        let at = props
            .iter()
            .position(|e| xml::tag(e) == Some("w:numPr"))
            .unwrap_or(props.len());
        props.insert(at, xml::el("w:pageBreakBefore", &[], Vec::new()));
    }
    if f.after.is_some() || f.line != 0 {
        let mut attrs: Vec<(&str, String)> = Vec::new();
        if let Some(a) = f.after {
            attrs.push(("w:after", a.to_string()));
        }
        if f.line != 0 {
            attrs.push(("w:line", f.line.to_string()));
            attrs.push(("w:lineRule", "auto".to_string()));
        }
        let refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        props.push(xml::el("w:spacing", &refs, Vec::new()));
    }
    if f.indent != 0 || f.first != 0 {
        let mut attrs: Vec<(&str, String)> = vec![("w:left", f.indent.to_string())];
        if f.first > 0 {
            attrs.push(("w:firstLine", f.first.to_string()));
        } else if f.first < 0 {
            attrs.push(("w:hanging", (-i32::from(f.first)).to_string()));
        }
        let refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        props.push(xml::el("w:ind", &refs, Vec::new()));
    }
    let jc = match p.align {
        Align::Left => None,
        Align::Center => Some("center"),
        Align::Right => Some("right"),
        Align::Justify => Some("both"),
    };
    if let Some(j) = jc {
        props.push(xml::el("w:jc", &[("w:val", j)], Vec::new()));
    }
    let mut kids = Vec::new();
    if !props.is_empty() {
        kids.push(xml::el("w:pPr", &[], props));
    }
    if let Some(n) = p.picture
        && let (Some(pic), Some((_, rid))) = (doc.picture(n), pics.iter().find(|(k, _)| *k == n))
    {
        kids.push(drawing_xml(pic, rid, n));
    }
    let mut i = 0;
    while i < p.runs.len() {
        let link = p.runs[i].style.link;
        let mut j = i + 1;
        while link.is_some() && j < p.runs.len() && p.runs[j].style.link == link {
            j += 1;
        }
        let runs: Vec<Value> = p.runs[i..j]
            .iter()
            .map(|r| run_xml(&r.text, r.style, doc))
            .collect();
        let rid = link
            .and_then(|l| links.iter().find(|(n, _)| *n == l))
            .map(|(_, id)| id.as_str());
        match rid {
            Some(id) => kids.push(xml::el("w:hyperlink", &[("r:id", id)], runs)),
            None => kids.extend(runs),
        }
        i = j;
    }
    xml::el("w:p", &[], kids)
}

fn style_xml(
    id: &str,
    name: &str,
    size_half_points: Option<u32>,
    bold: bool,
    based: Option<&str>,
) -> Value {
    let mut props = Vec::new();
    if bold {
        props.push(xml::el("w:b", &[], Vec::new()));
    }
    if let Some(s) = size_half_points {
        props.push(xml::el("w:sz", &[("w:val", &s.to_string())], Vec::new()));
    }
    let mut kids = vec![xml::el("w:name", &[("w:val", name)], Vec::new())];
    if let Some(b) = based {
        kids.push(xml::el("w:basedOn", &[("w:val", b)], Vec::new()));
    }
    kids.push(xml::el("w:qFormat", &[], Vec::new()));
    if !props.is_empty() {
        kids.push(xml::el("w:rPr", &[], props));
    }
    xml::el(
        "w:style",
        &[("w:type", "paragraph"), ("w:styleId", id)],
        kids,
    )
}

fn wanted_styles() -> Vec<Value> {
    vec![
        style_xml("Heading1", "heading 1", Some(40), true, Some("Normal")),
        style_xml("Heading2", "heading 2", Some(32), true, Some("Normal")),
        style_xml("Heading3", "heading 3", Some(26), true, Some("Normal")),
        style_xml("Heading4", "heading 4", Some(24), true, Some("Normal")),
        style_xml("Heading5", "heading 5", Some(22), true, Some("Normal")),
        style_xml("Heading6", "heading 6", Some(22), true, Some("Normal")),
        style_xml(
            "ListParagraph",
            "List Paragraph",
            None,
            false,
            Some("Normal"),
        ),
    ]
}

fn styles_part() -> Value {
    let mut kids = vec![xml::el(
        "w:style",
        &[
            ("w:type", "paragraph"),
            ("w:default", "1"),
            ("w:styleId", "Normal"),
        ],
        vec![xml::el("w:name", &[("w:val", "Normal")], Vec::new())],
    )];
    kids.extend(wanted_styles());
    xml::el("w:styles", &[("xmlns:w", MAIN_NS)], kids)
}

/// One level of a list definition: its number format and text, and how far in it sits.
fn level_xml(level: usize, format: &str, text: &str) -> Value {
    let left = (720 * (level + 1)).to_string();
    xml::el(
        "w:lvl",
        &[("w:ilvl", &level.to_string())],
        vec![
            xml::el("w:start", &[("w:val", "1")], Vec::new()),
            xml::el("w:numFmt", &[("w:val", format)], Vec::new()),
            xml::el("w:lvlText", &[("w:val", text)], Vec::new()),
            xml::el("w:lvlJc", &[("w:val", "left")], Vec::new()),
            xml::el(
                "w:pPr",
                &[],
                vec![xml::el(
                    "w:ind",
                    &[("w:left", &left), ("w:hanging", "360")],
                    Vec::new(),
                )],
            ),
        ],
    )
}

fn abstract_bullet(id: &str) -> Value {
    let marks = ["\u{2022}", "\u{25e6}", "\u{25aa}"];
    xml::el(
        "w:abstractNum",
        &[("w:abstractNumId", id)],
        (0..9)
            .map(|l| level_xml(l, "bullet", marks[l % 3]))
            .collect(),
    )
}

fn abstract_decimal(id: &str) -> Value {
    let formats = ["decimal", "lowerLetter", "lowerRoman"];
    xml::el(
        "w:abstractNum",
        &[("w:abstractNumId", id)],
        (0..9)
            .map(|l| level_xml(l, formats[l % 3], &format!("%{}.", l + 1)))
            .collect(),
    )
}

fn rels_of(package: &Package, part: &str) -> Vec<office_fmt::rels::Rel> {
    package
        .get(&office_fmt::rels::rels_part(part))
        .and_then(|b| office_fmt::rels::parse(b).ok())
        .unwrap_or_default()
}

fn add_rel(package: &mut Package, kind: &str, target: &str) -> String {
    let mut list = rels_of(package, DOCUMENT);
    let id = (1..=u32::MAX)
        .map(|n| format!("rId{n}"))
        .find(|id| list.iter().all(|r| r.id != *id))
        .unwrap_or_default();
    list.push(office_fmt::rels::Rel {
        id: id.clone(),
        kind: kind.into(),
        target: target.into(),
        external: false,
    });
    package.set(
        &office_fmt::rels::rels_part(DOCUMENT),
        office_fmt::rels::to_bytes(&list),
    );
    id
}

/// The XML of a header or footer: a centred paragraph for each line, with page-number fields where `{page}` / `{pages}` are.
fn hf_xml(root: &str, text: &str) -> Value {
    let mut paras = Vec::new();
    for line in text.split('\n') {
        let mut kids = vec![xml::el(
            "w:pPr",
            &[],
            vec![xml::el("w:jc", &[("w:val", "center")], Vec::new())],
        )];
        let mut rest = line;
        while !rest.is_empty() {
            let next = ["{page}", "{pages}"]
                .iter()
                .filter_map(|t| rest.find(t).map(|i| (i, *t)))
                .min();
            let (before, token) = match next {
                Some((i, t)) => (&rest[..i], Some(t)),
                None => (rest, None),
            };
            if !before.is_empty() {
                kids.push(xml::el(
                    "w:r",
                    &[],
                    vec![xml::el(
                        "w:t",
                        &[("xml:space", "preserve")],
                        vec![xml::txt(before)],
                    )],
                ));
            }
            if let Some(t) = token {
                let instr = if t == "{page}" {
                    " PAGE "
                } else {
                    " NUMPAGES "
                };
                kids.push(xml::el(
                    "w:fldSimple",
                    &[("w:instr", instr)],
                    vec![xml::el(
                        "w:r",
                        &[],
                        vec![xml::el("w:t", &[], vec![xml::txt("1")])],
                    )],
                ));
                rest = &rest[before.len() + t.len()..];
            } else {
                rest = "";
            }
        }
        paras.push(xml::el("w:p", &[], kids));
    }
    xml::el(root, &[("xmlns:w", MAIN_NS), ("xmlns:r", REL_NS)], paras)
}

/// The section properties for the document: the file's own (or a new A4 one), with the paper and the header and footer set.
fn section_of(
    existing: Option<Value>,
    doc: &Doc,
    package: &mut Package,
    ct: &mut ContentTypes,
) -> Value {
    let mut section = existing.unwrap_or_else(|| {
        xml::el(
            "w:sectPr",
            &[],
            vec![
                xml::el("w:pgSz", &[("w:w", "11906"), ("w:h", "16838")], Vec::new()),
                xml::el(
                    "w:pgMar",
                    &[
                        ("w:top", "1440"),
                        ("w:right", "1440"),
                        ("w:bottom", "1440"),
                        ("w:left", "1440"),
                        ("w:header", "720"),
                        ("w:footer", "720"),
                        ("w:gutter", "0"),
                    ],
                    Vec::new(),
                ),
            ],
        )
    });
    if let Some(size) = xml::child_mut(&mut section, "w:pgSz") {
        let (w, h) = doc.paper().twips();
        xml::set_attr(size, "w:w", &w.to_string());
        xml::set_attr(size, "w:h", &h.to_string());
    }
    if let Some(kids) = xml::children_mut(&mut section) {
        kids.retain(|k| {
            !(matches!(xml::tag(k), Some("w:headerReference" | "w:footerReference"))
                && matches!(xml::attr(k, "w:type"), None | Some("default")))
        });
    }
    let mut refs = Vec::new();
    for (tag, text, part, root, kind, content) in [
        (
            "w:headerReference",
            doc.header(),
            "word/header1.xml",
            "w:hdr",
            "header",
            CT_HEADER,
        ),
        (
            "w:footerReference",
            doc.footer(),
            "word/footer1.xml",
            "w:ftr",
            "footer",
            CT_FOOTER,
        ),
    ] {
        if text.is_empty() {
            continue;
        }
        package.set(part, xml::write(&hf_xml(root, text)));
        ct.set_override(part, content);
        let rel =
            format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}");
        let id = add_rel(package, &rel, part.trim_start_matches("word/"));
        refs.push(xml::el(
            tag,
            &[("w:type", "default"), ("r:id", &id)],
            Vec::new(),
        ));
    }
    if let Some(kids) = xml::children_mut(&mut section) {
        for (i, r) in refs.into_iter().enumerate() {
            kids.insert(i, r);
        }
    }
    section
}

/// The paragraphs as the body: the cells of a table together make one `w:tbl`.
fn body_of(items: &[(Option<docs_text::Cell>, Value)], doc: &Doc) -> Vec<Value> {
    let mut body = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let Some(first) = items[i].0 else {
            body.push(items[i].1.clone());
            i += 1;
            continue;
        };
        let mut j = i;
        while j < items.len() && items[j].0.is_some_and(|c| c.table == first.table) {
            j += 1;
        }
        body.push(table_xml(
            &items[i..j],
            &doc.table_widths(first.table),
            doc.table_header(first.table),
        ));
        // Two tables with nothing between them would be one in Word.
        if items.get(j).is_some_and(|n| n.0.is_some()) {
            body.push(xml::el("w:p", &[], Vec::new()));
        }
        i = j;
    }
    // A table cannot be the last thing in a body.
    if items.last().is_some_and(|l| l.0.is_some()) {
        body.push(xml::el("w:p", &[], Vec::new()));
    }
    body
}

fn table_xml(cells: &[(Option<docs_text::Cell>, Value)], widths: &[u16], header: bool) -> Value {
    let cols = cells
        .iter()
        .filter_map(|c| c.0)
        .map(|c| usize::from(c.col) + 1)
        .max()
        .unwrap_or(1);
    let rows = cells
        .iter()
        .filter_map(|c| c.0)
        .map(|c| usize::from(c.row) + 1)
        .max()
        .unwrap_or(1);
    let wide = |c: usize| {
        (9000
            * u32::from(
                widths
                    .get(c)
                    .copied()
                    .unwrap_or_else(|| 1000 / u16::try_from(cols).unwrap_or(1)),
            )
            / 1000)
            .to_string()
    };
    let border = |name: &str| {
        xml::el(
            name,
            &[
                ("w:val", "single"),
                ("w:sz", "4"),
                ("w:space", "0"),
                ("w:color", "808080"),
            ],
            Vec::new(),
        )
    };
    let mut kids = vec![
        xml::el(
            "w:tblPr",
            &[],
            vec![
                xml::el("w:tblW", &[("w:w", "0"), ("w:type", "auto")], Vec::new()),
                xml::el(
                    "w:tblBorders",
                    &[],
                    [
                        "w:top",
                        "w:left",
                        "w:bottom",
                        "w:right",
                        "w:insideH",
                        "w:insideV",
                    ]
                    .iter()
                    .map(|n| border(n))
                    .collect(),
                ),
            ],
        ),
        xml::el(
            "w:tblGrid",
            &[],
            (0..cols)
                .map(|c| xml::el("w:gridCol", &[("w:w", &wide(c))], Vec::new()))
                .collect(),
        ),
    ];
    for r in 0..rows {
        let mut row = Vec::new();
        for c in 0..cols {
            let cell = cells.iter().find(|(k, _)| {
                k.is_some_and(|k| usize::from(k.row) == r && usize::from(k.col) == c)
            });
            let para = cell.map_or_else(|| xml::el("w:p", &[], Vec::new()), |(_, p)| p.clone());
            row.push(xml::el(
                "w:tc",
                &[],
                vec![
                    xml::el(
                        "w:tcPr",
                        &[],
                        vec![xml::el(
                            "w:tcW",
                            &[("w:w", &wide(c)), ("w:type", "dxa")],
                            Vec::new(),
                        )],
                    ),
                    para,
                ],
            ));
        }
        if header && r == 0 {
            row.insert(
                0,
                xml::el("w:trPr", &[], vec![xml::el("w:tblHeader", &[], Vec::new())]),
            );
        }
        kids.push(xml::el("w:tr", &[], row));
    }
    xml::el("w:tbl", &[], kids)
}

/// The pictures of the document as parts of the package (`word/media/lore<number>`), each with a relationship; the relationship
/// ids by picture number. Pictures an earlier write made are replaced.
fn add_pictures(package: &mut Package, ct: &mut ContentTypes, doc: &Doc) -> Vec<(u32, String)> {
    let kind = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
    let mut list: Vec<office_fmt::rels::Rel> = rels_of(package, DOCUMENT)
        .into_iter()
        .filter(|r| !(r.kind == kind && r.target.starts_with("media/lore")))
        .collect();
    let mut used: Vec<u32> = doc.paras().iter().filter_map(|p| p.picture).collect();
    used.sort_unstable();
    used.dedup();
    let mut out = Vec::new();
    for n in used {
        let Some(pic) = doc.picture(n) else { continue };
        let name = format!("media/lore{n}.{}", pic.extension());
        package.set(&format!("word/{name}"), pic.data.as_ref().clone());
        ct.set_default(pic.extension(), pic.mime);
        let id = (1..=u32::MAX)
            .map(|k| format!("rId{k}"))
            .find(|id| list.iter().all(|r| r.id != *id))
            .unwrap_or_default();
        list.push(office_fmt::rels::Rel {
            id: id.clone(),
            kind: kind.into(),
            target: name,
            external: false,
        });
        out.push((n, id));
    }
    package.set(
        &office_fmt::rels::rels_part(DOCUMENT),
        office_fmt::rels::to_bytes(&list),
    );
    out
}

/// One external relationship for each link the paragraphs use (the others from an earlier write are dropped); the relationship
/// ids by link number.
fn add_links(package: &mut Package, doc: &Doc) -> Vec<(u32, String)> {
    let mut used: Vec<u32> = doc
        .paras()
        .iter()
        .flat_map(|p| p.runs.iter().filter_map(|r| r.style.link))
        .collect();
    used.sort_unstable();
    used.dedup();
    let kind = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
    let mut list: Vec<office_fmt::rels::Rel> = rels_of(package, DOCUMENT)
        .into_iter()
        .filter(|r| !(r.external && r.kind == kind))
        .collect();
    let mut out = Vec::new();
    for l in used {
        let Some(url) = doc.url(l) else { continue };
        let id = (1..=u32::MAX)
            .map(|n| format!("rId{n}"))
            .find(|id| list.iter().all(|r| r.id != *id))
            .unwrap_or_default();
        list.push(office_fmt::rels::Rel {
            id: id.clone(),
            kind: kind.into(),
            target: url.into(),
            external: true,
        });
        out.push((l, id));
    }
    package.set(
        &office_fmt::rels::rels_part(DOCUMENT),
        office_fmt::rels::to_bytes(&list),
    );
    out
}

fn relate(package: &mut Package, from: &str, kind: &str, target: &str) {
    let mut list = rels_of(package, from);
    if list.iter().any(|r| r.kind == kind) {
        return;
    }
    let id = (1..=u32::MAX)
        .map(|n| format!("rId{n}"))
        .find(|id| list.iter().all(|r| r.id != *id))
        .unwrap_or_default();
    list.push(office_fmt::rels::Rel {
        id,
        kind: kind.into(),
        target: target.into(),
        external: false,
    });
    package.set(
        &office_fmt::rels::rels_part(from),
        office_fmt::rels::to_bytes(&list),
    );
}

/// Make sure the package has the heading and list styles the model uses, and a bullet list; returns its `numId`.
fn ensure_parts(package: &mut Package, ct: &mut ContentTypes) -> Result<(String, String), Error> {
    // Styles.
    if let Some(bytes) = package.get("word/styles.xml") {
        let mut root = xml::parse(bytes, false)?;
        let have: Vec<String> = xml::elements(&root, "w:style")
            .filter_map(|s| xml::attr(s, "w:styleId").map(str::to_string))
            .collect();
        if let Some(kids) = xml::children_mut(&mut root) {
            for s in wanted_styles() {
                if !have
                    .iter()
                    .any(|h| Some(h.as_str()) == xml::attr(&s, "w:styleId"))
                {
                    kids.push(s);
                }
            }
        }
        package.set("word/styles.xml", xml::write(&root));
    } else {
        package.set("word/styles.xml", xml::write(&styles_part()));
        ct.set_override("word/styles.xml", CT_STYLES);
        relate(package, DOCUMENT, REL_STYLES, "styles.xml");
    }
    // Numbering: reuse a bullet list and a decimal definition when there are some, else add ours.
    let mut root = match package.get("word/numbering.xml") {
        Some(b) => xml::parse(b, false)?,
        None => xml::el("w:numbering", &[("xmlns:w", MAIN_NS)], Vec::new()),
    };
    let format_of = |a: &Value| {
        xml::elements(a, "w:lvl")
            .next()
            .and_then(|l| xml::child(l, "w:numFmt"))
            .and_then(|f| xml::attr(f, "w:val"))
            .map(str::to_string)
    };
    let abstracts_with = |root: &Value, fmt: &str| -> Vec<String> {
        xml::elements(root, "w:abstractNum")
            .filter(|a| format_of(a).as_deref() == Some(fmt))
            .filter_map(|a| xml::attr(a, "w:abstractNumId").map(str::to_string))
            .collect()
    };
    let next_of = |root: &Value, name: &str, attr: &str| -> usize {
        xml::elements(root, name)
            .filter_map(|e| xml::attr(e, attr).and_then(|v| v.parse::<usize>().ok()))
            .max()
            .map_or(1, |m| m + 1)
    };
    let mut changed = false;
    let bullets = abstracts_with(&root, "bullet");
    let bullet_num = xml::elements(&root, "w:num").find_map(|n| {
        let abs = xml::child(n, "w:abstractNumId").and_then(|a| xml::attr(a, "w:val"))?;
        bullets
            .iter()
            .any(|b| b == abs)
            .then(|| xml::attr(n, "w:numId").map(str::to_string))
            .flatten()
    });
    let bullet_id = if let Some(id) = bullet_num {
        id
    } else {
        let (abs_id, num_id) = (
            next_of(&root, "w:abstractNum", "w:abstractNumId").to_string(),
            next_of(&root, "w:num", "w:numId").to_string(),
        );
        if let Some(kids) = xml::children_mut(&mut root) {
            let at = kids
                .iter()
                .position(|k| xml::tag(k) == Some("w:num"))
                .unwrap_or(kids.len());
            kids.insert(at, abstract_bullet(&abs_id));
            kids.push(xml::el(
                "w:num",
                &[("w:numId", &num_id)],
                vec![xml::el(
                    "w:abstractNumId",
                    &[("w:val", &abs_id)],
                    Vec::new(),
                )],
            ));
        }
        changed = true;
        num_id
    };
    let decimal = if let Some(id) = abstracts_with(&root, "decimal").into_iter().next() {
        id
    } else {
        let abs_id = next_of(&root, "w:abstractNum", "w:abstractNumId").to_string();
        if let Some(kids) = xml::children_mut(&mut root) {
            let at = kids
                .iter()
                .position(|k| xml::tag(k) == Some("w:num"))
                .unwrap_or(kids.len());
            kids.insert(at, abstract_decimal(&abs_id));
        }
        changed = true;
        abs_id
    };
    if changed {
        package.set("word/numbering.xml", xml::write(&root));
        ct.set_override("word/numbering.xml", CT_NUMBERING);
        relate(package, DOCUMENT, REL_NUMBERING, "numbering.xml");
    }
    Ok((bullet_id, decimal))
}

/// Add `count` lists that count from 1 (each run of numbered paragraphs is its own), returning their `numId`s.
fn add_numbered_lists(
    package: &mut Package,
    decimal: &str,
    count: usize,
) -> Result<Vec<String>, Error> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut root = xml::parse(package.get("word/numbering.xml").unwrap_or_default(), false)?;
    let mut next = xml::elements(&root, "w:num")
        .filter_map(|e| xml::attr(e, "w:numId").and_then(|v| v.parse::<usize>().ok()))
        .max()
        .map_or(1, |m| m + 1);
    let mut ids = Vec::new();
    for _ in 0..count {
        let id = next.to_string();
        next += 1;
        let restart = xml::el(
            "w:lvlOverride",
            &[("w:ilvl", "0")],
            vec![xml::el("w:startOverride", &[("w:val", "1")], Vec::new())],
        );
        if let Some(kids) = xml::children_mut(&mut root) {
            kids.push(xml::el(
                "w:num",
                &[("w:numId", &id)],
                vec![
                    xml::el("w:abstractNumId", &[("w:val", decimal)], Vec::new()),
                    restart,
                ],
            ));
        }
        ids.push(id);
    }
    package.set("word/numbering.xml", xml::write(&root));
    Ok(ids)
}

/// Put the model into a package (the one a file came in, or a new one) and write it.
///
/// # Errors
///
/// [`Error::Invalid`] when the package's own parts cannot be read or written.
pub fn write(doc: &Doc, package: &Package) -> Result<Vec<u8>, Error> {
    let mut package = package.clone();
    let mut ct = package
        .get("[Content_Types].xml")
        .map(ContentTypes::parse)
        .transpose()?
        .unwrap_or_default();
    let (bullet, decimal) = ensure_parts(&mut package, &mut ct)?;
    // Each run of numbered paragraphs is a list of its own, counting from 1.
    let mut runs = 0;
    let mut previous = false;
    for p in doc.paras() {
        let numbered = p.kind == Kind::Numbered;
        if numbered && !previous {
            runs += 1;
        }
        previous = numbered;
    }
    let ids = add_numbered_lists(&mut package, &decimal, runs)?;
    let link_ids = add_links(&mut package, doc);
    let pic_ids = add_pictures(&mut package, &mut ct, doc);
    let mut items: Vec<(Option<docs_text::Cell>, Value)> = Vec::new();
    let (mut current, mut previous): (usize, bool) = (0, false);
    for p in doc.paras() {
        let numbered = p.kind == Kind::Numbered;
        if numbered && !previous {
            current += 1;
        }
        previous = numbered;
        let list = match p.kind {
            Kind::Bullet => Some(bullet.as_str()),
            Kind::Numbered => ids.get(current - 1).map(String::as_str),
            _ => None,
        };
        items.push((p.cell, paragraph_xml(doc, p, list, &link_ids, &pic_ids)));
    }
    let paragraphs = body_of(&items, doc);
    // Keep the page setup (`w:sectPr`) and everything else around the body of the file that was read.
    let mut root = match package.get(DOCUMENT) {
        Some(b) => xml::parse(b, true)?,
        None => xml::el(
            "w:document",
            &[("xmlns:w", MAIN_NS), ("xmlns:r", REL_NS)],
            vec![xml::el("w:body", &[], Vec::new())],
        ),
    };
    if xml::child(&root, "w:body").is_none()
        && let Some(kids) = xml::children_mut(&mut root)
    {
        kids.push(xml::el("w:body", &[], Vec::new()));
    }
    if let Some(body) = xml::child_mut(&mut root, "w:body") {
        let section = xml::children(body)
            .iter()
            .find(|k| xml::tag(k) == Some("w:sectPr"))
            .cloned();
        let section = Some(section_of(section, doc, &mut package, &mut ct));
        if let Some(kids) = xml::children_mut(body) {
            *kids = paragraphs;
            kids.extend(section);
        }
    }
    package.set(DOCUMENT, xml::write(&root));
    ct.set_override(DOCUMENT, CT_DOCUMENT);
    ct.set_default(
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    );
    ct.set_default("xml", "application/xml");
    package.set("[Content_Types].xml", ct.to_bytes());
    if package.get("_rels/.rels").is_none() {
        let rels = vec![office_fmt::rels::Rel {
            id: "rId1".into(),
            kind: REL_DOCUMENT.into(),
            target: DOCUMENT.into(),
            external: false,
        }];
        package.set("_rels/.rels", office_fmt::rels::to_bytes(&rels));
    }
    Ok(package.write()?)
}

/// Write a document that did not come from a file.
///
/// # Errors
///
/// As [`write`].
pub fn write_new(doc: &Doc) -> Result<Vec<u8>, Error> {
    write(doc, &Package::new())
}
