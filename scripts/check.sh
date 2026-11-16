#!/usr/bin/env bash
# The gates every change passes: formatting, lints, tests, dependency policy, unused dependencies.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
# The pages' browser code is only built for wasm, so it is only linted there.
cargo clippy --target wasm32-unknown-unknown -p lore-launcher-app -p lore-docs-app -p lore-sheets-app -p lore-notes-app -p lore-pdf-app -- -D warnings
cargo test --workspace
cargo deny check
cargo machete
# The book: front matter, navigation and links.
anvil docs check
echo "all gates passed"
