//! Tables: `xl/tables/tableN.xml`, reached from the sheet's relationships and listed in its `<tableParts>`.

use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use office_fmt::{ContentTypes, Package};
use sheets_model::{Sheet, Table};

const KIND_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";

/// Read the sheet's tables into the model.
pub fn read(
    package: &Package,
    sheet_part: &str,
    sheet: &mut Sheet,
    custom: &std::collections::HashMap<String, String>,
) {
    let Some(list) = package
        .get(&rels::rels_part(sheet_part))
        .and_then(|b| rels::parse(b).ok())
    else {
        return;
    };
    let mut tables = Vec::new();
    for rel in list.iter().filter(|r| r.kind == KIND_TABLE && !r.external) {
        let part = rels::resolve(sheet_part, &rel.target);
        let Some(root) = package.get(&part).and_then(|b| xml::parse(b, true).ok()) else {
            continue;
        };
        let Some(range) = xml::attr(&root, "ref").and_then(crate::extras::parse_range) else {
            continue;
        };
        // The filter is the table's own `<autoFilter>` when it has one; a table without one has no arrows.
        let filter = xml::child(&root, "autoFilter")
            .and_then(crate::extras::read_filter)
            .unwrap_or_else(|| sheets_model::AutoFilter {
                range,
                allowed: std::collections::BTreeMap::default(),
            });
        let style = xml::child(&root, "tableStyleInfo")
            .map(|s| xml::attr(s, "name").unwrap_or("None").to_string())
            .unwrap_or_default();
        let style = custom.get(&style).cloned().unwrap_or(style);
        let banded = xml::child(&root, "tableStyleInfo")
            .is_some_and(|s| matches!(xml::attr(s, "showRowStripes"), Some("1" | "true")));
        let name = xml::attr(&root, "displayName")
            .or_else(|| xml::attr(&root, "name"))
            .unwrap_or("Table")
            .to_string();
        tables.push(Table {
            name,
            filter: sheets_model::AutoFilter { range, ..filter },
            banded,
            style,
        });
    }
    sheet.set_tables(tables);
}

fn next_id(list: &[Rel]) -> String {
    (1..=u32::MAX)
        .map(|n| format!("rId{n}"))
        .find(|id| list.iter().all(|r| r.id != *id))
        .unwrap_or_default()
}

/// Write the sheet's tables (replacing the table parts it had), and point the sheet at them. `ids` counts the tables of the
/// workbook so each gets a unique id.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn write(
    pkg: &mut Package,
    ct: &mut ContentTypes,
    sheet_part: &str,
    tree: &mut Value,
    sheet: &Sheet,
    ids: &mut u32,
    styles: &mut Value,
    heading: &dyn Fn(u32, u32) -> String,
) {
    let rels_name = rels::rels_part(sheet_part);
    let mut list = pkg
        .get(&rels_name)
        .and_then(|b| rels::parse(b).ok())
        .unwrap_or_default();
    if list.is_empty() && sheet.tables().is_empty() {
        return;
    }
    let old: Vec<Rel> = list
        .iter()
        .filter(|r| r.kind == KIND_TABLE && !r.external)
        .cloned()
        .collect();
    for r in &old {
        let part = rels::resolve(sheet_part, &r.target);
        pkg.remove(&part);
        ct.remove_override(&part);
    }
    list.retain(|r| r.kind != KIND_TABLE);
    let mut parts: Vec<Value> = Vec::new();
    for t in sheet.tables() {
        *ids += 1;
        let part = (1..=u32::MAX)
            .map(|n| format!("xl/tables/table{n}.xml"))
            .find(|p| pkg.get(p).is_none())
            .unwrap_or_default();
        let range = t.range();
        let columns: Vec<Value> = (range.start.col..=range.end.col)
            .enumerate()
            .map(|(i, c)| {
                xml::el(
                    "tableColumn",
                    &[
                        ("id", &(i + 1).to_string()),
                        ("name", &heading(range.start.row, c)),
                    ],
                    Vec::new(),
                )
            })
            .collect();
        let mut filter = crate::extras::filter_element(&t.filter);
        xml::set_attr(&mut filter, "ref", &range.to_string());
        let mut attrs: Vec<(&str, &str)> = Vec::new();
        crate::styles::ensure_table_style(styles, &t.style);
        match t.style.as_str() {
            "None" => {}
            "" => attrs.push(("name", "TableStyleLight1")),
            name => attrs.push(("name", name)),
        }
        attrs.extend([
            ("showFirstColumn", "0"),
            ("showLastColumn", "0"),
            ("showRowStripes", if t.banded { "1" } else { "0" }),
            ("showColumnStripes", "0"),
        ]);
        let style = xml::el("tableStyleInfo", &attrs, Vec::new());
        let root = xml::el(
            "table",
            &[
                ("xmlns", crate::styles::MAIN_NS),
                ("id", &ids.to_string()),
                ("name", &t.name),
                ("displayName", &t.name),
                ("ref", &range.to_string()),
                ("totalsRowShown", "0"),
            ],
            vec![
                filter,
                xml::el(
                    "tableColumns",
                    &[("count", &columns.len().to_string())],
                    columns,
                ),
                style,
            ],
        );
        pkg.set(&part, xml::write(&root));
        ct.set_override(&part, CT_TABLE);
        let base = sheet_part.rsplit_once('/').map_or("", |(d, _)| d);
        let ups = base.split('/').filter(|s| !s.is_empty()).count();
        let id = next_id(&list);
        list.push(Rel {
            id: id.clone(),
            kind: KIND_TABLE.into(),
            target: format!("{}{}", "../".repeat(ups), part),
            external: false,
        });
        parts.push(xml::el("tablePart", &[("r:id", &id)], Vec::new()));
    }
    if parts.is_empty() {
        crate::extras::put(tree, "tableParts", Vec::new());
    } else {
        if xml::attr(tree, "xmlns:r").is_none() {
            xml::set_attr(
                tree,
                "xmlns:r",
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            );
        }
        let count = parts.len().to_string();
        crate::extras::put(
            tree,
            "tableParts",
            vec![xml::el("tableParts", &[("count", &count)], parts)],
        );
    }
    if list.is_empty() {
        pkg.remove(&rels_name);
    } else {
        pkg.set(&rels_name, rels::to_bytes(&list));
    }
}
