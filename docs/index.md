---
title: Lore Office
nav_order: 1
permalink: /
---

Lore Office is an office suite for the browser, built on the Quench framework: a word processor (docs), spreadsheets (sheets), Markdown notes with slide decks (notes) and a PDF reader with page tools (pdf), behind a launcher. Everything works in the browser without an account; an optional cloud keeps each person's documents on the server and shares them by link. The pages are in English, German, Spanish, French and Polish.

* [Guides](guides/index.md): architecture, running it locally, authentication, sharing, deployment, testing.
* [Apps](apps/index.md): what each app opens, saves and does.
* [Services](services/index.md): the combined service `lore-office-server`, its HTTP API, configuration, data and operations.
* [Libraries](libs/index.md): the shared crates (`office-cloud`, `office-cloud-ui`, `office-ui`, `office-theme`, `office-i18n`).

## At a glance

| | |
|---|---|
| Repository | `lorehaven/lore-office` (one Cargo workspace) |
| Service | `lore-office-server`, one binary for the launcher, the four apps, the assets, sign-in and the API |
| Address in the estate | `https://ennor.ddns.net/lore-office/` |
| Sign-in | gatehouse (authorization code with PKCE, through `quench-auth`) |
| Cloud storage | a warehouse storage per person, 10 GiB by default; or a directory per person for local use |
| Image | `forge/lore-office/lore-office` |
| Deployment | the `lore-office` overlay of `homecloud` |
