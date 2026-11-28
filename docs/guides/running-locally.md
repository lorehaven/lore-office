---
title: Running locally
parent: Guides
nav_order: 2
---

`foreman` runs the combined service as part of a forge of its own in Docker; the standalone apps run with `quench dev`.

## The combined service

```bash
foreman                   # containers, schemas, gatehouse and warehouse, then builds and starts lore-office
foreman status
foreman logs lore-office
foreman stop              # lore-office only
foreman run forge-down    # gatehouse and warehouse
foreman stop all          # lore-office, Postgres, Redis and the proxy
```

Open `http://localhost:8080/lore-office/` and sign in with the account `foreman` prints (`admin_user` / `admin_password` in `foreman.toml`).

| Container | Image | On this machine |
|---|---|---|
| `lore-office-postgres` | `pgvector/pgvector:pg18` | `127.0.0.1:55432`, volume `lore-office-postgres` |
| `lore-office-redis` | `redis:7-alpine` | `127.0.0.1:56379` |
| `lore-office-proxy` | `nginx:1.27-alpine` | `127.0.0.1:8080` |
| `lore-office-gatehouse` | the released gatehouse | `127.0.0.1:5443` |
| `lore-office-warehouse` | the released warehouse (files only) | `127.0.0.1:6443`, volume `lore-office-warehouse` |

The proxy (`local/nginx.conf`) puts everything on one address with the cluster's paths: `/gatehouse`, `/warehouse`, `/lore-office` and the pages' assets under `/_quench/`. It is plain HTTP on this machine only; browsers accept the realm's `Secure` cookies from `localhost`. The containers use the host's network and listen on the loopback address, so this needs Docker on Linux.

Two tasks run before lore-office starts, in the order of their names:

* `forge` (`local/forge.sh up`): foundry installs the `gatehouse` and `warehouse` schemas, then gatehouse and warehouse start (made again each time). Gatehouse reads its clients and permission catalog from `local/gatehouse/`: `lore-office`, `lore-office-warehouse` and `warehouse`, as the cluster's overlay has them.
* `web` (`bash scripts/web.sh`): for each of `launcher`, `docs`, `sheets`, `notes` and `pdf` it builds `lore-<app>-app` for `wasm32-unknown-unknown` with the `wasm` profile and copies the module to `web/<app>.wasm`.

Passwords and secrets in `foreman.toml` are for this machine only. The images are the released ones named there; to use others, put `GATEHOUSE_IMAGE=`, `WAREHOUSE_IMAGE=` or `FOUNDRY_IMAGE=` in `local/.env` (not committed). Pulling them needs access to the registry. To start over, `foreman stop all`, `foreman run forge-down` and `docker volume rm lore-office-postgres lore-office-warehouse`.

## Without forge

```bash
bash scripts/web.sh
BASE_PATH=/lore-office WASM_DIR=web CLOUD_STORAGE=local DATA_DIR=.run/data HOST=127.0.0.1 PORT=3000 cargo run -p lore-office-server
```

Without `GATEHOUSE_URL` the pages offer no cloud and no account menu (see [Theme and language](theme-and-language.md#the-top-bar)). To try the cloud on the local store without signing in, set `GATEHOUSE_URL` to any address: without `SERVICE_AUTH_ENABLED=true` everyone is one local person, and the header `X-Lore-User: <name>` tries several.

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
