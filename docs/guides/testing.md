---
title: Testing
parent: Guides
nav_order: 8
---

`scripts/check.sh` is the one command that every change passes; the pipeline runs the same gates.

## The gates

| Step | Command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| lints of the browser code | `cargo clippy --target wasm32-unknown-unknown -p lore-launcher-app -p lore-docs-app -p lore-sheets-app -p lore-notes-app -p lore-pdf-app -- -D warnings` |
| tests | `cargo test --workspace` |
| dependency policy | `cargo deny check` (`deny.toml`) |
| unused dependencies | `cargo machete` |

`--all-features` matters: the standalone servers embed `dist/app.wasm` with the feature `embed`, so the pipeline creates empty placeholder files first (`scripts/ci-placeholders.sh`), and the lints of the feature build see the same code as the release.

## The tests

Tests sit in each crate's `tests/` directory and use the `quench_test` harness. The screens are tested without a browser: an app is mounted on a `TestRenderer`, events are fired at its nodes and the nodes are read back. `office-cloud-ui` tests the Cloud menu and panels with a fake transport; `apps/lore-office/server/tests/cloud.rs` calls the service as an `Endpoint` (isolation between people, the quota, conflicts, share rights, the base path and the return after signing in).

## Real files

`corpus/` holds files made by other programs: `libreoffice_basic.xlsx`, `.ods` and `.xls`, `openpyxl_basic.xlsx`, and `libreoffice_rich.docx`. The format tests embed them with `include_bytes!`, read them and compare what comes out, so the readers are checked against files they did not write. `make_corpus.py` records how they were made.

## In the browser

Each feature is also looked at in Chrome against the running service. These checks are by hand; they are not part of the gates.

## Pipeline

`.conveyor.toml` runs `format`, `lint` and `deps` (stage `check`), the server build (stage `build`), `anvil test --all` (stage `test`) and, by hand only, `release-image`. See [Deployment](deployment.md).
