use crate::scan;
use crate::strings::si_text;
use crate::{Error, Xlsx, formula, styles};
use office_fmt::Package;
use office_fmt::rels::{self, Rel};
use office_fmt::xml::{self, Value};
use sheets_calc::Engine;
use sheets_format::Format;
use sheets_formula::{Expr, parse, print};
use sheets_model::date::MAX_SERIAL;
use sheets_model::{CellError, CellRef, Content, Sheet, Value as Cell, Workbook};
use std::collections::{HashMap, HashSet};

/// A row's `ht` is in points; a pixel is 3/4 of one at 96 dpi.
pub const POINTS_TO_PIXELS: f64 = 4.0 / 3.0;
/// A column's `width` is in characters of the default font; a character is 7 pixels wide (padding is part of the width).
pub const CHARS_TO_PIXELS: f64 = 7.0;
/// A `<col>` over more columns than this is a sheet-wide default, not a width to keep per column.
const MAX_COLUMN_RANGE: u32 = 1000;
const MAX_CELLS: usize = 10_000_000;

/// The widths of columns that have their own (`<cols>`), in pixels.
fn read_columns(tree: &Value, sheet: &mut Sheet) {
    let Some(cols) = xml::child(tree, "cols") else {
        return;
    };
    for c in xml::elements(cols, "col") {
        let num = |n: &str| xml::attr(c, n).and_then(|v| v.parse::<u32>().ok());
        let (Some(min), Some(max)) = (num("min"), num("max")) else {
            continue;
        };
        let width = xml::attr(c, "width").and_then(|w| w.parse::<f64>().ok());
        let hidden = matches!(xml::attr(c, "hidden"), Some("1" | "true"));
        if min == 0 || max < min || max - min >= MAX_COLUMN_RANGE {
            continue;
        }
        for col in min..=max.min(sheets_model::MAX_COLS) {
            if let Some(w) = width.filter(|w| *w > 0.0) {
                sheet.set_col_width(col - 1, Some(w * CHARS_TO_PIXELS));
            }
            sheet.set_col_hidden(col - 1, hidden);
            sheet.set_col_style(
                col - 1,
                xml::attr(c, "style")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            );
            sheet.set_col_level(
                col - 1,
                xml::attr(c, "outlineLevel")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            );
        }
    }
}
const MAX_SHEETS: usize = 1000;
/// Days between the 1900 and the 1904 date systems.
const DATE1904_SHIFT: f64 = 1462.0;

/// The workbook-wide names a person defined: not the ones tied to one sheet, hidden ones, or the built-in `_xlnm.` ones.
fn defined_names(workbook: &Value) -> Vec<(String, String)> {
    let Some(list) = xml::child(workbook, "definedNames") else {
        return Vec::new();
    };
    xml::elements(list, "definedName")
        .filter(|n| {
            xml::attr(n, "localSheetId").is_none()
                && !matches!(xml::attr(n, "hidden"), Some("1" | "true"))
        })
        .filter_map(|n| {
            let name = xml::attr(n, "name")?;
            (!name.starts_with("_xlnm.")).then(|| {
                (
                    name.to_string(),
                    crate::formula::from_file(xml::text(n).trim_start_matches('=')),
                )
            })
        })
        .collect()
}

/// Print areas and repeated rows, which a file keeps as names tied to a sheet.
fn read_print_names(workbook: &Value, sheets: &mut [Sheet]) {
    let Some(list) = xml::child(workbook, "definedNames") else {
        return;
    };
    for n in xml::elements(list, "definedName") {
        let Some(sheet) = xml::attr(n, "localSheetId")
            .and_then(|v| v.parse::<usize>().ok())
            .and_then(|i| sheets.get_mut(i))
        else {
            continue;
        };
        let text = xml::text(n);
        let first = text.split(',').next().unwrap_or_default();
        let target = first
            .rsplit_once('!')
            .map_or(first, |(_, t)| t)
            .replace('$', "");
        let mut page = sheet.page().clone();
        match xml::attr(n, "name") {
            Some("_xlnm.Print_Area") => page.print_area = sheets_model::Range::parse(&target),
            Some("_xlnm.Print_Titles") => {
                // `$1:$2` repeats rows 1 to 2 (the part naming columns, `$A:$B`, is not used).
                let rows = text.split(',').find_map(|p| {
                    let t = p.rsplit_once('!').map_or(p, |(_, t)| t).replace('$', "");
                    let (a, b) = t.split_once(':')?;
                    Some((
                        a.parse::<u32>().ok()?.checked_sub(1)?,
                        b.parse::<u32>().ok()?.checked_sub(1)?,
                    ))
                });
                page.repeat_rows = rows;
            }
            _ => continue,
        }
        sheet.set_page(page);
    }
}

