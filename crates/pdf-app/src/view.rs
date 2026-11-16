//! The screen: a ribbon, the list of pages, and the text of the page.

use crate::state::{PdfState, ZOOMS};
use office_ui::MenuItem;
use quench_ui::event::prevent_default;
use quench_ui::prelude::*;

/// The whole screen.
#[must_use]
pub fn pdf_view(state: &PdfState) -> View {
    let state = state.clone();
    View::dynamic(move || {
        let _ = state.language.get();
        screen(&state)
    })
}

fn screen(state: &PdfState) -> View {
    let keys = state.clone();
    div()
        .class("pdf")
        .child(office_cloud_ui::overlay())
        .attr("tabindex", "-1")
        .on_keydown(move |k| {
            let handled = match k.key.as_str() {
                "ArrowRight" | "PageDown" if !k.ctrl => {
                    keys.step(1);
                    true
                }
                "ArrowLeft" | "PageUp" if !k.ctrl => {
                    keys.step(-1);
                    true
                }
                "Home" => {
                    keys.go_to(0);
                    true
                }
                "End" => {
                    keys.go_to(usize::MAX);
                    true
                }
                "+" | "=" if k.ctrl => {
                    keys.zoom_by(1);
                    true
                }
                "-" if k.ctrl => {
                    keys.zoom_by(-1);
                    true
                }
                "F3" => {
                    keys.next_hit(k.shift);
                    true
                }
                _ => false,
            };
            if handled {
                prevent_default();
            }
        })
        .child(ribbon(state))
        .child(sidebar(state))
        .child(reader(state))
        .into_view()
}

