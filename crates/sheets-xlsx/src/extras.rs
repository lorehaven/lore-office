//! The parts of a worksheet besides its cells that the suite shows and edits: frozen panes, merged cells, and so on. Read
//! from the sheet's XML tree into the model, and put back into it when the file is written; elements the suite does not
//! know stay as they were.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use office_fmt::xml::{self, Value};
use sheets_model::{
    Allow, AutoFilter, CellRef, Compare, Cond, CondRule, Dxf, NumberKind, Orientation, PageSetup,
    Paper, Protection, Range, Severity, Sheet, SparkKind, Sparkline, Validation,
};
use std::collections::{BTreeMap, BTreeSet};

/// The order of a worksheet's children, which a reader requires.
const ORDER: [&str; 33] = [
    "sheetPr",
    "dimension",
    "sheetViews",
    "sheetFormatPr",
    "cols",
    "sheetData",
    "sheetCalcPr",
    "sheetProtection",
    "protectedRanges",
    "scenarios",
    "autoFilter",
    "sortState",
    "dataConsolidate",
    "customSheetViews",
    "mergeCells",
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "tableParts",
    "extLst",
];

fn rank(tag: &str) -> usize {
    ORDER.iter().position(|t| *t == tag).unwrap_or(ORDER.len())
}

/// Replace every element called `tag` among the sheet's children with `elements` (none removes them), in the right place.
pub fn put(tree: &mut Value, tag: &str, elements: Vec<Value>) {
    let Some(kids) = xml::children_mut(tree) else {
        return;
    };
    kids.retain(|k| xml::tag(k) != Some(tag));
    let want = rank(tag);
    let at = kids
        .iter()
        .rposition(|k| xml::tag(k).is_some_and(|t| rank(t) <= want))
        .map_or(0, |i| i + 1);
    for (n, e) in elements.into_iter().enumerate() {
        kids.insert(at + n, e);
    }
}

pub fn parse_range(text: &str) -> Option<Range> {
    Range::parse(text).or_else(|| CellRef::parse(text).map(|c| Range::new(c, c)))
}

/// Read what the sheet's tree says into the model.
pub fn read(tree: &Value, sheet: &mut Sheet, dxfs: &[Dxf]) {
    read_validations(tree, sheet);
    read_page(tree, sheet);
    read_sparklines(tree, sheet);
    read_protection(tree, sheet);
    read_conditional(tree, sheet, dxfs);
    // Frozen panes: `<sheetViews><sheetView><pane xSplit ySplit state="frozen"/>`.
    if let Some(pane) = xml::child(tree, "sheetViews")
        .and_then(|v| xml::child(v, "sheetView"))
        .and_then(|v| xml::child(v, "pane"))
        && matches!(xml::attr(pane, "state"), Some("frozen" | "frozenSplit"))
    {
        let n = |name: &str| {
            xml::attr(pane, name)
                .and_then(|v| v.parse::<f64>().ok())
                .map_or(0, |v| v.max(0.0) as u32)
        };
        sheet.set_freeze(n("ySplit"), n("xSplit"));
    }
    if let Some(f) = xml::child(tree, "autoFilter").and_then(read_filter) {
        sheet.set_autofilter(Some(f));
    }
    if let Some(merges) = xml::child(tree, "mergeCells") {
        for m in xml::elements(merges, "mergeCell") {
            if let Some(r) = xml::attr(m, "ref").and_then(parse_range) {
                sheet.merge(r);
            }
        }
    }
}

/// Put the model's frozen panes and merged cells into the sheet's tree.
pub fn write(tree: &mut Value, sheet: &Sheet, styles: &mut Value) {
    write_validations(tree, sheet);
    write_page(tree, sheet);
    write_sparklines(tree, sheet);
    write_protection(tree, sheet);
    write_conditional(tree, sheet, styles);
    set_pane(tree, sheet.freeze());
    set_outline_levels(tree, sheet);
    let filter = sheet.autofilter().map(filter_element);
    put(tree, "autoFilter", filter.into_iter().collect());
    let merges: Vec<Value> = sheet
        .merges()
        .iter()
        .map(|m| xml::el("mergeCell", &[("ref", &m.to_string())], Vec::new()))
        .collect();
    let count = merges.len().to_string();
    put(
        tree,
        "mergeCells",
        if merges.is_empty() {
            Vec::new()
        } else {
            vec![xml::el("mergeCells", &[("count", &count)], merges)]
        },
    );
}

