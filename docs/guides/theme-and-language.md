---
title: Theme and language
parent: Guides
nav_order: 6
---

The look and the language are the person's, kept in the browser, and the same in the launcher and all four apps.

## The top bar

Every page has the bar the forge services have: the services dashboard (in forge only) and the home button, the app's picture and name, then the cloud button (in forge only), the language, the light-page button (docs, sheets and notes), the theme menu and the account menu (in forge, signed in: *Edit profile* at gatehouse, *Sign out*). The ribbon is under it.

*Edit profile* goes through gatehouse's sign-in page (`/ui/login?redirect=…/ui/account`), which renews the session without a form and goes on to the account page: the session of a sign-in to lore-office is only good for lore-office, and the account page itself would send the person to the dashboard. For the same reason the account picture does not show until gatehouse accepts that session for it.

A page is **in forge** when it has a cloud and knows where gatehouse is (`GATEHOUSE_URL`). Without that nothing about accounts or the cloud is offered: no dashboard, no account menu, no Cloud button or File menu items, and Ctrl+S saves a file.

## Theme

The theme menu (the palette at the top right) offers Light, Dark or System, and six accent palettes: blue, green, teal, purple, rose and orange. With no choice, the system's scheme decides and the accent is green, as in forge. The choice is stored under the keys `q:theme` and `q:palette` in the browser's storage; `office-theme`'s script applies it to `html[data-theme]` and `html[data-palette]` before the page is drawn.

The forge services keep the same choices in cookies on the address they share (`qtheme`, `fui_palette`, `qlocale` for the language). What those say wins when a page opens or is looked at again, and a choice made here is written to them, so one choice holds for all of forge. *System* clears the theme cookie (the other services then show their own default, dark).

## The light page

The button with a page icon next to the theme menu (docs, sheets and notes) shows the page, the sheet or the note's panes in light colours whatever the theme is. It is stored under `q:paper` and sets `html[data-paper="light"]`; the style sheet applies the light variables to the editing surfaces only, so the rest of the screen keeps its theme.

## Language

The picker at the top right offers English, German, Spanish, French and Polish (`en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL`), stored under `q:language`. Each app has its own catalogues in `crates/<app>-app/i18n/<locale>.ftl` (Fluent files). Labels are written in English in the code and found by an id made from the English text (`office_i18n::id_for`); a label with no entry in a language is shown in English. The translations other than English have not been reviewed by native speakers.
