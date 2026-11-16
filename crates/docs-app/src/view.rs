//! The screen: a small ribbon over the editable page.

use crate::state::{DocState, Range};
use docs_text::{Align, Family, Kind, Mark, Para};
use office_ui::MenuItem;
use quench_ui::prelude::*;
use quench_ui::{BlockPoint, EditSelectionEvent};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// The whole screen.
#[must_use]
pub fn docs_view(state: &DocState) -> View {
    let state = state.clone();
    View::dynamic(move || {
        let _ = state.language.get();
        screen(&state)
    })
}

fn screen(state: &DocState) -> View {
    div()
        .class("docs")
        .child(office_cloud_ui::overlay())
        .child(ribbon(state))
        .child(find_bar(state))
        .child(link_bar(state))
        .child(header_bar(state))
        .child({
            let z = state.zoom.clone();
            div()
                .class("canvas")
                .attr("role", "main")
                .css("zoom", move || format!("{}%", z.get()))
                .child(canvas(state))
        })
        .into_view()
}

fn point(p: BlockPoint) -> docs_text::Pos {
    docs_text::Pos {
        para: p.block,
        offset: p.offset,
    }
}

fn ribbon(state: &DocState) -> Element {
    let name = state.name.clone();
    let status = state.status.clone();
    let (new, open, docx, txt, md) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (pdf, print) = (state.clone(), state.clone());
    let file = office_ui::menu(
        "File",
        "New, open and save",
        office_cloud_ui::with_menu(vec![
            MenuItem::action("New", "", move || new.new_document()),
            MenuItem::action("Open\u{2026}", "", move || open.open_file()),
            MenuItem::Separator,
            MenuItem::action("Save as Word document (.docx)", "Ctrl+S", move || {
                docx.save_docx()
            }),
            MenuItem::action("Save as web page (.html)", "", {
                let html = state.clone();
                move || html.save_html()
            }),
            MenuItem::action("Save as Markdown (.md)", "", move || md.save_md()),
            MenuItem::action("Save as plain text (.txt)", "", move || txt.save_txt()),
            MenuItem::action("Export as PDF (.pdf)", "", move || pdf.export_pdf()),
            MenuItem::Separator,
            MenuItem::action("Print\u{2026}", "Ctrl+P", move || print.print()),
        ]),
    );
    let theme = extras(state);
    let (u, r) = (state.clone(), state.clone());
    let (can_u, can_r) = (state.clone(), state.clone());
    // A tool shows a drawing when it has one (`icon`), else its words; the tip is its tooltip and its accessible name.
    let mark = |label: &'static str,
                tip: &'static str,
                class: &'static str,
                icon: &'static str,
                m: Mark| {
        let (click, shown) = (state.clone(), state.clone());
        office_ui::looks(
            button()
                .attr("type", "button")
                .attr("title", crate::i18n::tr(tip))
                .attr("aria-label", crate::i18n::tr(tip))
                .class("tool")
                .class(class)
                // The page keeps the selection: a button does not take the focus from it.
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .class_if("on", move || shown.marked(m))
                .on_click(move || click.toggle_selected(m)),
            icon,
            label,
            tip,
        )
        .into_view()
    };
    let kind = |label: &'static str, tip: &'static str, icon: &'static str, k: Kind| {
        let (st, shown) = (state.clone(), state.clone());
        office_ui::looks(
            button()
                .attr("type", "button")
                .attr("title", crate::i18n::tr(tip))
                .attr("aria-label", crate::i18n::tr(tip))
                .class("tool")
                .class_if("on", move || {
                    let _ = shown.rev.get();
                    shown.kind_here() == k
                })
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .on_click(move || st.set_kind(k)),
            icon,
            label,
            tip,
        )
        .into_view()
    };
    let history = office_ui::group(
        "History",
        vec![
            button()
                .attr("type", "button")
                .class("tool")
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .attr_bool("disabled", move || {
                    let _ = can_u.rev.get();
                    !can_u.can_undo()
                })
                .attr("title", crate::i18n::tr("Undo"))
                .attr("aria-label", crate::i18n::tr("Undo"))
                .class("icon-tool")
                .child(office_ui::icon("undo"))
                .on_click(move || u.undo())
                .into_view(),
            button()
                .attr("type", "button")
                .class("tool")
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .attr_bool("disabled", move || {
                    let _ = can_r.rev.get();
                    !can_r.can_redo()
                })
                .attr("title", crate::i18n::tr("Redo"))
                .attr("aria-label", crate::i18n::tr("Redo"))
                .class("icon-tool")
                .child(office_ui::icon("redo"))
                .on_click(move || r.redo())
                .into_view(),
        ],
    );
    let font = office_ui::group(
        "Font",
        vec![
            mark("B", "Bold (Ctrl+B)", "bold", "", Mark::Bold),
            mark("I", "Italic (Ctrl+I)", "italic", "", Mark::Italic),
            mark("U", "Underline (Ctrl+U)", "underline", "", Mark::Underline),
            mark("S", "Strikethrough", "strike", "", Mark::Strike),
            mark("x\u{b2}", "Superscript", "sup", "", Mark::Superscript),
            mark("x\u{2082}", "Subscript", "sub", "", Mark::Subscript),
            mark(
                "Highlight",
                "Highlight",
                "highlight",
                "highlight",
                Mark::Highlight,
            ),
        ],
    );
    let align = |label: &'static str, tip: &'static str, icon: &'static str, a: Align| {
        let (click, shown) = (state.clone(), state.clone());
        office_ui::looks(
            button()
                .attr("type", "button")
                .attr("title", crate::i18n::tr(tip))
                .attr("aria-label", crate::i18n::tr(tip))
                .class("tool")
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .class_if("on", move || {
                    let _ = shown.rev.get();
                    shown.align_here() == a
                })
                .on_click(move || click.set_align(a)),
            icon,
            label,
            tip,
        )
        .into_view()
    };
    let paragraph = office_ui::group(
        "Paragraph",
        vec![
            kind("H1", "Heading 1: a large heading", "", Kind::Heading(1)),
            kind("H2", "Heading 2: a heading", "", Kind::Heading(2)),
            kind("H3", "Heading 3: a small heading", "", Kind::Heading(3)),
            kind("Bullets", "Bulleted list", "bullets", Kind::Bullet),
            kind("Numbers", "Numbered list", "numbers", Kind::Numbered),
            align("Left", "Align left", "left", Align::Left),
            align("Centre", "Centre", "center", Align::Center),
            align("Right", "Align right", "right", Align::Right),
            align("Justify", "Justify", "justify", Align::Justify),
            {
                let (less, more) = (state.clone(), state.clone());
                office_ui::menu(
                    "@spacing",
                    "Indent and line spacing",
                    vec![
                        MenuItem::action("Indent more", "", move || more.indent_by(720)),
                        MenuItem::action("Indent less", "", move || less.indent_by(-720)),
                        MenuItem::Separator,
                        MenuItem::action("Single spacing", "", {
                            let st = state.clone();
                            move || st.set_line_spacing(1.0)
                        }),
                        MenuItem::action("1.15 lines", "", {
                            let st = state.clone();
                            move || st.set_line_spacing(1.15)
                        }),
                        MenuItem::action("1.5 lines", "", {
                            let st = state.clone();
                            move || st.set_line_spacing(1.5)
                        }),
                        MenuItem::action("Double spacing", "", {
                            let st = state.clone();
                            move || st.set_line_spacing(2.0)
                        }),
                    ],
                )
                .into_view()
            },
        ],
    );
    let link_open = state.clone();
    let insert = office_ui::group(
        "Insert",
        vec![
            office_ui::icon_tool(
                "link",
                "Make the selected text a link (Ctrl+K)",
                move || link_open.open_link(),
            )
            .into_view(),
            office_ui::icon_tool(
                "pagebreak",
                "Start the paragraph on a new page (or take that off)",
                {
                    let st = state.clone();
                    move || st.toggle_page_break()
                },
            )
            .into_view(),
        ],
    );
    let (p0, p1, p2) = (state.clone(), state.clone(), state.clone());
    let picture_group = office_ui::group(
        "Picture",
        vec![
            office_ui::tool(
                "Picture\u{2026}",
                "Put a picture after the paragraph",
                move || p0.open_picture(),
            )
            .into_view(),
            office_ui::tool("Smaller", "Make the picture smaller", move || {
                p1.scale_picture(0.75)
            })
            .into_view(),
            office_ui::tool("Larger", "Make the picture larger", move || {
                p2.scale_picture(4.0 / 3.0)
            })
            .into_view(),
        ],
    );
    let sizes = [(2, 2), (3, 2), (3, 3), (4, 3), (5, 4), (8, 3)];
    let table_group = office_ui::group(
        "Table",
        vec![
            office_ui::menu(
                "Table",
                "Put a table after the paragraph (rows \u{d7} columns)",
                sizes
                    .iter()
                    .map(|&(r, c)| {
                        let st = state.clone();
                        MenuItem::action(&format!("{r} \u{d7} {c}"), "", move || {
                            st.insert_table(r, c)
                        })
                    })
                    .collect(),
            )
            .into_view(),
        ],
    );
    let (w1, w2, hd) = (state.clone(), state.clone(), state.clone());
    let (t1, t2, t3, t4, t5) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let edit_table_group = office_ui::group(
        "Edit table",
        vec![
            office_ui::tool("Row below", "Add a row below the caret's row", move || {
                t1.add_row()
            })
            .into_view(),
            office_ui::tool(
                "Column right",
                "Add a column right of the caret's column",
                move || t2.add_col(),
            )
            .into_view(),
            office_ui::tool("Delete row", "Delete the caret's row", move || {
                t3.delete_row()
            })
            .into_view(),
            office_ui::tool("Delete column", "Delete the caret's column", move || {
                t4.delete_col()
            })
            .into_view(),
            office_ui::tool("Wider column", "Make the caret's column wider", move || {
                w1.resize_column(50)
            })
            .into_view(),
            office_ui::tool(
                "Narrower column",
                "Make the caret's column narrower",
                move || w2.resize_column(-50),
            )
            .into_view(),
            office_ui::tool(
                "Header row",
                "Make the first row a header row (or an ordinary one)",
                move || hd.toggle_header_row(),
            )
            .into_view(),
            office_ui::tool("Delete table", "Delete the whole table", move || {
                t5.delete_table()
            })
            .into_view(),
        ],
    );
    let hf_toggle = state.clone();
    let page_group = office_ui::group(
        "Page",
        vec![
            office_ui::tool(
                "Header and footer",
                "Text at the top and foot of every page",
                move || {
                    hf_toggle.hf_open.set(!hf_toggle.hf_open.get_untracked());
                },
            )
            .into_view(),
            {
                let (out, now, inn) = (state.clone(), state.clone(), state.clone());
                let shown = state.zoom.clone();
                div()
                    .class("zoom-tools")
                    .child(office_ui::tool("\u{2212}", "Zoom out", move || {
                        out.zoom_by(-1)
                    }))
                    .child(
                        button()
                            .attr("type", "button")
                            .class("tool")
                            .attr("title", crate::i18n::tr("Back to 100%"))
                            .text_fn(move || format!("{}%", shown.get()))
                            .on_click(move || now.zoom_by(0)),
                    )
                    .child(office_ui::tool("+", "Zoom in", move || inn.zoom_by(1)))
                    .into_view()
            },
            {
                let st = state.clone();
                office_ui::menu(
                    "Paper",
                    "The size of the paper the pages are set for",
                    docs_text::PaperSize::ALL
                        .into_iter()
                        .map(|size| {
                            let st = st.clone();
                            MenuItem::action(size.name(), "", move || st.set_paper(size))
                        })
                        .collect(),
                )
                .into_view()
            },
        ],
    );
    let (pages_on, pages_shown, find_open) = (state.clone(), state.clone(), state.clone());
    let view_group = office_ui::group(
        "View",
        vec![
            button()
                .attr("type", "button")
                .attr("title", crate::i18n::tr("Show the pages as they print"))
                .class("tool")
                .class_if("on", move || pages_shown.pages_view.get())
                .text(crate::i18n::tr("Pages"))
                .on_click(move || {
                    pages_on
                        .pages_view
                        .set(!pages_on.pages_view.get_untracked())
                })
                .into_view(),
        ],
    );
    let find_group = office_ui::group(
        "Find",
        vec![
            office_ui::tool(
                "Word count",
                "Count the words (of the selection, if there is one)",
                {
                    let counting = state.clone();
                    move || counting.count_words()
                },
            )
            .into_view(),
            office_ui::tool("Find and replace", "Find and replace (Ctrl+F)", move || {
                find_open.open_find()
            })
            .into_view(),
        ],
    );
    let (family_state, size_state, color_state) = (state.clone(), state.clone(), state.clone());
    let family_shown = state.clone();
    let size_shown = state.clone();
    let option_of = |value: String, label: String, selected: Rc<dyn Fn() -> bool>| {
        option()
            .attr("value", value)
            .attr_bool("selected", move || selected())
            .text(crate::i18n::tr(&label))
            .into_view()
    };
    // "Default", the three kinds, then the popular typefaces by name under their kind.
    let named_option = |value: String, label: String, selected: Rc<dyn Fn() -> bool>| {
        option()
            .attr("value", value)
            .attr_bool("selected", move || selected())
            .text(label)
            .into_view()
    };
    let mut families = [
        ("", "Default"),
        ("sans", "Sans"),
        ("serif", "Serif"),
        ("mono", "Mono"),
    ]
    .into_iter()
    .map(|(v, l)| {
        let shown = family_shown.clone();
        option_of(
            v.to_string(),
            l.to_string(),
            Rc::new(move || {
                let _ = shown.rev.get();
                let now = shown.common_style().and_then(|s| s.family);
                shown.typeface_name().is_none()
                    && matches!(
                        (v, now),
                        ("", None)
                            | ("sans", Some(Family::Sans))
                            | ("serif", Some(Family::Serif))
                            | ("mono", Some(Family::Mono))
                    )
            }),
        )
    })
    .collect::<Vec<_>>();
    for (heading, kind) in [
        ("Sans serif", Family::Sans),
        ("Serif", Family::Serif),
        ("Fixed width", Family::Mono),
    ] {
        let mut faces: Vec<&docs_text::Typeface> = docs_text::TYPEFACES
            .iter()
            .filter(|t| t.family == kind)
            .collect();
        faces.sort_by_key(|t| t.name.to_lowercase());
        let group: Vec<View> = faces
            .into_iter()
            .map(|t| {
                let shown = family_shown.clone();
                let name = t.name;
                named_option(
                    name.to_string(),
                    name.to_string(),
                    Rc::new(move || {
                        let _ = shown.rev.get();
                        shown
                            .typeface_name()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    }),
                )
            })
            .collect();
        families.push(
            element("optgroup")
                .attr("label", crate::i18n::tr(heading))
                .children(group)
                .into_view(),
        );
    }
    let sizes = std::iter::once((String::new(), "Size".to_string(), 0_u16))
        .chain(
            [8_u16, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 72]
                .map(|pt| (pt.to_string(), pt.to_string(), pt * 2)),
        )
        .map(|(v, l, half)| {
            let shown = size_shown.clone();
            option_of(
                v,
                l,
                Rc::new(move || {
                    let _ = shown.rev.get();
                    shown.common_style().and_then(|s| s.size).unwrap_or(0) == half
                }),
            )
        })
        .collect::<Vec<_>>();
    let face = office_ui::group(
        "Typeface",
        vec![
            select()
                .class("tool-select")
                .attr("aria-label", crate::i18n::tr("Typeface"))
                .on_change(move |v| match v {
                    "sans" => family_state.set_family(Some(Family::Sans)),
                    "serif" => family_state.set_family(Some(Family::Serif)),
                    "mono" => family_state.set_family(Some(Family::Mono)),
                    name => family_state.set_typeface(name),
                })
                .children(families)
                .into_view(),
            select()
                .class("tool-select")
                .attr("aria-label", crate::i18n::tr("Size"))
                .on_change(move |v| size_state.set_size(v.parse::<u16>().ok().map(|pt| pt * 2)))
                .children(sizes)
                .into_view(),
            input()
                .attr("type", "color")
                .attr("title", crate::i18n::tr("Text colour"))
                .attr("aria-label", crate::i18n::tr("Text colour"))
                .class("tool-color")
                .on_change(move |v| {
                    color_state.set_color(
                        v.strip_prefix('#')
                            .and_then(|h| u32::from_str_radix(h, 16).ok()),
                    );
                })
                .into_view(),
        ],
    );
    office_ui::ribbon(
        "lore-docs",
        name,
        status,
        "Ribbon",
        file,
        theme,
        vec![
            ("Home", vec![history, font, face, paragraph, insert]),
            ("Insert", vec![picture_group, table_group, edit_table_group]),
            ("View", vec![view_group, page_group, find_group]),
        ],
    )
}

