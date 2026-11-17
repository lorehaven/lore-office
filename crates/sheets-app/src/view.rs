//! The screen: formula bar, headers, and a grid that builds only the cells in view.
//!
//! The grid is a native scroll area over a box as big as the sheet. Inside it, one CSS grid holds the visible rows and
//! columns, moved into place with a transform; cells are keyed by address, so scrolling by a row builds one row of cells
//! and drops one, and the rest keep their elements. Columns and rows may have their own sizes: where each starts is a sum
//! over the few that differ (`Axis`), and the grid's tracks are listed for the cells in view only.
//!
//! A small invisible element takes keyboard focus at the moving end of the selection, which is also what makes the browser
//! scroll it into view when the selection moves.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use crate::axis::Axes;
use crate::format::{PRESETS, color_css, parse_color};
use crate::state::{BorderMode, COLS, DialogId, FieldKind, PasteMode, ROW_H, ROWS, SheetState};
use office_ui::MenuItem;
use quench_ui::event::prevent_default;
use quench_ui::prelude::*;
use quench_ui::set_clipboard;
use sheets_model::{Align, Axis as Dim, CellRef, Range, Style, col_name};
use sheets_model::{ChartKind, Cond, SparkKind};
use std::rc::Rc;

/// Rows and columns built beyond the edge of the view, so a short scroll shows no gap.
const OVERSCAN: u32 = 2;

type Window = (u32, u32, u32, u32);

fn key_of(at: CellRef) -> u64 {
    u64::from(at.row) << 20 | u64::from(at.col)
}