fn set_pane(tree: &mut Value, (rows, cols): (u32, u32)) {
    if xml::child(tree, "sheetViews").is_none() {
        if rows == 0 && cols == 0 {
            return;
        }
        put(
            tree,
            "sheetViews",
            vec![xml::el(
                "sheetViews",
                &[],
                vec![xml::el("sheetView", &[("workbookViewId", "0")], Vec::new())],
            )],
        );
    }
    let Some(views) = xml::child_mut(tree, "sheetViews") else {
        return;
    };
    let Some(view) = xml::child_mut(views, "sheetView") else {
        return;
    };
    let Some(kids) = xml::children_mut(view) else {
        return;
    };
    kids.retain(|k| xml::tag(k) != Some("pane"));
    if rows == 0 && cols == 0 {
        return;
    }
    let top_left = CellRef::new(rows, cols).to_string();
    let active = match (rows > 0, cols > 0) {
        (true, true) => "bottomRight",
        (true, false) => "bottomLeft",
        _ => "topRight",
    };
    let mut attrs: Vec<(&str, String)> = Vec::new();
    if cols > 0 {
        attrs.push(("xSplit", cols.to_string()));
    }
    if rows > 0 {
        attrs.push(("ySplit", rows.to_string()));
    }
    attrs.push(("topLeftCell", top_left));
    attrs.push(("activePane", active.to_string()));
    attrs.push(("state", "frozen".to_string()));
    let pairs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    kids.insert(0, xml::el("pane", &pairs, Vec::new()));
}

// ---- data validation ----

fn text_of(e: &Value, tag: &str) -> String {
    xml::child(e, tag).map(xml::text).unwrap_or_default()
}

fn read_validations(tree: &Value, sheet: &mut Sheet) {
    let Some(list) = xml::child(tree, "dataValidations") else {
        return;
    };
    let mut out = Vec::new();
    for v in xml::elements(list, "dataValidation") {
        let kind = xml::attr(v, "type").unwrap_or("none");
        let (a, b) = (text_of(v, "formula1"), text_of(v, "formula2"));
        let op = xml::attr(v, "operator")
            .and_then(Compare::from_name)
            .unwrap_or(Compare::Between);
        let number = |kind| Allow::Number {
            kind,
            op,
            a: a.clone(),
            b: b.clone(),
        };
        let allow = match kind {
            "list" => a
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .map_or_else(
                    || Allow::ListFrom(a.clone()),
                    |inline| Allow::List(inline.split(',').map(str::to_string).collect()),
                ),
            "whole" => number(NumberKind::Whole),
            "decimal" => number(NumberKind::Decimal),
            "date" => number(NumberKind::Date),
            "time" => number(NumberKind::Time),
            "textLength" => number(NumberKind::Length),
            "custom" => Allow::Custom(a.clone()),
            _ => continue,
        };
        let severity = match xml::attr(v, "errorStyle") {
            Some("warning") => Severity::Warning,
            Some("information") => Severity::Information,
            _ => Severity::Stop,
        };
        let flag = |name: &str| matches!(xml::attr(v, name), Some("1" | "true"));
        let message = xml::attr(v, "error").unwrap_or_default().to_string();
        let title = xml::attr(v, "errorTitle").unwrap_or_default().to_string();
        for range in xml::attr(v, "sqref")
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(parse_range)
        {
            out.push(Validation {
                range,
                allow: allow.clone(),
                allow_blank: flag("allowBlank"),
                // `showDropDown` is the other way round: set, the arrow is hidden.
                dropdown: !flag("showDropDown"),
                severity,
                title: title.clone(),
                message: message.clone(),
            });
        }
    }
    sheet.set_validations(out);
}

