#!/usr/bin/env bash
# Bundles @ansible/ansible-language-server into one file shipped as app resource
# (DESIGN.md §14.1: Node is managed by the app, ALS is bundled at build time).
# Output is committed; `--check` (CI) verifies it is reproducible from the lockfile.
set -euo pipefail
cd "$(dirname "$0")/.."

out_dir=src-tauri/resources/als
entry=node_modules/@ansible/ansible-language-server/dist/cli.cjs
target_dir=$out_dir
if [[ "${1:-}" == "--check" ]]; then
  target_dir=$(mktemp -d)
  trap 'rm -rf "$target_dir"' EXIT
fi
mkdir -p "$target_dir"
pnpm exec esbuild "$entry" \
  --bundle --platform=node --target=node24 --format=cjs --minify \
  --legal-comments=external --log-level=warning \
  --outfile="$target_dir/ansible-language-server.cjs"

# package list with licenses (fails on licenses outside supply-chain/npm-licenses.toml)
python3 -I scripts/als-licenses.py "$target_dir/ansible-language-server.cjs.packages.txt"

if [[ "${1:-}" == "--check" ]]; then
  if ! diff -q "$target_dir/ansible-language-server.cjs.packages.txt" "$out_dir/ansible-language-server.cjs.packages.txt" >/dev/null \
    || ! diff -q "$target_dir/ansible-language-server.cjs" "$out_dir/ansible-language-server.cjs" >/dev/null \
    || ! diff -q "$target_dir/ansible-language-server.cjs.LEGAL.txt" "$out_dir/ansible-language-server.cjs.LEGAL.txt" >/dev/null; then
    echo "src-tauri/resources/als is out of date: run scripts/bundle-als.sh and commit the result" >&2
    exit 1
  fi
  echo "ALS bundle is up to date"
fi
