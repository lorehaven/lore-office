---
title: Libraries
nav_order: 5
has_children: true
permalink: /libs/
---

The crates the four apps and the service share. Each page lists the public API from the source; all are version 0.1.0 of the workspace and the Rust 2024 edition.

| Crate | Holds |
|---|---|
| [`office-cloud`](office-cloud/index.md) | what a cloud document and a share link are, the storage behind them with quotas, the rules |
| [`office-cloud-ui`](office-cloud-ui/index.md) | the Cloud menu, panels and banner, over an injectable transport |
| [`office-ui`](office-ui/index.md) | the ribbon, tools, menus, icons and the base style |
| [`office-theme`](office-theme/index.md) | the theme variables, palettes, the light-page button, the language picker |
| [`office-i18n`](office-i18n/index.md) | translation catalogues and ids |

The document model and file format crates (`docs-*`, `sheets-*`, `notes-*`, `pdf-*`, `office-fmt`) are described in [Architecture](../guides/architecture.md); their public items are documented in their sources.
