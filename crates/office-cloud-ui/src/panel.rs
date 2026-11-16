//! The documents panel: a browser of folders and documents, as a grid of cards or as a list, with what is picked acted on
//! from a toolbar (open, move, share, delete), and a section for what other people shared.

use crate::{Cloud, Panel, Section, account, keep_grid, size, t};
use office_ui::MenuItem;
use quench_ui::prelude::*;

/// The whole panel body.
pub fn documents(c: &Cloud) -> View {
    div()
        .class("cloud-browser")
        .child(toolbar(c))
        .child(
            div()
                .class("cloud-split")
                .child(sidebar(c))
                .child(listing(c)),
        )
        .into_view()
}

fn tool_button(
    icon: &str,
    label: &str,
    tip: &str,
    enabled: impl Fn() -> bool + 'static,
    action: impl Fn() + 'static,
) -> Element {
    button()
        .attr("type", "button")
        .attr("title", t(tip))
        .class("tool")
        .attr_bool("disabled", move || !enabled())
        .child(office_ui::icon(icon))
        .child(span().text(t(label)))
        .on_click(action)
}

#[allow(clippy::too_many_lines)]
fn toolbar(c: &Cloud) -> Element {
    let (sel_open, sel_share, sel_del) = (c.clone(), c.clone(), c.clone());
    let (open, share, del, save, make) = (c.clone(), c.clone(), c.clone(), c.clone(), c.clone());
    let (grid_on, grid_off, grid_now, list_now) = (c.clone(), c.clone(), c.clone(), c.clone());
    let (typed, shown, usage) = (c.clone(), c.clone(), c.usage.clone());
    let moving = c.clone();
    let one = move |c: &Cloud| c.selected.get().len() == 1;
    let mine = move |c: &Cloud| c.section.get() == Section::Mine;
    div()
        .class("cloud-tools")
        .child(tool_button(
            "file",
            "Open",
            "Open the picked document",
            move || one(&sel_open),
            move || open.open_selected(),
        ))
        .child(View::dynamic(move || {
            let c = moving.clone();
            let ready = mine(&c) && !c.selected.get().is_empty();
            if !ready {
                return tool_button(
                    "move",
                    "Move to\u{2026}",
                    "Put the picked documents in a folder",
                    || false,
                    || {},
                )
                .into_view();
            }
            let mut items = vec![{
                let c = c.clone();
                MenuItem::action("Top (no folder)", "", move || c.move_selected(""))
            }];
            for path in c.folders.get() {
                let depth = path.matches('/').count();
                let label = format!(
                    "{}{}",
                    "\u{2003}".repeat(depth),
                    path.rsplit('/').next().unwrap_or(&path)
                );
                let c = c.clone();
                let to = path.clone();
                items.push(MenuItem::action(&label, "", move || c.move_selected(&to)));
            }
            office_ui::menu(
                "Move to\u{2026}",
                "Put the picked documents in a folder",
                items,
            )
            .into_view()
        }))
        .child(tool_button(
            "user",
            "Share\u{2026}",
            "Share the picked document, by link or with a person",
            move || one(&sel_share) && mine(&sel_share),
            move || {
                let ids = share.selected.get_untracked();
                if let [id] = ids.as_slice() {
                    share.link.set(String::new());
                    share.panel.set(Panel::Sharing(id.clone()));
                    share.list_shares();
                    share.list_doc_grants(id);
                }
            },
        ))
        .child(tool_button(
            "trash",
            "Delete",
            "Delete the picked documents from the cloud",
            move || mine(&sel_del) && !sel_del.selected.get().is_empty(),
            move || del.delete_selected(),
        ))
        .child(span().class("cloud-gap"))
        .child(
            input()
                .attr("type", "text")
                .class("cloud-folder-name")
                .attr("placeholder", t("New folder"))
                .attr("aria-label", t("Name of a new folder"))
                .attr_fn("value", move || shown.folder_name.get())
                .on_input(move |v| typed.folder_name.set(v.to_string()))
                .on_keydown({
                    let make = make.clone();
                    move |k| {
                        if k.key == "Enter" {
                            make.make_folder();
                        }
                    }
                }),
        )
        .child(tool_button(
            "plus",
            "Create",
            "Make a folder with that name in the folder shown",
            {
                let c = c.clone();
                move || !c.folder_name.get().trim().is_empty() && c.section.get() == Section::Mine
            },
            move || make.make_folder(),
        ))
        .child(tool_button(
            "cloud",
            "Save here",
            "Save the open document to the cloud, in the folder shown",
            || true,
            move || save.save(),
        ))
        .child(span().class("cloud-gap"))
        .child(span().class("cloud-usage").text_fn(move || {
            usage.get().map_or(String::new(), |u| {
                format!("{} / {}", size(u.used), size(u.quota))
            })
        }))
        .child(
            div()
                .class("cloud-view-switch")
                .attr("role", "group")
                .attr("aria-label", t("How documents are shown"))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("title", t("Cards"))
                        .attr("aria-label", t("Cards"))
                        .class("tool icon-tool")
                        .class_if("on", move || grid_now.grid.get())
                        .child(office_ui::icon("grid"))
                        .on_click(move || {
                            grid_on.grid.set(true);
                            keep_grid(true);
                        }),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("title", t("List"))
                        .attr("aria-label", t("List"))
                        .class("tool icon-tool")
                        .class_if("on", move || !list_now.grid.get())
                        .child(office_ui::icon("list"))
                        .on_click(move || {
                            grid_off.grid.set(false);
                            keep_grid(false);
                        }),
                ),
        )
}

