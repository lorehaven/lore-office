# lore-office

An office suite for the browser, built on quench: **docs** (word processor, `.docx`), **sheets** (spreadsheets, `.xlsx`/`.ods`/`.xls`), **notes** (Markdown vault with slide decks) and **pdf** (reader and page tools), behind a **launcher**. English, German, Spanish, French and Polish. Everything works offline in the browser; the cloud is optional.

## The cloud (optional)

Behind gatehouse, the suite keeps each person's documents on the server and shares them by link:

- **Sign in** with gatehouse (authorization code + PKCE, through `quench-auth`).
- **Per-user storage**: each person gets their own warehouse storage (dynamic, made on the first save, **10 GiB** by default: `USER_QUOTA_BYTES`). Without warehouse (`CLOUD_STORAGE=local`) a directory per person under `DATA_DIR` does the same.
- **Cloud sync**: File → *Cloud…* lists, opens, deletes; *Save to the cloud* saves over the open document. When signed in, Ctrl+S saves to the cloud; docs and sheets send their changes by themselves, and every page asks every 5 s whether someone else saved, so an update arrives without reloading (with changes of your own, a banner lets you choose). A save says which version it began from (`If-Match` etag), so two people saving one document get a conflict message instead of a silent overwrite.
- **Folders and people**: the Cloud panel (70% of the window) browses folders as cards or a list, moves documents between them, and *Share…* can also share a document with a named person: it stays in your cloud and shows under *Shared with me* in theirs.
- **Share links** (docs, sheets, notes, pdf): *Share…* makes a 256-bit link with **view** or **edit** rights, optionally ending after N days. Anyone with the link, signed in or not, opens the document at `/<app>?share=<token>`; edit links save back to the owner's copy. The owner lists and withdraws links.

Without `SERVICE_AUTH_ENABLED=true` everyone is one local person (`X-Lore-User` picks another, for trying). The apps in `apps/lore-*/server` still run alone with no cloud.

## Layout

| | |
|---|---|
| `crates/*` | libraries: document models and file formats (`docs-*`, `sheets-*`, `pdf-*`), the app screens (`*-app`), `office-ui`/`office-theme`/`office-i18n`, `office-cloud` (storage, quotas, shares), `office-cloud-ui` (the Cloud menu and panels) |
| `apps/lore-*/app` | the browser module of each page; `apps/lore-*/server` runs one alone |
| `apps/lore-office/server` | **the service**: launcher, apps, assets, sign-in, `/api`, health, as one binary |

The API (`/lore-office/api`, see `apps/lore-office/server/src/lib.rs`): `status`, `usage`, `docs`, `docs/{id}`, `docs/{id}/meta`, `docs/{id}/move`, `docs/{id}/share`, `docs/{id}/grant(s)`, `folders`, `grants`, `shared/{owner}/{id}`, `shares`, `s/{token}`, `s/{token}/meta`.

## Documentation

The book is in `docs/` (guides, apps, the service and its API, libraries), built with `anvil docs build`.

## Run it

```sh
foreman                      # builds web/*.wasm and the server, serves http://localhost:3000/lore-office/
scripts/check.sh             # fmt, clippy, tests, cargo deny, cargo machete
conveyor run lorehaven/lore-office --stage release-image --wait   # the image, by hand
```

Settings (environment): `HOST`/`PORT`, `BASE_PATH`, `WASM_DIR`, `CLOUD_STORAGE` (`local`|`warehouse`), `DATA_DIR`, `USER_QUOTA_BYTES`, `MAX_UPLOAD_BYTES`, `SERVICE_AUTH_ENABLED`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`/`GATEHOUSE_CLIENT_SECRET`, `WAREHOUSE_URL`, `WAREHOUSE_CLIENT_ID`/`WAREHOUSE_CLIENT_SECRET`.

## Deploy

`homecloud/overlays/lore-office` (namespace `lore-office`, image `forge/lore-office/lore-office`), with the two gatehouse clients (`lore-office` login, `lore-office-warehouse` machine identity) and the `lore-office` service in `overlays/forge/gatehouse`. The pages' scripts are served from the absolute path `/_quench/`, so the ingress routes that to the service too.

## Not verified here

The gatehouse login and the live warehouse storage were written against the forge sources and tested against local fakes only; the first run against the real estate may need small fixes (a warehouse API detail, a client scope). The image build (`Dockerfile`) was not run.