fn write_validations(tree: &mut Value, sheet: &Sheet) {
    let items: Vec<Value> = sheet
        .validations()
        .iter()
        .map(|v| {
            let (kind, op, f1, f2): (&str, Option<Compare>, String, Option<String>) = match &v.allow
            {
                Allow::List(items) => ("list", None, format!("\"{}\"", items.join(",")), None),
                Allow::ListFrom(r) => ("list", None, r.clone(), None),
                Allow::Number { kind, op, a, b } => (
                    match kind {
                        NumberKind::Whole => "whole",
                        NumberKind::Decimal => "decimal",
                        NumberKind::Date => "date",
                        NumberKind::Time => "time",
                        NumberKind::Length => "textLength",
                    },
                    Some(*op),
                    a.clone(),
                    matches!(op, Compare::Between | Compare::NotBetween).then(|| b.clone()),
                ),
                Allow::Custom(f) => ("custom", None, f.clone(), None),
            };
            let mut attrs: Vec<(&str, String)> = vec![("type", kind.to_string())];
            if let Some(op) = op
                && op != Compare::Between
            {
                attrs.push(("operator", op.name().to_string()));
            }
            attrs.push((
                "errorStyle",
                match v.severity {
                    Severity::Stop => "stop",
                    Severity::Warning => "warning",
                    Severity::Information => "information",
                }
                .to_string(),
            ));
            if v.allow_blank {
                attrs.push(("allowBlank", "1".into()));
            }
            if !v.dropdown {
                attrs.push(("showDropDown", "1".into()));
            }
            attrs.push(("showErrorMessage", "1".into()));
            if !v.title.is_empty() {
                attrs.push(("errorTitle", v.title.clone()));
            }
            if !v.message.is_empty() {
                attrs.push(("error", v.message.clone()));
            }
            attrs.push(("sqref", v.range.to_string()));
            let pairs: Vec<(&str, &str)> = attrs.iter().map(|(k, x)| (*k, x.as_str())).collect();
            let mut kids = vec![xml::el("formula1", &[], vec![xml::txt(&f1)])];
            if let Some(f2) = f2 {
                kids.push(xml::el("formula2", &[], vec![xml::txt(&f2)]));
            }
            xml::el("dataValidation", &pairs, kids)
        })
        .collect();
    let count = items.len().to_string();
    put(
        tree,
        "dataValidations",
        if items.is_empty() {
            Vec::new()
        } else {
            vec![xml::el("dataValidations", &[("count", &count)], items)]
        },
    );
}

// ---- conditional formatting ----

fn read_color(e: &Value) -> Option<u32> {
    xml::attr(e, "rgb")
        .map(|t| if t.len() == 8 { &t[2..] } else { t })
        .and_then(|h| u32::from_str_radix(h, 16).ok())
}

fn quoted_text(formula: &str) -> Option<String> {
    // NOT(ISERROR(SEARCH("x",A1))) and friends: the text between the first pair of quotes.
    let start = formula.find('"')? + 1;
    let end = formula[start..].find('"')? + start;
    Some(formula[start..end].replace("\"\"", "\""))
}

/// One `<cfRule>` as the model has it, or `None` when it is of a kind not understood.
fn read_rule(rule: &Value, range: Range, dxfs: &[Dxf]) -> Option<CondRule> {
    let kind = xml::attr(rule, "type")?;
    let formula = text_of(rule, "formula");
    let formulas: Vec<String> = xml::elements(rule, "formula").map(xml::text).collect();
    let text = xml::attr(rule, "text").map(str::to_string);
    let cond = match kind {
        "cellIs" => Cond::CellIs {
            op: xml::attr(rule, "operator").and_then(Compare::from_name)?,
            a: formulas.first().cloned().unwrap_or_default(),
            b: formulas.get(1).cloned().unwrap_or_default(),
        },
        "containsText" => Cond::Contains(text.or_else(|| quoted_text(&formula))?),
        "notContainsText" => Cond::NotContains(text.or_else(|| quoted_text(&formula))?),
        "beginsWith" => Cond::BeginsWith(text.or_else(|| quoted_text(&formula))?),
        "endsWith" => Cond::EndsWith(text.or_else(|| quoted_text(&formula))?),
        "expression" => Cond::Expression(formula),
        "duplicateValues" => Cond::Duplicates,
        "uniqueValues" => Cond::Unique,
        "containsBlanks" => Cond::Blanks,
        "notContainsBlanks" => Cond::NotBlanks,
        "containsErrors" => Cond::Errors,
        "top10" => Cond::Top {
            n: xml::attr(rule, "rank")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            bottom: matches!(xml::attr(rule, "bottom"), Some("1" | "true")),
            percent: matches!(xml::attr(rule, "percent"), Some("1" | "true")),
        },
        "aboveAverage" => Cond::AboveAverage {
            below: matches!(xml::attr(rule, "aboveAverage"), Some("0" | "false")),
        },
        "colorScale" => {
            let scale = xml::child(rule, "colorScale")?;
            let colors: Vec<u32> = xml::elements(scale, "color")
                .filter_map(read_color)
                .collect();
            match colors.as_slice() {
                [low, high] => Cond::ColorScale {
                    low: *low,
                    mid: None,
                    high: *high,
                },
                [low, mid, high] => Cond::ColorScale {
                    low: *low,
                    mid: Some(*mid),
                    high: *high,
                },
                _ => return None,
            }
        }
        "dataBar" => {
            let bar = xml::child(rule, "dataBar")?;
            Cond::DataBar {
                color: xml::child(bar, "color")
                    .and_then(read_color)
                    .unwrap_or(0x0063_8EC6),
            }
        }
        _ => return None,
    };
    let dxf = xml::attr(rule, "dxfId")
        .and_then(|v| v.parse::<usize>().ok())
        .and_then(|i| dxfs.get(i).cloned())
        .unwrap_or_default();
    Some(CondRule {
        range,
        cond,
        dxf,
        priority: xml::attr(rule, "priority")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1),
        stop_if_true: matches!(xml::attr(rule, "stopIfTrue"), Some("1" | "true")),
    })
}