fn row(
    icon: &str,
    label: String,
    depth: usize,
    active: bool,
    action: impl Fn() + 'static,
) -> Element {
    button()
        .attr("type", "button")
        .class("cloud-side-row")
        .class_if("on", move || active)
        .css("padding-left", move || format!("{}px", 8 + depth * 14))
        .child(office_ui::icon(icon))
        .child(span().class("cloud-side-name").text(label))
        .on_click(action)
}

fn sidebar(c: &Cloud) -> Element {
    let list = c.clone();
    div()
        .class("cloud-side")
        .child(View::dynamic(move || {
            let c = list.clone();
            let (section, here, folders) = (c.section.get(), c.folder.get(), c.folders.get());
            let mut rows: Vec<View> = Vec::new();
            {
                let go = c.clone();
                rows.push(
                    row(
                        "cloud",
                        t("My documents"),
                        0,
                        section == Section::Mine && here.is_empty(),
                        move || go.go(""),
                    )
                    .into_view(),
                );
            }
            for path in folders {
                let depth = path.matches('/').count() + 1;
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                let active = section == Section::Mine && here == path;
                let go = c.clone();
                let to = path.clone();
                rows.push(row("folder", name, depth, active, move || go.go(&to)).into_view());
            }
            let go = c.clone();
            let shared = c.grants.get().len();
            rows.push(div().class("cloud-side-sep").into_view());
            rows.push(
                row(
                    "user",
                    format!("{} ({shared})", t("Shared with me")),
                    0,
                    section == Section::Shared,
                    move || go.go_shared(),
                )
                .into_view(),
            );
            div().class("cloud-side-list").children(rows).into_view()
        }))
        .child(account(c))
}

