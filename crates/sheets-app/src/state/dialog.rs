//! Small forms that ask for the details of a command: data validation, conditional formats, names, notes.

use super::SheetState;
use crate::format::{color_css, parse_color};
use sheets_model::{
    Allow, Compare, Cond, CondRule, Dxf, NumberKind, Orientation, PageSetup, Paper, Protection,
    Range, Severity, Validation,
};

/// How a field is entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// A list of (value, label).
    Choice(Vec<(String, String)>),
    Check,
    Color,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
    pub hint: String,
}

/// Which command a form belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogId {
    ValidList,
    ValidNumber,
    ValidCustom,
    CondCompare,
    CondText,
    CondTop,
    CondExpr,
    CondScale,
    CondBar,
    DefineName,
    DeleteName,
    TableName,
    TableLook,
    Comment,
    PageSetup,
    Chart,
    Sparkline,
    Pivot,
    Protect,
    Unprotect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub id: DialogId,
    pub title: String,
    pub intro: String,
    pub fields: Vec<Field>,
    /// What is wrong with what was entered.
    pub error: String,
}

fn text(label: &str, value: &str, hint: &str) -> Field {
    Field {
        label: crate::i18n::tr(label),
        kind: FieldKind::Text,
        value: value.into(),
        hint: hint.into(),
    }
}

fn check(label: &str, on: bool) -> Field {
    Field {
        label: crate::i18n::tr(label),
        kind: FieldKind::Check,
        value: if on { "1".into() } else { String::new() },
        hint: String::new(),
    }
}

fn color(label: &str, value: u32) -> Field {
    Field {
        label: crate::i18n::tr(label),
        kind: FieldKind::Color,
        value: color_css(value),
        hint: String::new(),
    }
}

fn choice(label: &str, value: &str, options: &[(&str, &str)]) -> Field {
    Field {
        label: crate::i18n::tr(label),
        kind: FieldKind::Choice(
            options
                .iter()
                .map(|(v, l)| ((*v).to_string(), crate::i18n::tr(l)))
                .collect(),
        ),
        value: value.into(),
        hint: String::new(),
    }
}

const COMPARES: [(&str, &str); 8] = [
    ("greaterThan", "greater than"),
    ("lessThan", "less than"),
    ("between", "between"),
    ("equal", "equal to"),
    ("notEqual", "not equal to"),
    ("greaterThanOrEqual", "greater than or equal to"),
    ("lessThanOrEqual", "less than or equal to"),
    ("notBetween", "not between"),
];

/// The look fields every conditional format form ends with.
fn look_fields() -> Vec<Field> {
    vec![
        color("Fill colour", 0x00FF_C7CE),
        color("Text colour", 0x009C_0006),
        check("Bold", false),
    ]
}

fn dxf_from(fields: &[Field], at: usize) -> Dxf {
    let value = |i: usize| {
        fields
            .get(at + i)
            .map(|f| f.value.as_str())
            .unwrap_or_default()
    };
    Dxf {
        fill: parse_color(value(0)),
        color: parse_color(value(1)),
        bold: !value(2).is_empty(),
        italic: false,
    }
}

