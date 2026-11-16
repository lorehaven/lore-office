//! The screen: the ribbon, the list of notes, the editor and preview, the links panel, and the presenter.

use crate::i18n::{text, tr};
use crate::render::{OnLink, render};
use crate::state::{Mode, NotesState, PromptKind, THEMES};
use notes_md::parse_front;
use office_ui::MenuItem;
use quench_ui::event::prevent_default;
use quench_ui::prelude::*;
use std::rc::Rc;

/// The whole screen. It is drawn again when the language changes, so every label follows.
#[must_use]
pub fn notes_view(state: &NotesState) -> View {
    let state = state.clone();
    View::dynamic(move || {
        let _ = state.language.get();
        screen(&state)
    })
}

fn link_handler(state: &NotesState) -> OnLink {
    let st = state.clone();
    Rc::new(move |target: &str, heading: Option<&str>| st.open_link(target, heading))
}

fn screen(state: &NotesState) -> View {
    let keys = state.clone();
    div()
        .class("notes")
        .child(office_cloud_ui::overlay())
        .attr("tabindex", "-1")
        .on_keydown(move |k| {
            let command = k.ctrl || k.meta;
            match k.key.as_str() {
                "s" | "S" if command => {
                    prevent_default();
                    if !office_cloud_ui::save_shortcut() {
                        keys.save_md();
                    }
                }
                "o" | "O" if command => {
                    prevent_default();
                    keys.ask(PromptKind::Switch);
                }
                "n" | "N" if k.alt => {
                    prevent_default();
                    keys.ask(PromptKind::New);
                }
                "F5" => {
                    prevent_default();
                    keys.present(0);
                }
                "Escape" if keys.prompt.get_untracked().is_some() => keys.cancel_prompt(),
                _ => {}
            }
        })
        .child(ribbon(state))
        .child(sidebar(state))
        .child(work(state))
        .child(panel(state))
        .child(prompt_layer(state))
        .child(present_layer(state))
        .into_view()
}

// ---- the ribbon ----

fn ribbon(state: &NotesState) -> Element {
    let title = {
        let (rev, active, st) = (state.rev.clone(), state.active.clone(), state.clone());
        create_memo(move || {
            let _ = rev.get();
            let name = active.get();
            st.titles()
                .into_iter()
                .find(|(n, _)| *n == name)
                .map_or(name, |(_, t)| t)
        })
    };
    let name_signal = create_signal(String::new());
    {
        let (name_signal, title) = (name_signal.clone(), title);
        create_effect(move || name_signal.set(title.get()));
    }
    let tabs = vec![
        ("Home", home_groups(state)),
        ("Insert", insert_groups(state)),
        ("Deck", deck_groups(state)),
    ];
    office_ui::ribbon(
        "lore-notes",
        name_signal,
        state.status.clone(),
        "Ribbon",
        file_menu(state),
        extras(state),
        tabs,
    )
}

fn extras(state: &NotesState) -> Element {
    let language = state.language.clone();
    let current = Rc::new(move || language.get()) as Rc<dyn Fn() -> String>;
    let pick = state.clone();
    div()
        .child(office_cloud_ui::cloud_button())
        .child(office_theme::language_picker(
            &text("language_label"),
            crate::i18n::CODES,
            &current,
            Rc::new(move |code| pick.set_language(code)),
        ))
        .child(office_theme::paper_button(&text("paper_toggle")))
        .child(office_theme::theme_menu(&office_theme::ThemeLabels {
            toggle: text("theme_toggle"),
            mode: text("theme_mode"),
            light: text("theme_light"),
            dark: text("theme_dark"),
            system: text("theme_system"),
            palette: text("theme_palette"),
            palettes: ["blue", "green", "teal", "purple", "rose", "orange"]
                .map(|name| text(&format!("palette_{name}"))),
        }))
}