/// A hash of what a paragraph shows, so that only changed ones are built again.
/// A paragraph as the page list holds it: its key, the paragraph, the addresses of its links, and its table's columns and picture.
type KeyedPara = (
    u64,
    Para,
    Vec<(u32, String)>,
    (
        Vec<u16>,
        Option<docs_text::Picture>,
        String,
        Vec<(u32, String)>,
        bool,
    ),
);

/// The columns of the grid the page is laid on.
const GRID: usize = 840;

fn key_of(
    p: &Para,
    cols: &[u16],
    picture: Option<(f64, f64)>,
    marker: &str,
    starts_page: bool,
) -> u64 {
    let mut h = DefaultHasher::new();
    starts_page.hash(&mut h);
    marker.hash(&mut h);
    p.hash(&mut h);
    cols.hash(&mut h);
    if let Some((w, h2)) = picture {
        w.to_bits().hash(&mut h);
        h2.to_bits().hash(&mut h);
    }
    h.finish()
}

thread_local! {
    /// The `data:` addresses of pictures already made, by the place their bytes are (a picture's bytes are not copied).
    static SOURCES: std::cell::RefCell<std::collections::HashMap<usize, String>> = std::cell::RefCell::default();
}

/// The address a picture is shown from: its file as a `data:` URL.
fn source_of(pic: &docs_text::Picture) -> String {
    let key = Rc::as_ptr(&pic.data) as usize;
    SOURCES.with(|m| {
        m.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                format!(
                    "data:{};base64,{}",
                    pic.mime,
                    quench_core::base64::STANDARD.encode(pic.data.as_slice())
                )
            })
            .clone()
    })
}

