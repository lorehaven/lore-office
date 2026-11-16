---
title: Theme and language
parent: Guides
nav_order: 5
---

The look and the language are the person's, kept in the browser, and the same in all four apps.

## Theme

The theme menu (the half-filled circle at the top right) offers Light, Dark or System, and six accent palettes: blue, green, teal, purple, rose and orange. With no choice, the system's scheme decides and the accent is green, as in forge. The choice is stored under the keys `q:theme` and `q:palette` in the browser's storage; `office-theme`'s script applies it to `html[data-theme]` and `html[data-palette]` before the page is drawn.

## The light page

The button with a page icon next to the theme menu (docs, sheets and notes) shows the page, the sheet or the note's panes in light colours whatever the theme is. It is stored under `q:paper` and sets `html[data-paper="light"]`; the style sheet applies the light variables to the editing surfaces only, so the rest of the screen keeps its theme.

## Language

The picker at the top right offers English, German, Spanish, French and Polish (`en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL`), stored under `q:language`. Each app has its own catalogues in `crates/<app>-app/i18n/<locale>.ftl` (Fluent files). Labels are written in English in the code and found by an id made from the English text (`office_i18n::id_for`); a label with no entry in a language is shown in English. The translations other than English have not been reviewed by native speakers.
