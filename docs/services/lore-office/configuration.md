---
title: Configuration
parent: lore-office
grand_parent: Services
nav_order: 3
---

The service reads its settings from the environment. Secrets are named here, never given.

## The service

| Variable | Default | Meaning |
|---|---|---|
| `HOST` | `0.0.0.0` | address to listen on |
| `PORT` | `3000` | port to listen on (the image sets `8080`) |
| `BASE_PATH` | empty | where the service is served (`/lore-office`); `/` means none |
| `WASM_DIR` | `web` | folder with `launcher.wasm`, `docs.wasm`, `sheets.wasm`, `notes.wasm`, `pdf.wasm`; unused with the feature `embed` |
| `MAX_UPLOAD_BYTES` | `268435456` (256 MiB) | the largest body a save may send; larger answers `413` |
| `QUENCH_DEV` | unset | when set, every page reloads when the service restarts |
| `RUST_LOG` | `info` | log level |

## Storage

| Variable | Default | Meaning |
|---|---|---|
| `CLOUD_STORAGE` | `local` | `warehouse`, or anything else for a directory per person |
| `DATA_DIR` | `data` | the directory for `local` (the image sets `/data`) |
| `USER_QUOTA_BYTES` | `10737418240` (10 GiB) | what one person may keep; an unreadable value falls back to the default |
| `WAREHOUSE_URL` | none | warehouse as the service reaches it, with its base path (`http://warehouse.forge.svc.cluster.local/warehouse`); required for `warehouse` |
| `WAREHOUSE_CLIENT_ID` | empty | the machine client (`lore-office-warehouse`) |
| `WAREHOUSE_CLIENT_SECRET` | empty | its secret |
| `SHARES_STORAGE` | `lore-office-shares` | the warehouse storage that holds the share links (made safe as a name) |

With `CLOUD_STORAGE=warehouse` the service stops at start when `WAREHOUSE_URL` or `GATEHOUSE_URL` is missing.

## Sign-in

| Variable | Default | Meaning |
|---|---|---|
| `SERVICE_AUTH_ENABLED` | unset | `true` turns gatehouse sign-in on; anything else is the local person |
| `SERVICE_NAME` | | the audience a token must carry (`lore-office`) |
| `SERVICE_REALM` | | set by the overlay (`https://ennor.ddns.net/token`); not read by the service's own code |
| `GATEHOUSE_URL` | | gatehouse, for sign-in, keys and the warehouse token |
| `GATEHOUSE_CLIENT_ID` | | the login client (`lore-office`) |
| `GATEHOUSE_CLIENT_SECRET` | | its secret |
| `GATEHOUSE_TLS_VERIFY` | `true` | read by `quench-auth` |
| `AUTH_COOKIE_NAME`, `AUTH_REFRESH_COOKIE_NAME` | `forge_session`, `forge_refresh` | cookie names |
| `AUTH_COOKIE_DOMAIN` | unset (host only) | parent domain of the cookies |
| `REFRESH_TOKEN_TTL_SECS` | `604800` | lifetime of the refresh cookie |
| `AUTH_REDIRECT_HOSTS` | empty | other prefixes `quench-auth` accepts as `?redirect=` targets |

The last six are read by `quench-auth`; see its documentation.

## Build features

| Feature | Meaning |
|---|---|
| `embed` | compile the five modules `apps/lore-*/dist/app.wasm` into the binary instead of reading `WASM_DIR` |
