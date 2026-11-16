#!/usr/bin/env bash
# The standalone servers embed their page's module (`include_bytes!("../../dist/app.wasm")`, feature `embed`), and `--all-features`
# lints and tests turn that on. The real modules are built by `quench build`; for checking, an empty file is enough (nothing
# runs it). Existing files are left alone.
set -euo pipefail
cd "$(dirname "$0")/.."
for app in docs sheets notes pdf launcher; do
  mkdir -p "apps/lore-$app/dist"
  [ -e "apps/lore-$app/dist/app.wasm" ] || : > "apps/lore-$app/dist/app.wasm"
done
