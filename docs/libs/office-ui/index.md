---
title: office-ui
parent: Libraries
nav_order: 3
---

The parts of the screen the apps share: the top bar, the ribbon, tools, drop-down menus, icons and the base style. Version 0.1.0.

Serve `office_ui::STYLE` after `office_theme::STYLE`; it uses the `--office-*` variables of `office-theme`. Labels go through the app's translator (`set_translator`), so a language change rebuilds them in the new language.

## Public items

| Item | Meaning |
|---|---|
| `STYLE` | the base page style, scrollbars, ribbon, menus, icons and the cloud panel |
| `set_translator(f)`, `t(text)` | the function labels go through; a label in the language in use |
| `set_home(href)` | where the suite's home is; every top bar then shows the home button |
| `set_frame(lead, end)` | what the host adds to the top bar in forge: the dashboard button before home, the account menu at the end (`Slot`s; `None` for neither) |
| `topbar(app, actions)` | the forge-style bar: dashboard, home, the app's picture and name, `actions`, the account menu |
| `svg_address(svg)` | an SVG as a `data:` address |
| `ribbon(...)`, `ribbon_named(...)` | the top bar (with the extras in it) and the two-line ribbon under it: `lead` (the File menu), one tab for each group set, the document's name and status; `ribbon_named` makes the name a field and hands what is typed to a `Rename` |
| `group(name, items)` | one section of a tab |
| `tool(label, tip, action)`, `icon_tool(name, tip, action)` | a text button; a drawing button |
| `icon(name)`, `looks(button, icon, label, tip)` | a drawing by name (`bullets`, `numbers`, `left`, `center`, `right`, `justify`, `indent`, `outdent`, `spacing`, `undo`, `redo`, `highlight`, `link`, `pagebreak`, `home`, `cloud`); a button as a drawing or as words |
| `MenuItem` | `Action(label, hint, handler)`, `Submenu(label, items)`, `Separator`; `MenuItem::action`, `MenuItem::sub` |
| `menu(label, tip, items)` | a drop-down menu; a label starting with `@` is a drawing's name |
| `context_menu(at, items)` | a menu at a point of the screen, for the secondary button |
| `renamed(old, typed)`, `rename_to(signal, typed)` | the name a document gets from what was typed (trimmed, no path characters, the old extension kept when none was typed) |

Where the browser supports CSS anchor positioning, the ribbon's lines scroll sideways in a narrow window and menus open under their button; elsewhere the tools wrap onto more lines.