fn rels_of(package: &Package, part: &str) -> Result<Vec<Rel>, Error> {
    package
        .get(&rels::rels_part(part))
        .map_or_else(|| Ok(Vec::new()), |b| Ok(rels::parse(b)?))
}

/// Open an `.xlsx`.
#[allow(clippy::too_many_lines)]
pub fn read(bytes: &[u8]) -> Result<Xlsx, Error> {
    let package = Package::read(bytes)?;
    let root = rels_of(&package, "")?;
    let workbook_part = root
        .iter()
        .find(|r| r.kind == rels::OFFICE_DOCUMENT)
        .map(|r| rels::resolve("", &r.target))
        .ok_or_else(|| Error::Invalid("no office document relationship".into()))?;
    let wb_xml = xml::parse(
        package
            .get(&workbook_part)
            .ok_or_else(|| Error::Invalid(format!("{workbook_part} is missing")))?,
        false,
    )?;
    let wb_rels = rels_of(&package, &workbook_part)?;
    let target_of = |kind: &str| {
        wb_rels
            .iter()
            .find(|r| r.kind == kind && !r.external)
            .map(|r| rels::resolve(&workbook_part, &r.target))
    };
    let date1904 = xml::child(&wb_xml, "workbookPr")
        .and_then(|p| xml::attr(p, "date1904"))
        .is_some_and(|v| v == "1" || v == "true");

    let styles_part = target_of(rels::STYLES).filter(|p| package.get(p).is_some());
    let (style_list, dxfs, table_styles) = match &styles_part {
        Some(p) => {
            let root = xml::parse(package.get(p).unwrap_or_default(), false)?;
            (
                styles::read(&root),
                styles::read_dxfs(&root),
                styles::read_table_styles(&root),
            )
        }
        None => (
            vec![sheets_model::Style::default()],
            Vec::new(),
            std::collections::HashMap::new(),
        ),
    };
    let date_styles: HashSet<u32> = style_list
        .iter()
        .enumerate()
        .filter(|(_, s)| Format::parse(&s.number_format).is_ok_and(|f| f.is_date()))
        .map(|(i, _)| u32::try_from(i).unwrap_or(0))
        .collect();

    let (original_strings, strings): (Vec<Value>, Vec<String>) =
        match target_of(rels::SHARED_STRINGS).and_then(|p| package.get(&p)) {
            Some(b) => {
                let sst = xml::parse(b, true)?;
                let items: Vec<Value> = xml::elements(&sst, "si").cloned().collect();
                let texts = items.iter().map(si_text).collect();
                (items, texts)
            }
            None => (Vec::new(), Vec::new()),
        };

    let mut workbook = Workbook {
        sheets: Vec::new(),
        styles: style_list,
        names: defined_names(&wb_xml),
    };
    let mut sheet_parts = Vec::new();
    let entries: Vec<&Value> = xml::child(&wb_xml, "sheets")
        .map(|s| xml::elements(s, "sheet").collect())
        .unwrap_or_default();
    if entries.len() > MAX_SHEETS {
        return Err(Error::Invalid(format!("{} sheets", entries.len())));
    }
    let mut cells = 0;
    for e in entries {
        let name = xml::attr(e, "name").unwrap_or("Sheet").to_string();
        let Some(rel) = xml::attr_local(e, "id").and_then(|id| wb_rels.iter().find(|r| r.id == id))
        else {
            continue;
        };
        if !rel.kind.ends_with("/worksheet") {
            continue; // chart sheets and the like stay in the package
        }
        let part = rels::resolve(&workbook_part, &rel.target);
        let bytes = package
            .get(&part)
            .ok_or_else(|| Error::Invalid(format!("{part} is missing")))?;
        let (tree, data) = split_sheet(bytes)?;
        let mut sheet = Sheet::new(name);
        read_sheet(
            &tree,
            data,
            &strings,
            &mut sheet,
            &workbook.styles,
            &dxfs,
            &date_styles,
            date1904,
            &mut cells,
        )?;
        crate::comments::read(&package, &part, &mut sheet);
        crate::charts::read(&package, &part, &mut sheet);
        crate::tables::read(&package, &part, &mut sheet, &table_styles);
        workbook.sheets.push(sheet);
        sheet_parts.push(Some(part));
    }
    if workbook.sheets.is_empty() {
        return Err(Error::Invalid("no worksheets".into()));
    }
    read_print_names(&wb_xml, &mut workbook.sheets);
    Ok(Xlsx {
        original_styles: workbook.styles.len(),
        engine: Engine::new(workbook),
        package,
        workbook_part,
        sheet_parts,
        styles_part,
        original_strings,
    })
}