/// The track list of a CSS grid for items `from..to`: `96px 120px ...`.
fn tracks(from: u32, to: u32, size: impl Fn(u32) -> f64) -> String {
    (from..to)
        .map(|i| format!("{}px", size(i)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The whole screen for a sheet. It is drawn again when the language changes, so every label follows.
#[must_use]
pub fn sheet_view(state: &SheetState) -> View {
    let state = state.clone();
    View::dynamic(move || {
        let _ = state.language.get();
        sheet_screen(&state)
    })
}

fn sheet_screen(state: &SheetState) -> View {
    let scroll_top = create_signal(0.0_f64);
    let scroll_left = create_signal(0.0_f64);
    let viewport = create_signal((800.0_f64, 480.0_f64));

    // The cells to build: changes only when the first or last row or column in view does.
    let window: Memo<Window> = {
        let (top, left, vp, axes) = (
            scroll_top.clone(),
            scroll_left.clone(),
            viewport.clone(),
            state.axes.clone(),
        );
        create_memo(move || {
            let (w, h) = vp.get();
            let (top, left) = (top.get().max(0.0), left.get().max(0.0));
            let a: Rc<Axes> = axes.get();
            let r0 = a.rows.index_at(top).saturating_sub(OVERSCAN);
            let r1 = (a.rows.index_at(top + h) + 1 + OVERSCAN).min(ROWS);
            let c0 = a.cols.index_at(left).saturating_sub(OVERSCAN);
            let c1 = (a.cols.index_at(left + w) + 1 + OVERSCAN).min(COLS);
            (r0, r1.max(r0), c0, c1.max(c0))
        })
    };
    {
        let (vp, pages) = (viewport.clone(), state.page_rows.clone());
        create_effect(move || pages.set(((vp.get().1 / ROW_H) as u32).max(1)));
    }

    // The grid is drawn zoomed (`zoom` in the style sheet), so what the browser reports is bigger than the sheet's pixels: the
    // scroll position and the size of the view are kept in the sheet's, and are scaled when the zoom changes.
    {
        let (zoom, top, left, vp) = (
            state.zoom.clone(),
            scroll_top.clone(),
            scroll_left.clone(),
            viewport.clone(),
        );
        let before = std::rc::Rc::new(std::cell::Cell::new(100_u32));
        create_effect(move || {
            let now = zoom.get();
            let was = before.replace(now);
            if was != now {
                let k = f64::from(was) / f64::from(now);
                batch(|| {
                    top.set(top.get_untracked() * k);
                    left.set(left.get_untracked() * k);
                    let (w, h) = vp.get_untracked();
                    vp.set((w * k, h * k));
                });
            }
        });
    }
    let shown_zoom = state.zoom.clone();
    // A border being dragged is followed wherever the pointer goes: its moves bubble up to here.
    let (moving, releasing, dragged) = (state.clone(), state.clone(), state.clone());
    let (drag_top, drag_left, drag_vp, drag_axes) = (
        scroll_top.clone(),
        scroll_left.clone(),
        viewport.clone(),
        state.axes.clone(),
    );
    div()
        .class("sheet")
        .css("--z", move || {
            format!("{}", f64::from(shown_zoom.get()) / 100.0)
        })
        .child(office_cloud_ui::overlay())
        .on(EventKind::PointerMove, move |e| {
            if let Event::PointerMove(p) = e {
                moving.resize_pointer(p.x, p.y);
                moving.chart_pointer(p.x, p.y);
                // A selection being dragged past the edge of the grid goes on growing, and the grid scrolls with it.
                dragged.drag_moved(|at| {
                    let a: Rc<Axes> = drag_axes.get_untracked();
                    let (w, h) = drag_vp.get_untracked();
                    let (top, left) = (drag_top.get_untracked(), drag_left.get_untracked());
                    let (first_row, last_row) = (a.rows.index_at(top), a.rows.index_at(top + h));
                    let (first_col, last_col) = (a.cols.index_at(left), a.cols.index_at(left + w));
                    let dir = |at: u32, first: u32, last: u32, scrolled: bool| {
                        if at + 1 >= last {
                            1
                        } else if at <= first && scrolled {
                            -1
                        } else {
                            0
                        }
                    };
                    (
                        dir(at.row, first_row, last_row, top > 0.0),
                        dir(at.col, first_col, last_col, left > 0.0),
                    )
                });
            }
        })
        .on(EventKind::PointerUp, move |_| {
            releasing.pointer_up();
            releasing.end_resize();
        })
        .child(ribbon(state))
        .child(formula_bar(state))
        .child(div().class("corner"))
        .child(column_headers(state, &window, &scroll_left))
        .child(row_headers(state, &window, &scroll_top))
        .child(grid(state, &window, &scroll_top, &scroll_left, &viewport))
        .child(tabs(state))
        .child(dialog_layer(state))
        .child(context_menu(state))
        .into_view()
}

/// The menu of the secondary button on cells: what can be done with the selected cells.
fn context_menu(state: &SheetState) -> View {
    let st = state.clone();
    office_ui::context_menu(state.context.clone(), move || cell_menu(&st))
}

/// What the secondary button on a row or column heading offers.
fn header_menu(state: &SheetState, row: bool) -> Vec<MenuItem> {
    let act = |label: &'static str, hint: &'static str, f: fn(&SheetState)| {
        let st = state.clone();
        MenuItem::action(label, hint, move || f(&st))
    };
    let dim = if row { Dim::Row } else { Dim::Col };
    let (insert, delete, hide, show) = if row {
        (
            "Rows above",
            "Selected rows",
            "Hide rows",
            "Show hidden rows",
        )
    } else {
        (
            "Columns to the left",
            "Selected columns",
            "Hide columns",
            "Show hidden columns",
        )
    };
    let st = |f: fn(&SheetState, Dim)| {
        let s = state.clone();
        move || f(&s, dim)
    };
    vec![
        act("Cut", "Ctrl+X", |s| {
            let _ = s.copy(true);
        }),
        act("Copy", "Ctrl+C", |s| {
            let _ = s.copy(false);
        }),
        act("Paste", "Ctrl+V", |s| {
            if let Some(text) = s.clip_text() {
                s.paste(&text);
            }
        }),
        MenuItem::Separator,
        MenuItem::sub(
            "Insert",
            vec![MenuItem::action(
                insert,
                "",
                st(SheetState::insert_selected),
            )],
        ),
        MenuItem::sub(
            "Delete",
            vec![MenuItem::action(
                delete,
                "",
                st(SheetState::delete_selected),
            )],
        ),
        act("Clear contents", "Delete", SheetState::clear_selected),
        act("Clear formatting", "", SheetState::clear_formatting),
        MenuItem::Separator,
        MenuItem::action(hide, "", st(SheetState::hide_selected)),
        MenuItem::action(show, "", st(SheetState::unhide_around)),
        MenuItem::Separator,
        MenuItem::sub(
            "Sort",
            vec![
                act("Sort A to Z (smallest first)", "", |s| {
                    s.sort_selection(true)
                }),
                act("Sort Z to A (largest first)", "", |s| {
                    s.sort_selection(false)
                }),
            ],
        ),
        act("Filter on or off", "", SheetState::toggle_filter),
    ]
}

fn cell_menu(state: &SheetState) -> Vec<MenuItem> {
    if let Some(row) = state.context_header.get_untracked() {
        return header_menu(state, row);
    }
    let act = |label: &'static str, hint: &'static str, f: fn(&SheetState)| {
        let st = state.clone();
        MenuItem::action(label, hint, move || f(&st))
    };
    let many = state.selection().start != state.selection().end;
    let mut items = vec![
        act("Cut", "Ctrl+X", |s| {
            let _ = s.copy(true);
        }),
        act("Copy", "Ctrl+C", |s| {
            let _ = s.copy(false);
        }),
        act("Paste", "Ctrl+V", |s| {
            if let Some(text) = s.clip_text() {
                s.paste(&text);
            }
        }),
        MenuItem::sub(
            "Paste special",
            vec![
                act("Values only", "", |s| s.paste_special(PasteMode::Values)),
                act("Formulas without formats", "", |s| {
                    s.paste_special(PasteMode::Formulas)
                }),
                act("Formats only", "", |s| s.paste_special(PasteMode::Formats)),
                act("Transposed", "", |s| s.paste_special(PasteMode::Transpose)),
            ],
        ),
        MenuItem::Separator,
        MenuItem::sub(
            "Insert",
            vec![
                act("Rows above", "Ctrl++", |s| s.insert_selected(Dim::Row)),
                act("Columns to the left", "", |s| s.insert_selected(Dim::Col)),
            ],
        ),
        MenuItem::sub(
            "Delete",
            vec![
                act("Selected rows", "Ctrl+-", |s| s.delete_selected(Dim::Row)),
                act("Selected columns", "", |s| s.delete_selected(Dim::Col)),
            ],
        ),
        act("Clear contents", "Delete", SheetState::clear_selected),
        act("Clear formatting", "", SheetState::clear_formatting),
        MenuItem::Separator,
        act("Bold", "Ctrl+B", SheetState::toggle_bold),
    ];
    if many {
        items.push(act("Merge cells", "", SheetState::merge_selection));
    }
    items.push(act("Unmerge cells", "", SheetState::unmerge_selection));
    items.push(MenuItem::Separator);
    items.push(MenuItem::sub(
        "Sort",
        vec![
            act("Sort A to Z (smallest first)", "", |s| {
                s.sort_selection(true)
            }),
            act("Sort Z to A (largest first)", "", |s| {
                s.sort_selection(false)
            }),
        ],
    ));
    items.push(act("Filter on or off", "", SheetState::toggle_filter));
    if state.table_at_cursor().is_some() {
        items.push(MenuItem::sub(
            "Table",
            vec![
                act("Table name\u{2026}", "", |s| {
                    s.open_dialog(DialogId::TableName)
                }),
                act("Banded rows on or off", "", SheetState::toggle_table_bands),
                MenuItem::sub("Look", table_style_items(state)),
                act("Convert to a range", "", SheetState::remove_table),
            ],
        ));
    } else {
        items.push(act("Make a table", "Ctrl+T", SheetState::insert_table));
    }
    items.push(MenuItem::sub(
        "Hide",
        vec![
            act("Hide rows", "", |s| s.hide_selected(Dim::Row)),
            act("Hide columns", "", |s| s.hide_selected(Dim::Col)),
            act("Show hidden rows", "", |s| s.unhide_around(Dim::Row)),
            act("Show hidden columns", "", |s| s.unhide_around(Dim::Col)),
        ],
    ));
    items.push(MenuItem::Separator);
    items.push(act("Drop-down list\u{2026}", "", |s| {
        s.open_dialog(DialogId::ValidList)
    }));
    items.push(act("Note\u{2026}", "", |s| {
        s.open_dialog(DialogId::Comment)
    }));
    items
}

/// The looks a table can be given, as menu items.
fn table_style_items(state: &SheetState) -> Vec<MenuItem> {
    crate::state::TABLE_STYLES
        .iter()
        .map(|(label, name)| {
            let st = state.clone();
            MenuItem::action(label, "", move || st.set_table_style(name))
        })
        .chain([MenuItem::Separator, {
            let st = state.clone();
            MenuItem::action("Your own colours\u{2026}", "", move || {
                st.open_dialog(DialogId::TableLook);
            })
        }])
        .collect()
}

/// The File and Edit sections of the ribbon.
#[allow(clippy::too_many_lines)]
fn file_groups(state: &SheetState) -> Vec<Element> {
    let setup = state.clone();
    let (new, open, xlsx, ods, csv, tsv, pdf, print) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (undo, redo) = (state.clone(), state.clone());
    let (can_undo, can_redo) = (state.can_undo.clone(), state.can_redo.clone());
    let history_button = |label: &'static str,
                          tip: &'static str,
                          enabled: Signal<bool>,
                          action: Box<dyn Fn() + 'static>| {
        button()
            .attr("type", "button")
            .attr("title", crate::i18n::tr(tip))
            .class("tool")
            .text(crate::i18n::tr(label))
            .attr_bool("disabled", move || !enabled.get())
            .on_click(action)
    };
    let file = office_ui::menu(
        "File",
        "New, open and save",
        office_cloud_ui::with_menu(vec![
            MenuItem::action("New", "", move || new.new_file()),
            MenuItem::action("Open\u{2026}", "Ctrl+O", move || open.open_file()),
            MenuItem::Separator,
            MenuItem::sub(
                "Save\u{2026}",
                vec![
                    MenuItem::action("Excel workbook (.xlsx)", file_hint(), move || {
                        xlsx.save_xlsx();
                    }),
                    MenuItem::action("OpenDocument spreadsheet (.ods)", "", move || {
                        ods.save_ods();
                    }),
                    MenuItem::action("CSV, this sheet (.csv)", "", move || csv.save_csv()),
                    MenuItem::action("Tab separated, this sheet (.tsv)", "", move || {
                        tsv.save_tsv();
                    }),
                ],
            ),
            MenuItem::sub(
                "Export\u{2026}",
                vec![MenuItem::action("PDF document (.pdf)", "", move || {
                    pdf.export_pdf();
                })],
            ),
            MenuItem::Separator,
            MenuItem::action("Page setup\u{2026}", "", move || {
                setup.open_dialog(DialogId::PageSetup)
            }),
            MenuItem::action("Print\u{2026}", "Ctrl+P", move || print.print()),
        ]),
    );
    let (pv, pf, pt, ptr) = (state.clone(), state.clone(), state.clone(), state.clone());
    let edit = group(
        "Edit",
        vec![
            office_ui::menu(
                "Paste special",
                "Paste what was copied in a particular way",
                vec![
                    MenuItem::action("Values only", "", move || {
                        pv.paste_special(PasteMode::Values)
                    }),
                    MenuItem::action("Formulas without formats", "", move || {
                        pf.paste_special(PasteMode::Formulas)
                    }),
                    MenuItem::action("Formats only", "", move || {
                        pt.paste_special(PasteMode::Formats)
                    }),
                    MenuItem::action("Transposed", "", move || {
                        ptr.paste_special(PasteMode::Transpose)
                    }),
                ],
            ),
            history_button(
                "Undo",
                "Take back the last change (Ctrl+Z)",
                can_undo,
                Box::new(move || undo.undo()),
            ),
            history_button(
                "Redo",
                "Do again what was taken back (Ctrl+Y)",
                can_redo,
                Box::new(move || redo.redo()),
            ),
        ],
    );
    let (ins_row, ins_col, del_row, del_col) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let cells = group(
        "Cells",
        vec![
            office_ui::menu(
                "Insert",
                "Insert rows or columns",
                vec![
                    MenuItem::action("Rows above", "Ctrl++", move || {
                        ins_row.insert_selected(Dim::Row);
                    }),
                    MenuItem::action("Columns to the left", "", move || {
                        ins_col.insert_selected(Dim::Col);
                    }),
                ],
            ),
            office_ui::menu(
                "Delete",
                "Delete rows or columns",
                vec![
                    MenuItem::action("Selected rows", "Ctrl+-", move || {
                        del_row.delete_selected(Dim::Row);
                    }),
                    MenuItem::action("Selected columns", "", move || {
                        del_col.delete_selected(Dim::Col);
                    }),
                ],
            ),
        ],
    );
    let (merge, unmerge, hide_r, hide_c, show_r, show_c) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let layout = group(
        "Layout",
        vec![
            office_ui::menu(
                "Merge",
                "Merge or split cells",
                vec![
                    MenuItem::action("Merge cells", "", move || merge.merge_selection()),
                    MenuItem::action("Unmerge cells", "", move || unmerge.unmerge_selection()),
                ],
            ),
            office_ui::menu(
                "Hide",
                "Hide or show rows and columns",
                vec![
                    MenuItem::action("Hide rows", "", move || hide_r.hide_selected(Dim::Row)),
                    MenuItem::action("Hide columns", "", move || hide_c.hide_selected(Dim::Col)),
                    MenuItem::Separator,
                    MenuItem::action("Show hidden rows", "", move || {
                        show_r.unhide_around(Dim::Row);
                    }),
                    MenuItem::action("Show hidden columns", "", move || {
                        show_c.unhide_around(Dim::Col);
                    }),
                ],
            ),
            freeze_menu(state),
            outline_menu(state),
        ],
    );
    let (sort_up, sort_down, filter_toggle, filter_clear) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let (tb_new, tb_name, tb_band, tb_remove) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let data = group(
        "Data",
        vec![
            office_ui::menu(
                "Table",
                "Make the cells a table with sort and filter arrows",
                vec![
                    MenuItem::action("Make a table", "Ctrl+T", move || tb_new.insert_table()),
                    MenuItem::action("Table name\u{2026}", "", move || {
                        tb_name.open_dialog(DialogId::TableName)
                    }),
                    MenuItem::action("Banded rows on or off", "", move || {
                        tb_band.toggle_table_bands()
                    }),
                    MenuItem::sub("Look", table_style_items(state)),
                    MenuItem::Separator,
                    MenuItem::action("Convert to a range", "", move || tb_remove.remove_table()),
                ],
            ),
            office_ui::menu(
                "Sort",
                "Sort the rows by the cursor's column",
                vec![
                    MenuItem::action("Sort A to Z (smallest first)", "", move || {
                        sort_up.sort_selection(true)
                    }),
                    MenuItem::action("Sort Z to A (largest first)", "", move || {
                        sort_down.sort_selection(false)
                    }),
                ],
            ),
            office_ui::menu(
                "Filter",
                "Show only some rows of a table",
                vec![
                    MenuItem::action("Turn filter on or off", "", move || {
                        filter_toggle.toggle_filter()
                    }),
                    MenuItem::action("Show all rows", "", move || filter_clear.clear_filters()),
                ],
            ),
        ],
    );
    let (vl, vn, vc, vx, ct, cf, cs, cb, cq) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (dp, un, ab, bl, bk, er, cl_sel, cl_all, ce) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (protect_on, protect_off, lock_on, lock_off) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let (name_def, name_del, note_edit) = (state.clone(), state.clone(), state.clone());
    let rules = group(
        "Rules",
        vec![
            office_ui::menu(
                "Validation",
                "Limit what can be entered in cells",
                vec![
                    MenuItem::action("Drop-down list\u{2026}", "", move || {
                        vl.open_dialog(DialogId::ValidList)
                    }),
                    MenuItem::action("Number rule\u{2026}", "", move || {
                        vn.open_dialog(DialogId::ValidNumber)
                    }),
                    MenuItem::action("Formula rule\u{2026}", "", move || {
                        vc.open_dialog(DialogId::ValidCustom)
                    }),
                    MenuItem::Separator,
                    MenuItem::action("Clear validation", "", move || vx.clear_validation()),
                ],
            ),
            office_ui::menu(
                "Conditional",
                "Colour cells by what they hold",
                vec![
                    MenuItem::action("Highlight cells\u{2026}", "", move || {
                        ct.open_dialog(DialogId::CondCompare)
                    }),
                    MenuItem::action("Highlight text\u{2026}", "", move || {
                        cf.open_dialog(DialogId::CondText)
                    }),
                    MenuItem::action("Largest or smallest\u{2026}", "", move || {
                        cs.open_dialog(DialogId::CondTop)
                    }),
                    MenuItem::action("Colour scale\u{2026}", "", move || {
                        cb.open_dialog(DialogId::CondScale)
                    }),
                    MenuItem::action("Data bars\u{2026}", "", move || {
                        cq.open_dialog(DialogId::CondBar)
                    }),
                    MenuItem::action("Formula\u{2026}", "", move || {
                        ce.open_dialog(DialogId::CondExpr)
                    }),
                    MenuItem::Separator,
                    MenuItem::action("Duplicate values", "", move || {
                        dp.quick_cond(Cond::Duplicates)
                    }),
                    MenuItem::action("Unique values", "", move || un.quick_cond(Cond::Unique)),
                    MenuItem::action("Above average", "", move || {
                        ab.quick_cond(Cond::AboveAverage { below: false })
                    }),
                    MenuItem::action("Below average", "", move || {
                        bl.quick_cond(Cond::AboveAverage { below: true })
                    }),
                    MenuItem::action("Blank cells", "", move || bk.quick_cond(Cond::Blanks)),
                    MenuItem::action("Errors", "", move || er.quick_cond(Cond::Errors)),
                    MenuItem::Separator,
                    MenuItem::action("Clear rules from selection", "", move || {
                        cl_sel.clear_cond(false)
                    }),
                    MenuItem::action("Clear rules from the sheet", "", move || {
                        cl_all.clear_cond(true)
                    }),
                ],
            ),
            office_ui::menu(
                "Protect",
                "Keep cells from being changed",
                vec![
                    MenuItem::action("Protect the sheet\u{2026}", "", move || {
                        protect_on.open_dialog(DialogId::Protect)
                    }),
                    MenuItem::action("Unprotect the sheet\u{2026}", "", move || {
                        protect_off.open_dialog(DialogId::Unprotect)
                    }),
                    MenuItem::Separator,
                    MenuItem::action("Lock the selected cells", "", move || {
                        lock_on.set_locked(true)
                    }),
                    MenuItem::action("Unlock the selected cells", "", move || {
                        lock_off.set_locked(false)
                    }),
                ],
            ),
            office_ui::menu(
                "Names",
                "Names for cells and values",
                vec![
                    MenuItem::action("Define a name\u{2026}", "", move || {
                        name_def.open_dialog(DialogId::DefineName)
                    }),
                    MenuItem::action("Delete a name\u{2026}", "", move || {
                        name_del.open_dialog(DialogId::DeleteName)
                    }),
                ],
            ),
            button()
                .attr("type", "button")
                .class("tool")
                .attr(
                    "title",
                    crate::i18n::tr("Add or change the note on the cell"),
                )
                .text(crate::i18n::tr("Note"))
                .on_click(move || note_edit.open_dialog(DialogId::Comment)),
        ],
    );
    let kinds: Vec<MenuItem> = [
        (ChartKind::Column, "Columns"),
        (ChartKind::Bar, "Bars"),
        (ChartKind::Line, "Lines"),
        (ChartKind::Area, "Areas"),
        (ChartKind::Pie, "Pie"),
        (ChartKind::Scatter, "Scatter plot"),
    ]
    .into_iter()
    .map(|(k, label)| {
        let s = state.clone();
        MenuItem::action(label, "", move || s.new_chart_dialog(k))
    })
    .collect();
    let (chart_edit, chart_delete) = (state.clone(), state.clone());
    let mut chart_items = kinds;
    chart_items.push(MenuItem::Separator);
    chart_items.push(MenuItem::action(
        "Change the picked chart\u{2026}",
        "",
        move || chart_edit.edit_chart_dialog(),
    ));
    chart_items.push(MenuItem::action(
        "Remove the picked chart",
        "Del",
        move || chart_delete.delete_chart(),
    ));
    let spark_items: Vec<MenuItem> = [
        (SparkKind::Line, "Line\u{2026}"),
        (SparkKind::Column, "Columns\u{2026}"),
        (SparkKind::WinLoss, "Win / loss\u{2026}"),
    ]
    .into_iter()
    .map(|(k, label)| {
        let s = state.clone();
        MenuItem::action(label, "", move || s.new_sparkline_dialog(k))
    })
    .chain([MenuItem::Separator, {
        let s = state.clone();
        MenuItem::action("Remove from selection", "", move || s.clear_sparklines())
    }])
    .collect();
    let insert = group(
        "Insert",
        vec![
            office_ui::menu("Chart", "Draw cells as a chart", chart_items),
            office_ui::menu("Sparkline", "A tiny chart in a cell", spark_items),
            pivot_button(state),
        ],
    );
    vec![file, edit, cells, layout, data, rules, insert]
}

/// The arrows in the heading cells of filtered blocks and tables.
fn filter_layer(state: &SheetState, window: &Memo<Window>) -> View {
    let (st, w, build) = (state.clone(), window.clone(), state.clone());
    View::keyed(
        move || {
            let _ = st.rev.get();
            let (r0, r1, c0, c1) = w.get();
            let mut out: Vec<(usize, u32, u32, bool)> = Vec::new();
            for (target, f) in st.filters() {
                if !(r0..r1).contains(&f.range.start.row) {
                    continue;
                }
                let code = target.map_or(0, |i| i + 1);
                out.extend(
                    (f.range.start.col..=f.range.end.col)
                        .filter(|c| (c0..c1).contains(c))
                        .map(|c| (code, f.range.start.row, c, f.allowed.contains_key(&c))),
                );
            }
            out
        },
        |(t, r, c, on): &(usize, u32, u32, bool)| {
            (*t as u64) << 48 | u64::from(*r) << 24 | u64::from(*c) << 1 | u64::from(*on)
        },
        move |(code, row, col, on)| {
            let open = build.clone();
            let target = code.checked_sub(1);
            place(
                div().class("filter-cell").attr("role", "presentation"),
                &build,
                move || Range::new(CellRef::new(row, col), CellRef::new(row, col)),
            )
            .child(
                button()
                    .attr("type", "button")
                    .class(if on { "filter-btn on" } else { "filter-btn" })
                    .attr("aria-label", crate::i18n::tr("Filter"))
                    .attr("title", crate::i18n::tr("Sort or filter this column"))
                    .on_click(move || open.open_filter(target, col)),
            )
            .into_view()
        },
    )
}

/// The list of values under a heading, with a tick for each row to keep.
fn filter_panel(state: &SheetState) -> View {
    let (open, st) = (state.filter_open.clone(), state.clone());
    View::keyed(
        move || open.get().into_iter().collect::<Vec<u32>>(),
        |c: &u32| *c,
        move |col| {
            let f = st.open_target_filter().unwrap_or_default();
            let (sort_up, sort_down) = (st.clone(), st.clone());
            let (values, more) = st.filter_values(col);
            let (all_state, none_state, apply, cancel, clear, keys, all_check) = (
                st.clone(),
                st.clone(),
                st.clone(),
                st.clone(),
                st.clone(),
                st.clone(),
                st.clone(),
            );
            let rows: Vec<View> = values
                .into_iter()
                .map(|v| {
                    let (shown, toggle, checked) = (v.clone(), st.clone(), st.clone());
                    let name = v.clone();
                    label()
                        .class("check filter-item")
                        .child(
                            input()
                                .attr("type", "checkbox")
                                .attr_bool("checked", move || {
                                    checked.filter_pick.get().contains(&name)
                                })
                                .on_click(move || toggle.toggle_filter_value(&v)),
                        )
                        .child(span().text(if shown.is_empty() {
                            "(blank)".to_string()
                        } else {
                            shown
                        }))
                        .into_view()
                })
                .collect();
            let row = f.range.start.row + 1;
            let mut panel = div()
                .class("filter-panel")
                .attr("role", "dialog")
                .attr("aria-label", crate::i18n::tr("Filter values"))
                .on_keydown(move |k| {
                    if k.key == "Escape" {
                        prevent_default();
                        keys.close_filter();
                    }
                })
                .child(
                    div()
                        .class("filter-all")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(crate::i18n::tr("Sort A to Z"))
                                .on_click(move || sort_up.sort_filter_column(true)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(crate::i18n::tr("Sort Z to A"))
                                .on_click(move || sort_down.sort_filter_column(false)),
                        ),
                )
                .child(
                    div()
                        .class("filter-all")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(crate::i18n::tr("Select all"))
                                .on_click(move || all_state.set_all_filter_values(true)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(crate::i18n::tr("Select none"))
                                .on_click(move || none_state.set_all_filter_values(false)),
                        ),
                )
                .child(div().class("filter-list").children(rows));
            if more {
                panel = panel.child(
                    div()
                        .class("filter-more")
                        .text(crate::i18n::tr("Only the first values are listed")),
                );
            }
            let _ = &all_check;
            let panel = panel.child(
                div()
                    .class("filter-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool primary")
                            .text(crate::i18n::tr("Apply"))
                            .on_click(move || apply.apply_filter()),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool")
                            .text(crate::i18n::tr("Show all"))
                            .on_click({
                                let st = clear.clone();
                                move || {
                                    st.set_filter_column(col, None);
                                    st.close_filter();
                                }
                            }),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool")
                            .text(crate::i18n::tr("Cancel"))
                            .on_click(move || cancel.close_filter()),
                    ),
            );
            let (ax, ay) = (st.axes.clone(), st.axes.clone());
            panel
                .css("left", move || format!("{}px", ax.get().cols.offset(col)))
                .css("top", move || format!("{}px", ay.get().rows.offset(row)))
                .into_view()
        },
    )
}

/// The arrow beside a cell that has a list rule, and the list.
/// The charts of the sheet, drawn over the cells as images; a picked one has a frame and can be carried.
fn charts_layer(state: &SheetState) -> View {
    let (st, build) = (state.clone(), state.clone());
    View::keyed(
        move || {
            let _ = st.rev.get();
            st.charts()
                .into_iter()
                .enumerate()
                .map(|(i, c)| (i, c.at, c.width, c.height))
                .collect::<Vec<(usize, CellRef, u32, u32)>>()
        },
        |(i, at, w, h): &(usize, CellRef, u32, u32)| {
            (*i as u64) << 48 ^ key_of(*at) << 20 ^ u64::from(*w) << 10 ^ u64::from(*h)
        },
        move |(index, at, width, height)| {
            let (down, edit, picked, axes_x, axes_y) = (
                build.clone(),
                build.clone(),
                build.clone(),
                build.axes.clone(),
                build.axes.clone(),
            );
            let image = {
                let st = build.clone();
                create_memo(move || {
                    let _ = st.rev.get();
                    st.chart_image(index)
                })
            };
            div()
                .attr_fn("class", move || {
                    if picked.chart_selected.get() == Some(index) {
                        "chart picked".to_string()
                    } else {
                        "chart".to_string()
                    }
                })
                .attr("role", "img")
                .attr("aria-label", crate::i18n::tr("Chart"))
                .css("left", move || {
                    format!("{}px", axes_x.get().cols.offset(at.col))
                })
                .css("top", move || {
                    format!("{}px", axes_y.get().rows.offset(at.row))
                })
                .css("width", {
                    let v = format!("{width}px");
                    move || v.clone()
                })
                .css("height", {
                    let v = format!("{height}px");
                    move || v.clone()
                })
                .on(EventKind::PointerDown, move |e| {
                    if let Event::PointerDown(p) = e {
                        down.begin_chart_drag(index, p.x, p.y);
                    }
                })
                .on(EventKind::DoubleClick, move |_| {
                    edit.select_chart(index);
                    edit.edit_chart_dialog();
                })
                .child(
                    img()
                        .attr_fn("src", move || image.get())
                        .attr("alt", "")
                        .attr("draggable", "false"),
                )
                .into_view()
        },
    )
}

/// Where the carried chart would land.
fn chart_ghost(state: &SheetState) -> View {
    let (drop, st) = (state.chart_drop.clone(), state.clone());
    View::keyed(
        move || drop.get().into_iter().collect::<Vec<CellRef>>(),
        |at: &CellRef| key_of(*at),
        move |at| {
            let (ax, ay) = (st.axes.clone(), st.axes.clone());
            let picked = st
                .chart_selected
                .get_untracked()
                .and_then(|i| st.charts().get(i).cloned());
            let (w, h) = picked.map_or((100, 60), |c| (c.width, c.height));
            div()
                .class("chart-ghost")
                .attr("role", "presentation")
                .css("left", move || {
                    format!("{}px", ax.get().cols.offset(at.col))
                })
                .css("top", move || format!("{}px", ay.get().rows.offset(at.row)))
                .css("width", {
                    let v = format!("{w}px");
                    move || v.clone()
                })
                .css("height", {
                    let v = format!("{h}px");
                    move || v.clone()
                })
                .into_view()
        },
    )
}

fn dropdown_layer(state: &SheetState) -> View {
    let (st, open) = (state.clone(), state.dropdown_open.clone());
    View::keyed(
        move || {
            let _ = st.rev.get();
            let at = st.selected.get();
            if st.dropdown_items().is_some() {
                vec![(at, open.get())]
            } else {
                Vec::new()
            }
        },
        |(at, open): &(CellRef, bool)| key_of(*at) << 1 | u64::from(*open),
        {
            let build = state.clone();
            move |(at, shown)| {
                let (toggle, pick_state) = (build.clone(), build.clone());
                let mut cell = place(
                    div().class("dv-cell").attr("role", "presentation"),
                    &build,
                    move || Range::new(at, at),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .class("dv-btn")
                        .attr("aria-label", crate::i18n::tr("Show the list"))
                        .attr("title", crate::i18n::tr("Choose from the list"))
                        .on_click(move || toggle.dropdown_open.update(|o| *o = !*o)),
                );
                if shown {
                    let items = build.dropdown_items().unwrap_or_default();
                    let rows: Vec<View> = items
                        .into_iter()
                        .map(|item| {
                            let (pick, label) = (pick_state.clone(), item.clone());
                            button()
                                .attr("type", "button")
                                .attr("role", "option")
                                .class("menu-item dv-item")
                                .text(label)
                                .on_click(move || {
                                    pick.dropdown_open.set(false);
                                    pick.set_text(at, &item);
                                    pick.refocus();
                                })
                                .into_view()
                        })
                        .collect();
                    cell = cell.child(
                        div()
                            .class("dv-list")
                            .attr("role", "listbox")
                            .children(rows),
                    );
                }
                cell.into_view()
            }
        },
    )
}

/// The form for the command being set up.
fn dialog_layer(state: &SheetState) -> View {
    let (current, st) = (state.dialog.clone(), state.clone());
    View::keyed(
        move || {
            current
                .get()
                .map(|d| d.id as u32)
                .into_iter()
                .collect::<Vec<u32>>()
        },
        |id: &u32| *id,
        move |_| dialog_form(&st).into_view(),
    )
}

fn dialog_form(state: &SheetState) -> Element {
    let Some(d) = state.dialog.get_untracked() else {
        return div();
    };
    let (keys, ok, cancel, shown) = (state.clone(), state.clone(), state.clone(), state.clone());
    let fields: Vec<View> = d
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (st, value, label_state) = (state.clone(), state.clone(), state.clone());
            let current = move || {
                value
                    .dialog
                    .get()
                    .and_then(|d| d.fields.get(i).map(|f| f.value.clone()))
                    .unwrap_or_default()
            };
            let input_el: View = match &f.kind {
                FieldKind::Text => {
                    let focus_first = i == 0;
                    input()
                        .attr("type", "text")
                        .attr("spellcheck", "false")
                        .attr("placeholder", f.hint.clone())
                        .attr("aria-label", f.label.clone())
                        .attr_fn("value", current)
                        .on_input(move |v| st.set_field(i, v))
                        .on_keydown({
                            let enter = label_state.clone();
                            move |k| {
                                if k.key == "Enter" {
                                    prevent_default();
                                    enter.submit_dialog();
                                }
                            }
                        })
                        .focus_if(move || focus_first)
                        .into_view()
                }
                FieldKind::Choice(options) => {
                    let opts: Vec<View> = options
                        .iter()
                        .map(|(v, l)| {
                            let (sel, mine) = (label_state.clone(), v.clone());
                            option()
                                .attr("value", v.clone())
                                .attr_bool("selected", move || {
                                    sel.dialog
                                        .get()
                                        .and_then(|d| d.fields.get(i).map(|f| f.value == mine))
                                        .unwrap_or(false)
                                })
                                .text(l.clone())
                                .into_view()
                        })
                        .collect();
                    select()
                        .class("format-list")
                        .attr("aria-label", f.label.clone())
                        .on_change(move |v| st.set_field(i, v))
                        .children(opts)
                        .into_view()
                }
                FieldKind::Check => {
                    let (toggle, checked) = (st, label_state.clone());
                    input()
                        .attr("type", "checkbox")
                        .attr("aria-label", f.label.clone())
                        .attr_bool("checked", move || {
                            checked
                                .dialog
                                .get()
                                .and_then(|d| d.fields.get(i).map(|f| !f.value.is_empty()))
                                .unwrap_or(false)
                        })
                        .on_click(move || toggle.toggle_field(i))
                        .into_view()
                }
                FieldKind::Color => input()
                    .attr("type", "color")
                    .attr("aria-label", f.label.clone())
                    .attr_fn("value", current)
                    .on_change(move |v| st.set_field(i, v))
                    .into_view(),
            };
            label()
                .class("dialog-field")
                .child(span().class("dialog-label").text(f.label.clone()))
                .child(input_el)
                .into_view()
        })
        .collect();
    div().class("dialog-backdrop").child(
        div()
            .class("dialog")
            .attr("role", "dialog")
            .attr("aria-label", d.title.clone())
            .on_keydown(move |k| {
                if k.key == "Escape" {
                    prevent_default();
                    keys.cancel_dialog();
                }
            })
            .child(div().class("dialog-title").text(d.title.clone()))
            .child(div().class("dialog-intro").text(d.intro.clone()))
            .child(div().class("dialog-fields").children(fields))
            .child(
                div()
                    .class("dialog-error")
                    .attr("role", "alert")
                    .text_fn(move || shown.dialog.get().map(|d| d.error).unwrap_or_default()),
            )
            .child(
                div()
                    .class("dialog-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool primary")
                            .text(crate::i18n::tr("OK"))
                            .on_click(move || ok.submit_dialog()),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool")
                            .text(crate::i18n::tr("Cancel"))
                            .on_click(move || cancel.cancel_dialog()),
                    ),
            ),
    )
}