#[allow(clippy::too_many_arguments)]
fn para_view(
    state: &DocState,
    p: &Para,
    urls: &[(u32, String)],
    cols: &[u16],
    pic: Option<&docs_text::Picture>,
    marker: &str,
    fonts: &[(u32, String)],
    new_page: bool,
) -> View {
    let tag = match p.kind {
        Kind::Heading(1) => "h1",
        Kind::Heading(2) => "h2",
        Kind::Heading(3) => "h3",
        Kind::Heading(4) => "h4",
        Kind::Heading(5) => "h5",
        Kind::Heading(_) => "h6",
        _ => "p",
    };
    let mut el = element(tag);
    el = el.class(match p.align {
        Align::Left => "",
        Align::Center => "ac",
        Align::Right => "ar",
        Align::Justify => "aj",
    });
    if new_page {
        // The gap between two sheets is drawn from here (see `.newpage` in the style).
        el = el.class("newpage");
    }
    if !marker.is_empty() {
        // The marker is drawn from an attribute so that it is no text of the editable element. A deeper item sits further in.
        el = el.class("li").attr("data-marker", marker.to_string());
        let level = f64::from(p.fmt.level.min(8)) * 1.8;
        if level > 0.0 {
            el = el.css("margin-left", move || format!("{level}em"));
        }
    }
    if let Some(pic) = pic {
        // A picture paragraph holds the picture and nothing to type: the picture is not editable itself.
        let (w, _) = pic.size();
        let px = w * 4.0 / 3.0;
        return el
            .class("pic")
            .child(
                img()
                    .attr("src", source_of(pic))
                    .attr("alt", "")
                    .attr("contenteditable", "false")
                    .attr("draggable", "false")
                    .css("width", move || format!("{px}px"))
                    .on_click({
                        let (st, id) = (state.clone(), p.id);
                        move || st.focus_paragraph(id)
                    }),
            )
            .into_view();
    }
    let f = p.fmt;
    if f.indent != 0 || f.first != 0 {
        let (left, first) = (f64::from(f.indent) / 15.0, f64::from(f.first) / 15.0);
        el = el
            .css("margin-left", move || format!("{left}px"))
            .css("text-indent", move || format!("{first}px"));
    }
    if let Some(a) = f.after {
        let px = f64::from(a) / 15.0;
        el = el.css("margin-bottom", move || format!("{px}px"));
    }
    if f.line != 0 {
        let lh = f64::from(f.line) / 240.0 * 1.5;
        el = el.css("line-height", move || format!("{lh}"));
    }
    if f.page_break {
        el = el.class("pb");
    }
    if let Some(c) = p.cell {
        // The page is a grid of 840 columns (a number most column counts divide); a cell spans its share of them.
        // Where the cell's column starts and ends on the grid (the widths are thousandths of the table).
        let edge = |upto: usize| {
            let sum: u32 = cols.iter().take(upto).map(|w| u32::from(*w)).sum();
            (sum as usize * GRID + 500) / 1000
        };
        let (a, b) = (usize::from(c.col), usize::from(c.col) + 1);
        let span = if b >= cols.len() {
            GRID - edge(a)
        } else {
            edge(b) - edge(a)
        }
        .max(1);
        el = el
            .class("cell")
            .css("grid-column", move || format!("span {span}"));
        if c.row == 0 {
            el = el.class("r0");
        }
        if c.col == 0 {
            el = el.class("c0");
        }
    }
    if p.runs.is_empty() {
        // An empty paragraph needs something in it to have a height and to hold a caret.
        return el.child(element("br")).into_view();
    }
    for r in &p.runs {
        let mut class = String::new();
        for (on, c) in [
            (r.style.bold, "b"),
            (r.style.italic, "i"),
            (r.style.underline, "u"),
            (r.style.strike, "s"),
            (r.style.sup, "sup"),
            (r.style.sub, "sub"),
        ] {
            if on {
                class.push_str(c);
                class.push(' ');
            }
        }
        if r.style.link.is_some() {
            class.push_str("lk ");
        }
        // A line break inside a paragraph is one character in the model and in the page, drawn as a new line.
        let shown: String = r
            .text
            .chars()
            .map(|c| if c == docs_text::LINE_BREAK { '\n' } else { c })
            .collect();
        let mut run = span().class(class.trim().to_string()).text(shown);
        let st = r.style;
        if let Some(h) = st.highlight {
            run = run.css("background-color", move || format!("#{h:06x}"));
        }
        if let Some(url) = st.link.and_then(|l| urls.iter().find(|(n, _)| *n == l)) {
            run = run.attr("title", url.1.clone());
        }
        if let Some(sz) = st.size {
            // Half points to pixels (a point is 4/3 pixel).
            run = run.css("font-size", move || {
                format!("{}px", f64::from(sz) * 2.0 / 3.0)
            });
        }
        if let Some(c) = st.color {
            run = run.css("color", move || format!("#{c:06x}"));
        }
        if let Some(f) = st.family {
            let named = st
                .font
                .and_then(|n| fonts.iter().find(|(k, _)| *k == n))
                .map(|(_, name)| name.replace(['"', '\\'], ""));
            run = run.css("font-family", move || {
                let generic = match f {
                    Family::Sans => "\"Liberation Sans\", Arial, sans-serif",
                    Family::Serif => "\"Liberation Serif\", \"Times New Roman\", serif",
                    Family::Mono => "\"Liberation Mono\", \"Courier New\", monospace",
                };
                named.as_ref().map_or_else(
                    || generic.to_string(),
                    |n| {
                        docs_text::typeface(n).map_or_else(
                            || format!("\"{n}\", {generic}"),
                            |t| format!("{}, {generic}", t.css),
                        )
                    },
                )
            });
        }
        el = el.child(run);
    }
    // A line break at the very end draws no new line unless something follows it: a zero-width space gives the caret a place.
    if p.runs
        .last()
        .is_some_and(|r| r.text.ends_with(docs_text::LINE_BREAK))
    {
        el = el.child(span().class("end").text("\u{200b}"));
    }
    el.into_view()
}

