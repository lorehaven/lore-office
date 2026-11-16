---
title: Running locally
parent: Guides
nav_order: 2
---

`foreman` builds and starts the combined service; the standalone apps run with `quench dev`.

## The combined service

```bash
foreman                   # builds web/*.wasm and the server, serves http://localhost:3000/lore-office/
foreman status
foreman logs lore-office
foreman stop
```

`foreman.toml` runs a `web` task first (`bash scripts/web.sh`: for each of `launcher`, `docs`, `sheets`, `notes` and `pdf` it builds `lore-<app>-app` for `wasm32-unknown-unknown` with the `wasm` profile and copies the module to `web/<app>.wasm`), then starts `lore-office-server` with:

| Variable | Value |
|---|---|
| `HOST`, `PORT` | `127.0.0.1`, `3000` |
| `BASE_PATH` | `/lore-office` |
| `WASM_DIR` | `${project_root}/web` |
| `CLOUD_STORAGE` | `local` |
| `DATA_DIR` | `${project_root}/.run/data` |

`${project_root}` is the directory of `foreman.toml`; `foreman` expands it in a service's environment (not in `[vars]`).

Without `SERVICE_AUTH_ENABLED=true` there is no sign-in: everyone is one local person and the documents are kept under `.run/data`. To try several people, send the header `X-Lore-User: <name>`.

## Signing in locally

Start the forge estate (`foreman` in the forge repository), then set `SERVICE_AUTH_ENABLED=true`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID` and `GATEHOUSE_CLIENT_SECRET` in `foreman.toml`. The client is `lore-office` in the gatehouse overlay of `homecloud`.

## One app alone

```bash
cd apps/lore-docs
quench dev                 # development, reloads on change
quench build --release     # dist/lore-docs-server (module embedded) and dist/app.wasm
PORT=3000 dist/lore-docs-server
```

The same for `lore-sheets`, `lore-notes`, `lore-pdf` and `lore-launcher`. These servers have no cloud. The launcher takes the addresses of the apps from `LORE_DOCS_URL`, `LORE_SHEETS_URL`, `LORE_NOTES_URL` and `LORE_PDF_URL`.

## Checks before a commit

```bash
scripts/check.sh
```

See [Testing](testing.md).