fn file_menu(state: &NotesState) -> Element {
    let pdf = state.clone();
    let (new, open, save, zip, html, deck, switch) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    office_ui::menu(
        "File",
        "New, open and save",
        office_cloud_ui::with_menu(vec![
            MenuItem::action("New note\u{2026}", "Alt+N", move || {
                new.ask(PromptKind::New)
            }),
            MenuItem::action("Go to note\u{2026}", "Ctrl+O", move || {
                switch.ask(PromptKind::Switch)
            }),
            MenuItem::action("Open files\u{2026}", "", move || open.open_files()),
            MenuItem::Separator,
            MenuItem::action("Save note (.md)", file_hint(), move || save.save_md()),
            MenuItem::action("Save all notes (.zip)", "", move || zip.save_zip()),
            MenuItem::sub(
                "Export\u{2026}",
                vec![
                    MenuItem::action("Web page (.html)", "", move || html.export_html()),
                    MenuItem::action("Slide show (.html)", "", move || deck.export_deck()),
                    MenuItem::action("Slides on paper (.pdf)", "", move || pdf.export_pdf()),
                ],
            ),
        ]),
    )
}

fn home_groups(state: &NotesState) -> Vec<Element> {
    let (new, rename, delete) = (state.clone(), state.clone(), state.clone());
    let note = office_ui::group(
        "Note",
        vec![
            office_ui::tool("New", "Make a note", move || new.ask(PromptKind::New)).into_view(),
            office_ui::tool(
                "Rename",
                "Give this note another name; links follow",
                move || rename.ask(PromptKind::Rename),
            )
            .into_view(),
            office_ui::tool("Delete", "Delete this note", move || {
                delete.ask(PromptKind::Delete)
            })
            .into_view(),
        ],
    );
    let mode_button = |label: &'static str, tip: &'static str, mode: Mode| {
        let (set, shown) = (state.clone(), state.mode.clone());
        button()
            .attr("type", "button")
            .attr("title", tr(tip))
            .class("tool")
            .class_if("on", move || shown.get() == mode)
            .text(tr(label))
            .on_click(move || set.mode.set(mode))
            .into_view()
    };
    let view = office_ui::group(
        "View",
        vec![
            mode_button("Edit", "Only the text", Mode::Edit),
            mode_button("Split", "Text and preview side by side", Mode::Split),
            mode_button("Preview", "Only the preview", Mode::Preview),
        ],
    );
    let present = state.clone();
    let show = office_ui::group(
        "Present",
        vec![
            office_ui::tool("Present", "Show this note as slides (F5)", move || {
                present.present(0)
            })
            .into_view(),
        ],
    );
    // The text tools: they act on the selection of the editor and give the focus back to it.
    let tool = |label: &'static str, tip: &'static str, act: fn(&NotesState)| {
        let st = state.clone();
        office_ui::tool(label, tip, move || {
            act(&st);
            st.refocus_editor();
        })
        .into_view()
    };
    let format = office_ui::group(
        "Format",
        vec![
            tool("Bold", "Make the selection bold (Ctrl+B)", |s| {
                s.wrap_selection("**", "**", &tr("bold"))
            }),
            tool("Italic", "Make the selection italic (Ctrl+I)", |s| {
                s.wrap_selection("*", "*", &tr("italic"))
            }),
            tool("Code", "Set the selection as code", |s| {
                s.wrap_selection("`", "`", &tr("code"))
            }),
            tool("Link", "Link the selection to a note", |s| {
                s.wrap_selection("[[", "]]", &tr("note"))
            }),
            tool("Heading", "Make the line a heading", |s| {
                s.toggle_line_prefix("## ")
            }),
            tool("Quote", "Make the lines a quotation", |s| {
                s.toggle_line_prefix("> ")
            }),
            tool("List", "Make the lines a list", |s| {
                s.toggle_line_prefix("- ")
            }),
        ],
    );
    vec![note, view, format, show]
}