/// The editable page.
fn page(state: &DocState) -> Element {
    let keys = state.clone();
    let (input, moved, restore, paras) = (
        state.clone(),
        state.clone(),
        state.restore.clone(),
        state.clone(),
    );
    let list = {
        let rev = state.rev.clone();
        View::keyed(
            move || {
                let _ = rev.get();
                paras.with_doc(|d| {
                    let markers = d.list_markers();
                    // Where the pages end, as the print layout has them: the paragraph a new page starts at gets the gap.
                    let breaks: std::collections::HashSet<usize> =
                        docs_print::layout(d, &docs_print::Setup::for_doc(d))
                            .breaks
                            .into_iter()
                            .collect();
                    d.paras()
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let marker = markers[i].clone().unwrap_or_default();
                            let fonts: Vec<(u32, String)> = p
                                .runs
                                .iter()
                                .filter_map(|r| r.style.font)
                                .filter_map(|n| d.font_name(n).map(|f| (n, f.to_string())))
                                .collect();
                            let urls: Vec<(u32, String)> = p
                                .runs
                                .iter()
                                .filter_map(|r| r.style.link)
                                .filter_map(|l| d.url(l).map(|u| (l, u.to_string())))
                                .collect();
                            let cols = p.cell.map_or_else(Vec::new, |c| d.table_widths(c.table));
                            let pic = p.picture.and_then(|n| d.picture(n)).cloned();
                            (
                                key_of(
                                    p,
                                    &cols,
                                    pic.as_ref().map(docs_text::Picture::size),
                                    &marker,
                                    breaks.contains(&i),
                                ),
                                p.clone(),
                                urls,
                                (cols, pic, marker, fonts, breaks.contains(&i)),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            },
            |(k, ..): &KeyedPara| *k,
            {
                let st = state.clone();
                move |(_, p, urls, (cols, pic, marker, fonts, new_page)): KeyedPara| {
                    para_view(
                        &st,
                        &p,
                        &urls,
                        &cols,
                        pic.as_ref(),
                        &marker,
                        &fonts,
                        new_page,
                    )
                }
            },
        )
    };
    let (wide, tall, rev_w, rev_t) = (
        state.clone(),
        state.clone(),
        state.rev.clone(),
        state.rev.clone(),
    );
    div()
        .class("page")
        // The sheet is the paper the document is set for, with the margins the print layout uses (an inch).
        .css("width", move || {
            let _ = rev_w.get();
            let w = wide.with_doc(|d| docs_print::Paper::of(d.paper()).width) * 4.0 / 3.0;
            format!("min({w}px, 100%)")
        })
        .css("min-height", move || {
            let _ = rev_t.get();
            let h = tall.with_doc(|d| docs_print::Paper::of(d.paper()).height) * 4.0 / 3.0;
            format!("{h}px")
        })
        .attr("contenteditable", "true")
        .attr("role", "textbox")
        .attr("aria-multiline", "true")
        .attr("aria-label", crate::i18n::tr("Document"))
        .attr("spellcheck", "true")
        .attr_fn("edit-selection", move || restore.get())
        // The browser has nothing of its own to undo (it is not allowed to change the page), so it sends no undo: the keys are ours.
        .on_keydown(move |k| {
            if k.ctrl || k.meta {
                match k.key.as_str() {
                    "s" | "S" => {
                        quench_ui::prevent_default();
                        keys.save_docx();
                    }
                    "p" | "P" => {
                        quench_ui::prevent_default();
                        keys.print();
                    }
                    "k" | "K" => {
                        quench_ui::prevent_default();
                        keys.open_link();
                    }
                    "f" | "F" | "h" | "H" => {
                        quench_ui::prevent_default();
                        keys.open_find();
                    }
                    "z" | "Z" if !k.shift => {
                        quench_ui::prevent_default();
                        keys.undo();
                    }
                    "z" | "Z" | "y" | "Y" => {
                        quench_ui::prevent_default();
                        keys.redo();
                    }
                    _ => {}
                }
            } else if k.key == "Tab"
                && (keys.cell_move(k.shift) || keys.nest(if k.shift { -1 } else { 1 }))
            {
                quench_ui::prevent_default();
            }
        })
        .on(EventKind::BeforeInput, move |e| {
            if let Event::BeforeInput(b) = e {
                input.before_input(b);
            }
        })
        .on(EventKind::EditSelection, move |e| {
            if let Event::EditSelection(EditSelectionEvent { anchor, focus }) = e {
                moved.select(Range {
                    anchor: point(*anchor),
                    focus: point(*focus),
                });
            }
        })
        .child(list)
}

/// The language picker and the theme menu, at the end of the top line.
fn extras(state: &DocState) -> Element {
    use crate::i18n::text;
    let language = state.language.clone();
    let current = std::rc::Rc::new(move || language.get()) as std::rc::Rc<dyn Fn() -> String>;
    let pick = state.clone();
    div()
        .child(office_cloud_ui::cloud_button())
        .child(office_theme::language_picker(
            &text("language_label"),
            crate::i18n::CODES,
            &current,
            std::rc::Rc::new(move |code| pick.set_language(code)),
        ))
        .child(office_theme::theme_menu(&office_theme::ThemeLabels {
            toggle: text("theme_toggle"),
            mode: text("theme_mode"),
            light: text("theme_light"),
            dark: text("theme_dark"),
            system: text("theme_system"),
            palette: text("theme_palette"),
            palettes: ["blue", "green", "teal", "purple", "rose", "orange"]
                .map(|n| text(&format!("palette_{n}"))),
        }))
}

/// The editable text, or the pages as they print (read only) when that is asked for.
fn canvas(state: &DocState) -> View {
    let (st, shown) = (state.clone(), state.pages_view.clone());
    View::dynamic(move || {
        if shown.get() {
            pages(&st)
        } else {
            page(&st).into_view()
        }
    })
}

/// The CSS for a PDF font: family, weight and slant.
fn css_font(f: pdf_write::Font) -> (&'static str, &'static str, &'static str) {
    use pdf_write::Font::{
        Courier, CourierBold, CourierBoldOblique, CourierOblique, Helvetica, HelveticaBold,
        HelveticaBoldOblique, HelveticaOblique, Times, TimesBold, TimesBoldItalic, TimesItalic,
    };
    let sans = "\"Liberation Sans\", Arial, Helvetica, sans-serif";
    let serif = "\"Liberation Serif\", \"Times New Roman\", serif";
    let mono = "\"Liberation Mono\", \"Courier New\", monospace";
    match f {
        Helvetica => (sans, "400", "normal"),
        HelveticaBold => (sans, "700", "normal"),
        HelveticaOblique => (sans, "400", "italic"),
        HelveticaBoldOblique => (sans, "700", "italic"),
        Times => (serif, "400", "normal"),
        TimesBold => (serif, "700", "normal"),
        TimesItalic => (serif, "400", "italic"),
        TimesBoldItalic => (serif, "700", "italic"),
        Courier => (mono, "400", "normal"),
        CourierBold => (mono, "700", "normal"),
        CourierOblique => (mono, "400", "italic"),
        CourierBoldOblique => (mono, "700", "italic"),
    }
}

/// The pages as they print: `docs-print`'s layout drawn on sheets of paper (a point is 4/3 of a pixel).
fn pages(state: &DocState) -> View {
    let (st, rev) = (state.clone(), state.rev.clone());
    View::dynamic(move || {
        let _ = rev.get();
        let setup = st.with_doc(docs_print::Setup::for_doc);
        let laid = st.with_doc(|d| docs_print::layout(d, &setup));
        let (w, h) = (
            setup.paper.width * 4.0 / 3.0,
            setup.paper.height * 4.0 / 3.0,
        );
        let sheets: Vec<View> = laid
            .pages
            .into_iter()
            .enumerate()
            .map(|(n, items)| {
                let mut sheet = div()
                    .class("sheet")
                    .css("width", move || format!("{w}px"))
                    .css("height", move || format!("{h}px"));
                for r in laid.rules.get(n).map_or(&[][..], Vec::as_slice) {
                    let (x, y) = (r.x1.min(r.x2) * 4.0 / 3.0, r.y1.min(r.y2) * 4.0 / 3.0);
                    let (w, h) = (
                        ((r.x2 - r.x1).abs() * 4.0 / 3.0).max(1.0),
                        ((r.y2 - r.y1).abs() * 4.0 / 3.0).max(1.0),
                    );
                    sheet = sheet.child(
                        div()
                            .class("rule")
                            .css("left", move || format!("{x}px"))
                            .css("top", move || format!("{y}px"))
                            .css("width", move || format!("{w}px"))
                            .css("height", move || format!("{h}px")),
                    );
                }
                for im in laid.images.get(n).map_or(&[][..], Vec::as_slice) {
                    let (x, y) = (im.x * 4.0 / 3.0, im.y * 4.0 / 3.0);
                    let (w, h) = (im.width * 4.0 / 3.0, im.height * 4.0 / 3.0);
                    let pic = docs_text::Picture {
                        mime: im.mime,
                        data: im.data.clone(),
                        width: 0,
                        height: 0,
                    };
                    sheet = sheet.child(
                        img()
                            .class("piece")
                            .attr("src", source_of(&pic))
                            .attr("alt", "")
                            .css("left", move || format!("{x}px"))
                            .css("top", move || format!("{y}px"))
                            .css("width", move || format!("{w}px"))
                            .css("height", move || format!("{h}px")),
                    );
                }
                for i in items {
                    let (family, weight, slant) = css_font(i.font);
                    let (x, y, size, color) = (
                        i.x * 4.0 / 3.0,
                        i.size.mul_add(-0.85, i.y) * 4.0 / 3.0,
                        i.size * 4.0 / 3.0,
                        i.color,
                    );
                    let mut piece = div()
                        .class("piece")
                        .css("left", move || format!("{x}px"))
                        .css("top", move || format!("{y}px"))
                        .css("font-size", move || format!("{size}px"))
                        .css("font-family", move || family.to_string())
                        .css("font-weight", move || weight.to_string())
                        .css("font-style", move || slant.to_string())
                        .css("color", move || format!("#{color:06x}"))
                        .text(i.text);
                    if i.underline {
                        piece = piece.class("u");
                    }
                    if i.strike {
                        piece = piece.class("s");
                    }
                    if let Some(h) = i.highlight {
                        piece = piece.css("background-color", move || format!("#{h:06x}"));
                    }
                    sheet = sheet.child(piece);
                }
                sheet.into_view()
            })
            .collect();
        div()
            .class("sheets")
            .attr("aria-label", crate::i18n::tr("Pages"))
            .children(sheets)
            .into_view()
    })
}

/// The find and replace bar, built when it is opened.
fn find_bar(state: &DocState) -> Element {
    let (open, st) = (state.find_open.clone(), state.clone());
    let slot = View::dynamic(move || {
        if !open.get() {
            return View::Empty;
        }
        let (query, with, case) = (
            st.find_query.clone(),
            st.replace_with.clone(),
            st.match_case.clone(),
        );
        let (typed, replaced, entered, case_set) = (st.clone(), st.clone(), st.clone(), st.clone());
        let (next, prev, one, all, close) =
            (st.clone(), st.clone(), st.clone(), st.clone(), st.clone());
        let btn = |label: &'static str, tip: &'static str| {
            button()
                .attr("type", "button")
                .class("tool")
                .attr("title", crate::i18n::tr(tip))
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .text(crate::i18n::tr(label))
        };
        div()
            .class("findbar")
            .attr("role", "search")
            .child(
                input()
                    .attr("type", "text")
                    .class("find-field")
                    .attr("aria-label", crate::i18n::tr("Find"))
                    .attr("placeholder", crate::i18n::tr("Find"))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || query.get())
                    .on_input(move |v| typed.find_query.set(v.to_string()))
                    .on_keydown(move |k| {
                        if k.key == "Enter" {
                            quench_ui::prevent_default();
                            entered.find_next(k.shift);
                        }
                    })
                    .focus_if(|| true),
            )
            .child(
                input()
                    .attr("type", "text")
                    .class("find-field")
                    .attr("aria-label", crate::i18n::tr("Replace with"))
                    .attr("placeholder", crate::i18n::tr("Replace with"))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || with.get())
                    .on_input(move |v| replaced.replace_with.set(v.to_string())),
            )
            .child(
                btn("Previous", "Previous match (Shift+Enter)")
                    .on_click(move || prev.find_next(true)),
            )
            .child(btn("Next", "Next match (Enter)").on_click(move || next.find_next(false)))
            .child(btn("Replace", "Replace this match").on_click(move || one.replace_current()))
            .child(btn("Replace all", "Replace every match").on_click(move || all.replace_all()))
            .child(
                label()
                    .class("check")
                    .child(
                        input()
                            .attr("type", "checkbox")
                            .attr_bool("checked", move || case.get())
                            .on_click(move || {
                                case_set
                                    .match_case
                                    .set(!case_set.match_case.get_untracked())
                            }),
                    )
                    .child(span().text(crate::i18n::tr("Match case"))),
            )
            .child(btn("Close", "Close the find bar").on_click(move || close.find_open.set(false)))
            .into_view()
    });
    div().class("findslot").child(slot)
}

/// The bar for the address of a link, built when it is opened.
fn link_bar(state: &DocState) -> Element {
    let (open, st) = (state.link_open.clone(), state.clone());
    let slot = View::dynamic(move || {
        if !open.get() {
            return View::Empty;
        }
        let url = st.link_url.clone();
        let (typed, entered, apply, remove, close) =
            (st.clone(), st.clone(), st.clone(), st.clone(), st.clone());
        let btn = |label: &'static str, tip: &'static str| {
            button()
                .attr("type", "button")
                .class("tool")
                .attr("title", crate::i18n::tr(tip))
                .on(EventKind::PointerDown, |_| quench_ui::prevent_default())
                .text(crate::i18n::tr(label))
        };
        div()
            .class("findbar")
            .child(
                input()
                    .attr("type", "text")
                    .class("find-field link-field")
                    .attr("aria-label", crate::i18n::tr("Address"))
                    .attr("placeholder", "https://")
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || url.get())
                    .on_input(move |v| typed.link_url.set(v.to_string()))
                    .on_keydown(move |k| {
                        if k.key == "Enter" {
                            quench_ui::prevent_default();
                            entered.apply_link();
                        }
                    })
                    .focus_if(|| true),
            )
            .child(
                btn("Apply", "Make the selected text a link").on_click(move || apply.apply_link()),
            )
            .child(
                btn("Remove link", "Take the link off the selected text")
                    .on_click(move || remove.remove_link()),
            )
            .child(btn("Close", "Close the link bar").on_click(move || close.link_open.set(false)))
            .into_view()
    });
    div().class("findslot").child(slot)
}