fn outline_menu(state: &SheetState) -> Element {
    let item = |label: &'static str, f: fn(&SheetState)| {
        let s = state.clone();
        MenuItem::action(label, "", move || f(&s))
    };
    office_ui::menu(
        "Group",
        "Group rows or columns so they can be collapsed",
        vec![
            item("Group rows", |s| s.group_selected(true)),
            item("Group columns", |s| s.group_selected(false)),
            item("Ungroup rows", |s| s.ungroup_selected(true)),
            item("Ungroup columns", |s| s.ungroup_selected(false)),
            MenuItem::Separator,
            item("Collapse row groups", |s| s.collapse_groups(true)),
            item("Expand row groups", |s| s.expand_groups(true)),
            item("Collapse column groups", |s| s.collapse_groups(false)),
            item("Expand column groups", |s| s.expand_groups(false)),
        ],
    )
}

fn pivot_button(state: &SheetState) -> Element {
    let s = state.clone();
    button()
        .attr("type", "button")
        .class("tool")
        .attr(
            "title",
            "A table that adds up one column by the values of another",
        )
        .text(crate::i18n::tr("Summary table"))
        .on_click(move || s.open_pivot_dialog())
}

fn freeze_menu(state: &SheetState) -> Element {
    let item = |label: &'static str, rows: Option<u32>, cols: Option<u32>| {
        let s = state.clone();
        MenuItem::action(label, "", move || match (rows, cols) {
            (None, None) => s.freeze_at_cursor(),
            (r, c) => s.freeze(r.unwrap_or(0), c.unwrap_or(0)),
        })
    };
    office_ui::menu(
        "Freeze",
        "Keep rows and columns in view while scrolling",
        vec![
            item("Freeze above and left of the cursor", None, None),
            item("Freeze top row", Some(1), None),
            item("Freeze first column", None, Some(1)),
            MenuItem::Separator,
            item("Unfreeze", Some(0), Some(0)),
        ],
    )
}

