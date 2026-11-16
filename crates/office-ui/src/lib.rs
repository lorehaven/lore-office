//! The parts of the screen the lore apps share: theme, ribbon and drop-down menus.
//!
//! Serve [`STYLE`] ahead of the app's own style sheet and after `office-theme`'s, whose `--office-*` colour variables it
//! uses. Labels go through the app's translator ([`set_translator`]), so a language change rebuilds them in the new language.

use quench_ui::event::prevent_default;
use quench_ui::prelude::*;
use std::rc::Rc;

/// Theme variables, base page style, themed scrollbars, ribbon and menu rules.
pub const STYLE: &str = include_str!("office.css");

thread_local! {
    static HOME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static TRANSLATE: std::cell::Cell<fn(&str) -> String> = std::cell::Cell::new(str::to_string);
}

/// The function every label goes through on its way to the screen: English in, the language in use out. Without one the
/// label is shown as written.
pub fn set_translator(f: fn(&str) -> String) {
    TRANSLATE.with(|t| t.set(f));
}

/// Where the suite's home page is, so every ribbon links to it (`None`: this app stands alone and has no home).
pub fn set_home(href: Option<String>) {
    HOME.with(|h| *h.borrow_mut() = href);
}

/// A label in the language in use.
#[must_use]
pub fn t(text: &str) -> String {
    TRANSLATE.with(|f| f.get())(text)
}

/// One section of the ribbon: its tools over a small label.
#[must_use]
pub fn group(name: &str, items: Vec<View>) -> Element {
    let name = t(name);
    div()
        .class("group")
        .attr("role", "group")
        .attr("aria-label", name.clone())
        .child(div().class("group-body").children(items))
        .child(div().class("group-label").text(name))
}

/// A small drawing for a tool, by name (`bullets`, `left`, `undo`, …: the `.ic-*` rules of the style sheet).
#[must_use]
pub fn icon(name: &str) -> Element {
    span().class("icon").class(format!("ic-{name}"))
}

/// What a tool shows: a drawing (`icon` not empty) or its words; the button also says what it does to a screen reader.
#[must_use]
pub fn looks(button: Element, icon_name: &str, label: &str, tip: &str) -> Element {
    if icon_name.is_empty() {
        button.text(t(label))
    } else {
        button
            .class("icon-tool")
            .attr("aria-label", t(tip))
            .child(icon(icon_name))
    }
}

/// A tool button that is only a drawing; `tip` is its tooltip and its accessible name.
#[must_use]
pub fn icon_tool(name: &str, tip: &str, action: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .attr("title", t(tip))
        .attr("aria-label", t(tip))
        .class("tool icon-tool")
        .child(icon(name))
        .on_click(action)
}

/// A plain tool button for a group.
#[must_use]
pub fn tool(label: &str, tip: &str, action: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .attr("title", t(tip))
        .class("tool")
        .text(t(label))
        .on_click(action)
}

/// The link to the suite's home page, left of the app name; nothing when there is no home.
fn home_link() -> View {
    HOME.with(|h| h.borrow().clone())
        .map_or(View::Empty, |href| {
            a().class("home-link")
                .attr("href", href)
                .attr("title", t("Home"))
                .attr("aria-label", t("Home"))
                .child(span().class("icon icon-home"))
                .into_view()
        })
}

/// The name a document gets from what a person typed in its place.
///
/// It is trimmed, without characters a file name cannot have, and with the old extension when none was typed (`Report` over
/// `Untitled.docx` is `Report.docx`). Nothing typed keeps the old name.
#[must_use]
pub fn renamed(old: &str, typed: &str) -> String {
    let cleaned: String = typed
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .take(200)
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned == "." {
        return old.to_string();
    }
    let extension = old
        .rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty() && e.len() <= 5);
    match (cleaned.contains('.'), extension) {
        (false, Some(e)) => format!("{cleaned}.{e}"),
        _ => cleaned,
    }
}

/// Rename a document from what was typed: the new name goes in `name` (set twice, so a field showing what was typed shows
/// the name that was kept even when it did not change).
pub fn rename_to(name: &Signal<String>, typed: &str) {
    let new = renamed(&name.get_untracked(), typed);
    name.set(String::new());
    name.set(new);
}