fn quote_sheet(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

impl SheetState {
    /// The selection written as a reference with its sheet: `Sheet1!$A$1:$B$5`.
    #[must_use]
    pub fn selection_reference(&self) -> String {
        let sel = self.selection();
        let sheet = self
            .sheet_names()
            .get(self.sheet())
            .cloned()
            .unwrap_or_default();
        let one =
            |c: sheets_model::CellRef| format!("${}${}", sheets_model::col_name(c.col), c.row + 1);
        if sel.start == sel.end {
            format!("{}!{}", quote_sheet(&sheet), one(sel.start))
        } else {
            format!(
                "{}!{}:{}",
                quote_sheet(&sheet),
                one(sel.start),
                one(sel.end)
            )
        }
    }

    /// The names that fit the formula being typed (none when it is not a formula or nothing fits).
    #[must_use]
    pub fn suggestion_list(&self) -> Vec<String> {
        if !self.editing.get() {
            return Vec::new();
        }
        let names: Vec<String> = self
            .engine()
            .workbook()
            .names
            .iter()
            .map(|(n, _)| n.clone())
            .collect();
        crate::completion::suggestions(&self.draft.get(), &names, 8)
    }

    /// Take a suggestion into the formula being typed.
    pub fn accept_suggestion(&self, index: usize) {
        let list = self.suggestion_list();
        if let Some(name) = list.get(index.min(list.len().saturating_sub(1))) {
            let next = crate::completion::accept(&self.draft.get_untracked(), name);
            quench_ui::prelude::batch(|| {
                self.draft.set(next);
                self.suggest_index.set(0);
            });
        }
    }

    /// Jump to a cell, a range, or a defined name typed in the name box. Returns whether it was found.
    pub fn go_to(&self, text: &str) -> bool {
        let text = text.trim();
        let (sheet_part, cell_part) = text
            .rsplit_once('!')
            .map_or((None, text), |(a, b)| (Some(a.trim_matches('\'')), b));
        let reference = |t: &str| {
            let t = t.replace('$', "");
            Range::parse(&t).or_else(|| sheets_model::CellRef::parse(&t).map(|c| Range::new(c, c)))
        };
        let mut target: Option<(Option<String>, Range)> =
            reference(cell_part).map(|r| (sheet_part.map(str::to_string), r));
        if target.is_none() {
            // A defined name that stands for a cell or a range.
            target = self.engine().name_expr(text).and_then(|e| match e {
                sheets_formula::Expr::Ref(r) => Some((r.sheet, Range::new(r.cell, r.cell))),
                sheets_formula::Expr::Range(r) => Some((
                    r.sheet.or(r.start.sheet),
                    Range::new(r.start.cell, r.end.cell),
                )),
                _ => None,
            });
        }
        let Some((sheet, range)) = target else {
            self.status.set(format!("Cannot find {text}"));
            return false;
        };
        if let Some(name) = sheet {
            match self
                .sheet_names()
                .iter()
                .position(|n| n.eq_ignore_ascii_case(&name))
            {
                Some(i) if i != self.sheet() => self.switch_sheet(i),
                Some(_) => {}
                None => {
                    self.status.set(format!("There is no sheet called {name}"));
                    return false;
                }
            }
        }
        self.select_range(range);
        true
    }

    /// Open the form for a command.
    pub fn open_dialog(&self, id: DialogId) {
        let existing = self
            .engine()
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(|s| s.validation_at(self.selected.get_untracked()).cloned());
        let dialog = match id {
            DialogId::ValidList => {
                let items = match existing.as_ref().map(|v| &v.allow) {
                    Some(Allow::List(items)) => items.join(", "),
                    Some(Allow::ListFrom(r)) => r.clone(),
                    _ => String::new(),
                };
                Dialog {
                    id,
                    title: crate::i18n::tr("Drop-down list"),
                    intro: crate::i18n::tr(
                        "Allow only the items listed. Type them separated by commas, or give a range of cells such as Sheet1!$A$1:$A$9 or a name.",
                    ),
                    fields: vec![
                        text("Items", &items, "red, green, blue"),
                        check(
                            "Show an arrow in the cell",
                            existing.as_ref().is_none_or(|v| v.dropdown),
                        ),
                        check(
                            "Ignore blank cells",
                            existing.as_ref().is_none_or(|v| v.allow_blank),
                        ),
                        choice(
                            "When something else is entered",
                            "stop",
                            &[
                                ("stop", "Refuse it"),
                                ("warning", "Warn but keep it"),
                                ("information", "Tell, but keep it"),
                            ],
                        ),
                        text(
                            "Message",
                            existing.as_ref().map_or("", |v| v.message.as_str()),
                            "Pick one of the items",
                        ),
                    ],
                    error: String::new(),
                }
            }
            DialogId::ValidNumber => Dialog {
                id,
                title: crate::i18n::tr("Number rule"),
                intro: crate::i18n::tr(
                    "Allow only numbers, dates or texts of a length that fit. A bound may be a value or a formula.",
                ),
                fields: vec![
                    choice(
                        "Allow",
                        "whole",
                        &[
                            ("whole", "Whole numbers"),
                            ("decimal", "Decimal numbers"),
                            ("date", "Dates"),
                            ("time", "Times"),
                            ("length", "Text of a length"),
                        ],
                    ),
                    choice("Which are", "between", &COMPARES),
                    text("Value (or minimum)", "", "1"),
                    text("Maximum (for between)", "", "10"),
                    choice(
                        "When something else is entered",
                        "stop",
                        &[
                            ("stop", "Refuse it"),
                            ("warning", "Warn but keep it"),
                            ("information", "Tell, but keep it"),
                        ],
                    ),
                    text("Message", "", "Enter a number from 1 to 10"),
                ],
                error: String::new(),
            },
            DialogId::ValidCustom => Dialog {
                id,
                title: crate::i18n::tr("Formula rule"),
                intro: crate::i18n::tr(
                    "Allow a value when the formula is true. Write it for the first cell of the selection, such as =COUNTIF($A:$A,A1)=1.",
                ),
                fields: vec![text("Formula", "", "=A1>0"), text("Message", "", "")],
                error: String::new(),
            },
            DialogId::CondCompare => {
                let mut fields = vec![
                    choice("Highlight cells that are", "greaterThan", &COMPARES),
                    text("Value (or minimum)", "", "100"),
                    text("Maximum (for between)", "", ""),
                ];
                fields.extend(look_fields());
                Dialog {
                    id,
                    title: crate::i18n::tr("Highlight cells"),
                    intro: crate::i18n::tr(
                        "Format the cells whose number compares as chosen. A value may be a formula.",
                    ),
                    fields,
                    error: String::new(),
                }
            }
            DialogId::CondText => {
                let mut fields = vec![
                    choice(
                        "Highlight cells that",
                        "contains",
                        &[
                            ("contains", "contain"),
                            ("notContains", "do not contain"),
                            ("begins", "begin with"),
                            ("ends", "end with"),
                        ],
                    ),
                    text("Text", "", ""),
                ];
                fields.extend(look_fields());
                Dialog {
                    id,
                    title: crate::i18n::tr("Highlight text"),
                    intro: String::new(),
                    fields,
                    error: String::new(),
                }
            }
            DialogId::CondTop => {
                let mut fields = vec![
                    choice(
                        "Highlight the",
                        "top",
                        &[("top", "largest"), ("bottom", "smallest")],
                    ),
                    text("How many", "10", "10"),
                    check("As a percent of the cells", false),
                ];
                fields.extend(look_fields());
                Dialog {
                    id,
                    title: crate::i18n::tr("Largest or smallest"),
                    intro: String::new(),
                    fields,
                    error: String::new(),
                }
            }
            DialogId::CondExpr => {
                let mut fields = vec![text("Formula", "", "=$B1>100")];
                fields.extend(look_fields());
                Dialog {
                    id,
                    title: crate::i18n::tr("Formula format"),
                    intro: crate::i18n::tr(
                        "Format a cell when the formula is true. Write it for the first cell of the selection; use $ to keep a column or row fixed.",
                    ),
                    fields,
                    error: String::new(),
                }
            }
            DialogId::CondScale => Dialog {
                id,
                title: crate::i18n::tr("Colour scale"),
                intro: crate::i18n::tr(
                    "Shade each cell by where its number falls between the smallest and the largest.",
                ),
                fields: vec![
                    choice(
                        "Colours",
                        "3",
                        &[("2", "Two colours"), ("3", "Three colours")],
                    ),
                    color("Smallest", 0x00F8_696B),
                    color("Middle (three colours)", 0x00FF_EB84),
                    color("Largest", 0x0063_BE7B),
                ],
                error: String::new(),
            },
            DialogId::CondBar => Dialog {
                id,
                title: crate::i18n::tr("Data bars"),
                intro: crate::i18n::tr("Draw a bar in each cell as long as its number is large."),
                fields: vec![color("Bar colour", 0x0063_8EC6)],
                error: String::new(),
            },
            DialogId::DefineName => Dialog {
                id,
                title: crate::i18n::tr("Define a name"),
                intro: crate::i18n::tr(
                    "A name stands for cells or a value in formulas, such as =SUM(Sales).",
                ),
                fields: vec![
                    text("Name", "", "Sales"),
                    text("Refers to", &self.selection_reference(), "Sheet1!$A$1:$A$9"),
                ],
                error: String::new(),
            },
            DialogId::TableName => {
                let Some(name) = self.table_name() else {
                    self.status
                        .set(crate::i18n::tr("The cursor is not in a table"));
                    return;
                };
                Dialog {
                    id,
                    title: crate::i18n::tr("Table name"),
                    intro: crate::i18n::tr(
                        "Letters, digits and underscores, starting with a letter.",
                    ),
                    fields: vec![text("Name", &name, "Table1")],
                    error: String::new(),
                }
            }
            DialogId::TableLook => {
                let Some(i) = self.table_at_cursor() else {
                    self.status
                        .set(crate::i18n::tr("The cursor is not in a table"));
                    return;
                };
                let current = self
                    .engine_tables()
                    .get(i)
                    .and_then(|t| crate::state::table_palette(&t.style));
                let (head, text, band) = current.unwrap_or((0x005B_6B7F, 0x00FF_FFFF, 0x00E8_ECF1));
                Dialog {
                    id,
                    title: crate::i18n::tr("Table look"),
                    intro: crate::i18n::tr("Colours of the heading, its text and every other row."),
                    fields: vec![
                        color("Heading", head),
                        color("Heading text", text),
                        color("Tinted rows", band),
                    ],
                    error: String::new(),
                }
            }
            DialogId::DeleteName => {
                let names: Vec<(String, String)> = self
                    .engine()
                    .workbook()
                    .names
                    .iter()
                    .map(|(n, r)| (n.clone(), format!("{n}  ({r})")))
                    .collect();
                if names.is_empty() {
                    self.status
                        .set(crate::i18n::tr("There are no names to delete"));
                    return;
                }
                let first = names[0].0.clone();
                Dialog {
                    id,
                    title: crate::i18n::tr("Delete a name"),
                    intro: crate::i18n::tr("Formulas that use a deleted name show #NAME?."),
                    fields: vec![Field {
                        label: "Name".into(),
                        kind: FieldKind::Choice(names),
                        value: first,
                        hint: String::new(),
                    }],
                    error: String::new(),
                }
            }
            DialogId::Chart => {
                self.new_chart_dialog(sheets_model::ChartKind::Column);
                return;
            }
            DialogId::Sparkline => {
                self.new_sparkline_dialog(sheets_model::SparkKind::Line);
                return;
            }
            DialogId::Pivot => {
                self.open_pivot_dialog();
                return;
            }
            DialogId::Protect => Dialog {
                id,
                title: crate::i18n::tr("Protect the sheet"),
                intro: crate::i18n::tr(
                    "Locked cells cannot be changed, and rows, columns, merges and the like cannot be rearranged. All cells are locked until you unlock some (Lock or unlock under Protect). A password keeps others from lifting it; leave it empty for none. The password is a convenience, not a secret.",
                ),
                fields: vec![text("Password", "", ""), text("Type it again", "", "")],
                error: String::new(),
            },
            DialogId::Unprotect => {
                if self.protection().is_none() {
                    self.status
                        .set(crate::i18n::tr("The sheet is not protected"));
                    return;
                }
                if self
                    .protection()
                    .is_some_and(|p| p.password_hash.is_none() && !p.opaque)
                {
                    let sheet = self.sheet();
                    self.whole_book_step_raw(|e| {
                        e.update_sheet(sheet, |s| s.set_protection(None)).is_some()
                    });
                    self.status
                        .set(crate::i18n::tr("The sheet is not protected any more"));
                    return;
                }
                Dialog {
                    id,
                    title: crate::i18n::tr("Unprotect the sheet"),
                    intro: String::new(),
                    fields: vec![text("Password", "", "")],
                    error: String::new(),
                }
            }
            DialogId::PageSetup => {
                let page = self
                    .engine()
                    .workbook()
                    .sheets
                    .get(self.sheet())
                    .map(|s| s.page().clone())
                    .unwrap_or_default();
                let range_text = |r: Range| {
                    if r.start == r.end {
                        r.start.to_string()
                    } else {
                        r.to_string()
                    }
                };
                Dialog {
                    id,
                    title: crate::i18n::tr("Page setup"),
                    intro: crate::i18n::tr(
                        "How this sheet is laid out when it is printed or exported as a PDF.",
                    ),
                    fields: vec![
                        choice(
                            "Paper",
                            if page.paper == Paper::Letter {
                                "letter"
                            } else {
                                "a4"
                            },
                            &[("a4", "A4"), ("letter", "US Letter")],
                        ),
                        choice(
                            "Orientation",
                            match page.orientation {
                                Orientation::Auto => "auto",
                                Orientation::Portrait => "portrait",
                                Orientation::Landscape => "landscape",
                            },
                            &[
                                ("auto", "Wide sheets turn sideways"),
                                ("portrait", "Portrait"),
                                ("landscape", "Landscape"),
                            ],
                        ),
                        text("Margins (mm)", &page.margin_mm.to_string(), "13"),
                        check("Shrink to fit the page width", page.fit_width),
                        check("Print the lines between cells", page.gridlines),
                        text(
                            "Repeat rows on every page",
                            &page
                                .repeat_rows
                                .map_or(String::new(), |(a, b)| format!("{}:{}", a + 1, b + 1)),
                            "1:1",
                        ),
                        text(
                            "Print only this area",
                            &page.print_area.map_or(String::new(), range_text),
                            "A1:H40",
                        ),
                    ],
                    error: String::new(),
                }
            }
            DialogId::Comment => {
                let at = self.selected.get_untracked();
                let note = self
                    .engine()
                    .workbook()
                    .sheets
                    .get(self.sheet())
                    .and_then(|s| s.comment(at).map(str::to_string))
                    .unwrap_or_default();
                Dialog {
                    id,
                    title: format!("Note on {at}"),
                    intro: crate::i18n::tr("Leave empty to remove the note."),
                    fields: vec![text("Note", &note, "")],
                    error: String::new(),
                }
            }
        };
        self.dialog.set(Some(dialog));
    }

    pub fn set_field(&self, index: usize, value: &str) {
        self.dialog.update(|d| {
            if let Some(f) = d.as_mut().and_then(|d| d.fields.get_mut(index)) {
                f.value = value.to_string();
            }
        });
    }

    pub fn toggle_field(&self, index: usize) {
        self.dialog.update(|d| {
            if let Some(f) = d.as_mut().and_then(|d| d.fields.get_mut(index)) {
                f.value = if f.value.is_empty() {
                    "1".into()
                } else {
                    String::new()
                };
            }
        });
    }

    pub fn cancel_dialog(&self) {
        self.dialog.set(None);
        self.refocus();
    }

    /// Apply the form. A problem with what was entered is shown in the form, which stays open.
    pub fn submit_dialog(&self) {
        let Some(d) = self.dialog.get_untracked() else {
            return;
        };
        let f = |i: usize| {
            d.fields
                .get(i)
                .map(|x| x.value.trim().to_string())
                .unwrap_or_default()
        };
        let on = |i: usize| d.fields.get(i).is_some_and(|x| !x.value.is_empty());
        let range = self.selection();
        let result: Result<String, String> = match d.id {
            DialogId::ValidList => {
                let items = f(0);
                if items.is_empty() {
                    Err(crate::i18n::tr(
                        "Enter the items, or a range of cells that holds them",
                    ))
                } else {
                    let looks_like_ref = items.contains('!')
                        || Range::parse(items.trim_start_matches('=')).is_some()
                        || self.engine().workbook().name(&items).is_some();
                    let allow = if looks_like_ref {
                        Allow::ListFrom(items.trim_start_matches('=').to_string())
                    } else {
                        Allow::List(
                            items
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect(),
                        )
                    };
                    self.add_validation(Validation {
                        range,
                        allow,
                        allow_blank: on(2),
                        dropdown: on(1),
                        severity: severity_of(&f(3)),
                        title: String::new(),
                        message: f(4),
                    })
                }
            }
            DialogId::ValidNumber => {
                let kind = match f(0).as_str() {
                    "decimal" => NumberKind::Decimal,
                    "date" => NumberKind::Date,
                    "time" => NumberKind::Time,
                    "length" => NumberKind::Length,
                    _ => NumberKind::Whole,
                };
                let op = Compare::from_name(&f(1)).unwrap_or(Compare::Between);
                if f(2).is_empty()
                    || (matches!(op, Compare::Between | Compare::NotBetween) && f(3).is_empty())
                {
                    Err(crate::i18n::tr(
                        "Enter the value (and the maximum for between)",
                    ))
                } else {
                    self.add_validation(Validation {
                        range,
                        allow: Allow::Number {
                            kind,
                            op,
                            a: f(2),
                            b: f(3),
                        },
                        allow_blank: true,
                        dropdown: true,
                        severity: severity_of(&f(4)),
                        title: String::new(),
                        message: f(5),
                    })
                }
            }
            DialogId::ValidCustom => {
                let formula = f(0).trim_start_matches('=').to_string();
                if sheets_formula::parse(&formula).is_err() {
                    Err(crate::i18n::tr("That formula cannot be read"))
                } else {
                    self.add_validation(Validation {
                        range,
                        allow: Allow::Custom(formula),
                        allow_blank: true,
                        dropdown: true,
                        severity: Severity::Stop,
                        title: String::new(),
                        message: f(1),
                    })
                }
            }
            DialogId::CondCompare => {
                let op = Compare::from_name(&f(0)).unwrap_or(Compare::Greater);
                if f(1).is_empty() {
                    Err(crate::i18n::tr("Enter a value"))
                } else {
                    self.add_cond(
                        range,
                        Cond::CellIs {
                            op,
                            a: f(1),
                            b: f(2),
                        },
                        dxf_from(&d.fields, 3),
                    );
                    Ok("Added a highlight rule".to_string())
                }
            }
            DialogId::CondText => {
                let t = f(1);
                if t.is_empty() {
                    Err(crate::i18n::tr("Enter the text to look for"))
                } else {
                    let cond = match f(0).as_str() {
                        "notContains" => Cond::NotContains(t),
                        "begins" => Cond::BeginsWith(t),
                        "ends" => Cond::EndsWith(t),
                        _ => Cond::Contains(t),
                    };
                    self.add_cond(range, cond, dxf_from(&d.fields, 2));
                    Ok("Added a highlight rule".to_string())
                }
            }
            DialogId::CondTop => match f(1).parse::<u32>() {
                Ok(n) if n > 0 => {
                    self.add_cond(
                        range,
                        Cond::Top {
                            n,
                            bottom: f(0) == "bottom",
                            percent: on(2),
                        },
                        dxf_from(&d.fields, 3),
                    );
                    Ok("Added a highlight rule".to_string())
                }
                _ => Err(crate::i18n::tr("Enter how many, as a whole number")),
            },
            DialogId::CondExpr => {
                let formula = f(0).trim_start_matches('=').to_string();
                if sheets_formula::parse(&formula).is_err() {
                    Err(crate::i18n::tr("That formula cannot be read"))
                } else {
                    self.add_cond(range, Cond::Expression(formula), dxf_from(&d.fields, 1));
                    Ok("Added a formula rule".to_string())
                }
            }
            DialogId::CondScale => {
                let c = |i: usize| parse_color(&f(i)).unwrap_or(0);
                let cond = Cond::ColorScale {
                    low: c(1),
                    mid: (f(0) == "3").then(|| c(2)),
                    high: c(3),
                };
                self.add_cond(range, cond, Dxf::default());
                Ok("Added a colour scale".to_string())
            }
            DialogId::CondBar => {
                self.add_cond(
                    range,
                    Cond::DataBar {
                        color: parse_color(&f(0)).unwrap_or(0x0063_8EC6),
                    },
                    Dxf::default(),
                );
                Ok("Added data bars".to_string())
            }
            DialogId::DefineName => {
                if self
                    .doc
                    .borrow_mut()
                    .engine
                    .define_name(&f(0), f(1).trim_start_matches('='))
                {
                    self.bump();
                    Ok(format!("Defined {}", f(0)))
                } else {
                    Err(crate::i18n::tr(
                        "A name starts with a letter, has no spaces, and is not a cell address; what it refers to must be a formula or a range",
                    ))
                }
            }
            DialogId::TableLook => {
                let c = |i: usize, d: u32| parse_color(&f(i)).unwrap_or(d);
                let style = sheets_model::custom_table_style(
                    c(0, 0x005B_6B7F),
                    c(1, 0x00FF_FFFF),
                    c(2, 0x00E8_ECF1),
                );
                self.set_table_style(&style);
                Ok("Changed the look of the table".to_string())
            }
            DialogId::TableName => self
                .rename_table(&f(0))
                .map(|()| format!("Renamed to {}", f(0).trim())),
            DialogId::DeleteName => {
                self.doc_mut().engine.define_name(&f(0), "");
                self.bump();
                Ok(format!("Deleted {}", f(0)))
            }
            DialogId::Sparkline => {
                let values: Vec<String> = d.fields.iter().map(|x| x.value.clone()).collect();
                self.submit_sparkline(&values)
            }
            DialogId::Pivot => {
                let values: Vec<String> = d.fields.iter().map(|x| x.value.clone()).collect();
                self.submit_pivot(&values)
            }
            DialogId::Protect => {
                let (password, again) = (f(0), f(1));
                if password == again {
                    let sheet = self.sheet();
                    let protection = Protection {
                        password_hash: (!password.is_empty()).then(|| Protection::hash(&password)),
                        opaque: false,
                    };
                    self.whole_book_step_raw(|e| {
                        e.update_sheet(sheet, |s| s.set_protection(Some(protection)))
                            .is_some()
                    });
                    Ok("The sheet is protected".to_string())
                } else {
                    Err(crate::i18n::tr("The two passwords are not the same"))
                }
            }
            DialogId::Unprotect => {
                let sheet = self.sheet();
                match self.protection() {
                    Some(p) if p.accepts(&f(0)) => {
                        self.whole_book_step_raw(|e| {
                            e.update_sheet(sheet, |s| s.set_protection(None)).is_some()
                        });
                        Ok("The sheet is not protected any more".to_string())
                    }
                    Some(p) if p.opaque => Err(crate::i18n::tr(
                        "This sheet was protected by another program with a password this one cannot check; unprotect it there",
                    )),
                    Some(_) => Err(crate::i18n::tr("That is not the password")),
                    None => Ok("The sheet was not protected".to_string()),
                }
            }
            DialogId::Chart => {
                let values: Vec<String> = d.fields.iter().map(|x| x.value.clone()).collect();
                self.submit_chart(&values)
            }
            DialogId::PageSetup => {
                let margin = f(2).parse::<u32>();
                let rows = f(5);
                let repeat = if rows.is_empty() {
                    Ok(None)
                } else {
                    rows.split_once(':')
                        .or(Some((rows.as_str(), rows.as_str())))
                        .and_then(|(a, b)| {
                            Some((
                                a.trim().parse::<u32>().ok()?.checked_sub(1)?,
                                b.trim().parse::<u32>().ok()?.checked_sub(1)?,
                            ))
                        })
                        .filter(|(a, b)| a <= b)
                        .map(Some)
                        .ok_or_else(|| "Rows to repeat are written like 1:2".to_string())
                };
                let area_text = f(6);
                let area = if area_text.is_empty() {
                    Ok(None)
                } else {
                    let t = area_text.replace('$', "");
                    Range::parse(&t)
                        .or_else(|| sheets_model::CellRef::parse(&t).map(|c| Range::new(c, c)))
                        .map(Some)
                        .ok_or_else(|| "The print area is written like A1:H40".to_string())
                };
                match (margin, repeat, area) {
                    (Ok(m), Ok(repeat_rows), Ok(print_area)) if m <= 100 => {
                        let page = PageSetup {
                            paper: if f(0) == "letter" {
                                Paper::Letter
                            } else {
                                Paper::A4
                            },
                            orientation: match f(1).as_str() {
                                "portrait" => Orientation::Portrait,
                                "landscape" => Orientation::Landscape,
                                _ => Orientation::Auto,
                            },
                            margin_mm: m,
                            fit_width: on(3),
                            gridlines: on(4),
                            repeat_rows,
                            print_area,
                        };
                        let sheet = self.sheet();
                        self.whole_book_step(|e| {
                            e.update_sheet(sheet, |s| s.set_page(page)).is_some()
                        });
                        Ok("Page setup saved".to_string())
                    }
                    (Ok(_), Ok(_), Ok(_)) | (Err(_), ..) => Err(crate::i18n::tr(
                        "Margins are a number of millimetres from 0 to 100",
                    )),
                    (_, Err(e), _) | (_, _, Err(e)) => Err(e),
                }
            }
            DialogId::Comment => {
                let at = self.selected.get_untracked();
                let note = d
                    .fields
                    .first()
                    .map(|x| x.value.clone())
                    .unwrap_or_default();
                let sheet = self.sheet();
                self.whole_book_step(|e| {
                    e.update_sheet(sheet, |s| s.set_comment(at, note.trim()))
                        .is_some()
                });
                Ok("Note saved".to_string())
            }
        };
        match result {
            Ok(message) => {
                self.dialog.set(None);
                self.status.set(message);
                self.refocus();
            }
            Err(error) => self.dialog.update(|d| {
                if let Some(d) = d {
                    d.error = error;
                }
            }),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn add_validation(&self, v: Validation) -> Result<String, String> {
        let sheet = self.sheet();
        let range = v.range;
        self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut all: Vec<Validation> = s
                    .validations()
                    .iter()
                    .filter(|x| !overlaps(&x.range, &range))
                    .cloned()
                    .collect();
                all.push(v);
                s.set_validations(all);
            })
            .is_some()
        });
        Ok("Rule added".to_string())
    }

    /// Add a conditional format over a range; it takes priority over the ones there already.
    pub fn add_cond(&self, range: Range, cond: Cond, dxf: Dxf) {
        let sheet = self.sheet();
        self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let mut all: Vec<CondRule> = s
                    .cond_rules()
                    .iter()
                    .cloned()
                    .map(|mut r| {
                        r.priority += 1;
                        r
                    })
                    .collect();
                all.push(CondRule {
                    range,
                    cond,
                    dxf,
                    priority: 1,
                    stop_if_true: false,
                });
                s.set_cond_rules(all);
            })
            .is_some()
        });
    }

    /// Conditional formats that need no questions, in the usual red.
    pub fn quick_cond(&self, cond: Cond) {
        self.add_cond(
            self.selection(),
            cond,
            Dxf {
                fill: Some(0x00FF_C7CE),
                color: Some(0x009C_0006),
                bold: false,
                italic: false,
            },
        );
        self.status.set(crate::i18n::tr("Added a rule"));
    }

    /// Remove the data validation from the selected cells.
    pub fn clear_validation(&self) {
        let (sheet, range) = (self.sheet(), self.selection());
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let before = s.validations().len();
                let kept: Vec<Validation> = s
                    .validations()
                    .iter()
                    .filter(|v| !overlaps(&v.range, &range))
                    .cloned()
                    .collect();
                let changed = kept.len() != before;
                s.set_validations(kept);
                changed
            })
            .unwrap_or(false)
        }) {
            self.status
                .set(crate::i18n::tr("Removed the validation rules"));
        }
    }

    /// Remove the conditional formats that touch the selection (`all`: those of the whole sheet).
    pub fn clear_cond(&self, all: bool) {
        let (sheet, range) = (self.sheet(), self.selection());
        if self.whole_book_step(|e| {
            e.update_sheet(sheet, |s| {
                let before = s.cond_rules().len() + s.cond_raw().len();
                let kept: Vec<CondRule> = if all {
                    Vec::new()
                } else {
                    s.cond_rules()
                        .iter()
                        .filter(|r| !overlaps(&r.range, &range))
                        .cloned()
                        .collect()
                };
                if all {
                    s.set_cond_raw(Vec::new());
                }
                let changed = kept.len() + s.cond_raw().len() != before;
                s.set_cond_rules(kept);
                changed
            })
            .unwrap_or(false)
        }) {
            self.status.set(crate::i18n::tr("Removed the rules"));
        }
    }

    // ---- protection ----

    /// How the sheet on screen is protected, if it is.
    #[must_use]
    pub fn protection(&self) -> Option<Protection> {
        self.doc
            .borrow()
            .engine
            .workbook()
            .sheets
            .get(self.sheet())
            .and_then(sheets_model::Sheet::protection)
    }

    /// Whether any of these cells is locked on a protected sheet (so a change to it is refused).
    #[must_use]
    pub fn protected_cells(
        &self,
        sheet: usize,
        cells: impl Iterator<Item = sheets_model::CellRef>,
    ) -> bool {
        let e = self.engine();
        if e.workbook()
            .sheets
            .get(sheet)
            .and_then(sheets_model::Sheet::protection)
            .is_none()
        {
            return false;
        }
        cells.into_iter().any(|at| {
            e.workbook()
                .styles
                .get(e.style(sheet, at) as usize)
                .is_none_or(|s| s.locked)
        })
    }

    /// Lock or unlock the selected cells (it matters once the sheet is protected).
    pub fn set_locked(&self, locked: bool) {
        self.format_selection(move |s| s.locked = locked);
        self.status.set(if locked {
            "Locked the selected cells".to_string()
        } else {
            "Unlocked the selected cells: they can be changed when the sheet is protected"
                .to_string()
        });
    }

    // ---- what a validated cell offers ----

    /// The choices of the list rule on the active cell, when it has one that shows an arrow.
    #[must_use]
    pub fn dropdown_items(&self) -> Option<Vec<String>> {
        let at = self.selected.get_untracked();
        let e = self.engine();
        let sheet = self.sheet();
        let v = e.workbook().sheets.get(sheet)?.validation_at(at)?;
        if !v.dropdown {
            return None;
        }
        crate::rules::list_items(&e, sheet, v)
    }

    /// Check what the cell holds against its rule after an entry. A refused entry is taken back. Returns the message when the
    /// entry broke a rule.
    pub(crate) fn enforce_validation(
        &self,
        at: sheets_model::CellRef,
    ) -> Option<(Severity, String)> {
        let e = self.engine();
        let sheet = self.sheet();
        let v = e.workbook().sheets.get(sheet)?.validation_at(at)?.clone();
        crate::rules::check(&e, sheet, at, &v)
            .err()
            .map(|m| (v.severity, m))
    }
}

fn severity_of(text: &str) -> Severity {
    match text {
        "warning" => Severity::Warning,
        "information" => Severity::Information,
        _ => Severity::Stop,
    }
}

fn overlaps(a: &Range, b: &Range) -> bool {
    a.start.row <= b.end.row
        && b.start.row <= a.end.row
        && a.start.col <= b.end.col
        && b.start.col <= a.end.col
}