/// The bar for the header and footer text, built when it is opened.
fn header_bar(state: &DocState) -> Element {
    let (open, st) = (state.hf_open.clone(), state.clone());
    let slot = View::dynamic(move || {
        if !open.get() {
            return View::Empty;
        }
        let field = |label: &'static str, value: String, set: Rc<dyn Fn(String)>| {
            input()
                .attr("type", "text")
                .class("find-field hf-field")
                .attr("aria-label", crate::i18n::tr(label))
                .attr("placeholder", crate::i18n::tr(label))
                .attr("value", value)
                .on_input(move |v| set(v.to_string()))
        };
        let (h, f, close) = (st.clone(), st.clone(), st.clone());
        let (header, footer) = st.with_doc(|d| {
            (
                d.header().replace('\n', " | "),
                d.footer().replace('\n', " | "),
            )
        });
        div()
            .class("findbar")
            .child(field(
                "Header",
                header,
                Rc::new(move |v| h.set_header(&v.replace(" | ", "\n"))),
            ))
            .child(field(
                "Footer ({page} is the page number)",
                footer,
                Rc::new(move |v| f.set_footer(&v.replace(" | ", "\n"))),
            ))
            .child(span().class("hint").text("{page}  {pages}"))
            .child(
                button()
                    .attr("type", "button")
                    .class("tool")
                    .text(crate::i18n::tr("Close"))
                    .on_click(move || close.hf_open.set(false)),
            )
            .into_view()
    });
    div().class("findslot").child(slot)
}