/// The ribbon, in two lines.
///
/// The top line holds the app name, `lead` (the File menu), one tab for each of `tabs`, then the document name and the last
/// thing that happened. The line under it holds the groups of tools of the chosen tab.
///
/// Every tab's groups stay in the tree (the others are only hidden), so the state of a tool is never lost on a switch.
#[must_use]
pub fn ribbon(
    app: &'static str,
    document: Signal<String>,
    status: Signal<String>,
    label: &str,
    lead: Element,
    extras: Element,
    tabs: Vec<(&'static str, Vec<Element>)>,
) -> Element {
    ribbon_named(app, document, None, status, label, lead, extras, tabs)
}

/// What is done with a name typed in the ribbon.
pub type Rename = Rc<dyn Fn(&str)>;

/// As [`ribbon`], and when `rename` is given the document's name is a field: what is typed there is handed to it.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn ribbon_named(
    app: &'static str,
    document: Signal<String>,
    rename: Option<Rename>,
    status: Signal<String>,
    label: &str,
    lead: Element,
    extras: Element,
    tabs: Vec<(&'static str, Vec<Element>)>,
) -> Element {
    let chosen = create_signal(0usize);
    let mut bar = div()
        .class("ribbon-tabs")
        .attr("role", "navigation")
        .attr("aria-label", t(label))
        .child(home_link())
        .child(span().class("app-name").text(app))
        .child(div().class("ribbon-lead").child(lead));
    let mut panes = div().class("panes");
    for (i, (name, groups)) in tabs.into_iter().enumerate() {
        let (click, on, aria, shown) = (
            chosen.clone(),
            chosen.clone(),
            chosen.clone(),
            chosen.clone(),
        );
        bar = bar.child(
            button()
                .attr("type", "button")
                .attr_fn("aria-pressed", move || (aria.get() == i).to_string())
                .class("ribbon-tab")
                .class_if("on", move || on.get() == i)
                .text(t(name))
                .on_click(move || click.set(i)),
        );
        panes = panes.child(
            div()
                .class("groups")
                .attr("role", "toolbar")
                .attr("aria-label", t(name))
                .class_if("hidden", move || shown.get() != i)
                .children(groups.into_iter().map(IntoView::into_view)),
        );
    }
    bar = bar
        .child(match rename {
            Some(rename) => {
                let shown = document;
                input()
                    .attr("type", "text")
                    .class("file-name file-name-input")
                    .attr("title", t("Rename the document"))
                    .attr("aria-label", t("Document name"))
                    .attr("spellcheck", "false")
                    .attr_fn("value", move || shown.get())
                    .on_change(move |typed| rename(typed))
                    .into_view()
            }
            None => span()
                .class("file-name")
                .text_fn(move || document.get())
                .into_view(),
        })
        .child(
            span()
                .class("status")
                .attr("role", "status")
                .attr("aria-live", "polite")
                .text_fn(move || status.get()),
        )
        .child(div().class("ribbon-extras").child(extras));
    div().class("ribbon").child(bar).child(panes)
}

/// What a menu offers.
#[derive(Clone)]
pub enum MenuItem {
    /// A choice: its label, the shortcut shown beside it (may be empty), and what it does.
    Action(String, String, Rc<dyn Fn()>),
    /// A choice that opens another list beside it.
    Submenu(String, Vec<Self>),
    Separator,
}

impl std::fmt::Debug for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Action(label, ..) => write!(f, "Action({label})"),
            Self::Submenu(label, items) => write!(f, "Submenu({label}, {items:?})"),
            Self::Separator => f.write_str("Separator"),
        }
    }
}

impl MenuItem {
    #[must_use]
    pub fn action(label: &str, hint: &str, f: impl Fn() + 'static) -> Self {
        Self::Action(t(label), hint.to_string(), Rc::new(f))
    }

    /// A choice that opens another list beside it.
    #[must_use]
    pub fn sub(label: &str, items: Vec<Self>) -> Self {
        Self::Submenu(t(label), items)
    }
}

/// A button that opens a list of choices under it.
///
/// A choice closes the menu and does its thing; Escape or a click anywhere else closes it. The first choice takes focus
/// when it opens, and Tab moves between the choices.
#[must_use]
pub fn menu(label: &str, tip: &str, items: Vec<MenuItem>) -> Element {
    // A label that starts with `@` is the name of a drawing: the button shows it and the tip says what it is.
    let drawing = label.strip_prefix('@').map(str::to_string);
    let (label, tip) = (t(label), t(tip));
    let open = create_signal(false);
    let (toggle, shown) = (open.clone(), open.clone());
    let face = drawing.map_or_else(
        || span().text(label.clone()),
        |name| span().class("menu-drawing").child(icon(&name)),
    );
    div()
        .class("menu")
        .child(
            button()
                .attr("type", "button")
                .attr("title", tip.clone())
                .attr("aria-label", tip)
                .attr("aria-haspopup", "menu")
                .attr_fn("aria-expanded", {
                    let open = open.clone();
                    move || open.get().to_string()
                })
                .class("tool")
                .class("menu-button")
                .child(face)
                .on_click(move || toggle.update(|o| *o = !*o)),
        )
        .child(View::keyed(
            move || if shown.get() { vec![0_u8] } else { Vec::new() },
            |k: &u8| *k,
            move |_| menu_list(&open, &items).into_view(),
        ))
}