fn insert_groups(state: &NotesState) -> Vec<Element> {
    let snippet = |label: &'static str, tip: &'static str, markdown: &'static str| {
        let st = state.clone();
        office_ui::tool(label, tip, move || {
            st.insert_block(markdown);
            st.refocus_editor();
        })
        .into_view()
    };
    let blocks = office_ui::group(
        "Blocks",
        vec![
            snippet("Heading", "Add a heading", "## Heading\n"),
            snippet("List", "Add a bulleted list", "- one\n- two\n"),
            snippet("Tasks", "Add a list of tasks", "- [ ] to do\n- [x] done\n"),
            snippet("Table", "Add a table", "| A | B |\n|---|---|\n| 1 | 2 |\n"),
            snippet("Code", "Add a block of code", "```\ncode\n```\n"),
            snippet("Quote", "Add a quotation", "> quoted text\n"),
            snippet(
                "Callout",
                "Add a callout",
                "> [!note] Title\n> Text of the callout\n",
            ),
            snippet("Math", "Add a formula", "$$\nE = mc^2\n$$\n"),
        ],
    );
    let slide = office_ui::group(
        "Slides",
        vec![
            snippet("New slide", "Start another slide here", "---\n\n"),
            snippet(
                "Speaker notes",
                "Add notes only the speaker sees",
                "???\nNotes for the speaker\n",
            ),
        ],
    );
    let links_state = state.clone();
    let names: Vec<MenuItem> = state
        .titles()
        .into_iter()
        .map(|(name, title)| {
            let st = links_state.clone();
            MenuItem::action(&title, "", move || {
                st.insert_text(&format!("[[{name}]]"));
                st.refocus_editor();
            })
        })
        .collect();
    let link = office_ui::group(
        "Links",
        vec![office_ui::menu("Link to a note", "Add a link to another note", names).into_view()],
    );
    vec![blocks, slide, link]
}

fn deck_groups(state: &NotesState) -> Vec<Element> {
    let themes: Vec<MenuItem> = THEMES
        .iter()
        .map(|(key, label)| {
            let st = state.clone();
            MenuItem::action(label, "", move || st.set_theme(key))
        })
        .collect();
    let aspects: Vec<MenuItem> = [
        ("16:9", "Widescreen (16:9)"),
        ("16:10", "Wide (16:10)"),
        ("4:3", "Classic (4:3)"),
        ("1:1", "Square (1:1)"),
    ]
    .into_iter()
    .map(|(key, label)| {
        let st = state.clone();
        MenuItem::action(label, "", move || st.set_aspect(key))
    })
    .collect();
    let (html, present, pdf) = (state.clone(), state.clone(), state.clone());
    vec![
        office_ui::group(
            "Look",
            vec![
                office_ui::menu("Theme", "How the slides look", themes).into_view(),
                office_ui::menu("Shape", "The proportions of a slide", aspects).into_view(),
            ],
        ),
        office_ui::group(
            "Show",
            vec![
                office_ui::tool("Present", "Show this note as slides (F5)", move || {
                    present.present(0)
                })
                .into_view(),
                office_ui::tool(
                    "Save as web page",
                    "Save the slides as a page that works on its own",
                    move || html.export_deck(),
                )
                .into_view(),
                office_ui::tool(
                    "Save as PDF",
                    "Save the slides as a PDF with a page for each",
                    move || pdf.export_pdf(),
                )
                .into_view(),
            ],
        ),
    ]
}

// ---- the list of notes ----

