//! Notes on cells: `xl/commentsN.xml` (the text) and a VML drawing (the boxes a spreadsheet shows them in), both reached
//! from the sheet's relationships.

use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use office_fmt::{ContentTypes, Package};
use sheets_model::{CellRef, Sheet};
use std::fmt::Write as _;

const KIND_COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const KIND_VML: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
const CT_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";

/// All the text under an element: the `<t>` of every run.
fn texts(e: &Value, out: &mut String) {
    if xml::tag(e) == Some("t") {
        out.push_str(&xml::text(e));
        return;
    }
    for k in xml::children(e) {
        if k.is_object() {
            texts(k, out);
        }
    }
}

/// Read the sheet's notes into the model.
pub fn read(package: &Package, sheet_part: &str, sheet: &mut Sheet) {
    let Some(list) = package
        .get(&rels::rels_part(sheet_part))
        .and_then(|b| rels::parse(b).ok())
    else {
        return;
    };
    for rel in list
        .iter()
        .filter(|r| r.kind == KIND_COMMENTS && !r.external)
    {
        let part = rels::resolve(sheet_part, &rel.target);
        let Some(root) = package.get(&part).and_then(|b| xml::parse(b, true).ok()) else {
            continue;
        };
        let Some(comments) = xml::child(&root, "commentList") else {
            continue;
        };
        for c in xml::elements(comments, "comment") {
            let Some(at) = xml::attr(c, "ref").and_then(CellRef::parse) else {
                continue;
            };
            let mut text = String::new();
            if let Some(t) = xml::child(c, "text") {
                texts(t, &mut text);
            }
            sheet.set_comment(at, text.trim());
        }
    }
}

fn next_id(list: &[Rel]) -> String {
    (1..=u32::MAX)
        .map(|n| format!("rId{n}"))
        .find(|id| list.iter().all(|r| r.id != *id))
        .unwrap_or_default()
}

fn fresh(pkg: &Package, pattern: &dyn Fn(u32) -> String) -> String {
    (1..=u32::MAX)
        .map(pattern)
        .find(|p| pkg.get(p).is_none())
        .unwrap_or_default()
}

/// Write the sheet's notes (replacing the comment parts it had), and point the sheet at them.
pub fn write(
    pkg: &mut Package,
    ct: &mut ContentTypes,
    sheet_part: &str,
    tree: &mut Value,
    sheet: &Sheet,
) {
    let rels_name = rels::rels_part(sheet_part);
    let mut list = pkg
        .get(&rels_name)
        .and_then(|b| rels::parse(b).ok())
        .unwrap_or_default();
    if list.is_empty() && sheet.comments().next().is_none() {
        return;
    }
    // The parts this sheet had for notes go; new ones take their place.
    let old: Vec<Rel> = list
        .iter()
        .filter(|r| (r.kind == KIND_COMMENTS || r.kind == KIND_VML) && !r.external)
        .cloned()
        .collect();
    for r in &old {
        let part = rels::resolve(sheet_part, &r.target);
        pkg.remove(&part);
        ct.remove_override(&part);
    }
    list.retain(|r| r.kind != KIND_COMMENTS && r.kind != KIND_VML);
    let notes: Vec<(CellRef, &str)> = sheet.comments().collect();
    if notes.is_empty() {
        crate::extras::put(tree, "legacyDrawing", Vec::new());
    } else {
        let comments_part = fresh(pkg, &|n| format!("xl/comments{n}.xml"));
        let vml_part = fresh(pkg, &|n| format!("xl/drawings/vmlDrawing{n}.vml"));
        let items: Vec<Value> = notes
            .iter()
            .map(|(at, text)| {
                xml::el(
                    "comment",
                    &[("ref", &at.to_string()), ("authorId", "0")],
                    vec![xml::el(
                        "text",
                        &[],
                        vec![xml::el(
                            "t",
                            &[("xml:space", "preserve")],
                            vec![xml::txt(text)],
                        )],
                    )],
                )
            })
            .collect();
        let root = xml::el(
            "comments",
            &[("xmlns", crate::styles::MAIN_NS)],
            vec![
                xml::el(
                    "authors",
                    &[],
                    vec![xml::el("author", &[], vec![xml::txt("lore-sheets")])],
                ),
                xml::el("commentList", &[], items),
            ],
        );
        pkg.set(&comments_part, xml::write(&root));
        ct.set_override(&comments_part, CT_COMMENTS);
        pkg.set(&vml_part, vml(&notes).into_bytes());
        ct.set_default("vml", CT_VML);
        let base = sheet_part.rsplit_once('/').map_or("", |(d, _)| d);
        let rel_to = |part: &str| -> String {
            // A path from the sheet's folder: up out of it and down to the part.
            let ups = base.split('/').filter(|s| !s.is_empty()).count();
            format!("{}{}", "../".repeat(ups), part)
        };
        let cid = next_id(&list);
        list.push(Rel {
            id: cid,
            kind: KIND_COMMENTS.into(),
            target: rel_to(&comments_part),
            external: false,
        });
        let vid = next_id(&list);
        list.push(Rel {
            id: vid.clone(),
            kind: KIND_VML.into(),
            target: rel_to(&vml_part),
            external: false,
        });
        if xml::attr(tree, "xmlns:r").is_none() {
            xml::set_attr(
                tree,
                "xmlns:r",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            );
        }
        crate::extras::put(
            tree,
            "legacyDrawing",
            vec![xml::el("legacyDrawing", &[("r:id", &vid)], Vec::new())],
        );
    }
    if list.is_empty() {
        pkg.remove(&rels_name);
    } else {
        pkg.set(&rels_name, rels::to_bytes(&list));
    }
}

/// The drawing that gives each note a box (hidden until the cell is pointed at).
fn vml(notes: &[(CellRef, &str)]) -> String {
    let mut out = String::from(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\n\
<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>\n\
<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>\n",
    );
    for (i, (at, _)) in notes.iter().enumerate() {
        let (row, col) = (at.row, at.col);
        let _ = writeln!(
            out,
            "<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:80pt;margin-top:5pt;width:140pt;height:60pt;z-index:{};visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 2, {}, 15, {}, 16</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column></x:ClientData></v:shape>",
            1025 + i,
            i + 1,
            col + 1,
            row.saturating_sub(1),
            col + 3,
            row + 3,
        );
    }
    out.push_str("</xml>");
    out
}