/// The zoom of the grid: out, back to 100 percent, in.
fn zoom_group(state: &SheetState) -> Element {
    let (out, now, inn, shown) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.zoom.clone(),
    );
    group(
        "Zoom",
        vec![
            office_ui::tool("\u{2212}", "Zoom out (Ctrl+wheel)", move || out.zoom_by(-1)),
            button()
                .attr("type", "button")
                .class("tool")
                .attr("title", crate::i18n::tr("Back to 100%"))
                .text_fn(move || format!("{}%", shown.get()))
                .on_click(move || now.zoom_by(0)),
            office_ui::tool("\u{FF0B}", "Zoom in (Ctrl+wheel)", move || inn.zoom_by(1)),
        ],
    )
}

/// One labelled section of the ribbon.
fn group(name: &'static str, items: Vec<Element>) -> Element {
    office_ui::group(name, items.into_iter().map(IntoView::into_view).collect())
}

/// The ribbon: the document's name and what happened last, then the sections of buttons.
fn ribbon(state: &SheetState) -> Element {
    // The groups, by what they are for: Home (file, edit, text), Cells (and what to insert), Layout, Data.
    let mut all = file_groups(state).into_iter();
    let file = all.next();
    let (edit, cells, layout, data, rules, insert) = (
        all.next(),
        all.next(),
        all.next(),
        all.next(),
        all.next(),
        all.next(),
    );
    let mut format = format_groups(state).into_iter();
    let (font, alignment, number, clear) =
        (format.next(), format.next(), format.next(), format.next());
    let home: Vec<Element> = edit.into_iter().chain(font).chain(alignment).collect();
    let tabs = vec![
        ("Home", home),
        ("Number", number.into_iter().chain(clear).collect()),
        ("Cells", cells.into_iter().chain(insert).collect()),
        (
            "Layout",
            layout.into_iter().chain(Some(zoom_group(state))).collect(),
        ),
        ("Data", data.into_iter().chain(rules).collect()),
    ];
    office_ui::ribbon_named(
        "lore-sheets",
        state.name.clone(),
        Some({
            let n = state.name.clone();
            Rc::new(move |typed: &str| office_ui::rename_to(&n, typed)) as Rc<dyn Fn(&str)>
        }),
        state.status.clone(),
        "Ribbon",
        file.unwrap_or_else(div),
        extras(state),
        tabs,
    )
    .child(find_bar(state))
}

/// The language picker and the theme menu, at the end of the top line.
fn extras(state: &SheetState) -> Element {
    let language = state.language.clone();
    let current = std::rc::Rc::new(move || language.get()) as std::rc::Rc<dyn Fn() -> String>;
    let pick = state.clone();
    let tr = crate::i18n::text;
    div()
        .child(office_cloud_ui::cloud_button())
        .child(office_theme::language_picker(
            &tr("language_label"),
            crate::i18n::CODES,
            &current,
            std::rc::Rc::new(move |code| pick.set_language(code)),
        ))
        .child(office_theme::paper_button(&tr("paper_toggle")))
        .child(office_theme::theme_menu(&office_theme::ThemeLabels {
            toggle: tr("theme_toggle"),
            mode: tr("theme_mode"),
            light: tr("theme_light"),
            dark: tr("theme_dark"),
            system: tr("theme_system"),
            palette: tr("theme_palette"),
            palettes: ["blue", "green", "teal", "purple", "rose", "orange"]
                .map(|name| tr(&format!("palette_{name}"))),
        }))
}

/// The find and replace bar, built when it is opened (and again by Ctrl+F while it is open, which brings focus back to it).
fn find_bar(state: &SheetState) -> View {
    let (open, epoch, st) = (
        state.find_open.clone(),
        state.find_epoch.clone(),
        state.clone(),
    );
    View::keyed(
        move || {
            if open.get() {
                vec![epoch.get()]
            } else {
                Vec::new()
            }
        },
        |e: &u32| *e,
        move |_| find_bar_contents(&st).into_view(),
    )
}

#[allow(clippy::too_many_lines)]
fn find_bar_contents(state: &SheetState) -> Element {
    let (query, typed_query, keys) = (
        state.find_query.clone(),
        state.find_query.clone(),
        state.clone(),
    );
    let (with, typed_with, replace_keys) = (
        state.replace_with.clone(),
        state.replace_with.clone(),
        state.clone(),
    );
    let (first_a, first_b) = (state.replace_first.clone(), state.replace_first.clone());
    let (prev, next, one, all, close) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (case_on, case_click, whole_on, whole_click) = (
        state.match_case.clone(),
        state.match_case.clone(),
        state.whole_cell.clone(),
        state.whole_cell.clone(),
    );
    let plain = |label: &'static str, tip: &'static str, action: Box<dyn Fn()>| {
        button()
            .attr("type", "button")
            .attr("title", crate::i18n::tr(tip))
            .class("tool")
            .text(crate::i18n::tr(label))
            .on_click(action)
    };
    let counted = state.clone();
    let (regex_on, regex_click, sheets_on, sheets_click) = (
        state.find_regex.clone(),
        state.find_regex.clone(),
        state.find_all_sheets.clone(),
        state.find_all_sheets.clone(),
    );
    div()
        .class("findbar")
        .attr("role", "search")
        .child(
            label().class("find-field").text(crate::i18n::tr("Find")).child(
                input()
                    .attr("type", "text")
                    .class("find-input")
                    .attr("aria-label", crate::i18n::tr("Find"))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || query.get())
                    .on_input(move |v| typed_query.set(v.to_string()))
                    .on_keydown(move |k| match k.key.as_str() {
                        "Enter" => {
                            prevent_default();
                            keys.find_next(!k.shift);
                        }
                        "Escape" => {
                            prevent_default();
                            keys.close_find();
                        }
                        _ => {}
                    })
                    .focus_if(move || !first_a.get()),
            ),
        )
        .child(
            span()
                .class("find-count")
                .attr("role", "status")
                .text_fn(move || {
                    let _ = (
                        counted.rev.get(),
                        counted.find_query.get(),
                        counted.match_case.get(),
                        counted.whole_cell.get(),
                        counted.find_regex.get(),
                        counted.find_all_sheets.get(),
                    );
                    let _ = counted.matcher_ready();
                    match counted.find_count() {
                        _ if counted.find_query.get().is_empty() => String::new(),
                        _ if !counted.find_problem.get().is_empty() => "Not a valid pattern".to_string(),
                        Some(0) => "No matches".to_string(),
                        Some(1) => "1 match".to_string(),
                        Some(n) => format!("{n} matches"),
                        None => "Many matches".to_string(),
                    }
                }),
        )
        .child(plain(
            "Previous",
            "Previous match (Shift+Enter)",
            Box::new(move || {
                prev.find_next(false);
            }),
        ))
        .child(plain(
            "Next",
            "Next match (Enter)",
            Box::new(move || {
                next.find_next(true);
            }),
        ))
        .child(
            label()
                .class("check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("aria-label", crate::i18n::tr("Match case"))
                        .attr_bool("checked", move || case_on.get())
                        .on_click(move || case_click.update(|v| *v = !*v)),
                )
                .child(span().text(crate::i18n::tr("Match case"))),
        )
        .child(
            label()
                .class("check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("aria-label", crate::i18n::tr("Whole cell"))
                        .attr_bool("checked", move || whole_on.get())
                        .on_click(move || whole_click.update(|v| *v = !*v)),
                )
                .child(span().text(crate::i18n::tr("Whole cell"))),
        )
        .child(
            label()
                .class("check")
                .attr("title", crate::i18n::tr("The text is a regular expression, such as ^\\d{3}$ or (\\w+)@(\\w+); in a replacement $1 is the first group"))
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("aria-label", crate::i18n::tr("Regular expression"))
                        .attr_bool("checked", move || regex_on.get())
                        .on_click(move || regex_click.update(|v| *v = !*v)),
                )
                .child(span().text(crate::i18n::tr("Regex"))),
        )
        .child(
            label()
                .class("check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("aria-label", crate::i18n::tr("All sheets"))
                        .attr_bool("checked", move || sheets_on.get())
                        .on_click(move || sheets_click.update(|v| *v = !*v)),
                )
                .child(span().text(crate::i18n::tr("All sheets"))),
        )
        .child(
            label().class("find-field").text(crate::i18n::tr("Replace with")).child(
                input()
                    .attr("type", "text")
                    .class("replace-input")
                    .attr("aria-label", crate::i18n::tr("Replace with"))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || with.get())
                    .on_input(move |v| typed_with.set(v.to_string()))
                    .on_keydown(move |k| match k.key.as_str() {
                        "Enter" => {
                            prevent_default();
                            replace_keys.replace_current();
                        }
                        "Escape" => {
                            prevent_default();
                            replace_keys.close_find();
                        }
                        _ => {}
                    })
                    .focus_if(move || first_b.get()),
            ),
        )
        .child(plain(
            "Replace",
            "Replace in this cell and go to the next match",
            Box::new(move || one.replace_current()),
        ))
        .child(plain(
            "Replace all",
            "Replace in every matching cell",
            Box::new(move || {
                all.replace_all();
            }),
        ))
        .child(plain(
            "Close",
            "Close the find bar (Escape)",
            Box::new(move || close.close_find()),
        ))
}