fn sidebar(state: &NotesState) -> Element {
    let (typed, search) = (state.clone(), state.query.clone());
    let list = {
        let (st, rev, query, active) = (
            state.clone(),
            state.rev.clone(),
            state.query.clone(),
            state.active.clone(),
        );
        View::dynamic(move || {
            let _ = rev.get();
            let query = query.get();
            let current = active.get();
            let pick = |name: String, title: String, line: String| {
                let (go, name_for_click, is_active) = (st.clone(), name.clone(), name == current);
                button()
                    .attr("type", "button")
                    .class("note-item")
                    .class_if("active", move || is_active)
                    .child(span().class("note-title").text(title))
                    .child(span().class("note-line").text(line))
                    .on_click(move || go.select(&name_for_click))
                    .into_view()
            };
            let items: Vec<View> = if query.trim().is_empty() {
                st.titles()
                    .into_iter()
                    .map(|(n, t)| pick(n, t, String::new()))
                    .collect()
            } else {
                st.search(&query)
                    .into_iter()
                    .map(|h| pick(h.name.clone(), h.name, h.line))
                    .collect()
            };
            if items.is_empty() {
                return p().class("empty").text(tr("No note matches")).into_view();
            }
            div().class("note-list").children(items).into_view()
        })
    };
    let tags = {
        let (st, rev) = (state.clone(), state.rev.clone());
        View::dynamic(move || {
            let _ = rev.get();
            let all = st.tags();
            if all.is_empty() {
                return View::Empty;
            }
            let chips: Vec<View> = all
                .into_iter()
                .map(|(t, n)| {
                    let (set, label) = (st.clone(), format!("#{t}"));
                    button()
                        .attr("type", "button")
                        .class("chip")
                        .text(format!("{label} {n}"))
                        .on_click(move || set.query.set(label.clone()))
                        .into_view()
                })
                .collect();
            div()
                .class("tags")
                .child(div().class("panel-label").text(tr("Tags")))
                .children(chips)
                .into_view()
        })
    };
    element("aside")
        .class("sidebar")
        .child(
            input()
                .attr("type", "search")
                .class("search")
                .attr("placeholder", tr("Search the notes"))
                .attr("aria-label", tr("Search the notes"))
                .attr_fn("value", move || search.get())
                .on_input(move |v| typed.query.set(v.to_string())),
        )
        .child(list)
        .child(tags)
}

// ---- the editor and the preview ----

/// The text field of the note. Built again (and focused) when a button of the ribbon has changed its text.
fn editor(state: &NotesState) -> View {
    let (st, epoch) = (state.clone(), state.editor_epoch.clone());
    View::dynamic(move || {
        let focused = epoch.get() > 0;
        let (typed, value, moved, keys, caret) = (
            st.clone(),
            st.buffer.clone(),
            st.clone(),
            st.clone(),
            st.caret.clone(),
        );
        textarea()
            .class("editor")
            .attr("spellcheck", "false")
            .attr("aria-label", tr("Note text"))
            .attr_fn("value", move || value.get())
            .attr_fn("selection", move || {
                let (a, b) = caret.get();
                format!("{a},{b}")
            })
            .on(EventKind::Selection, move |e| {
                if let Event::Selection(s) = e {
                    moved.set_caret(s.start, s.end);
                }
            })
            .on_keydown(move |k| {
                let command = k.ctrl || k.meta;
                match k.key.as_str() {
                    "Tab" if !command => {
                        prevent_default();
                        keys.indent(k.shift);
                    }
                    "b" | "B" if command => {
                        prevent_default();
                        keys.wrap_selection("**", "**", &tr("bold"));
                    }
                    "i" | "I" if command => {
                        prevent_default();
                        keys.wrap_selection("*", "*", &tr("italic"));
                    }
                    _ => {}
                }
            })
            .on_input(move |v| typed.edit(v))
            .focus_if(move || focused)
            .into_view()
    })
}

fn work(state: &NotesState) -> Element {
    let mode = state.mode.clone();
    let editor = editor(state);
    let preview = {
        let (buffer, on_link) = (state.buffer.clone(), link_handler(state));
        View::dynamic(move || {
            let text = buffer.get();
            render(parse_front(&text).1, &on_link).into_view()
        })
    };
    let (m1, m2) = (state.mode.clone(), state.mode.clone());
    div()
        .class("work")
        .attr("role", "main")
        .attr_fn("data-mode", move || {
            match mode.get() {
                Mode::Edit => "edit",
                Mode::Split => "split",
                Mode::Preview => "preview",
            }
            .to_string()
        })
        .child(
            div()
                .class("pane editor-pane")
                .class_if("hidden", move || m1.get() == Mode::Preview)
                .child(editor),
        )
        .child(
            div()
                .class("pane preview-pane")
                .class_if("hidden", move || m2.get() == Mode::Edit)
                .child(div().class("preview").child(preview)),
        )
}