/// A worksheet as the tree of everything but its cells, and the text of its `<sheetData>`.
pub fn split_sheet(bytes: &[u8]) -> Result<(Value, Option<&str>), Error> {
    let Some((from, to)) = scan::data_range(bytes) else {
        return Ok((xml::parse(bytes, true)?, None));
    };
    let mut rest = Vec::with_capacity(bytes.len() - (to - from) + 12);
    rest.extend_from_slice(&bytes[..from]);
    rest.extend_from_slice(b"<sheetData/>");
    rest.extend_from_slice(&bytes[to..]);
    let data = std::str::from_utf8(&bytes[from..to])
        .map_err(|_| Error::Invalid("a sheet is not UTF-8".into()))?;
    Ok((xml::parse(&rest, true)?, Some(data)))
}

/// What reading the cells of one sheet needs and keeps.
struct Reader<'a> {
    sheet: &'a mut Sheet,
    strings: &'a [String],
    styles: usize,
    date_styles: &'a HashSet<u32>,
    date1904: bool,
    cells: &'a mut usize,
    shared: HashMap<String, (CellRef, Expr)>,
    /// The rest of the cells an array formula fills: their cached values are not cells of their own.
    filled: Vec<sheets_model::Range>,
    row_no: u32,
    col_no: u32,
    error: Option<Error>,
}