fn read_conditional(tree: &Value, sheet: &mut Sheet, dxfs: &[Dxf]) {
    let mut rules = Vec::new();
    let mut raw = Vec::new();
    for block in xml::children(tree)
        .iter()
        .filter(|k| xml::tag(k) == Some("conditionalFormatting"))
    {
        let ranges: Vec<Range> = xml::attr(block, "sqref")
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(parse_range)
            .collect();
        let parsed: Option<Vec<CondRule>> = match ranges.as_slice() {
            [range] => xml::elements(block, "cfRule")
                .map(|r| read_rule(r, *range, dxfs))
                .collect(),
            _ => None,
        };
        match parsed {
            Some(list) if !list.is_empty() => rules.extend(list),
            _ => raw.push(xml::to_text(block)),
        }
    }
    sheet.set_cond_rules(rules);
    sheet.set_cond_raw(raw);
}

fn cfvo(kind: &str) -> Value {
    xml::el("cfvo", &[("type", kind)], Vec::new())
}

#[allow(clippy::too_many_lines)]
fn write_rule(rule: &CondRule, styles: &mut Value, priority: usize) -> Value {
    let cell = rule.range.start.to_string();
    let escape = |t: &str| t.replace('"', "\"\"");
    let mut attrs: Vec<(&str, String)> = Vec::new();
    let mut kids: Vec<Value> = Vec::new();
    let formula = |f: String| xml::el("formula", &[], vec![xml::txt(&f)]);
    match &rule.cond {
        Cond::CellIs { op, a, b } => {
            attrs.push(("type", "cellIs".into()));
            attrs.push(("operator", op.name().into()));
            kids.push(formula(a.clone()));
            if matches!(op, Compare::Between | Compare::NotBetween) {
                kids.push(formula(b.clone()));
            }
        }
        Cond::Contains(t) => {
            attrs.extend([
                ("type", "containsText".to_string()),
                ("operator", "containsText".into()),
                ("text", t.clone()),
            ]);
            kids.push(formula(format!(
                "NOT(ISERROR(SEARCH(\"{}\",{cell})))",
                escape(t)
            )));
        }
        Cond::NotContains(t) => {
            attrs.extend([
                ("type", "notContainsText".to_string()),
                ("operator", "notContains".into()),
                ("text", t.clone()),
            ]);
            kids.push(formula(format!(
                "ISERROR(SEARCH(\"{}\",{cell}))",
                escape(t)
            )));
        }
        Cond::BeginsWith(t) => {
            attrs.extend([
                ("type", "beginsWith".to_string()),
                ("operator", "beginsWith".into()),
                ("text", t.clone()),
            ]);
            kids.push(formula(format!(
                "LEFT({cell},{})=\"{}\"",
                t.chars().count(),
                escape(t)
            )));
        }
        Cond::EndsWith(t) => {
            attrs.extend([
                ("type", "endsWith".to_string()),
                ("operator", "endsWith".into()),
                ("text", t.clone()),
            ]);
            kids.push(formula(format!(
                "RIGHT({cell},{})=\"{}\"",
                t.chars().count(),
                escape(t)
            )));
        }
        Cond::Expression(f) => {
            attrs.push(("type", "expression".into()));
            kids.push(formula(f.clone()));
        }
        Cond::Duplicates => attrs.push(("type", "duplicateValues".into())),
        Cond::Unique => attrs.push(("type", "uniqueValues".into())),
        Cond::Blanks => {
            attrs.push(("type", "containsBlanks".into()));
            kids.push(formula(format!("LEN(TRIM({cell}))=0")));
        }
        Cond::NotBlanks => {
            attrs.push(("type", "notContainsBlanks".into()));
            kids.push(formula(format!("LEN(TRIM({cell}))>0")));
        }
        Cond::Errors => {
            attrs.push(("type", "containsErrors".into()));
            kids.push(formula(format!("ISERROR({cell})")));
        }
        Cond::Top { n, bottom, percent } => {
            attrs.push(("type", "top10".into()));
            attrs.push(("rank", n.to_string()));
            if *bottom {
                attrs.push(("bottom", "1".into()));
            }
            if *percent {
                attrs.push(("percent", "1".into()));
            }
        }
        Cond::AboveAverage { below } => {
            attrs.push(("type", "aboveAverage".into()));
            if *below {
                attrs.push(("aboveAverage", "0".into()));
            }
        }
        Cond::ColorScale { low, mid, high } => {
            attrs.push(("type", "colorScale".into()));
            let mut points = vec![cfvo("min")];
            let mut colors = vec![xml::el(
                "color",
                &[("rgb", &format!("FF{low:06X}"))],
                vec![],
            )];
            if let Some(m) = mid {
                points.push(xml::el(
                    "cfvo",
                    &[("type", "percentile"), ("val", "50")],
                    vec![],
                ));
                colors.push(xml::el("color", &[("rgb", &format!("FF{m:06X}"))], vec![]));
            }
            points.push(cfvo("max"));
            colors.push(xml::el(
                "color",
                &[("rgb", &format!("FF{high:06X}"))],
                vec![],
            ));
            points.extend(colors);
            kids.push(xml::el("colorScale", &[], points));
        }
        Cond::DataBar { color } => {
            attrs.push(("type", "dataBar".into()));
            kids.push(xml::el(
                "dataBar",
                &[],
                vec![
                    cfvo("min"),
                    cfvo("max"),
                    xml::el("color", &[("rgb", &format!("FF{color:06X}"))], vec![]),
                ],
            ));
        }
    }
    let visual = matches!(rule.cond, Cond::ColorScale { .. } | Cond::DataBar { .. });
    if !visual {
        let id = crate::styles::dxf_for(styles, &rule.dxf);
        attrs.push(("dxfId", id.to_string()));
    }
    attrs.push(("priority", priority.to_string()));
    if rule.stop_if_true {
        attrs.push(("stopIfTrue", "1".into()));
    }
    let pairs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    xml::el("cfRule", &pairs, kids)
}