// ---- links, slides ----

fn panel(state: &NotesState) -> Element {
    let go_list = |st: &NotesState, names: Vec<String>| -> View {
        if names.is_empty() {
            return p().class("empty").text("\u{2014}").into_view();
        }
        div()
            .class("link-list")
            .children(names.into_iter().map(|n| {
                let (go, name) = (st.clone(), n.clone());
                button()
                    .attr("type", "button")
                    .class("link-item")
                    .text(n)
                    .on_click(move || go.select(&name))
                    .into_view()
            }))
            .into_view()
    };
    let links =
        {
            let (st, rev, active) = (state.clone(), state.rev.clone(), state.active.clone());
            View::dynamic(move || {
                let _ = (rev.get(), active.get());
                let (found, missing) = st.outgoing();
                let mut out = div()
                    .child(div().class("panel-label").text(tr("Backlinks")))
                    .child(go_list(&st, st.backlinks()))
                    .child(div().class("panel-label").text(tr("Links from this note")))
                    .child(go_list(&st, found));
                if !missing.is_empty() {
                    out =
                        out.child(div().class("panel-label").text(tr("Not written yet")))
                            .child(div().class("link-list").children(missing.into_iter().map(
                                |n| {
                                    let (go, name) = (st.clone(), n.clone());
                                    button()
                                        .attr("type", "button")
                                        .class("link-item missing")
                                        .text(n)
                                        .on_click(move || go.open_link(&name, None))
                                        .into_view()
                                },
                            )));
                }
                out.into_view()
            })
        };
    let slides = {
        let (st, buffer) = (state.clone(), state.buffer.clone());
        View::dynamic(move || {
            let _ = buffer.get();
            let deck = st.deck();
            let items: Vec<View> = deck
                .slides
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let first = s
                        .markdown
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .trim_start_matches('#')
                        .trim()
                        .to_string();
                    let go = st.clone();
                    button()
                        .attr("type", "button")
                        .class("link-item")
                        .text(format!(
                            "{}. {}",
                            i + 1,
                            if first.is_empty() {
                                tr("(empty)")
                            } else {
                                first
                            }
                        ))
                        .on_click(move || go.present(i))
                        .into_view()
                })
                .collect();
            div()
                .child(div().class("panel-label").text(format!(
                    "{} ({})",
                    tr("Slides"),
                    items.len()
                )))
                .child(div().class("link-list").children(items))
                .into_view()
        })
    };
    let words = {
        let (st, buffer) = (state.clone(), state.buffer.clone());
        View::dynamic(move || {
            let _ = buffer.get();
            p().class("count")
                .text(format!("{} {}", st.word_count(), tr("words")))
                .into_view()
        })
    };
    element("aside")
        .class("panel")
        .child(links)
        .child(slides)
        .child(words)
}

// ---- questions ----

