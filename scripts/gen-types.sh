#!/usr/bin/env bash
# Regenerates the TypeScript bindings from Rust types (ts-rs) into src/lib/generated/.
# The directory is wiped first so that removed/renamed Rust types do not leave stale files.
set -euo pipefail
cd "$(dirname "$0")/.."

rm -rf src/lib/generated
cargo test --quiet --manifest-path src-tauri/Cargo.toml export_bindings >/dev/null