fn write_conditional(tree: &mut Value, sheet: &Sheet, styles: &mut Value) {
    let mut blocks: Vec<Value> = Vec::new();
    for (i, rule) in sheet.cond_rules().iter().enumerate() {
        let r = write_rule(rule, styles, i + 1);
        blocks.push(xml::el(
            "conditionalFormatting",
            &[("sqref", &rule.range.to_string())],
            vec![r],
        ));
    }
    for text in sheet.cond_raw() {
        if let Ok(e) = xml::from_text(text) {
            blocks.push(e);
        }
    }
    put(tree, "conditionalFormatting", blocks);
}

/// `<sheetFormatPr outlineLevelRow outlineLevelCol>` says how deep the groups go, which a spreadsheet needs to show their
/// buttons.
fn set_outline_levels(tree: &mut Value, sheet: &Sheet) {
    let rows = sheet.row_levels().map(|(_, l)| l).max().unwrap_or(0);
    let cols = sheet.col_levels().map(|(_, l)| l).max().unwrap_or(0);
    if xml::child(tree, "sheetFormatPr").is_none() {
        if rows == 0 && cols == 0 {
            return;
        }
        put(
            tree,
            "sheetFormatPr",
            vec![xml::el(
                "sheetFormatPr",
                &[("defaultRowHeight", "15")],
                Vec::new(),
            )],
        );
    }
    let Some(f) = xml::child_mut(tree, "sheetFormatPr") else {
        return;
    };
    for (name, level) in [("outlineLevelRow", rows), ("outlineLevelCol", cols)] {
        if level == 0 {
            xml::remove_attr(f, name);
        } else {
            xml::set_attr(f, name, &level.to_string());
        }
    }
}

