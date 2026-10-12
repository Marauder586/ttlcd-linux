#!/usr/bin/env bash
# Build the web UI (the `web/` crate) to WebAssembly and generate its JS
# loader into src/web/pkg/, where the daemon embeds it from.
#
# Needs the wasm32-unknown-unknown Rust target and a wasm-bindgen CLI of the
# same version as the wasm-bindgen crate in Cargo.lock; `nix develop`
# provides both.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --package ttlcd-web --target wasm32-unknown-unknown "$@"
wasm-bindgen --target web --no-typescript --out-dir src/web/pkg \
  target/wasm32-unknown-unknown/release/ttlcd_web.wasm
