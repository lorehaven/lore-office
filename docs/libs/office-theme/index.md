---
title: office-theme
parent: Libraries
nav_order: 4
---

The theme of the suite: the `--office-*` variables, the palettes, the script that keeps the choice, and the small menus for it. It depends on no other suite crate and none of forge's. Version 0.1.0.

Serve `STYLE` first, and put `SCRIPT` before the page's own script so the stored choice is applied before the page is drawn.

## Public items

| Item | Meaning |
|---|---|
| `STYLE`, `SCRIPT` | the style sheet and the script |
| `icon_script(svg)` | a script that sets the page's icon from an SVG when it has none |
| `PALETTES` | `("blue", ..)`, `("green", ..)`, `("teal", ..)`, `("purple", ..)`, `("rose", ..)`, `("orange", ..)` with the swatch colours; green is used until another is chosen |
| `ThemeLabels` | the menu's texts in the app's language |
| `theme_menu(&labels)` | the menu: Light, Dark, System and the palette swatches |
| `paper_button(label)` | the button that shows the page or sheet in light colours whatever the theme is |
| `language_picker(...)`, `flag(locale)` | the language list; a locale's flag (`pl-PL` as its regional indicator letters) |

The variables follow the design tokens of the forge estate: Untitled UI grays, forge's brand scales (600 for light, 400 for dark), radius 6 px and 10 px, forge's shadows.

## Storage

The script keeps the choices in the browser's storage under `q:theme`, `q:palette` and `q:paper`, and sets `html[data-theme]`, `html[data-palette]` and `html[data-paper]`. It takes up the forge cookies `qtheme`, `fui_palette` and `qlocale` when they are set and writes them when a person chooses; it also closes the bar's menus on a click elsewhere or Escape, and shows the account picture once it has loaded.
