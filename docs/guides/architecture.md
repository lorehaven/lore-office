---
title: Architecture
parent: Guides
nav_order: 1
---

Each app is a library of screens (`*-app`) over a document model and file format crates. One service renders and serves the pages; the browser takes each page over and keeps working without the server.

## Workspace

| Group | Crates | What they hold |
|---|---|---|
| docs | `docs-text`, `docs-docx`, `docs-md`, `docs-print`, `docs-app` | the paragraph model, `.docx` read and write, Markdown, the page layout and PDF output, the screen |
| sheets | `sheets-model`, `sheets-formula`, `sheets-calc`, `sheets-format`, `sheets-xlsx`, `sheets-ods`, `sheets-xls`, `sheets-csv`, `sheets-chart`, `sheets-print`, `sheets-app` | cells and workbooks, formulas, recalculation, number formats, the file formats, charts, printing, the screen |
| notes | `notes-md`, `notes-pdf`, `notes-app` | Markdown notes and decks, deck PDF export, the screen |
| pdf | `pdf-write`, `pdf-edit`, `pdf-app` | writing PDF, editing pages, the reader |
| shared | `office-fmt`, `office-i18n`, `office-theme`, `office-ui`, `office-cloud`, `office-cloud-ui` | package and XML helpers, translations, theme, ribbon and menus, the cloud's storage and its screens |
| apps | `apps/lore-launcher`, `apps/lore-docs`, `apps/lore-sheets`, `apps/lore-notes`, `apps/lore-pdf` | the browser module (`app/`) and a standalone server (`server/`) of each page |
| service | `apps/lore-office/server` | `lore-office-server` |

## A page

`quench_fullstack` renders the page on the server into the first response and the browser module takes it over. The page's state travels with it: `quench_ui_web::initial_state()` returns the JSON that `Page::state` carried. For the cloud that JSON is the `Context` of `office-cloud-ui`: the API address, who is signed in, where signing in starts, and the share link the page was opened by.

The service keeps the five modules in memory and serves each page at `/<app>`. The script and style assets are hashed and are served from the absolute path `/_quench/`, whichever page asked, so the service answers that path itself and passes the request to the app whose hash it is. A request for an asset is therefore also an ingress concern: see [Deployment](deployment.md).

## Base path

`BASE_PATH` (for example `/lore-office`) is stripped from the request path before routing. The service rebuilds each request with the path rewritten (`Request::new` with the new URI and the same body and container), because the apps are mounted at `/`. The page context is handed to the app in the header `x-lore-context`, which the service sets itself for each request.

## The cloud in the browser

The pages are built for `wasm32-unknown-unknown` and must not pull the server side of `office-cloud` (storage, `tokio`, the registry client). `office-cloud` has a Cargo feature `server` (on by default) for that; `office-cloud-ui` and the apps use it with `default-features = false`, the service enables it.

The cloud screens go through a `Transport` (the browser's `fetch` in the page, a fake in tests). A call is made with the cookies of the page; the API is described in [the API](../services/lore-office/api.md).

## Standalone apps

Each `apps/lore-*/server` serves one app alone, without the cloud (the page context is empty, so there is no cloud button and no account menu). They are built with `quench build`; with the Cargo feature `embed` the module `dist/app.wasm` is compiled into the server. `lore-office-server` has the same feature and embeds the five modules of the standalone apps; without it, it reads `WASM_DIR` at start.

## Look

`office-theme` holds the `--office-*` variables, the palettes and the script that keeps the choice in the browser; `office-ui` holds the ribbon, menus, icons and the base style. Both follow the design tokens of the forge estate (Untitled UI grays, the same brand scales, radii and shadows), copied in, so the suite does not depend on forge crates. See [Theme and language](theme-and-language.md).