impl scan::Visitor for Reader<'_> {
    fn row(&mut self, row: scan::Row) -> bool {
        self.row_no = row.number.map_or(self.row_no + 1, |r| r.max(1));
        self.col_no = 0;
        let value = |name: &str| {
            row.attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        if matches!(value("customFormat"), Some("1" | "true"))
            && let Some(style) = value("s").and_then(|v| v.parse::<u32>().ok())
        {
            self.sheet.set_row_style(self.row_no - 1, style);
        }
        if let Some(level) = value("outlineLevel").and_then(|v| v.parse::<u8>().ok()) {
            self.sheet.set_row_level(self.row_no - 1, level);
        }
        if matches!(value("hidden"), Some("1" | "true")) {
            self.sheet.set_row_hidden(self.row_no - 1, true);
        }
        if matches!(value("customHeight"), Some("1" | "true"))
            && let Some(points) = value("ht").and_then(|h| h.parse::<f64>().ok())
        {
            self.sheet
                .set_row_height(self.row_no - 1, Some(points * POINTS_TO_PIXELS));
        }
        true
    }

    fn cell(&mut self, c: scan::Cell) -> bool {
        let at = c
            .address
            .as_deref()
            .and_then(CellRef::parse)
            .unwrap_or_else(|| CellRef::new(self.row_no.saturating_sub(1), self.col_no));
        self.col_no = at.col + 1;
        *self.cells += 1;
        if *self.cells > MAX_CELLS {
            self.error = Some(Error::Invalid("more than ten million cells".into()));
            return false;
        }
        let style = c
            .style
            .as_deref()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|s| (*s as usize) < self.styles)
            .unwrap_or(0);
        if style != 0 {
            self.sheet.set_style(at, style);
        }
        if self.filled.iter().any(|r| r.contains(at)) {
            return true;
        }
        let v = c.value.as_deref();
        let value = match c.kind.as_deref().unwrap_or("n") {
            "s" => v
                .and_then(|i| i.trim().parse::<usize>().ok())
                .and_then(|i| self.strings.get(i))
                .map(|s| Cell::Text(s.clone())),
            "str" => v.map(|s| Cell::Text(s.to_string())),
            "inlineStr" => c.inline.map(Cell::Text),
            "b" => v.map(|s| Cell::Bool(s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"))),
            "e" => v.map(|s| Cell::Error(CellError::parse(s.trim()).unwrap_or(CellError::Value))),
            "d" => v.and_then(|s| iso_serial(s.trim())).map(Cell::Number),
            _ => v
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|n| n.is_finite())
                .map(|mut n| {
                    if self.date1904 && self.date_styles.contains(&style) {
                        n += DATE1904_SHIFT;
                    }
                    Cell::Number(n)
                }),
        };
        if let Some(f) = &c.formula
            && let Some(text) = formula_text(f, at, &mut self.shared)
        {
            if f.kind.as_deref() == Some("array")
                && let Some(range) = f.reference.as_deref().and_then(sheets_model::Range::parse)
                && range.start != range.end
            {
                self.filled.push(range);
            }
            // What the file says the formula came to is kept for formulas this suite cannot calculate.
            let unknown = formula::uses_unknown_function(&text);
            self.sheet.set(at, Content::Formula(text));
            if unknown && let Some(v) = value {
                self.sheet.set_cached(at, v);
            }
            return true;
        }
        if let Some(v) = value {
            self.sheet.set(at, Content::Value(v));
        }
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn read_sheet(
    tree: &Value,
    data: Option<&str>,
    strings: &[String],
    sheet: &mut Sheet,
    styles: &[sheets_model::Style],
    dxfs: &[sheets_model::Dxf],
    date_styles: &HashSet<u32>,
    date1904: bool,
    cells: &mut usize,
) -> Result<(), Error> {
    read_columns(tree, sheet);
    crate::extras::read(tree, sheet, dxfs);
    let Some(data) = data else { return Ok(()) };
    let mut reader = Reader {
        sheet,
        strings,
        styles: styles.len(),
        date_styles,
        date1904,
        cells,
        shared: HashMap::new(),
        filled: Vec::new(),
        row_no: 0,
        col_no: 0,
        error: None,
    };
    scan::scan(data, &mut reader).map_err(Error::Invalid)?;
    reader.error.map_or(Ok(()), Err)
}

/// The formula of an `<f>`: its own text, or for a follower of a shared formula, the master's moved to this cell.
fn formula_text(
    f: &scan::Formula,
    at: CellRef,
    shared: &mut HashMap<String, (CellRef, Expr)>,
) -> Option<String> {
    if f.kind.as_deref() == Some("shared")
        && let Some(si) = &f.shared_index
    {
        if f.text.trim().is_empty() {
            let (origin, expr) = shared.get(si)?;
            let moved = expr.shifted(
                i64::from(at.row) - i64::from(origin.row),
                i64::from(at.col) - i64::from(origin.col),
            );
            return Some(print(&moved));
        }
        let text = formula::from_file(&f.text);
        if let Ok(e) = parse(&text) {
            shared.insert(si.clone(), (at, e));
        }
        return Some(text);
    }
    if f.text.trim().is_empty() {
        return None;
    }
    Some(formula::from_file(&f.text))
}

/// `2024-02-29` or `2024-02-29T13:30:00` (cells of type `d`) as a serial number.
#[allow(clippy::many_single_char_names)]
fn iso_serial(s: &str) -> Option<f64> {
    let (date, time) = s.split_once('T').map_or((s, None), |(d, t)| (d, Some(t)));
    let mut it = date.split('-');
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let serial = sheets_model::date::to_serial(y, m, d);
    if !(1..=MAX_SERIAL).contains(&serial) {
        return None;
    }
    let frac = match time {
        Some(t) => {
            let mut p = t.trim_end_matches('Z').split(':');
            let h: f64 = p.next()?.parse().ok()?;
            let mi: f64 = p.next().map_or(Some(0.0), |x| x.parse().ok())?;
            let se: f64 = p.next().map_or(Some(0.0), |x| x.parse().ok())?;
            h.mul_add(3600.0, mi.mul_add(60.0, se)) / 86_400.0
        }
        None => 0.0,
    };
    #[allow(clippy::cast_precision_loss)]
    Some(serial as f64 + frac)
}