/// The format buttons: what is bold, what colour, how aligned, which number format.
#[allow(clippy::too_many_lines)]
fn format_groups(state: &SheetState) -> Vec<Element> {
    // What the toolbar shows is the format of the active cell, looked at again when the cell or the workbook changes.
    let active: Memo<Style> = {
        let st = state.clone();
        create_memo(move || {
            let _ = (st.rev.get(), st.selected.get());
            st.active_style()
        })
    };
    let toggle = |label: &'static str,
                  tip: &'static str,
                  extra: &'static str,
                  on: Box<dyn Fn() -> bool>,
                  action: Box<dyn Fn()>| {
        let on_for_aria = {
            // `on` is used twice (class and aria), so share it.
            let shared: Rc<dyn Fn() -> bool> = Rc::from(on);
            (shared.clone(), shared)
        };
        let (for_class, for_aria) = on_for_aria;
        button()
            .attr("type", "button")
            .attr("title", crate::i18n::tr(tip))
            // The icons of the alignment buttons are chosen by this, which the language does not change.
            .attr("data-tip", tip)
            .class("tool")
            .class(extra)
            .class_if("on", move || for_class())
            .attr_fn("aria-pressed", move || for_aria().to_string())
            .text(crate::i18n::tr(label))
            .on_click(action)
    };
    let (a_b, a_i, a_u, a_w) = (
        active.clone(),
        active.clone(),
        active.clone(),
        active.clone(),
    );
    let (a_l, a_c, a_r) = (active.clone(), active.clone(), active.clone());
    let (s_b, s_i, s_u, s_w, s_l, s_c, s_r) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (active_for_font, active_for_size) = (active.clone(), active.clone());
    let (color_state, fill_state, none_state, color_value, fill_value) = (
        state.clone(),
        state.clone(),
        state.clone(),
        active.clone(),
        active.clone(),
    );
    let (format_state, more, fewer, clear) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let preset_options: Vec<View> = PRESETS
        .iter()
        .map(|(label, code)| {
            let current = active.clone();
            option()
                .attr("value", *code)
                .attr_bool("selected", move || current.get().number_format == *code)
                .text(crate::i18n::tr(label))
                .into_view()
        })
        .collect();
    let (custom_a, custom_b, custom_c) = (active.clone(), active.clone(), active);
    let custom = option()
        .attr_fn("value", move || custom_a.get().number_format)
        .attr_bool("selected", move || {
            !PRESETS
                .iter()
                .any(|(_, c)| *c == custom_b.get().number_format)
        })
        .attr_bool("hidden", {
            let hidden = custom_c.clone();
            move || {
                PRESETS
                    .iter()
                    .any(|(_, c)| *c == hidden.get().number_format)
            }
        })
        .text_fn(move || custom_c.get().number_format);
    let button_plain = |label: &'static str, tip: &'static str, action: Box<dyn Fn()>| {
        button()
            .attr("type", "button")
            .attr("title", crate::i18n::tr(tip))
            .class("tool")
            .text(crate::i18n::tr(label))
            .on_click(action)
    };
    vec![
        group(
            "Font",
            vec![
                toggle(
                    "B",
                    "Bold",
                    "bold",
                    Box::new(move || a_b.get().bold),
                    Box::new(move || s_b.toggle_bold()),
                ),
                toggle(
                    "I",
                    "Italic",
                    "italic",
                    Box::new(move || a_i.get().italic),
                    Box::new(move || s_i.toggle_italic()),
                ),
                toggle(
                    "U",
                    "Underline",
                    "underline",
                    Box::new(move || a_u.get().underline),
                    Box::new(move || s_u.toggle_underline()),
                ),
                font_name_list(&active_for_font, state),
                font_size_list(&active_for_size, state),
                borders_menu(state),
                label()
                    .class("swatch")
                    .attr("title", crate::i18n::tr("Text colour"))
                    .text(crate::i18n::tr("A"))
                    .child(
                        input()
                            .attr("type", "color")
                            .attr("aria-label", crate::i18n::tr("Text colour"))
                            .attr_fn("value", move || {
                                color_css(color_value.get().color.unwrap_or(0))
                            })
                            .on_change(move |v| color_state.set_color(parse_color(v))),
                    ),
                label()
                    .class("swatch fill")
                    .attr("title", crate::i18n::tr("Fill colour"))
                    .text(crate::i18n::tr("Fill"))
                    .child(
                        input()
                            .attr("type", "color")
                            .attr("aria-label", crate::i18n::tr("Fill colour"))
                            .attr_fn("value", move || {
                                color_css(fill_value.get().fill.unwrap_or(0xFF_FFFF))
                            })
                            .on_change(move |v| fill_state.set_fill(parse_color(v))),
                    ),
                button_plain(
                    "No fill",
                    "Remove the fill colour",
                    Box::new(move || none_state.set_fill(None)),
                ),
            ],
        ),
        group(
            "Alignment",
            vec![
                toggle(
                    "Left",
                    "Align left",
                    "align",
                    Box::new(move || a_l.get().align == Align::Left),
                    Box::new(move || s_l.set_align(Align::Left)),
                ),
                toggle(
                    "Center",
                    "Centre",
                    "align",
                    Box::new(move || a_c.get().align == Align::Center),
                    Box::new(move || s_c.set_align(Align::Center)),
                ),
                toggle(
                    "Right",
                    "Align right",
                    "align",
                    Box::new(move || a_r.get().align == Align::Right),
                    Box::new(move || s_r.set_align(Align::Right)),
                ),
                toggle(
                    "Wrap",
                    "Wrap long text",
                    "wrap-btn",
                    Box::new(move || a_w.get().wrap),
                    Box::new(move || s_w.toggle_wrap()),
                ),
            ],
        ),
        group(
            "Number",
            vec![
                select()
                    .class("format-list")
                    .attr("aria-label", crate::i18n::tr("Number format"))
                    .on_change(move |v| format_state.set_number_format(v))
                    .children(preset_options)
                    .child(custom),
                button_plain(
                    ".0+",
                    "More decimal places",
                    Box::new(move || more.more_decimals(1)),
                ),
                button_plain(
                    ".0-",
                    "Fewer decimal places",
                    Box::new(move || fewer.more_decimals(-1)),
                ),
            ],
        ),
        group(
            "Clear",
            vec![button_plain(
                "Clear",
                "Remove colours, fonts, alignment and number format",
                Box::new(move || clear.clear_formatting()),
            )],
        ),
    ]
}

const FONTS: [&str; 10] = [
    "Calibri",
    "Arial",
    "Helvetica",
    "Times New Roman",
    "Georgia",
    "Verdana",
    "Tahoma",
    "Courier New",
    "Cambria",
    "Segoe UI",
];

const SIZES: [u16; 16] = [
    16, 18, 20, 22, 24, 28, 32, 36, 40, 48, 56, 72, 96, 144, 160, 200,
];

fn font_name_list(active: &Memo<Style>, state: &SheetState) -> Element {
    let pick = state.clone();
    let mut options: Vec<View> = vec![
        option()
            .attr("value", "")
            .text(crate::i18n::tr("Default font"))
            .into_view(),
    ];
    for name in FONTS {
        let s = active.clone();
        options.push(
            option()
                .attr("value", name)
                .attr_bool("selected", move || {
                    s.get().font_name.as_deref() == Some(name)
                })
                .text(name)
                .into_view(),
        );
    }
    select()
        .class("format-list font-list")
        .attr("aria-label", crate::i18n::tr("Font"))
        .on_change(move |v| pick.set_font_name((!v.is_empty()).then_some(v)))
        .children(options)
}

fn font_size_list(active: &Memo<Style>, state: &SheetState) -> Element {
    let pick = state.clone();
    let mut options: Vec<View> = vec![
        option()
            .attr("value", "")
            .text(crate::i18n::tr("Size"))
            .into_view(),
    ];
    for half in SIZES {
        let s = active.clone();
        let label = if half % 2 == 0 {
            (half / 2).to_string()
        } else {
            format!("{}.5", half / 2)
        };
        options.push(
            option()
                .attr("value", half.to_string())
                .attr_bool("selected", move || s.get().font_size == Some(half))
                .text(label)
                .into_view(),
        );
    }
    select()
        .class("format-list size-list")
        .attr("aria-label", crate::i18n::tr("Font size"))
        .on_change(move |v| pick.set_font_size(v.parse().ok()))
        .children(options)
}

fn borders_menu(state: &SheetState) -> Element {
    let item = |label: &'static str, mode: BorderMode| {
        let s = state.clone();
        MenuItem::action(label, "", move || s.set_borders(mode))
    };
    office_ui::menu(
        "Borders",
        "Lines around cells",
        vec![
            item("All borders", BorderMode::All),
            item("Outside borders", BorderMode::Outside),
            item("Thick outside borders", BorderMode::ThickOutside),
            MenuItem::Separator,
            item("Top border", BorderMode::Top),
            item("Bottom border", BorderMode::Bottom),
            item("Double bottom border", BorderMode::DoubleBottom),
            item("Left border", BorderMode::Left),
            item("Right border", BorderMode::Right),
            MenuItem::Separator,
            item("No borders", BorderMode::None),
        ],
    )
}

/// The sheet tabs under the grid.
fn tabs(state: &SheetState) -> Element {
    let (names_state, render_state, add_state) = (state.clone(), state.clone(), state.clone());
    let list = View::keyed(
        move || {
            let _ = names_state.rev.get();
            let (renaming, asked) = (names_state.renaming.get(), names_state.pending_delete.get());
            names_state
                .sheet_names()
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    let mode = if renaming == Some(i) {
                        TabMode::Renaming
                    } else if asked == Some(i) {
                        TabMode::Deleting
                    } else {
                        TabMode::Plain
                    };
                    (i, n, mode)
                })
                .collect::<Vec<(usize, String, TabMode)>>()
        },
        |(i, n, m): &(usize, String, TabMode)| format!("{i}:{n}:{m:?}"),
        move |(i, name, mode)| tab(&render_state, i, name, mode),
    );
    div()
        .class("tabs")
        .attr("role", "tablist")
        .child(list)
        .child(
            button()
                .attr("type", "button")
                .attr("title", crate::i18n::tr("Add a sheet"))
                .class("tab add")
                .text(crate::i18n::tr("+"))
                .on_click(move || add_state.add_sheet()),
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabMode {
    Plain,
    Renaming,
    Deleting,
}

/// One tab: a button to switch (double-click renames), a delete mark on the active one, or the field that renames it, or the
/// question before it is deleted.
fn tab(state: &SheetState, i: usize, name: String, mode: TabMode) -> View {
    match mode {
        TabMode::Renaming => {
            let (draft, typed, keys, blur) = (
                state.rename_draft.clone(),
                state.rename_draft.clone(),
                state.clone(),
                state.clone(),
            );
            input()
                .attr("type", "text")
                .class("tab-edit")
                .attr("aria-label", crate::i18n::tr("Sheet name"))
                .attr("spellcheck", "false")
                .attr_fn("value", move || draft.get())
                .on_input(move |v| typed.set(v.to_string()))
                .on_keydown(move |k| match k.key.as_str() {
                    "Enter" => {
                        prevent_default();
                        keys.commit_rename();
                    }
                    "Escape" => {
                        prevent_default();
                        keys.cancel_rename();
                    }
                    _ => {}
                })
                .on(EventKind::Blur, move |_| blur.commit_rename())
                .focus_if(|| true)
                .into_view()
        }
        TabMode::Deleting => {
            let (yes, no) = (state.clone(), state.clone());
            div()
                .class("tab-confirm")
                .child(span().text(format!("Delete \"{name}\"? This cannot be undone.")))
                .child(
                    button()
                        .attr("type", "button")
                        .class("tab danger")
                        .text(crate::i18n::tr("Delete"))
                        .on_click(move || yes.confirm_delete()),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .class("tab")
                        .text(crate::i18n::tr("Keep"))
                        .on_click(move || no.cancel_delete()),
                )
                .into_view()
        }
        TabMode::Plain => {
            let (click, dbl, active, show_close, close) = (
                state.clone(),
                state.clone(),
                state.active.clone(),
                state.clone(),
                state.clone(),
            );
            let is_active = move || active.get() == i;
            let closable = is_active.clone();
            div()
                .class("tab-wrap")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("role", "tab")
                        .class("tab")
                        .class_if("active", is_active)
                        .attr("title", crate::i18n::tr("Double-click to rename"))
                        .text(name)
                        .on_click(move || click.switch_sheet(i))
                        .on(EventKind::DoubleClick, move |_| dbl.begin_rename(i)),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .class("tab-close")
                        .attr("title", crate::i18n::tr("Delete this sheet"))
                        .attr("aria-label", crate::i18n::tr("Delete this sheet"))
                        .attr_bool("hidden", move || {
                            !closable() || show_close.sheet_names().len() < 2
                        })
                        .text(crate::i18n::tr("\u{00d7}"))
                        .on_click(move || close.ask_delete(i)),
                )
                .into_view()
        }
    }
}