// ---- page setup ----

fn read_page(tree: &Value, sheet: &mut Sheet) {
    let flag = |e: &Value, name: &str| matches!(xml::attr(e, name), Some("1" | "true"));
    let mut page: PageSetup = sheet.page().clone();
    page.gridlines = xml::child(tree, "printOptions").is_some_and(|p| flag(p, "gridLines"));
    if let Some(setup) = xml::child(tree, "pageSetup") {
        page.orientation = match xml::attr(setup, "orientation") {
            Some("landscape") => Orientation::Landscape,
            Some("portrait") => Orientation::Portrait,
            _ => Orientation::Auto,
        };
        page.paper = if xml::attr(setup, "paperSize") == Some("1") {
            Paper::Letter
        } else {
            Paper::A4
        };
        let fit_to_page = xml::child(tree, "sheetPr")
            .and_then(|p| xml::child(p, "pageSetUpPr"))
            .is_some_and(|p| flag(p, "fitToPage"));
        page.fit_width = fit_to_page && xml::attr(setup, "fitToWidth") != Some("0");
    }
    if let Some(m) = xml::child(tree, "pageMargins") {
        let inches = |name: &str| xml::attr(m, name).and_then(|v| v.parse::<f64>().ok());
        if let Some(left) = inches("left") {
            page.margin_mm = (left * 25.4).round().clamp(0.0, 100.0) as u32;
        }
    }
    sheet.set_page(page);
}

fn write_page(tree: &mut Value, sheet: &Sheet) {
    let page = sheet.page();
    put(
        tree,
        "printOptions",
        if page.gridlines {
            vec![xml::el("printOptions", &[("gridLines", "1")], Vec::new())]
        } else {
            Vec::new()
        },
    );
    let inches = format!("{:.3}", f64::from(page.margin_mm) / 25.4);
    put(
        tree,
        "pageMargins",
        vec![xml::el(
            "pageMargins",
            &[
                ("left", &inches),
                ("right", &inches),
                ("top", &inches),
                ("bottom", &inches),
                ("header", "0.3"),
                ("footer", "0.3"),
            ],
            Vec::new(),
        )],
    );
    // The page setup keeps what the file had (printer settings, scale) except what is set here.
    let mut setup = xml::child(tree, "pageSetup")
        .cloned()
        .unwrap_or_else(|| xml::el("pageSetup", &[], Vec::new()));
    match page.orientation {
        Orientation::Auto => xml::remove_attr(&mut setup, "orientation"),
        Orientation::Portrait => xml::set_attr(&mut setup, "orientation", "portrait"),
        Orientation::Landscape => xml::set_attr(&mut setup, "orientation", "landscape"),
    }
    xml::set_attr(
        &mut setup,
        "paperSize",
        if page.paper == Paper::Letter {
            "1"
        } else {
            "9"
        },
    );
    if page.fit_width {
        xml::set_attr(&mut setup, "fitToWidth", "1");
        xml::set_attr(&mut setup, "fitToHeight", "0");
    } else {
        xml::remove_attr(&mut setup, "fitToWidth");
        xml::remove_attr(&mut setup, "fitToHeight");
    }
    put(tree, "pageSetup", vec![setup]);
    // `fitToPage` lives in the sheet's properties.
    if page.fit_width && xml::child(tree, "sheetPr").is_none() {
        put(tree, "sheetPr", vec![xml::el("sheetPr", &[], Vec::new())]);
    }
    if let Some(props) = xml::child_mut(tree, "sheetPr") {
        let existing = xml::child(props, "pageSetUpPr").cloned();
        if let Some(kids) = xml::children_mut(props) {
            kids.retain(|k| xml::tag(k) != Some("pageSetUpPr"));
            if page.fit_width {
                let mut e = existing.unwrap_or_else(|| xml::el("pageSetUpPr", &[], Vec::new()));
                xml::set_attr(&mut e, "fitToPage", "1");
                kids.push(e);
            } else if let Some(mut e) = existing {
                xml::remove_attr(&mut e, "fitToPage");
                if !xml::attrs(&e).is_empty() {
                    kids.push(e);
                }
            }
        }
    }
}