fn ribbon(state: &PdfState) -> Element {
    let (open, viewer) = (state.clone(), state.clone());
    let file = office_ui::menu(
        "File",
        "Open a file",
        office_cloud_ui::with_menu(vec![
            MenuItem::action("Open\u{2026}", "", move || open.open_file()),
            MenuItem::action("Open in the browser's viewer", "", move || {
                viewer.open_in_viewer()
            }),
        ]),
    );
    let theme = extras(state);
    let (prev, next, jump) = (state.clone(), state.clone(), state.clone());
    let shown = state.clone();
    let counter = create_memo(move || {
        let _ = shown.rev.get();
        format!("{} / {}", shown.page.get() + 1, shown.page_count().max(1))
    });
    let pages = office_ui::group(
        "Pages",
        vec![
            office_ui::tool("\u{25C0}", "Previous page", move || prev.step(-1)).into_view(),
            span()
                .class("page-counter")
                .text_fn(move || counter.get())
                .into_view(),
            office_ui::tool("\u{25B6}", "Next page", move || next.step(1)).into_view(),
            input()
                .attr("type", "number")
                .class("jump")
                .attr("min", "1")
                .attr("aria-label", crate::i18n::tr("Go to page"))
                .attr("placeholder", crate::i18n::tr("Page"))
                .on_change(move |v| {
                    if let Ok(n) = v.trim().parse::<usize>() {
                        jump.go_to(n.saturating_sub(1));
                    }
                })
                .into_view(),
        ],
    );
    let (smaller, larger, level) = (state.clone(), state.clone(), state.zoom.clone());
    let zoom = office_ui::group(
        "Zoom",
        vec![
            office_ui::tool("A\u{2212}", "Smaller text (Ctrl+\u{2212})", move || {
                smaller.zoom_by(-1)
            })
            .into_view(),
            span()
                .class("page-counter")
                .text_fn(move || format!("{}%", ZOOMS[level.get()]))
                .into_view(),
            office_ui::tool("A+", "Larger text (Ctrl++)", move || larger.zoom_by(1)).into_view(),
        ],
    );
    let (typed, value, prev_hit, next_hit, count, current) = (
        state.clone(),
        state.query.clone(),
        state.clone(),
        state.clone(),
        state.hit_count.clone(),
        state.current.clone(),
    );
    let search = office_ui::group(
        "Search",
        vec![
            input()
                .attr("type", "search")
                .class("find")
                .attr("aria-label", crate::i18n::tr("Search the text"))
                .attr("placeholder", crate::i18n::tr("Search"))
                .attr_fn("value", move || value.get())
                .on_input(move |v| typed.search(v))
                .into_view(),
            office_ui::tool("\u{25B2}", "Previous match (Shift+F3)", move || {
                prev_hit.next_hit(true)
            })
            .into_view(),
            office_ui::tool("\u{25BC}", "Next match (F3)", move || {
                next_hit.next_hit(false)
            })
            .into_view(),
            span()
                .class("page-counter")
                .text_fn(move || {
                    let n = count.get();
                    match current.get() {
                        Some(i) => format!("{} of {n}", i + 1),
                        None => String::new(),
                    }
                })
                .into_view(),
        ],
    );
    let (dark, dark_on) = (state.clone(), state.dark_pages.clone());
    let (flip, shown_as) = (state.clone(), state.as_picture.clone());
    let view = office_ui::group(
        "View",
        vec![
            button()
                .attr("type", "button")
                .class("tool")
                .attr(
                    "title",
                    crate::i18n::tr("Show the page as it is drawn, or only its text"),
                )
                .text_fn(move || {
                    crate::i18n::tr(if shown_as.get() {
                        "Text only"
                    } else {
                        "As drawn"
                    })
                })
                .on_click(move || flip.as_picture.set(!flip.as_picture.get_untracked()))
                .into_view(),
            button()
                .attr("type", "button")
                .class("tool")
                .class_if("on", move || dark_on.get())
                .attr("title", crate::i18n::tr("Show the pages with dark paper"))
                .text(crate::i18n::tr("Dark pages"))
                .on_click(move || dark.dark_pages.set(!dark.dark_pages.get_untracked()))
                .into_view(),
        ],
    );
    let mark_button = |label: &'static str, tip: &'static str, kind: &'static str| {
        let (on, shown) = (state.clone(), state.mark.clone());
        button()
            .attr("type", "button")
            .class("tool")
            .class_if("on", move || shown.get() == kind)
            .attr("title", crate::i18n::tr(tip))
            .text(crate::i18n::tr(label))
            .on_click(move || {
                let now = on.mark.get_untracked();
                on.mark.set(if now == kind {
                    String::new()
                } else {
                    kind.to_string()
                });
            })
            .into_view()
    };
    let (note_in, note_val, undo, save_marked) = (
        state.clone(),
        state.note.clone(),
        state.clone(),
        state.clone(),
    );
    let mark_group = office_ui::group(
        "Mark text",
        vec![
            mark_button(
                "Highlight",
                "Click a line of text to highlight it",
                "Highlight",
            ),
            mark_button(
                "Underline",
                "Click a line of text to underline it",
                "Underline",
            ),
            mark_button(
                "Strike out",
                "Click a line of text to strike it out",
                "StrikeOut",
            ),
            mark_button("Note", "Click a line of text to put a note on it", "Text"),
            mark_button(
                "Type",
                "Click a line of text, then type words after it (to fill in a form)",
                "Type",
            ),
            input()
                .attr("type", "text")
                .class("find note-field")
                .attr("aria-label", crate::i18n::tr("Words of the note"))
                .attr("placeholder", crate::i18n::tr("Words of the note"))
                .attr_fn("value", move || note_val.get())
                .on_input(move |v| note_in.note.set(v.to_string()))
                .into_view(),
            office_ui::tool("Undo", "Take back the last change", move || {
                undo.undo_edit()
            })
            .into_view(),
            office_ui::tool(
                "Save a copy",
                "Save the file with the changes made",
                move || save_marked.save_copy(),
            )
            .into_view(),
        ],
    );
    let (r1, r2) = (state.clone(), state.clone());
    let turn = office_ui::group(
        "Turn",
        vec![
            office_ui::tool(
                "\u{21b6}",
                "Turn the page a quarter turn to the left",
                move || r1.rotate_page(-90),
            )
            .into_view(),
            office_ui::tool(
                "\u{21b7}",
                "Turn the page a quarter turn to the right",
                move || r2.rotate_page(90),
            )
            .into_view(),
        ],
    );
    let (m1, m2, d1) = (state.clone(), state.clone(), state.clone());
    let arrange = office_ui::group(
        "Arrange",
        vec![
            office_ui::tool(
                "Move earlier",
                "Move the page one place earlier",
                move || m1.move_page(-1),
            )
            .into_view(),
            office_ui::tool("Move later", "Move the page one place later", move || {
                m2.move_page(1)
            })
            .into_view(),
            office_ui::tool("Delete page", "Take the page out of the file", move || {
                d1.delete_page()
            })
            .into_view(),
        ],
    );
    let (a1, c1, c2) = (state.clone(), state.clone(), state.clone());
    let files = office_ui::group(
        "File",
        vec![
            office_ui::tool(
                "Add pages from\u{2026}",
                "Put the pages of another PDF after this one",
                move || a1.append_file(),
            )
            .into_view(),
            office_ui::tool(
                "Save a copy",
                "Save the file with the changes made",
                move || c1.save_copy(),
            )
            .into_view(),
            office_ui::tool(
                "Save this page",
                "Save the page shown as a file of its own",
                move || c2.save_page(),
            )
            .into_view(),
        ],
    );
    let name = state.name.clone();
    office_ui::ribbon(
        "lore-pdf",
        name,
        state.status.clone(),
        "Ribbon",
        file,
        theme,
        vec![
            ("Home", vec![pages, zoom, search, view]),
            ("Annotate", vec![mark_group]),
            ("Edit pages", vec![turn, arrange, files]),
        ],
    )
}