fn formula_bar(state: &SheetState) -> Element {
    let name = state.selected.clone();
    let draft = state.draft.clone();
    let (typed_draft, typed_editing, typed_cell) = (
        state.draft.clone(),
        state.editing.clone(),
        state.edit_in_cell.clone(),
    );
    let (focus_editing, focus_cell, keys) = (
        state.editing.clone(),
        state.edit_in_cell.clone(),
        state.clone(),
    );
    let field = input()
        .attr("type", "text")
        .attr("class", "formula")
        .attr("aria-label", crate::i18n::tr("Formula"))
        .attr("spellcheck", "false")
        .attr("autocomplete", "off")
        .attr_fn("value", move || draft.get())
        .on_input(move |v| {
            batch(|| {
                typed_draft.set(v.to_string());
                typed_cell.set(false);
                typed_editing.set(true);
            });
        })
        .on_keydown(move |k| edit_key(&keys, k, false))
        .focus_if(move || focus_editing.get() && !focus_cell.get());
    let (jump, escape) = (state.clone(), state.clone());
    let typed_name = create_signal(String::new());
    let (name_value, name_typed, name_enter) =
        (typed_name.clone(), typed_name.clone(), typed_name.clone());
    let name_box = input()
        .attr("type", "text")
        .class("name")
        .attr("aria-label", crate::i18n::tr("Name box"))
        .attr("spellcheck", "false")
        .attr("autocomplete", "off")
        .attr_fn("value", move || {
            let typed = name_value.get();
            if typed.is_empty() {
                name.get().to_string()
            } else {
                typed
            }
        })
        .on_input(move |v| name_typed.set(v.to_string()))
        .on_keydown(move |k| match k.key.as_str() {
            "Enter" => {
                prevent_default();
                let text = name_enter.get_untracked();
                name_enter.set(String::new());
                if jump.go_to(&text) {
                    jump.refocus();
                }
            }
            "Escape" => {
                prevent_default();
                typed_name.set(String::new());
                escape.refocus();
            }
            _ => {}
        });
    div()
        .class("bar")
        .child(name_box)
        .child(field)
        .child(suggestion_panel(state))
}

/// The names that fit the formula being typed, and the arguments of the function being written.
fn suggestion_panel(state: &SheetState) -> View {
    let (st, draft) = (state.clone(), state.draft.clone());
    View::keyed(
        move || {
            let list = st.suggestion_list();
            let hint = crate::completion::hint(&draft.get());
            if list.is_empty() && hint.is_none() || !st.editing.get() {
                Vec::new()
            } else {
                vec![(list, hint)]
            }
        },
        |(list, hint): &(Vec<String>, Option<(String, usize)>)| format!("{list:?}{hint:?}"),
        {
            let build = state.clone();
            move |(list, hint): (Vec<String>, Option<(String, usize)>)| {
                let mut panel = div().class("suggest").attr("role", "listbox");
                if let Some((signature, arg)) = hint {
                    panel = panel.child(
                        div()
                            .class("suggest-hint")
                            .text(format!("{signature}  (argument {})", arg + 1)),
                    );
                }
                let rows: Vec<View> = list
                    .into_iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let (pick, current) = (build.clone(), build.clone());
                        button()
                            .attr("type", "button")
                            .attr("role", "option")
                            .class("suggest-item")
                            .class_if("current", move || current.suggest_index.get() == i)
                            // The pointer goes down on it before the field loses focus: pressing keeps the edit.
                            .on(EventKind::PointerDown, move |_| pick.accept_suggestion(i))
                            .text(name)
                            .into_view()
                    })
                    .collect();
                panel.children(rows).into_view()
            }
        },
    )
}

/// Enter, Tab and Escape in either text field commit or drop the edit.
/// `in_grid`: the field is inside the grid, so a key it uses will bubble up to the grid's own handler, which must skip it.
fn edit_key(state: &SheetState, k: &KeyEvent, in_grid: bool) {
    if in_grid && matches!(k.key.as_str(), "Enter" | "Tab" | "Escape") {
        state.swallow_key();
    }
    // While names are suggested, Tab takes the picked one and the arrows pick.
    let suggested = state.suggestion_list();
    if !suggested.is_empty() {
        let at = state.suggest_index.get_untracked().min(suggested.len() - 1);
        match k.key.as_str() {
            "Tab" if !k.shift => {
                prevent_default();
                state.accept_suggestion(at);
                return;
            }
            "ArrowDown" => {
                prevent_default();
                state.suggest_index.set((at + 1) % suggested.len());
                return;
            }
            "ArrowUp" => {
                prevent_default();
                state
                    .suggest_index
                    .set((at + suggested.len() - 1) % suggested.len());
                return;
            }
            _ => {}
        }
    }
    match k.key.as_str() {
        "Enter" => {
            prevent_default();
            state.commit(if k.shift { -1 } else { 1 }, 0);
        }
        "Tab" => {
            prevent_default();
            state.commit(0, if k.shift { -1 } else { 1 });
        }
        "Escape" => {
            prevent_default();
            state.cancel_edit();
        }
        _ => {}
    }
}

/// A border handle on a header: dragging it resizes, a double click fits the content.
fn handle(state: &SheetState, row: bool, index: u32) -> Element {
    let (down, fit) = (state.clone(), state.clone());
    div()
        .class(if row {
            "hdr-handle row"
        } else {
            "hdr-handle col"
        })
        .attr("role", "presentation")
        .on(EventKind::PointerDown, move |e| {
            if let Event::PointerDown(p) = e {
                down.begin_resize(row, index, if row { p.y } else { p.x });
            }
        })
        .on(EventKind::DoubleClick, move |_| fit.autofit(row, index))
}

fn column_headers(state: &SheetState, window: &Memo<Window>, scroll_left: &Signal<f64>) -> Element {
    let (w1, w2, w3, left, st, axes_a, axes_b) = (
        window.clone(),
        window.clone(),
        window.clone(),
        scroll_left.clone(),
        state.clone(),
        state.axes.clone(),
        state.axes.clone(),
    );
    let head = {
        let st = st;
        move |c: u32| {
            let (click, press, menu) = (st.clone(), st.clone(), st.clone());
            let level = {
                let st = st.clone();
                create_memo(move || {
                    let _ = st.rev.get();
                    st.level_of(false, c)
                })
            };
            div()
                .attr_fn("class", move || match level.get() {
                    0 => "colhdr".to_string(),
                    n => format!("colhdr lvl lvl{n}"),
                })
                .text(col_name(c))
                .on(EventKind::ContextMenu, move |e| {
                    if let Event::ContextMenu(p) = e {
                        menu.open_header_context(false, c, p.x, p.y);
                    }
                })
                .on(EventKind::PointerDown, move |_| press.clear_resized())
                .on_click(move || {
                    if !click.take_resized() {
                        click.select_range(Range::new(
                            CellRef::new(0, c),
                            CellRef::new(ROWS - 1, c),
                        ));
                    }
                })
                .child(handle(&st, false, c))
                .into_view()
        }
    };
    let pinned = pinned_headers(state, false, head.clone());
    let heads = View::keyed(
        move || {
            let (_, _, c0, c1) = w1.get();
            (c0..c1).collect::<Vec<u32>>()
        },
        |c: &u32| *c,
        head,
    );
    let inner = div()
        .class("hdr-window")
        .css("transform", move || {
            let a = axes_a.get();
            format!("translateX({}px)", a.cols.offset(w2.get().2) - left.get())
        })
        .css("grid-template-columns", move || {
            let (_, _, c0, c1) = w3.get();
            let a = axes_b.get();
            tracks(c0, c1, |c| a.cols.size(c))
        })
        .child(heads);
    div()
        .class("colhdrs")
        .attr("role", "presentation")
        .child(inner)
        .child(pinned)
}

/// The headers of the frozen rows (or columns): they stay where they are while the others scroll under them.
fn pinned_headers(
    state: &SheetState,
    rows: bool,
    head: impl Fn(u32) -> View + Clone + 'static,
) -> View {
    let (axes, tracks_axes) = (state.axes.clone(), state.axes.clone());
    View::keyed(
        move || {
            let f = axes.get().frozen;
            let n = if rows { f.0 } else { f.1 };
            if n == 0 { Vec::new() } else { vec![n] }
        },
        |n: &u32| *n,
        move |n| {
            let a = tracks_axes.clone();
            let template = move || {
                tracks(0, n, |i| {
                    let axes = a.get();
                    if rows {
                        axes.rows.size(i)
                    } else {
                        axes.cols.size(i)
                    }
                })
            };
            let el = div().class("hdr-window pinned");
            let el = if rows {
                el.css("grid-template-rows", template)
            } else {
                el.css("grid-template-columns", template)
            };
            el.children((0..n).map(&head)).into_view()
        },
    )
}

fn row_headers(state: &SheetState, window: &Memo<Window>, scroll_top: &Signal<f64>) -> Element {
    let (w1, w2, w3, top, st, axes_a, axes_b) = (
        window.clone(),
        window.clone(),
        window.clone(),
        scroll_top.clone(),
        state.clone(),
        state.axes.clone(),
        state.axes.clone(),
    );
    let head = {
        let st = st;
        move |r: u32| {
            let (click, press, menu) = (st.clone(), st.clone(), st.clone());
            let level = {
                let st = st.clone();
                create_memo(move || {
                    let _ = st.rev.get();
                    st.level_of(true, r)
                })
            };
            div()
                .attr_fn("class", move || match level.get() {
                    0 => "rowhdr".to_string(),
                    n => format!("rowhdr lvl lvl{n}"),
                })
                .text((r + 1).to_string())
                .on(EventKind::ContextMenu, move |e| {
                    if let Event::ContextMenu(p) = e {
                        menu.open_header_context(true, r, p.x, p.y);
                    }
                })
                .on(EventKind::PointerDown, move |_| press.clear_resized())
                .on_click(move || {
                    if !click.take_resized() {
                        click.select_range(Range::new(
                            CellRef::new(r, 0),
                            CellRef::new(r, crate::state::COLS - 1),
                        ));
                    }
                })
                .child(handle(&st, true, r))
                .into_view()
        }
    };
    let pinned = pinned_headers(state, true, head.clone());
    let heads = View::keyed(
        move || {
            let (r0, r1, _, _) = w1.get();
            (r0..r1).collect::<Vec<u32>>()
        },
        |r: &u32| *r,
        head,
    );
    let inner = div()
        .class("hdr-window")
        .css("transform", move || {
            let a = axes_a.get();
            format!("translateY({}px)", a.rows.offset(w2.get().0) - top.get())
        })
        .css("grid-template-rows", move || {
            let (r0, r1, _, _) = w3.get();
            let a = axes_b.get();
            tracks(r0, r1, |r| a.rows.size(r))
        })
        .child(heads);
    div()
        .class("rowhdrs")
        .attr("role", "presentation")
        .child(inner)
        .child(pinned)
}