// ---- sparklines (an extension: `x14:sparklineGroups` in the sheet's `extLst`) ----

const SPARK_URI: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

fn name_of(e: &Value) -> &str {
    xml::tag(e).map_or("", |t| t.rsplit(':').next().unwrap_or(t))
}

fn read_sparklines(tree: &Value, sheet: &mut Sheet) {
    let mut out = Vec::new();
    let Some(list) = xml::child(tree, "extLst") else {
        return;
    };
    for ext in xml::children(list)
        .iter()
        .filter(|e| xml::attr(e, "uri") == Some(SPARK_URI))
    {
        for groups in xml::children(ext)
            .iter()
            .filter(|g| name_of(g) == "sparklineGroups")
        {
            for group in xml::children(groups)
                .iter()
                .filter(|g| name_of(g) == "sparklineGroup")
            {
                let kind = match xml::attr(group, "type") {
                    Some("column") => SparkKind::Column,
                    Some("stacked") => SparkKind::WinLoss,
                    _ => SparkKind::Line,
                };
                let color = xml::children(group)
                    .iter()
                    .find(|c| name_of(c) == "colorSeries")
                    .and_then(|c| xml::attr(c, "rgb"))
                    .map(|t| if t.len() == 8 { &t[2..] } else { t })
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .unwrap_or(0x0037_6092);
                for lines in xml::children(group)
                    .iter()
                    .filter(|c| name_of(c) == "sparklines")
                {
                    for sp in xml::children(lines)
                        .iter()
                        .filter(|c| name_of(c) == "sparkline")
                    {
                        let part = |n: &str| {
                            xml::children(sp)
                                .iter()
                                .find(|c| name_of(c) == n)
                                .map(xml::text)
                        };
                        let (Some(f), Some(cell)) = (part("f"), part("sqref")) else {
                            continue;
                        };
                        let data = f
                            .rsplit_once('!')
                            .map_or(f.as_str(), |(_, r)| r)
                            .replace('$', "");
                        if let (Some(data), Some(cell)) =
                            (parse_range(&data), CellRef::parse(cell.trim()))
                        {
                            out.push(Sparkline {
                                cell,
                                data,
                                kind,
                                color,
                            });
                        }
                    }
                }
            }
        }
    }
    sheet.set_sparklines(out);
}