fn sidebar(state: &PdfState) -> Element {
    let (st, rev, page) = (state.clone(), state.rev.clone(), state.page.clone());
    let list = View::dynamic(move || {
        let _ = rev.get();
        let now = page.get();
        let marks = st.bookmarks();
        if !marks.is_empty() {
            let items: Vec<View> = marks
                .into_iter()
                .map(|m| {
                    let go = st.clone();
                    let level = m.level.min(6);
                    let target = m.page;
                    button()
                        .attr("type", "button")
                        .class("thumb")
                        .class(format!("level-{level}"))
                        .class_if("active", move || target == Some(now))
                        .attr("disabled", if target.is_some() { "false" } else { "true" })
                        .child(span().class("thumb-line").text(if m.title.is_empty() {
                            "\u{2014}".to_string()
                        } else {
                            m.title
                        }))
                        .on_click(move || {
                            if let Some(p) = target {
                                go.go_to(p);
                            }
                        })
                        .into_view()
                })
                .collect();
            return div().class("thumbs outline").children(items).into_view();
        }
        let items: Vec<View> = st
            .outline()
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                let go = st.clone();
                button()
                    .attr("type", "button")
                    .class("thumb")
                    .class_if("active", move || i == now)
                    .child(span().class("thumb-number").text(format!("{}", i + 1)))
                    .child(span().class("thumb-line").text(line))
                    .on_click(move || go.go_to(i))
                    .into_view()
            })
            .collect();
        if items.is_empty() {
            return p()
                .class("empty")
                .text(crate::i18n::tr("Open a PDF to read its text"))
                .into_view();
        }
        div().class("thumbs").children(items).into_view()
    });
    element("aside").class("sidebar").child(list)
}