#[allow(clippy::too_many_lines)]
fn grid(
    state: &SheetState,
    window: &Memo<Window>,
    top: &Signal<f64>,
    left: &Signal<f64>,
    viewport: &Signal<(f64, f64)>,
) -> Element {
    let shown = window_grid(state, window, None, None);
    let axes_w = state.axes.clone();
    let axes_h = state.axes.clone();

    let sizer = div()
        .class("sizer")
        .attr("role", "presentation")
        .css("width", move || format!("{}px", axes_w.get().cols.total()))
        .css("height", move || format!("{}px", axes_h.get().rows.total()))
        .child(shown)
        .child(merged_layer(state, window, None, None))
        .child(range_overlay(state))
        .child(cursor(state))
        .child(focus_target(state))
        .child(cell_editor(state))
        .child(filter_layer(state, window))
        .child(filter_panel(state))
        .child(dropdown_layer(state))
        .child(charts_layer(state))
        .child(chart_ghost(state))
        .child(panes(state, window));

    let (top, left, vp) = (top.clone(), left.clone(), viewport.clone());
    let (zoom_scroll, zoom_size, zoom_pad_top, zoom_pad_left, zoom_wheel) = (
        state.zoom.clone(),
        state.zoom.clone(),
        state.zoom.clone(),
        state.zoom.clone(),
        state.clone(),
    );
    let (keys, key_up, up, copy, cut, paste) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (pad_top, pad_left) = (state.axes.clone(), state.axes.clone());
    div()
        .class("viewport")
        .attr("role", "grid")
        .attr("tabindex", "0")
        // Moving into view lands beyond the frozen rows and columns, not under them.
        .css("scroll-padding-top", move || {
            let a = pad_top.get();
            let z = f64::from(zoom_pad_top.get()) / 100.0;
            format!("{}px", a.rows.offset(a.frozen.0) * z)
        })
        .css("scroll-padding-left", move || {
            let a = pad_left.get();
            let z = f64::from(zoom_pad_left.get()) / 100.0;
            format!("{}px", a.cols.offset(a.frozen.1) * z)
        })
        // Ctrl and the wheel zoom the grid (the browser's own page zoom does not get it).
        .on(EventKind::Wheel, move |e| {
            if let Event::Wheel(w) = e
                && (w.ctrl || w.meta)
            {
                prevent_default();
                zoom_wheel.zoom_by(if w.dy > 0.0 { -1 } else { 1 });
            }
        })
        .on_keydown(move |k| {
            keys.set_shift(k.shift);
            // The cell editor is inside the grid, so its keys arrive here too.
            if !keys.skip_key() && key_down(&keys, k) {
                prevent_default();
            }
        })
        .on(EventKind::KeyUp, move |e| {
            if let Event::KeyUp(k) = e {
                key_up.set_shift(k.shift);
            }
        })
        .on(EventKind::PointerUp, move |_| up.pointer_up())
        // Copy, cut and paste reach the grid while focus is in it; in the formula bar they act on its text as usual.
        .on(EventKind::Copy, move |_| {
            if copy.editing.get_untracked() {
                return;
            }
            if let Some((text, html)) = copy.copy(false) {
                set_clipboard(&text, &html);
            }
        })
        .on(EventKind::Cut, move |_| {
            if cut.editing.get_untracked() {
                return;
            }
            if let Some((text, html)) = cut.copy(true) {
                set_clipboard(&text, &html);
            }
        })
        .on(EventKind::Paste, move |e| {
            if let Event::Paste(p) = e
                && !paste.editing.get_untracked()
                && paste.paste(&p.text)
            {
                prevent_default();
            }
        })
        .attr("aria-rowcount", ROWS.to_string())
        .attr("aria-colcount", COLS.to_string())
        .on(EventKind::Scroll, move |e| {
            if let Event::Scroll(s) = e {
                let z = f64::from(zoom_scroll.get_untracked()) / 100.0;
                batch(|| {
                    top.set(s.top / z);
                    left.set(s.left / z);
                });
            }
        })
        .on(EventKind::Resize, move |e| {
            if let Event::Resize(s) = e {
                let z = f64::from(zoom_size.get_untracked()) / 100.0;
                vp.set((s.width / z, s.height / z));
            }
        })
        .child(sizer)
}

/// The cells of a pane as one CSS grid: the rows and columns in view, or only the first `n` rows (or columns) for a frozen part.
fn window_grid(
    state: &SheetState,
    window: &Memo<Window>,
    fixed_rows: Option<u32>,
    fixed_cols: Option<u32>,
) -> Element {
    let (cells_window, shift_window, cols_window, rows_window) = (
        window.clone(),
        window.clone(),
        window.clone(),
        window.clone(),
    );
    let (axes_t, axes_c, axes_r) = (state.axes.clone(), state.axes.clone(), state.axes.clone());
    let span = move |w: Window| {
        let (r0, r1) = fixed_rows.map_or((w.0, w.1), |n| (0, n));
        let (c0, c1) = fixed_cols.map_or((w.2, w.3), |n| (0, n));
        (r0, r1, c0, c1)
    };
    let cell_state = state.clone();
    let cells = View::keyed(
        move || {
            let (r0, r1, c0, c1) = span(cells_window.get());
            (r0..r1)
                .flat_map(|r| (c0..c1).map(move |c| CellRef::new(r, c)))
                .collect::<Vec<_>>()
        },
        |at: &CellRef| key_of(*at),
        move |at| cell(&cell_state, at),
    );
    div()
        .class("window")
        .attr("role", "presentation")
        .css("transform", move || {
            let (r0, _, c0, _) = span(shift_window.get());
            let a = axes_t.get();
            format!(
                "translate({}px, {}px)",
                a.cols.offset(c0),
                a.rows.offset(r0)
            )
        })
        .css("grid-template-columns", move || {
            let (_, _, c0, c1) = span(cols_window.get());
            let a = axes_c.get();
            tracks(c0, c1, |c| a.cols.size(c))
        })
        .css("grid-template-rows", move || {
            let (r0, r1, _, _) = span(rows_window.get());
            let a = axes_r.get();
            tracks(r0, r1, |r| a.rows.size(r))
        })
        .child(cells)
}

/// Merged ranges that are in view, drawn over their cells as one cell. The pointer passes through them to the cells below,
/// which select the whole merged range.
fn merged_layer(
    state: &SheetState,
    window: &Memo<Window>,
    fixed_rows: Option<u32>,
    fixed_cols: Option<u32>,
) -> View {
    let (st, w, build) = (state.clone(), window.clone(), state.clone());
    View::keyed(
        move || {
            let _ = st.rev.get();
            let (wr0, wr1, wc0, wc1) = w.get();
            let (r0, r1) = fixed_rows.map_or((wr0, wr1), |n| (0, n));
            let (c0, c1) = fixed_cols.map_or((wc0, wc1), |n| (0, n));
            st.merges()
                .into_iter()
                .filter(|m| {
                    m.start.row < r1 && m.end.row >= r0 && m.start.col < c1 && m.end.col >= c0
                })
                .collect::<Vec<Range>>()
        },
        |m: &Range| key_of(m.start) ^ key_of(m.end).rotate_left(21),
        move |m| {
            place(
                div().class("merged").attr("role", "presentation"),
                &build,
                move || m,
            )
            .child(cell(&build, m.start))
            .into_view()
        },
    )
}

/// The frozen rows and columns: parts of the sheet that stay in view while the rest scrolls. Each is a zero-size element that
/// sticks to the edge of the scroll area and holds a box showing its cells at their place in the sheet.
fn panes(state: &SheetState, window: &Memo<Window>) -> View {
    let (axes, st, w) = (state.axes.clone(), state.clone(), window.clone());
    View::keyed(
        move || {
            let f = axes.get().frozen;
            if f == (0, 0) { Vec::new() } else { vec![f] }
        },
        |f: &(u32, u32)| u64::from(f.0) << 20 | u64::from(f.1),
        move |(rows, cols)| {
            let mut out: Vec<View> = Vec::new();
            let pane =
                |class: &'static str, sticky: &'static str, fr: Option<u32>, fc: Option<u32>| {
                    let (a1, a2, a3, a4) = (
                        st.axes.clone(),
                        st.axes.clone(),
                        st.axes.clone(),
                        st.axes.clone(),
                    );
                    let inner = div()
                        .class(format!("pane-box {class}"))
                        .attr("role", "presentation")
                        .css("width", move || {
                            let a = a1.get();
                            format!(
                                "{}px",
                                fc.map_or_else(|| a.cols.total(), |n| a.cols.offset(n))
                            )
                        })
                        .css("height", move || {
                            let a = a2.get();
                            format!(
                                "{}px",
                                fr.map_or_else(|| a.rows.total(), |n| a.rows.offset(n))
                            )
                        })
                        .child(window_grid(&st, &w, fr, fc))
                        .child(merged_layer(&st, &w, fr, fc))
                        .child(range_overlay(&st))
                        .child(cursor(&st));
                    let _ = (&a3, &a4);
                    div()
                        .class(format!("pane {sticky}"))
                        .attr("role", "presentation")
                        .child(inner)
                        .into_view()
                };
            if rows > 0 {
                out.push(pane("top", "pane-top", Some(rows), None));
            }
            if cols > 0 {
                out.push(pane("left", "pane-left", None, Some(cols)));
            }
            if rows > 0 && cols > 0 {
                out.push(pane("corner", "pane-corner", Some(rows), Some(cols)));
            }
            div().class("panes").children(out).into_view()
        },
    )
}

