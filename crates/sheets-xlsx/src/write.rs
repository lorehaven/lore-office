use crate::strings::Table;
use crate::{Error, Xlsx, formula, styles};
use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use office_fmt::{ContentTypes, Package};
use sheets_calc::Engine;
use sheets_model::{CellRef, Content, Sheet, Value as Cell};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What an empty `<sheetData>` is written as.
const PLACEHOLDER: &[u8] = b"<sheetData/>";
const MAIN_NS: &str = styles::MAIN_NS;
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_WORKBOOK: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const CT_SHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const CT_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";
const CALC_CHAIN: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";

fn parse(pkg: &Package, part: &str, keep_ws: bool) -> Result<Option<Value>, Error> {
    pkg.get(part)
        .map(|b| xml::parse(b, keep_ws).map_err(Error::from))
        .transpose()
}

/// The target of a relationship from `base` to `part`: relative when it is below `base`'s folder.
fn relative_to(base: &str, part: &str) -> String {
    let dir = base.rsplit_once('/').map_or("", |(d, _)| d);
    part.strip_prefix(&format!("{dir}/"))
        .filter(|_| !dir.is_empty())
        .map_or_else(|| format!("/{part}"), str::to_string)
}

fn next_rid(list: &[Rel]) -> String {
    let n = list
        .iter()
        .filter_map(|r| r.id.strip_prefix("rId")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("rId{}", n + 1)
}

fn check_names(engine: &Engine) -> Result<(), Error> {
    for s in &engine.workbook().sheets {
        let n = &s.name;
        if n.is_empty()
            || n.chars().count() > 31
            || n.contains(['[', ']', ':', '*', '?', '/', '\\'])
            || n.starts_with('\'')
            || n.ends_with('\'')
        {
            return Err(Error::Invalid(format!(
                "sheet name {n:?} is not allowed in a workbook"
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub fn write(x: &Xlsx) -> Result<Vec<u8>, Error> {
    check_names(&x.engine)?;
    let wb = x.engine.workbook();
    let mut pkg = x.package.clone();
    let mut ct = parse_ct(&pkg)?;
    ct.set_default("rels", CT_RELS);
    ct.set_default("xml", "application/xml");

    let wb_part = x.workbook_part.clone();
    let mut wbx = parse(&pkg, &wb_part, false)?.unwrap_or_else(|| {
        xml::el(
            "workbook",
            &[("xmlns", MAIN_NS), ("xmlns:r", REL_NS)],
            vec![xml::el("sheets", &[], Vec::new())],
        )
    });
    let mut wb_rels = pkg
        .get(&rels::rels_part(&wb_part))
        .map(rels::parse)
        .transpose()?
        .unwrap_or_default();
    let mut root_rels = pkg
        .get("_rels/.rels")
        .map(rels::parse)
        .transpose()?
        .unwrap_or_default();
    if !root_rels.iter().any(|r| r.kind == rels::OFFICE_DOCUMENT) {
        let id = next_rid(&root_rels);
        root_rels.push(Rel {
            id,
            kind: rels::OFFICE_DOCUMENT.into(),
            target: wb_part.clone(),
            external: false,
        });
    }
    ct.set_override(&wb_part, CT_WORKBOOK);

    // Styles: the file's, with the workbook's new ones appended.
    let styles_part = x
        .styles_part
        .clone()
        .unwrap_or_else(|| "xl/styles.xml".into());
    let mut table_ids = 0_u32;
    let mut st = parse(&pkg, &styles_part, false)?
        .filter(|_| x.styles_part.is_some())
        .unwrap_or_else(styles::new_root);
    styles::extend(
        &mut st,
        &wb.styles,
        if x.styles_part.is_some() {
            x.original_styles
        } else {
            1
        },
    );
    ct.set_override(&styles_part, CT_STYLES);
    ensure_rel(&mut wb_rels, rels::STYLES, &styles_part, &wb_part);

    // Sheets.
    let mut table = Table::default();
    let mut assigned: Vec<String> = Vec::new();
    for i in 0..wb.sheets.len() {
        let part = x
            .sheet_parts
            .get(i)
            .cloned()
            .flatten()
            .unwrap_or_else(|| fresh_part(&pkg, &assigned));
        assigned.push(part.clone());
        // The sheet's own XML minus its cells (a small tree), and the attributes its rows had.
        let (mut tree, old_rows) = match pkg.get(&part) {
            Some(bytes) => {
                let (tree, data) = crate::read::split_sheet(bytes)?;
                (
                    tree,
                    data.map(crate::scan::row_attributes).unwrap_or_default(),
                )
            }
            None => (
                xml::el(
                    "worksheet",
                    &[("xmlns", MAIN_NS), ("xmlns:r", REL_NS)],
                    vec![xml::el("sheetData", &[], Vec::new())],
                ),
                HashMap::new(),
            ),
        };
        replace_sheet_data(&mut tree, xml::el("sheetData", &[], Vec::new()));
        set_dimension(&mut tree, &x.engine, i);
        update_columns(&mut tree, &wb.sheets[i]);
        crate::extras::write(&mut tree, &wb.sheets[i], &mut st);
        crate::comments::write(&mut pkg, &mut ct, &part, &mut tree, &wb.sheets[i]);
        crate::charts::write(&mut pkg, &mut ct, &part, &mut tree, &x.engine, i);
        let heading =
            |row: u32, col: u32| match x.engine.value(i, sheets_model::CellRef::new(row, col)) {
                sheets_model::Value::Empty => format!("Column{}", col + 1),
                v => v.to_string(),
            };
        crate::tables::write(
            &mut pkg,
            &mut ct,
            &part,
            &mut tree,
            &wb.sheets[i],
            &mut table_ids,
            &mut st,
            &heading,
        );
        // The cells are written as text straight into the place the empty `<sheetData/>` holds: a tree of a million cells
        // costs more to build and print than everything else in the file.
        let mut bytes = xml::write(&tree);
        let cells = sheet_data_xml(&x.engine, i, &mut table, &old_rows);
        if let Some(at) = bytes
            .windows(PLACEHOLDER.len())
            .position(|w| w == PLACEHOLDER)
        {
            bytes.splice(at..at + PLACEHOLDER.len(), cells.into_bytes());
        }
        pkg.set(&part, bytes);
        ct.set_override(&part, CT_SHEET);
    }

    // The conditional formats may have added differential formats.
    pkg.set(&styles_part, xml::write(&st));

    // Worksheets that were in the file and are no longer in the workbook go away with their relationships.
    let wb_rels_before = wb_rels.clone();
    for r in wb_rels_before
        .iter()
        .filter(|r| r.kind.ends_with("/worksheet") && !r.external)
    {
        let part = rels::resolve(&wb_part, &r.target);
        if !assigned.contains(&part) {
            wb_rels.retain(|o| o.id != r.id);
            pkg.remove(&part);
            pkg.remove(&rels::rels_part(&part));
            ct.remove_override(&part);
        }
    }
    update_sheet_list(
        &mut wbx,
        &mut wb_rels,
        &wb_part,
        &wb.sheets
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        &assigned,
    );

    // Shared strings.
    let sst_part = wb_rels
        .iter()
        .find(|r| r.kind == rels::SHARED_STRINGS && !r.external)
        .map_or_else(
            || "xl/sharedStrings.xml".to_string(),
            |r| rels::resolve(&wb_part, &r.target),
        );
    pkg.set(&sst_part, xml::write(&table.to_xml(&x.original_strings)));
    ct.set_override(&sst_part, CT_STRINGS);
    ensure_rel(&mut wb_rels, rels::SHARED_STRINGS, &sst_part, &wb_part);

    // The calculation chain lists formula cells; it would be wrong now, and Excel rebuilds it.
    let chains: Vec<String> = wb_rels
        .iter()
        .filter(|r| r.kind == CALC_CHAIN)
        .map(|r| rels::resolve(&wb_part, &r.target))
        .collect();
    for part in chains {
        pkg.remove(&part);
        ct.remove_override(&part);
    }
    wb_rels.retain(|r| r.kind != CALC_CHAIN);

    // The names the person defined (the others, tied to a sheet or built in, stay as they were).
    {
        let kept: Vec<Value> = xml::child(&wbx, "definedNames")
            .map(|l| {
                xml::elements(l, "definedName")
                    .filter(|n| {
                        let name = xml::attr(n, "name").unwrap_or_default();
                        // The suite writes print areas and titles itself.
                        !matches!(name, "_xlnm.Print_Area" | "_xlnm.Print_Titles")
                            && (xml::attr(n, "localSheetId").is_some()
                                || matches!(xml::attr(n, "hidden"), Some("1" | "true"))
                                || name.starts_with("_xlnm."))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut all: Vec<Value> = wb
            .names
            .iter()
            .map(|(n, r)| {
                xml::el(
                    "definedName",
                    &[("name", n)],
                    vec![xml::txt(&formula::to_file(r))],
                )
            })
            .collect();
        for (i, sheet) in wb.sheets.iter().enumerate() {
            let quoted = format!("'{}'", sheet.name.replace('\'', "''"));
            let id = i.to_string();
            if let Some(area) = sheet.page().print_area {
                let text = format!(
                    "{quoted}!${}${}:${}${}",
                    sheets_model::col_name(area.start.col),
                    area.start.row + 1,
                    sheets_model::col_name(area.end.col),
                    area.end.row + 1
                );
                all.push(xml::el(
                    "definedName",
                    &[("name", "_xlnm.Print_Area"), ("localSheetId", &id)],
                    vec![xml::txt(&text)],
                ));
            }
            if let Some((a, b)) = sheet.page().repeat_rows {
                let text = format!("{quoted}!${}:${}", a + 1, b + 1);
                all.push(xml::el(
                    "definedName",
                    &[("name", "_xlnm.Print_Titles"), ("localSheetId", &id)],
                    vec![xml::txt(&text)],
                ));
            }
        }
        all.extend(kept);
        if let Some(kids) = xml::children_mut(&mut wbx) {
            kids.retain(|k| xml::tag(k) != Some("definedNames"));
            if !all.is_empty() {
                let order = [
                    "fileVersion",
                    "fileSharing",
                    "workbookPr",
                    "workbookProtection",
                    "bookViews",
                    "sheets",
                    "functionGroups",
                    "externalReferences",
                ];
                let at = kids
                    .iter()
                    .rposition(|e| xml::tag(e).is_some_and(|t| order.contains(&t)))
                    .map_or(0, |i| i + 1);
                kids.insert(at, xml::el("definedNames", &[], all));
            }
        }
    }

    // Cached results are ours; ask for everything to be recalculated when the file is opened.
    set_calc(&mut wbx);
    if let Some(p) = xml::child_mut(&mut wbx, "workbookPr")
        && xml::attr(p, "date1904").is_some()
    {
        xml::set_attr(p, "date1904", "0");
    }

    pkg.set(&wb_part, xml::write(&wbx));
    pkg.set(&rels::rels_part(&wb_part), rels::to_bytes(&wb_rels));
    pkg.set("_rels/.rels", rels::to_bytes(&root_rels));
    pkg.set("[Content_Types].xml", ct.to_bytes());
    Ok(pkg.write()?)
}

fn parse_ct(pkg: &Package) -> Result<ContentTypes, Error> {
    Ok(pkg
        .get("[Content_Types].xml")
        .map(ContentTypes::parse)
        .transpose()?
        .unwrap_or_default())
}

fn fresh_part(pkg: &Package, taken: &[String]) -> String {
    (1..=u32::MAX)
        .map(|n| format!("xl/worksheets/sheet{n}.xml"))
        .find(|p| pkg.get(p).is_none() && !taken.contains(p))
        .unwrap_or_default()
}

fn ensure_rel(list: &mut Vec<Rel>, kind: &str, part: &str, base: &str) {
    let target = relative_to(base, part);
    if let Some(r) = list.iter_mut().find(|r| r.kind == kind && !r.external) {
        r.target = target;
    } else {
        let id = next_rid(list);
        list.push(Rel {
            id,
            kind: kind.into(),
            target,
            external: false,
        });
    }
}

/// `<sheets>` in the order of the workbook: the entries the file had (by their part) keep their attributes, new sheets
/// get new ones, and entries that are not worksheets (chart sheets) stay at the end.
fn update_sheet_list(
    wbx: &mut Value,
    rels_list: &mut Vec<Rel>,
    wb_part: &str,
    names: &[&str],
    parts: &[String],
) {
    let prefix = rel_prefix(wbx);
    if xml::child(wbx, "sheets").is_none()
        && let Some(k) = xml::children_mut(wbx)
    {
        k.insert(0, xml::el("sheets", &[], Vec::new()));
    }
    let Some(sheets) = xml::child_mut(wbx, "sheets") else {
        return;
    };
    let old: Vec<Value> = xml::elements(sheets, "sheet").cloned().collect();
    let mut by_part: HashMap<String, Value> = HashMap::new();
    let mut others = Vec::new();
    let mut max_id = 0u32;
    for e in old {
        max_id = max_id.max(
            xml::attr(&e, "sheetId")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        );
        match xml::attr_local(&e, "id").and_then(|id| rels_list.iter().find(|r| r.id == id)) {
            Some(r) if r.kind.ends_with("/worksheet") => {
                by_part.insert(rels::resolve(wb_part, &r.target), e);
            }
            Some(_) => others.push(e),
            None => {}
        }
    }
    let mut out = Vec::new();
    for (name, part) in names.iter().zip(parts) {
        if let Some(mut e) = by_part.remove(part) {
            xml::set_attr(&mut e, "name", name);
            out.push(e);
            continue;
        }
        max_id += 1;
        let id = next_rid(rels_list);
        rels_list.push(Rel {
            id: id.clone(),
            kind: rels::WORKSHEET.into(),
            target: relative_to(wb_part, part),
            external: false,
        });
        out.push(xml::el(
            "sheet",
            &[
                ("name", name),
                ("sheetId", &max_id.to_string()),
                (&format!("{prefix}:id"), &id),
            ],
            Vec::new(),
        ));
    }
    out.extend(others);
    if let Some(k) = xml::children_mut(sheets) {
        *k = out;
    }
}

/// The prefix the workbook uses for the relationships namespace (`r` almost always), declaring it when missing.
fn rel_prefix(root: &mut Value) -> String {
    let found = xml::attrs(root)
        .into_iter()
        .find(|(k, v)| k.starts_with("xmlns:") && *v == REL_NS)
        .map(|(k, _)| k["xmlns:".len()..].to_string());
    found.unwrap_or_else(|| {
        xml::set_attr(root, "xmlns:r", REL_NS);
        "r".to_string()
    })
}

fn set_calc(wbx: &mut Value) {
    if let Some(c) = xml::child_mut(wbx, "calcPr") {
        xml::set_attr(c, "fullCalcOnLoad", "1");
        return;
    }
    let calc = xml::el("calcPr", &[("fullCalcOnLoad", "1")], Vec::new());
    // After the elements that come before it in the schema.
    let order = [
        "fileVersion",
        "fileSharing",
        "workbookPr",
        "workbookProtection",
        "bookViews",
        "sheets",
        "functionGroups",
        "externalReferences",
        "definedNames",
    ];
    if let Some(k) = xml::children_mut(wbx) {
        let at = k
            .iter()
            .rposition(|e| xml::tag(e).is_some_and(|t| order.contains(&t)))
            .map_or(0, |i| i + 1);
        k.insert(at, calc);
    }
}

fn replace_sheet_data(tree: &mut Value, data: Value) {
    let Some(kids) = xml::children_mut(tree) else {
        return;
    };
    if let Some(i) = kids.iter().position(|k| xml::tag(k) == Some("sheetData")) {
        kids[i] = data;
        return;
    }
    let before = [
        "sheetPr",
        "dimension",
        "sheetViews",
        "sheetFormatPr",
        "cols",
    ];
    let at = kids
        .iter()
        .rposition(|k| xml::tag(k).is_some_and(|t| before.contains(&t)))
        .map_or(0, |i| i + 1);
    kids.insert(at, data);
}

fn set_dimension(tree: &mut Value, engine: &Engine, sheet: usize) {
    let used = engine
        .workbook()
        .sheets
        .get(sheet)
        .and_then(sheets_model::Sheet::used_range);
    let text = used.map_or_else(|| "A1".to_string(), |r| r.to_string());
    if let Some(d) = xml::child_mut(tree, "dimension") {
        xml::set_attr(d, "ref", &text);
    } else if let Some(k) = xml::children_mut(tree) {
        let at = usize::from(k.first().is_some_and(|e| xml::tag(e) == Some("sheetPr")));
        k.insert(at, xml::el("dimension", &[("ref", &text)], Vec::new()));
    }
}

fn escape(text: &str, quotes: bool, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quotes => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// `<row ...>` (not closed) with its attributes: the number, what the file had, the height the sheet has now.
fn row_open(
    r: u32,
    old: Option<&Vec<(String, String)>>,
    height: Option<f64>,
    hidden: bool,
    outline: (u8, bool),
    style: u32,
    out: &mut String,
) {
    use std::fmt::Write;
    let _ = write!(out, "<row r=\"{r}\"");
    let mut kept: Vec<(&str, String)> = old
        .map(|o| o.iter().map(|(k, v)| (k.as_str(), v.clone())).collect())
        .unwrap_or_default();
    if let Some(px) = height {
        kept.retain(|(k, _)| *k != "ht" && *k != "customHeight");
        kept.push(("ht", (px / crate::read::POINTS_TO_PIXELS).to_string()));
        kept.push(("customHeight", "1".to_string()));
    }
    kept.retain(|(k, _)| {
        !matches!(
            *k,
            "hidden" | "outlineLevel" | "collapsed" | "s" | "customFormat"
        )
    });
    if style > 0 {
        kept.push(("s", style.to_string()));
        kept.push(("customFormat", "1".to_string()));
    }
    if hidden {
        kept.push(("hidden", "1".to_string()));
    }
    if outline.0 > 0 {
        kept.push(("outlineLevel", outline.0.to_string()));
    }
    if outline.1 {
        kept.push(("collapsed", "1".to_string()));
    }
    for (k, v) in kept {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape(&v, true, out);
        out.push('"');
    }
}

/// The `<sheetData>` of a sheet as text.
fn sheet_data_xml(
    engine: &Engine,
    si: usize,
    table: &mut Table,
    old_rows: &HashMap<u32, Vec<(String, String)>>,
) -> String {
    let sheet = &engine.workbook().sheets[si];
    let mut cells: BTreeMap<CellRef, (Option<&Content>, u32)> = BTreeMap::new();
    for (at, c) in sheet.iter() {
        cells.insert(at, (Some(c), sheet.style(at)));
    }
    for (at, s) in sheet.styled() {
        cells.entry(at).or_insert((None, s));
    }
    // Cells a formula spread its result over are written as the values they show.
    for at in engine.spilled_cells(si) {
        cells.entry(at).or_insert_with(|| (None, sheet.style(at)));
    }
    let heights: BTreeMap<u32, f64> = sheet.row_heights().map(|(r, h)| (r + 1, h)).collect();
    let hidden: BTreeSet<u32> = sheet.hidden_rows().map(|r| r + 1).collect();
    let row_styles: BTreeMap<u32, u32> = sheet.row_styles().map(|(r, s)| (r + 1, s)).collect();
    let levels: BTreeMap<u32, u8> = sheet.row_levels().map(|(r, l)| (r + 1, l)).collect();
    // The row under a group that is hidden is marked as the one that shows it again.
    let outline_of = |r: u32| -> (u8, bool) {
        let level = levels.get(&r).copied().unwrap_or(0);
        let above = levels.get(&r.saturating_sub(1)).copied().unwrap_or(0);
        (level, r > 1 && hidden.contains(&(r - 1)) && above > level)
    };
    let collapsed: Vec<u32> = hidden
        .iter()
        .filter(|r| {
            levels.get(*r).copied().unwrap_or(0) > levels.get(&(*r + 1)).copied().unwrap_or(0)
        })
        .map(|r| r + 1)
        .collect();
    // Rows that have no cells but still have something to say (a height, an attribute) come in their place.
    let cell_rows: BTreeSet<u32> = cells.keys().map(|c| c.row + 1).collect();
    let mut bare: Vec<u32> = old_rows
        .keys()
        .chain(heights.keys())
        .chain(hidden.iter())
        .chain(levels.keys())
        .chain(row_styles.keys())
        .chain(collapsed.iter())
        .copied()
        .filter(|r| !cell_rows.contains(r))
        .collect();
    bare.sort_unstable();
    bare.dedup();
    let mut bare = bare.into_iter().peekable();
    let mut out = String::with_capacity(cells.len() * 48 + 64);
    out.push_str("<sheetData>");
    let mut current: Option<u32> = None;
    for (at, (content, style)) in cells {
        let r = at.row + 1;
        if current != Some(r) {
            if current.is_some() {
                out.push_str("</row>");
            }
            while let Some(b) = bare.next_if(|b| *b < r) {
                row_open(
                    b,
                    old_rows.get(&b),
                    heights.get(&b).copied(),
                    hidden.contains(&b),
                    outline_of(b),
                    row_styles.get(&b).copied().unwrap_or(0),
                    &mut out,
                );
                out.push_str("/>");
            }
            row_open(
                r,
                old_rows.get(&r),
                heights.get(&r).copied(),
                hidden.contains(&r),
                outline_of(r),
                row_styles.get(&r).copied().unwrap_or(0),
                &mut out,
            );
            out.push('>');
            current = Some(r);
        }
        cell_xml(engine, si, at, content, style, table, &mut out);
    }
    if current.is_some() {
        out.push_str("</row>");
    }
    for b in bare {
        row_open(
            b,
            old_rows.get(&b),
            heights.get(&b).copied(),
            hidden.contains(&b),
            outline_of(b),
            row_styles.get(&b).copied().unwrap_or(0),
            &mut out,
        );
        out.push_str("/>");
    }
    out.push_str("</sheetData>");
    out
}

/// `<cols>` after the sheet's own widths: an original `<col>` is kept as it was (style and all) except around the columns
/// whose width is now different, which are split out; columns with a width and no `<col>` get one.
#[allow(clippy::too_many_lines)]
fn update_columns(tree: &mut Value, sheet: &Sheet) {
    use crate::read::CHARS_TO_PIXELS;
    let model: BTreeMap<u32, f64> = sheet.col_widths().map(|(c, w)| (c + 1, w)).collect();
    let hidden: BTreeSet<u32> = sheet.hidden_cols().map(|c| c + 1).collect();
    let levels: BTreeMap<u32, u8> = sheet.col_levels().map(|(c, l)| (c + 1, l)).collect();
    let col_styles: BTreeMap<u32, u32> = sheet.col_styles().map(|(c, s)| (c + 1, s)).collect();
    let old: Vec<Value> = xml::child(tree, "cols")
        .map(|c| xml::elements(c, "col").cloned().collect())
        .unwrap_or_default();
    // What the model says about one column.
    let of = |c: u32| {
        (
            model.get(&c).copied(),
            hidden.contains(&c),
            levels.get(&c).copied().unwrap_or(0),
            col_styles.get(&c).copied().unwrap_or(0),
        )
    };
    let mut out: Vec<(u32, u32, Value)> = Vec::new();
    for el in old {
        let num = |n: &str| xml::attr(&el, n).and_then(|v| v.parse::<u32>().ok());
        let (Some(min), Some(max)) = (num("min"), num("max")) else {
            continue;
        };
        let was = xml::attr(&el, "width")
            .and_then(|w| w.parse::<f64>().ok())
            .map(|w| w * CHARS_TO_PIXELS);
        let was_hidden = matches!(xml::attr(&el, "hidden"), Some("1" | "true"));
        let was_level: u8 = num("outlineLevel")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(0);
        let was_style = num("style").unwrap_or(0);
        // A range this wide is a sheet-wide default and is not tracked per column.
        let tracked = max - min < 1000;
        let mut changed: BTreeSet<u32> = model
            .range(min..=max)
            .filter(|(_, w)| was.is_none_or(|o| (**w - o).abs() > 0.5))
            .map(|(c, _)| *c)
            .collect();
        if tracked {
            changed.extend((min..=max).filter(|c| {
                let (_, h, l, st) = of(*c);
                h != was_hidden || l != was_level || st != was_style
            }));
        }
        // A piece of the original element over `from..=to`; with `Some` the column's own facts are written into it.
        let piece = |from: u32, to: u32, facts: Option<(Option<f64>, bool, u8, u32)>| {
            let mut e = el.clone();
            xml::set_attr(&mut e, "min", &from.to_string());
            xml::set_attr(&mut e, "max", &to.to_string());
            if let Some((width, hide, level, style)) = facts {
                if let Some(px) = width {
                    xml::set_attr(&mut e, "width", &(px / CHARS_TO_PIXELS).to_string());
                    xml::set_attr(&mut e, "customWidth", "1");
                }
                // Facts the model tracks are written only for ranges it reads (a sheet-wide default keeps its own).
                for (name, on, value) in [
                    ("hidden", hide, "1".to_string()),
                    ("outlineLevel", level > 0, level.to_string()),
                    ("style", style > 0, style.to_string()),
                ]
                .into_iter()
                .filter(|_| tracked)
                {
                    if on {
                        xml::set_attr(&mut e, name, &value);
                    } else {
                        xml::remove_attr(&mut e, name);
                    }
                }
            }
            (from, to, e)
        };
        let mut cursor = min;
        for c in changed {
            if c > cursor {
                out.push(piece(cursor, c - 1, None));
            }
            out.push(piece(c, c, Some(of(c))));
            cursor = c + 1;
        }
        if cursor <= max {
            out.push(piece(cursor, max, None));
        }
    }
    let wanted: BTreeSet<u32> = model
        .keys()
        .chain(hidden.iter())
        .chain(levels.keys())
        .chain(col_styles.keys())
        .copied()
        .collect();
    for c in &wanted {
        if !out.iter().any(|(a, b, _)| c >= a && c <= b) {
            let (width, hide, level, style) = of(*c);
            let width_text = width.map_or(9.140_625, |w| w / CHARS_TO_PIXELS).to_string();
            let (text, level_text, style_text) =
                (c.to_string(), level.to_string(), style.to_string());
            let mut attrs = vec![
                ("min", text.as_str()),
                ("max", text.as_str()),
                ("width", width_text.as_str()),
            ];
            if width.is_some() {
                attrs.push(("customWidth", "1"));
            }
            if hide {
                attrs.push(("hidden", "1"));
            }
            if level > 0 {
                attrs.push(("outlineLevel", level_text.as_str()));
            }
            if style > 0 {
                attrs.push(("style", style_text.as_str()));
            }
            out.push((*c, *c, xml::el("col", &attrs, Vec::new())));
        }
    }
    out.sort_by_key(|(a, _, _)| *a);
    let list: Vec<Value> = out.into_iter().map(|(_, _, e)| e).collect();
    crate::extras::put(
        tree,
        "cols",
        if list.is_empty() {
            Vec::new()
        } else {
            vec![xml::el("cols", &[], list)]
        },
    );
}

fn number(n: f64) -> String {
    if n == 0.0 {
        "0".to_string()
    } else {
        n.to_string()
    }
}

fn cell_xml(
    engine: &Engine,
    si: usize,
    at: CellRef,
    content: Option<&Content>,
    style: u32,
    table: &mut Table,
    out: &mut String,
) {
    use std::fmt::Write;
    let formula = matches!(content, Some(Content::Formula(_)));
    let value = if content.is_some() || engine.is_spilled(si, at) {
        engine.value(si, at)
    } else {
        Cell::Empty
    };
    // The value's type and text.
    let (kind, v): (Option<&str>, Option<String>) = match value {
        Cell::Number(n) => (None, Some(number(n))),
        Cell::Text(s) if formula => (Some("str"), Some(s)),
        Cell::Text(s) => (Some("s"), Some(table.add(&s).to_string())),
        Cell::Bool(b) => (Some("b"), Some(u8::from(b).to_string())),
        Cell::Error(sheets_model::CellError::Circular) => (None, Some("0".into())),
        Cell::Error(sheets_model::CellError::Spill) => (Some("e"), Some("#VALUE!".into())),
        Cell::Error(e) => (Some("e"), Some(e.as_str().to_string())),
        Cell::Empty => (None, formula.then(|| "0".to_string())),
    };
    let _ = write!(out, "<c r=\"{at}\"");
    if style > 0 {
        let _ = write!(out, " s=\"{style}\"");
    }
    if let Some(k) = kind {
        let _ = write!(out, " t=\"{k}\"");
    }
    if !formula && v.is_none() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    if let Some(Content::Formula(text)) = content {
        match engine.spill_range(si, at) {
            Some(range) => {
                let _ = write!(out, "<f t=\"array\" ref=\"{range}\">");
            }
            None => out.push_str("<f>"),
        }
        escape(&formula::to_file(text), false, out);
        out.push_str("</f>");
    }
    if let Some(v) = v {
        let keep =
            formula && (v.starts_with(char::is_whitespace) || v.ends_with(char::is_whitespace));
        out.push_str(if keep {
            "<v xml:space=\"preserve\">"
        } else {
            "<v>"
        });
        escape(&v, false, out);
        out.push_str("</v>");
    }
    out.push_str("</c>");
}