fn prompt_layer(state: &NotesState) -> View {
    let (open, st) = (state.prompt.clone(), state.clone());
    View::keyed(
        move || open.get().map(|p| vec![p.kind as u8]).unwrap_or_default(),
        |k: &u8| *k,
        move |_| {
            let kind = st
                .prompt
                .get_untracked()
                .map_or(PromptKind::New, |p| p.kind);
            let (title, ask) = match kind {
                PromptKind::New => ("New note", "Name"),
                PromptKind::Rename => ("Rename the note", "New name"),
                PromptKind::Delete => ("Delete the note", ""),
                PromptKind::Switch => ("Go to note", "Name"),
            };
            let (typed, ok, cancel, enter, value, error, hits) = (
                st.clone(),
                st.clone(),
                st.clone(),
                st.clone(),
                st.prompt.clone(),
                st.prompt.clone(),
                st.clone(),
            );
            let field = (kind != PromptKind::Delete).then(|| {
                input()
                    .attr("type", "text")
                    .class("prompt-input")
                    .attr("aria-label", tr(ask))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || {
                        value.get().map(|p| p.value).unwrap_or_default()
                    })
                    .on_input(move |v| typed.set_prompt_value(v))
                    .on_keydown(move |k| match k.key.as_str() {
                        "Enter" => {
                            prevent_default();
                            enter.answer();
                        }
                        "Escape" => {
                            prevent_default();
                            enter.cancel_prompt();
                        }
                        _ => {}
                    })
                    .focus_if(|| true)
            });
            let suggestions = (kind == PromptKind::Switch).then(|| {
                let (prompt, go) = (hits.prompt.clone(), hits.clone());
                View::dynamic(move || {
                    let q = prompt.get().map(|p| p.value).unwrap_or_default();
                    let rows: Vec<View> = go
                        .search(&q)
                        .into_iter()
                        .take(6)
                        .map(|h| {
                            let (pick, name) = (go.clone(), h.name.clone());
                            button()
                                .attr("type", "button")
                                .class("link-item")
                                .text(h.name)
                                .on_click(move || {
                                    pick.select(&name);
                                    pick.cancel_prompt();
                                })
                                .into_view()
                        })
                        .collect();
                    div().class("link-list").children(rows).into_view()
                })
            });
            div()
                .class("prompt-backdrop")
                .child(
                    div()
                        .class("prompt")
                        .attr("role", "dialog")
                        .attr("aria-label", tr(title))
                        .child(div().class("prompt-title").text(tr(title)))
                        .child(field)
                        .child(suggestions)
                        .child(
                            div()
                                .class("prompt-error")
                                .attr("role", "alert")
                                .text_fn(move || error.get().map(|p| p.error).unwrap_or_default()),
                        )
                        .child(
                            div()
                                .class("prompt-buttons")
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("tool primary")
                                        .text(tr("OK"))
                                        .on_click(move || ok.answer()),
                                )
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("tool")
                                        .text(tr("Cancel"))
                                        .on_click(move || cancel.cancel_prompt()),
                                ),
                        ),
                )
                .into_view()
        },
    )
}

// ---- the presenter ----

fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn present_layer(state: &NotesState) -> View {
    let (on, st) = (state.presenting.clone(), state.clone());
    View::keyed(
        move || if on.get() { vec![0_u8] } else { Vec::new() },
        |k: &u8| *k,
        move |_| {
            let deck = st.deck();
            let (w, h) = (f64::from(deck.stage.0), f64::from(deck.stage.1));
            let on_link: OnLink = Rc::new(|_, _| {});
            let slide_view = {
                let (slide, deck, go) = (st.slide.clone(), deck.clone(), Rc::clone(&on_link));
                let revealed = st.revealed.clone();
                View::dynamic(move || {
                    let i = slide.get().min(deck.slides.len().saturating_sub(1));
                    let s = &deck.slides[i];
                    let layout = s
                        .layout
                        .as_deref()
                        .map_or_else(String::new, |l| format!(" layout-{l}"));
                    div()
                        .class(format!("slide theme-{}{layout}", deck.theme))
                        .child(render(
                            &if s
                                .directives
                                .iter()
                                .any(|(k, v)| k == "fragments" && v != "false")
                                || s.layout.as_deref() == Some("fragments")
                            {
                                crate::state::fragments_shown(&s.markdown, revealed.get())
                            } else {
                                s.markdown.clone()
                            },
                            &go,
                        ))
                        .into_view()
                })
            };
            let scale = st.room.clone();
            let size_w = st.room.clone();
            let fit = move || {
                let (rw, rh) = scale.get();
                (rw / w).min(rh / h).max(0.05)
            };
            let (fit_a, fit_b, fit_c) = (fit.clone(), fit.clone(), fit);
            let stage = div()
                .class("stage-box")
                .css("width", move || format!("{}px", w * fit_a()))
                .css("height", move || format!("{}px", h * fit_b()))
                .child(
                    div()
                        .class("stage")
                        .css("width", move || format!("{w}px"))
                        .css("height", move || format!("{h}px"))
                        .css("transform", move || format!("scale({})", fit_c()))
                        .child(slide_view),
                );
            let notes_panel = {
                let (show, slide, deck) = (st.show_notes.clone(), st.slide.clone(), deck.clone());
                View::dynamic(move || {
                    if !show.get() {
                        return View::Empty;
                    }
                    let i = slide.get().min(deck.slides.len().saturating_sub(1));
                    let notes = deck.slides[i].notes.clone();
                    let next = deck.slides.get(i + 1).map(|s| {
                        s.markdown
                            .lines()
                            .next()
                            .unwrap_or("")
                            .trim_start_matches('#')
                            .trim()
                            .to_string()
                    });
                    div()
                        .class("speaker")
                        .child(div().class("panel-label").text(tr("Speaker notes")))
                        .child(p().class("speaker-notes").text(if notes.is_empty() {
                            tr("(none)")
                        } else {
                            notes
                        }))
                        .child(div().class("panel-label").text(tr("Next")))
                        .child(p().text(next.unwrap_or_else(|| tr("The end"))))
                        .into_view()
                })
            };
            let (progress, timer, total) =
                (st.slide.clone(), st.elapsed.clone(), deck.slides.len());
            let (keys, close, back, forward, toggle) =
                (st.clone(), st.clone(), st.clone(), st.clone(), st.clone());
            div()
                .class("present")
                .attr("role", "dialog")
                .attr("aria-label", tr("Presentation"))
                .attr("tabindex", "0")
                .focus_if(|| true)
                .on_keydown(move |k| {
                    let handled = match k.key.as_str() {
                        "ArrowRight" | "ArrowDown" | "PageDown" | " " | "Enter" => {
                            keys.step(1);
                            true
                        }
                        "ArrowLeft" | "ArrowUp" | "PageUp" | "Backspace" => {
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
                        "n" | "N" => {
                            keys.show_notes.set(!keys.show_notes.get_untracked());
                            true
                        }
                        "Escape" | "q" | "Q" => {
                            keys.stop_presenting();
                            true
                        }
                        _ => false,
                    };
                    if handled {
                        prevent_default();
                    }
                })
                .child(
                    div()
                        .class("present-room")
                        .on(EventKind::Resize, move |e| {
                            if let Event::Resize(s) = e {
                                size_w.set((s.width, s.height));
                            }
                        })
                        .child(stage)
                        .child(notes_panel),
                )
                .child(
                    div()
                        .class("present-bar")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text("\u{25C0}")
                                .attr("aria-label", tr("Previous slide"))
                                .on_click(move || back.step(-1)),
                        )
                        .child(
                            span()
                                .class("progress")
                                .text_fn(move || format!("{} / {total}", progress.get() + 1)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text("\u{25B6}")
                                .attr("aria-label", tr("Next slide"))
                                .on_click(move || forward.step(1)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(tr("Notes"))
                                .on_click(move || {
                                    toggle.show_notes.set(!toggle.show_notes.get_untracked())
                                }),
                        )
                        .child(span().class("clock").text_fn(move || clock(timer.get())))
                        .child(
                            button()
                                .attr("type", "button")
                                .class("tool")
                                .text(tr("Close"))
                                .on_click(move || close.stop_presenting()),
                        ),
                )
                .into_view()
        },
    )
}

/// The shortcut shown beside saving a file: Ctrl+S, unless it saves to the cloud (then the cloud's item has it).
fn file_hint() -> &'static str {
    if office_cloud_ui::saves_to_cloud() {
        ""
    } else {
        "Ctrl+S"
    }
}