fn cell(state: &SheetState, at: CellRef) -> View {
    // One memo per cell: it looks again when the workbook changes, but tells the element only if what it shows or how it
    // looks is different.
    let look = {
        let st = state.clone();
        create_memo(move || {
            let _ = st.rev.get();
            st.look(at)
        })
    };
    // What does not change with the text: colours, lines, font. Its own memo, so a new value in the cell leaves it alone.
    let deco: Memo<Deco> = {
        let l = look.clone();
        create_memo(move || Deco::of(&l.get()))
    };
    let (class, text) = (look.clone(), look);
    let (color, fill, b_width, b_style, b_color, size, font, clip, bar, note) = (
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco.clone(),
        deco,
    );
    let (down, over, dbl, menu) = (state.clone(), state.clone(), state.clone(), state.clone());
    div()
        .attr_fn("class", move || class.get().class())
        .attr("role", "gridcell")
        .attr("aria-rowindex", (at.row + 1).to_string())
        .attr("aria-colindex", (at.col + 1).to_string())
        .css("color", move || color.get().color)
        .css("background-color", move || fill.get().fill)
        .css("border-width", move || b_width.get().border_width)
        .css("border-style", move || b_style.get().border_style)
        .css("border-color", move || b_color.get().border_color)
        .attr_fn("title", move || note.get().note)
        .css("clip-path", move || clip.get().spill)
        .css("background-image", move || {
            let d = bar.get();
            [d.spark, d.bar]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .css("font-size", move || size.get().size)
        .css("font-family", move || font.get().font)
        .text_fn(move || text.get().text)
        .on(EventKind::PointerDown, move |_| down.pointer_down(at))
        .on(EventKind::ContextMenu, move |e| {
            if let Event::ContextMenu(p) = e {
                menu.open_context(at, p.x, p.y);
            }
        })
        .on(EventKind::PointerMove, move |_| over.pointer_over(at))
        .on(EventKind::DoubleClick, move |_| {
            dbl.click_cell(at);
            dbl.begin_edit(None);
        })
        .into_view()
}

/// The parts of a cell's look that are CSS properties.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Deco {
    color: String,
    fill: String,
    border_width: String,
    border_style: String,
    border_color: String,
    spill: String,
    note: String,
    bar: String,
    spark: String,
    size: String,
    font: String,
}

impl Deco {
    fn of(look: &crate::state::Look) -> Self {
        Self {
            color: look.color.map(color_css).unwrap_or_default(),
            fill: look.fill.map(color_css).unwrap_or_default(),
            border_width: border_css(&look.border, Part::Width),
            border_style: border_css(&look.border, Part::Style),
            border_color: border_css(&look.border, Part::Color),
            bar: look.bar.map_or_else(String::new, |(part, color)| {
                format!(
                    "linear-gradient(to right, {} {part}%, transparent {part}%)",
                    color_css(color)
                )
            }),
            spark: look
                .spark
                .as_ref()
                .map_or_else(String::new, |u| format!("url(\"{u}\")")),
            note: look.note.clone().unwrap_or_default(),
            spill: if look.spill > 0 {
                format!("inset(0 -{}px 0 0)", look.spill)
            } else {
                String::new()
            },
            size: look
                .size
                .map(|h| format!("{}px", f64::from(h) / 2.0 * 4.0 / 3.0))
                .unwrap_or_default(),
            font: look.font.as_deref().map(font_stack).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy)]
enum Part {
    Width,
    Style,
    Color,
}

/// One of the three four-sided CSS border properties (top right bottom left) for a cell's border, or nothing when the cell
/// has none (the grid line the class draws stays). Sides without a line keep the grid line on the right and bottom.
fn border_css(b: &sheets_model::Border, part: Part) -> String {
    use sheets_model::LineKind;
    if b.is_none() {
        return String::new();
    }
    let side = |line: Option<sheets_model::Line>, grid: bool| match (line, part) {
        (Some(l), Part::Width) => match l.kind {
            LineKind::Thin | LineKind::Dashed | LineKind::Dotted => "1px",
            LineKind::Medium => "2px",
            LineKind::Thick | LineKind::Double => "3px",
        }
        .to_string(),
        (Some(l), Part::Style) => match l.kind {
            LineKind::Dashed => "dashed",
            LineKind::Dotted => "dotted",
            LineKind::Double => "double",
            _ => "solid",
        }
        .to_string(),
        (Some(l), Part::Color) => l
            .color
            .map_or_else(|| "var(--office-fg)".to_string(), color_css),
        (None, Part::Width) => if grid { "1px" } else { "0" }.to_string(),
        (None, Part::Style) => if grid { "solid" } else { "none" }.to_string(),
        (None, Part::Color) => "var(--office-grid)".to_string(),
    };
    format!(
        "{} {} {} {}",
        side(b.top, false),
        side(b.right, true),
        side(b.bottom, true),
        side(b.left, false)
    )
}

/// The CSS font list for a font name: the name, then the free fonts with the same metrics, then the generic family.
fn font_stack(name: &str) -> String {
    let fallback = match name {
        "Calibri" => "Carlito, sans-serif",
        "Arial" | "Helvetica" => "'Liberation Sans', Arimo, sans-serif",
        "Times New Roman" => "'Liberation Serif', Tinos, serif",
        "Courier New" => "'Liberation Mono', Cousine, monospace",
        "Cambria" => "Caladea, serif",
        "Georgia" | "Garamond" => "serif",
        _ => "sans-serif",
    };
    format!("'{}', {fallback}", name.replace('\'', ""))
}

/// Four reactive CSS properties that put an element over cells `from..=to`.
#[allow(clippy::many_single_char_names)]
fn place(el: Element, state: &SheetState, rect: impl Fn() -> Range + Clone + 'static) -> Element {
    let (a, b, c, d) = (
        state.axes.clone(),
        state.axes.clone(),
        state.axes.clone(),
        state.axes.clone(),
    );
    let (ra, rb, rc, rd) = (rect.clone(), rect.clone(), rect.clone(), rect);
    el.css("left", move || {
        format!("{}px", a.get().cols.offset(ra().start.col))
    })
    .css("top", move || {
        format!("{}px", b.get().rows.offset(rb().start.row))
    })
    .css("width", move || {
        let (axes, r) = (c.get(), rc());
        format!(
            "{}px",
            axes.cols.offset(r.end.col + 1) - axes.cols.offset(r.start.col)
        )
    })
    .css("height", move || {
        let (axes, r) = (d.get(), rd());
        format!(
            "{}px",
            axes.rows.offset(r.end.row + 1) - axes.rows.offset(r.start.row)
        )
    })
}

/// The rectangle of the selection, drawn over the cells when it is more than one, with the handle at its corner that fills
/// further cells when dragged.
fn range_overlay(state: &SheetState) -> Element {
    let st = state.clone();
    let rect = move || {
        let _ = (st.selected.get(), st.range_end.get());
        st.selection()
    };
    let multi = rect.clone();
    let press = state.clone();
    place(
        div()
            .class("range")
            .attr("role", "presentation")
            .attr_fn("data-multi", move || {
                let r = multi();
                (r.start != r.end).to_string()
            }),
        state,
        rect,
    )
    .child(
        div()
            .class("fill-handle")
            .attr("role", "presentation")
            .attr("title", crate::i18n::tr("Drag to fill"))
            .on(EventKind::PointerDown, move |_| press.begin_fill()),
    )
    .child(fill_preview(state))
}

/// The cells a fill being dragged would cover.
fn fill_preview(state: &SheetState) -> View {
    let (drag, st) = (state.fill_drag.clone(), state.clone());
    View::keyed(
        move || drag.get().into_iter().collect::<Vec<CellRef>>(),
        |at: &CellRef| key_of(*at),
        move |to| {
            let source = st.selection();
            let (a, b) = (st.axes.get_untracked(), st.axes.get_untracked());
            // The rectangle from the selection out to the pointer, along the longer way.
            let rows = if to.row > source.end.row {
                to.row - source.end.row
            } else {
                source.start.row.saturating_sub(to.row)
            };
            let cols = if to.col > source.end.col {
                to.col - source.end.col
            } else {
                source.start.col.saturating_sub(to.col)
            };
            let target = if rows >= cols {
                Range::new(
                    CellRef::new(source.start.row.min(to.row), source.start.col),
                    CellRef::new(source.end.row.max(to.row), source.end.col),
                )
            } else {
                Range::new(
                    CellRef::new(source.start.row, source.start.col.min(to.col)),
                    CellRef::new(source.end.row, source.end.col.max(to.col)),
                )
            };
            // Relative to the selection's own box, which this sits in.
            let (left, top) = (
                a.cols.offset(target.start.col) - a.cols.offset(source.start.col),
                a.rows.offset(target.start.row) - a.rows.offset(source.start.row),
            );
            let (width, height) = (
                b.cols.offset(target.end.col + 1) - b.cols.offset(target.start.col),
                b.rows.offset(target.end.row + 1) - b.rows.offset(target.start.row),
            );
            div()
                .class("fill-preview")
                .attr("role", "presentation")
                .css("left", {
                    let v = format!("{left}px");
                    move || v.clone()
                })
                .css("top", {
                    let v = format!("{top}px");
                    move || v.clone()
                })
                .css("width", {
                    let v = format!("{width}px");
                    move || v.clone()
                })
                .css("height", {
                    let v = format!("{height}px");
                    move || v.clone()
                })
                .into_view()
        },
    )
}

/// The active cell.
fn cursor(state: &SheetState) -> Element {
    let (selected, st) = (state.selected.clone(), state.clone());
    place(
        div().class("cursor").attr("role", "presentation"),
        state,
        move || {
            let at = selected.get();
            let _ = st.rev.get();
            st.merge_span(at).unwrap_or_else(|| Range::new(at, at))
        },
    )
}

/// An invisible element that takes keyboard focus at the moving end of the selection, so that the browser scrolls it into
/// view. It is rebuilt (and takes focus) whenever the selection moves, but never on first load.
fn focus_target(state: &SheetState) -> View {
    let (end, epoch, focus, st) = (
        state.focus_at.clone(),
        state.cursor_epoch.clone(),
        state.focus_cursor.clone(),
        state.clone(),
    );
    View::keyed(
        move || vec![(end.get(), epoch.get())],
        |(at, e): &(CellRef, u32)| key_of(*at) << 8 ^ u64::from(*e),
        move |(at, _)| {
            let focus = focus.clone();
            place(
                div()
                    .class("focus-target")
                    .attr("role", "presentation")
                    .attr("tabindex", "-1"),
                &st,
                move || Range::new(at, at),
            )
            .focus_if(move || focus.get())
            .into_view()
        },
    )
}

/// The text field over the active cell while it is being edited in the grid. It is built when editing starts (and takes
/// focus then, never at first load) and removed when it ends; the formula bar shows the same text.
fn cell_editor(state: &SheetState) -> View {
    let (editing, in_cell, selected, epoch, st) = (
        state.editing.clone(),
        state.edit_in_cell.clone(),
        state.selected.clone(),
        state.cursor_epoch.clone(),
        state.clone(),
    );
    View::keyed(
        move || {
            if editing.get() && in_cell.get() {
                vec![(selected.get_untracked(), epoch.get_untracked())]
            } else {
                Vec::new()
            }
        },
        |(at, e): &(CellRef, u32)| key_of(*at) << 8 ^ u64::from(*e),
        move |(at, _)| {
            let (draft, typed, keys) = (st.draft.clone(), st.draft.clone(), st.clone());
            place(
                input()
                    .attr("type", "text")
                    .attr("class", "cell-editor")
                    .attr("aria-label", crate::i18n::tr("Cell"))
                    .attr("spellcheck", "false")
                    .attr("autocomplete", "off")
                    .attr_fn("value", move || draft.get())
                    .on_input(move |v| typed.set(v.to_string()))
                    .on_keydown(move |k| edit_key(&keys, k, true))
                    .focus_if(|| true),
                &st,
                move || Range::new(at, at),
            )
            .into_view()
        },
    )
}

/// What a key does with the cursor. Returns whether it was used (and the browser's own action should not run).
fn key_down(state: &SheetState, k: &KeyEvent) -> bool {
    let command = k.ctrl || k.meta;
    let page = i64::from(state.page_rows.get_untracked());
    let step = |rows: i64, cols: i64| {
        if k.shift {
            state.extend_by(rows, cols);
        } else {
            state.move_by(rows, cols);
        }
    };
    match k.key.as_str() {
        "ArrowUp" if command => state.jump(-1, 0, k.shift),
        "ArrowDown" if command => state.jump(1, 0, k.shift),
        "ArrowLeft" if command => state.jump(0, -1, k.shift),
        "ArrowRight" if command => state.jump(0, 1, k.shift),
        "ArrowUp" => step(-1, 0),
        "ArrowDown" => step(1, 0),
        "ArrowLeft" => step(0, -1),
        "ArrowRight" => step(0, 1),
        "End" if command => state.select_last_used(),
        "PageDown" => step(page, 0),
        "PageUp" => step(-page, 0),
        "Tab" => state.move_by(0, if k.shift { -1 } else { 1 }),
        "Enter" => state.move_by(if k.shift { -1 } else { 1 }, 0),
        "Home" if command => state.select(CellRef::new(0, 0)),
        "Home" => state.select(CellRef::new(state.selected.get_untracked().row, 0)),
        "F2" => state.begin_edit(None),
        "Delete" | "Backspace" if state.chart_selected.get_untracked().is_some() => {
            state.delete_chart()
        }
        "Delete" | "Backspace" => state.clear_selected(),
        "a" | "A" if command => state.select_all(),
        "z" | "Z" if command => {
            if k.shift {
                state.redo();
            } else {
                state.undo();
            }
        }
        "y" | "Y" if command => state.redo(),
        "b" | "B" if command => state.toggle_bold(),
        "i" | "I" if command => state.toggle_italic(),
        "u" | "U" if command => state.toggle_underline(),
        "s" | "S" if command => {
            if !office_cloud_ui::save_shortcut() {
                state.save_xlsx();
            }
        }
        "p" | "P" if command => state.print(),
        "+" if command => state.insert_selected(state.structure_axis()),
        "=" if command && k.shift => state.insert_selected(state.structure_axis()),
        "-" if command => state.delete_selected(state.structure_axis()),
        "o" | "O" if command => state.open_file(),
        "f" | "F" if command => state.open_find(false),
        "h" | "H" if command => state.open_find(true),
        "t" | "T" if command => state.insert_table(),
        "d" | "D" if command => state.fill(true),
        "r" | "R" if command => state.fill(false),
        "F3" => {
            state.find_next(!k.shift);
        }
        "Escape" if state.find_open.get_untracked() => state.close_find(),
        // Copy, cut and paste are left to the browser: they arrive as their own events.
        "c" | "C" | "x" | "X" | "v" | "V" if command => return false,
        // A typed character starts an edit with an empty bar and is not cancelled: the browser then types it into the bar,
        // which has just taken focus, and the caret ends up after it. (Setting the bar's text from here would leave the
        // caret at the start.)
        key if !command && !k.alt && key.chars().count() == 1 => {
            state.begin_edit(Some(""));
            return false;
        }
        _ => return false,
    }
    true
}

/// The shortcut shown beside saving a file: Ctrl+S, unless it saves to the cloud (then the cloud's item has it).
fn file_hint() -> &'static str {
    if office_cloud_ui::saves_to_cloud() {
        ""
    } else {
        "Ctrl+S"
    }
}