/// A menu that opens at a point of the screen (the viewport's coordinates), for the secondary button.
///
/// `at` is where it is open (`None` while closed); `items` is asked each time it opens, so a menu can suit what is under
/// the pointer. A choice closes the menu; Escape or a click anywhere else does too.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn context_menu(
    at: Signal<Option<(f64, f64)>>,
    items: impl Fn() -> Vec<MenuItem> + 'static,
) -> View {
    let shown = at.clone();
    let items = Rc::new(items);
    View::keyed(
        move || {
            shown
                .get()
                .map(|(x, y)| vec![(x as i64, y as i64)])
                .unwrap_or_default()
        },
        |k: &(i64, i64)| ((k.0 << 32) ^ k.1) as u64,
        move |(x, y)| {
            let closing = at.clone();
            let items = closing_items(&items(), &closing);
            let (backdrop, away, keys) = (closing.clone(), closing.clone(), closing);
            let open = create_signal(true);
            div()
                .child(
                    div()
                        .class("menu-backdrop context-backdrop")
                        .on_click(move || backdrop.set(None))
                        .on(EventKind::ContextMenu, move |_| away.set(None)),
                )
                .child(
                    list(&open, &items, true)
                        .class("context-menu")
                        .css("left", move || format!("min({x}px, calc(100vw - 260px))"))
                        .css("top", move || format!("min({y}px, calc(100vh - 380px))"))
                        .on_keydown(move |k| {
                            if k.key == "Escape" {
                                prevent_default();
                                keys.set(None);
                            }
                        }),
                )
                .into_view()
        },
    )
}

/// The items with every choice also closing the context menu.
fn closing_items(items: &[MenuItem], at: &Signal<Option<(f64, f64)>>) -> Vec<MenuItem> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Action(label, hint, f) => {
                let (f, at) = (f.clone(), at.clone());
                MenuItem::Action(
                    label.clone(),
                    hint.clone(),
                    Rc::new(move || {
                        at.set(None);
                        f();
                    }),
                )
            }
            MenuItem::Submenu(label, inner) => {
                MenuItem::Submenu(label.clone(), closing_items(inner, at))
            }
            MenuItem::Separator => MenuItem::Separator,
        })
        .collect()
}

fn menu_list(open: &Signal<bool>, items: &[MenuItem]) -> Element {
    let backdrop = open.clone();
    let keys = open.clone();
    div()
        .child(
            div()
                .class("menu-backdrop")
                .on_click(move || backdrop.set(false)),
        )
        .child(list(open, items, true).on_keydown(move |k| {
            if k.key == "Escape" {
                prevent_default();
                keys.set(false);
            }
        }))
}

/// The rows of one list. Only the first list of a menu takes focus on its first row.
fn list(open: &Signal<bool>, items: &[MenuItem], focus_first: bool) -> Element {
    let mut first = focus_first;
    let rows: Vec<View> = items
        .iter()
        .map(|item| match item {
            MenuItem::Separator => div()
                .class("menu-separator")
                .attr("role", "separator")
                .into_view(),
            MenuItem::Action(label, hint, f) => {
                let (f, open) = (f.clone(), open.clone());
                let focus_me = std::mem::take(&mut first);
                button()
                    .attr("type", "button")
                    .attr("role", "menuitem")
                    .class("menu-item")
                    .child(span().text(label.clone()))
                    .child(span().class("hint").text(hint.clone()))
                    .focus_if(move || focus_me)
                    .on_click(move || {
                        open.set(false);
                        f();
                    })
                    .into_view()
            }
            MenuItem::Submenu(label, inner) => {
                let focus_me = std::mem::take(&mut first);
                let (toggle, root) = (create_signal(false), open.clone());
                let (flag, inner) = (toggle.clone(), inner.clone());
                let (toggled, shown) = (toggle.clone(), toggle);
                div()
                    .class("menu-sub")
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("role", "menuitem")
                            .attr("aria-haspopup", "menu")
                            .attr_fn("aria-expanded", move || shown.get().to_string())
                            .class("menu-item")
                            .child(span().text(label.clone()))
                            .child(span().class("hint").text("\u{25b8}"))
                            .focus_if(move || focus_me)
                            .on_click(move || toggled.update(|o| *o = !*o)),
                    )
                    .child(View::keyed(
                        move || if flag.get() { vec![0_u8] } else { Vec::new() },
                        |k: &u8| *k,
                        move |_| list(&root, &inner, false).class("menu-nested").into_view(),
                    ))
                    .into_view()
            }
        })
        .collect();
    div().class("menu-list").attr("role", "menu").children(rows)
}