fn reader(state: &PdfState) -> Element {
    let (st, rev, page, zoom) = (
        state.clone(),
        state.rev.clone(),
        state.page.clone(),
        state.zoom.clone(),
    );
    let _ = &st;
    let text = {
        let st = state.clone();
        let (as_picture, zoom_now) = (state.as_picture.clone(), state.zoom.clone());
        View::dynamic(move || {
            let _ = (rev.get(), st.query.get(), st.current.get());
            let n = page.get();
            // The page as it is drawn, when asked for and when it can be drawn.
            if as_picture.get()
                && let Some(pic) = st.picture(n)
            {
                // A point is 4/3 pixel; the zoom is a percent of that.
                let scale = 4.0 / 3.0 * f64::from(ZOOMS[zoom_now.get()]) / 100.0;
                let (width, height) = (pic.width * scale, pic.height * scale);
                // The text over the picture, invisible but selectable; the runs a search finds are marked.
                let query = st.query.get().trim().to_lowercase();
                let (_, current_here) = st.hits_on(n);
                let mut found = 0;
                let mut layer = div().class("text-layer");
                for (k, run) in pic.runs.iter().enumerate() {
                    let hit = !query.is_empty() && run.text.to_lowercase().contains(&query);
                    let is_current = hit && Some(found) == current_here;
                    found += usize::from(hit);
                    let (x, y) = (run.x * scale, run.size.mul_add(-0.85, run.y) * scale);
                    let (size, w) = (run.size * scale, run.width * scale);
                    layer = layer.child(
                        span()
                            .class(match (hit, is_current) {
                                (_, true) => "run hit current",
                                (true, _) => "run hit",
                                _ => "run",
                            })
                            .css("left", move || format!("{x:.1}px"))
                            .css("top", move || format!("{y:.1}px"))
                            .css("width", move || format!("{w:.1}px"))
                            .css("height", move || format!("{size:.1}px"))
                            .css("font-size", move || format!("{size:.1}px"))
                            .on_click({
                                let st = st.clone();
                                move || st.mark_run(k)
                            })
                            .text(run.text.clone()),
                    );
                }
                if let Some(k) = st.typing.get()
                    && let Some(run) = pic.runs.get(k)
                {
                    let (x, y) = (
                        run.size.mul_add(0.4, run.x + run.width) * scale,
                        (run.y - run.size) * scale,
                    );
                    let size = run.size.clamp(6.0, 36.0) * scale;
                    let (typed, commit, cancel, value) =
                        (st.clone(), st.clone(), st.clone(), st.typed.clone());
                    layer = layer.child(
                        input()
                            .attr("type", "text")
                            .class("typebox")
                            .attr("aria-label", crate::i18n::tr("Words to put after the line"))
                            .css("left", move || format!("{x:.1}px"))
                            .css("top", move || format!("{y:.1}px"))
                            .css("height", move || format!("{:.1}px", size * 1.3))
                            .css("font-size", move || format!("{size:.1}px"))
                            .attr_fn("value", move || value.get())
                            .on_input(move |v| typed.typed.set(v.to_string()))
                            .on_keydown(move |key| match key.key.as_str() {
                                "Enter" => commit.commit_text(),
                                "Escape" => cancel.typing.set(None),
                                _ => {}
                            })
                            .focus_if(|| true),
                    );
                }
                let mut links = div().class("link-layer");
                for l in &pic.links {
                    let (x, y, w, h) =
                        (l.x * scale, l.y * scale, l.width * scale, l.height * scale);
                    let style = |el: Element| {
                        el.class("link")
                            .css("left", move || format!("{x:.1}px"))
                            .css("top", move || format!("{y:.1}px"))
                            .css("width", move || format!("{w:.1}px"))
                            .css("height", move || format!("{h:.1}px"))
                    };
                    let made = match (&l.uri, l.page) {
                        (Some(uri), _) => Some(
                            style(element("a"))
                                .attr("href", uri.clone())
                                .attr("target", "_blank")
                                .attr("rel", "noopener noreferrer")
                                .attr("title", uri.clone())
                                .into_view(),
                        ),
                        (None, Some(p)) => {
                            let go = st.clone();
                            Some(
                                style(span())
                                    .attr("title", format!("{} {}", crate::i18n::tr("Page"), p + 1))
                                    .on_click(move || go.go_to(p))
                                    .into_view(),
                            )
                        }
                        _ => None,
                    };
                    if let Some(v) = made {
                        links = links.child(v);
                    }
                }
                return div()
                    .class("page-wrap")
                    .css("width", move || format!("{width:.0}px"))
                    .css("height", move || format!("{height:.0}px"))
                    .child(
                        img()
                            .class("page-image")
                            .class_if("dark", {
                                let d = st.dark_pages.clone();
                                move || d.get()
                            })
                            .attr("src", pic.address.clone())
                            .attr("alt", format!("{} {}", crate::i18n::tr("Page"), n + 1))
                            .attr("draggable", "false")
                            .css("width", move || format!("{width:.0}px")),
                    )
                    .child(layer)
                    .child(links)
                    .into_view();
            }
            let text = st.page_text(n);
            if st.page_count() == 0 {
                return View::Empty;
            }
            let (ranges, current) = st.hits_on(n);
            let mut out = pre().class("page-text");
            let mut at = 0;
            for (k, (a, b)) in ranges.iter().copied().enumerate() {
                if a < at
                    || b > text.len()
                    || !text.is_char_boundary(a)
                    || !text.is_char_boundary(b)
                {
                    continue;
                }
                out = out.child(text[at..a].to_string());
                out = out.child(
                    element("mark")
                        .class(if Some(k) == current { "current" } else { "" })
                        .text(text[a..b].to_string()),
                );
                at = b;
            }
            out = out.child(text[at..].to_string());
            if text.trim().is_empty() {
                return p().class("empty").text(crate::i18n::tr("No text on this page (it may be a picture): see it in the browser's viewer")).into_view();
            }
            out.into_view()
        })
    };
    div().class("reader").attr("role", "main").child(
        div()
            .class("paper")
            .attr_fn("data-zoom", move || ZOOMS[zoom.get()].to_string())
            .child(text),
    )
}

/// The language picker and the theme menu, at the end of the top line.
fn extras(state: &PdfState) -> Element {
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
