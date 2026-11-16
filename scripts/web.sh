#!/usr/bin/env bash
# Builds the browser module of each page for wasm and puts it in web/ (what the server reads: `WASM_DIR`, default `web`).
# The image does the same in its build; foreman runs this before a start.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p web
for app in launcher docs sheets notes pdf; do
  package="lore-${app}-app"
  cargo build -q --target wasm32-unknown-unknown --profile wasm -p "$package"
  cp "target/wasm32-unknown-unknown/wasm/${package//-/_}.wasm" "web/${app}.wasm"
  echo "$package -> web/${app}.wasm"
done