fn write_sparklines(tree: &mut Value, sheet: &Sheet) {
    // The other extensions the sheet has are kept; ours is made again.
    let others: Vec<Value> = xml::child(tree, "extLst")
        .map(|l| {
            xml::children(l)
                .iter()
                .filter(|e| e.is_object() && xml::attr(e, "uri") != Some(SPARK_URI))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut ext = others;
    let sparks = sheet.sparklines();
    if !sparks.is_empty() {
        let quoted = format!("'{}'", sheet.name.replace('\'', "''"));
        let mut groups: Vec<Value> = Vec::new();
        let mut seen: Vec<(SparkKind, u32)> = Vec::new();
        for (kind, color) in sparks.iter().map(|s| (s.kind, s.color)) {
            if seen.contains(&(kind, color)) {
                continue;
            }
            seen.push((kind, color));
            let lines: Vec<Value> = sparks
                .iter()
                .filter(|s| (s.kind, s.color) == (kind, color))
                .map(|s| {
                    let f = format!(
                        "{quoted}!${}${}:${}${}",
                        sheets_model::col_name(s.data.start.col),
                        s.data.start.row + 1,
                        sheets_model::col_name(s.data.end.col),
                        s.data.end.row + 1
                    );
                    xml::el(
                        "x14:sparkline",
                        &[],
                        vec![
                            xml::el("xm:f", &[], vec![xml::txt(&f)]),
                            xml::el("xm:sqref", &[], vec![xml::txt(&s.cell.to_string())]),
                        ],
                    )
                })
                .collect();
            let mut attrs: Vec<(&str, &str)> = vec![("displayEmptyCellsAs", "gap")];
            match kind {
                SparkKind::Column => attrs.push(("type", "column")),
                SparkKind::WinLoss => attrs.push(("type", "stacked")),
                SparkKind::Line => {}
            }
            let rgb = format!("FF{color:06X}");
            groups.push(xml::el(
                "x14:sparklineGroup",
                &attrs,
                vec![
                    xml::el("x14:colorSeries", &[("rgb", &rgb)], vec![]),
                    xml::el("x14:sparklines", &[], lines),
                ],
            ));
        }
        ext.push(xml::el(
            "ext",
            &[("uri", SPARK_URI), ("xmlns:x14", NS_X14)],
            vec![xml::el(
                "x14:sparklineGroups",
                &[("xmlns:xm", NS_XM)],
                groups,
            )],
        ));
    }
    put(
        tree,
        "extLst",
        if ext.is_empty() {
            Vec::new()
        } else {
            vec![xml::el("extLst", &[], ext)]
        },
    );
}

// ---- protection ----

fn read_protection(tree: &Value, sheet: &mut Sheet) {
    let on = xml::child(tree, "sheetProtection")
        .filter(|p| matches!(xml::attr(p, "sheet"), Some("1" | "true")));
    sheet.set_protection(on.map(|p| Protection {
        password_hash: xml::attr(p, "password").and_then(|h| u16::from_str_radix(h, 16).ok()),
        opaque: xml::attr(p, "hashValue").is_some(),
    }));
}

fn write_protection(tree: &mut Value, sheet: &Sheet) {
    let Some(p) = sheet.protection() else {
        put(tree, "sheetProtection", Vec::new());
        return;
    };
    // What else the file said (which things are still allowed) stays.
    let mut e = xml::child(tree, "sheetProtection")
        .cloned()
        .unwrap_or_else(|| xml::el("sheetProtection", &[], Vec::new()));
    xml::set_attr(&mut e, "sheet", "1");
    if xml::attr(&e, "objects").is_none() {
        xml::set_attr(&mut e, "objects", "1");
        xml::set_attr(&mut e, "scenarios", "1");
    }
    match p.password_hash.filter(|_| !p.opaque) {
        Some(h) => xml::set_attr(&mut e, "password", &format!("{h:04X}")),
        None => xml::remove_attr(&mut e, "password"),
    }
    put(tree, "sheetProtection", vec![e]);
}

/// A filter from an `<autoFilter>` element: its range and the lists of values its columns allow.
pub fn read_filter(f: &Value) -> Option<AutoFilter> {
    let range = xml::attr(f, "ref").and_then(parse_range)?;
    let mut allowed: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for column in xml::elements(f, "filterColumn") {
        let Some(id) = xml::attr(column, "colId").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        // Only lists of values are understood (not "greater than", top ten, colours).
        let Some(list) = xml::child(column, "filters") else {
            continue;
        };
        let mut set: BTreeSet<String> = xml::elements(list, "filter")
            .filter_map(|e| xml::attr(e, "val").map(str::to_string))
            .collect();
        if matches!(xml::attr(list, "blank"), Some("1" | "true")) {
            set.insert(String::new());
        }
        allowed.insert(range.start.col + id, set);
    }
    Some(AutoFilter { range, allowed })
}

/// The `<autoFilter>` element of a filter.
pub fn filter_element(f: &AutoFilter) -> Value {
    let columns: Vec<Value> = f
        .allowed
        .iter()
        .map(|(col, values)| {
            let id = col.saturating_sub(f.range.start.col).to_string();
            let blank = values.contains("");
            let items: Vec<Value> = values
                .iter()
                .filter(|v| !v.is_empty())
                .map(|v| xml::el("filter", &[("val", v)], Vec::new()))
                .collect();
            let attrs: Vec<(&str, &str)> = if blank {
                vec![("blank", "1")]
            } else {
                Vec::new()
            };
            xml::el(
                "filterColumn",
                &[("colId", &id)],
                vec![xml::el("filters", &attrs, items)],
            )
        })
        .collect();
    xml::el("autoFilter", &[("ref", &f.range.to_string())], columns)
}