/// `2026-11-17` from seconds since 1970 (UTC).
fn date(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

struct Item {
    /// What the selection holds for it.
    key: String,
    name: String,
    sub: String,
    kind: String,
}

fn card(c: &Cloud, item: &Item, picked: bool) -> Element {
    let (toggle, pick, open) = (c.clone(), c.clone(), c.clone());
    let (key, key2, key3) = (item.key.clone(), item.key.clone(), item.key.clone());
    div()
        .class("cloud-item")
        .class_if("sel", move || picked)
        .child(
            input()
                .attr("type", "checkbox")
                .class("cloud-pick")
                .attr("aria-label", format!("{} {}", t("Pick"), item.name))
                .attr_bool("checked", move || picked)
                .on_click(move || {
                    toggle.selected.update(|s| {
                        if let Some(at) = s.iter().position(|k| *k == key) {
                            s.remove(at);
                        } else {
                            s.push(key.clone());
                        }
                    });
                }),
        )
        .child(
            div()
                .class("cloud-item-main")
                .child(
                    span()
                        .class("cloud-ico")
                        .class(format!("k-{}", item.kind))
                        .child(office_ui::icon("file")),
                )
                .child(
                    span()
                        .class("cloud-name")
                        .attr("title", item.name.clone())
                        .text(item.name.clone()),
                )
                .child(span().class("cloud-sub").text(item.sub.clone()))
                .on_click(move || pick.selected.set(vec![key2.clone()]))
                .on(EventKind::DoubleClick, move |_| {
                    open.selected.set(vec![key3.clone()]);
                    open.open_selected();
                }),
        )
}

fn folder_card(c: &Cloud, path: &str, docs_inside: usize) -> Element {
    let (go, del) = (c.clone(), c.clone());
    let (to, gone) = (path.to_string(), path.to_string());
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    div()
        .class("cloud-item folder")
        .child(
            div()
                .class("cloud-item-main")
                .child(
                    span()
                        .class("cloud-ico folder")
                        .child(office_ui::icon("folder")),
                )
                .child(
                    span()
                        .class("cloud-name")
                        .attr("title", name.clone())
                        .text(name),
                )
                .child(span().class("cloud-sub").text(if docs_inside == 0 {
                    t("empty")
                } else {
                    format!("{docs_inside}")
                }))
                .on_click(move || go.go(&to)),
        )
        .child(
            button()
                .attr("type", "button")
                .class("tool icon-tool cloud-folder-delete")
                .attr("title", t("Delete this folder (when it is empty)"))
                .attr("aria-label", t("Delete this folder (when it is empty)"))
                .child(office_ui::icon("trash"))
                .on_click(move || del.delete_folder(&gone)),
        )
}

#[allow(clippy::too_many_lines)]
fn listing(c: &Cloud) -> Element {
    let me = c.clone();
    div().class("cloud-main").child(View::dynamic(move || {
        let c = me.clone();
        let (section, here, grid) = (c.section.get(), c.folder.get(), c.grid.get());
        let picked = c.selected.get();
        let mut head = div().class("cloud-crumbs");
        let mut cards: Vec<View> = Vec::new();
        match section {
            Section::Mine => {
                let go = c.clone();
                head = head.child(
                    button()
                        .attr("type", "button")
                        .class("cloud-crumb")
                        .text(t("My documents"))
                        .on_click(move || go.go("")),
                );
                let mut path = String::new();
                for part in here.split('/').filter(|p| !p.is_empty()) {
                    if !path.is_empty() {
                        path.push('/');
                    }
                    path.push_str(part);
                    let (go, to) = (c.clone(), path.clone());
                    head = head
                        .child(span().class("cloud-crumb-sep").text("\u{203A}"))
                        .child(
                            button()
                                .attr("type", "button")
                                .class("cloud-crumb")
                                .text(part.to_string())
                                .on_click(move || go.go(&to)),
                        );
                }
                let docs = c.docs.get();
                for f in c.folders.get() {
                    let parent = f.rsplit_once('/').map_or("", |(p, _)| p);
                    if parent == here {
                        let inside = docs
                            .iter()
                            .filter(|d| d.folder == f || d.folder.starts_with(&format!("{f}/")))
                            .count();
                        cards.push(folder_card(&c, &f, inside).into_view());
                    }
                }
                for d in docs.iter().filter(|d| d.folder == here) {
                    let item = Item {
                        key: d.id.clone(),
                        name: d.name.clone(),
                        sub: format!("{} \u{b7} {}", size(d.size), date(d.modified)),
                        kind: d.kind.clone(),
                    };
                    let on = picked.contains(&item.key);
                    cards.push(card(&c, &item, on).into_view());
                }
            }
            Section::Shared => {
                head = head.child(span().class("cloud-crumb").text(t("Shared with me")));
                for g in c.grants.get() {
                    let item = Item {
                        key: format!("{}/{}", g.owner, g.doc),
                        name: g.name.clone(),
                        sub: format!(
                            "{} \u{b7} {}",
                            g.owner,
                            if g.rights == office_cloud::Rights::Edit {
                                t("can edit")
                            } else {
                                t("view only")
                            }
                        ),
                        kind: g.kind.clone(),
                    };
                    let on = picked.contains(&item.key);
                    cards.push(card(&c, &item, on).into_view());
                }
            }
        }
        let empty = cards.is_empty();
        div()
            .class("cloud-listing")
            .child(head)
            .child(if empty {
                p().class("empty")
                    .text(match section {
                        Section::Mine => t("Nothing here yet: save a document to the cloud."),
                        Section::Shared => t("Nothing has been shared with you yet."),
                    })
                    .into_view()
            } else {
                div()
                    .class(if grid {
                        "cloud-items grid"
                    } else {
                        "cloud-items list"
                    })
                    .children(cards)
                    .into_view()
            })
            .into_view()
    }))
}
