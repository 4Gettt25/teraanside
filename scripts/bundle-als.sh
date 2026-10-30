#!/usr/bin/env bash
# Bundles @ansible/ansible-language-server into one file shipped as app resource
# (DESIGN.md §14.1). Built from the package's unbundled ES modules (lib/) so that
# its dependencies come from OUR lockfile: pnpm overrides, `pnpm audit` and the license
# allowlist then cover exactly the code we ship (dist/cli.cjs has deps pre-inlined).
# Output is committed; `--check` (CI) verifies it is reproducible.
set -euo pipefail
cd "$(dirname "$0")/.."

out_dir=src-tauri/resources/als
pkg=$(cd node_modules/@ansible/ansible-language-server && pwd -P)
target_dir=$out_dir
if [[ "${1:-}" == "--check" ]]; then
  target_dir=$(mktemp -d)
  trap 'rm -rf "$target_dir"' EXIT
fi
mkdir -p "$target_dir"
meta=$(mktemp)
pnpm exec esbuild "$pkg/lib/cli.js" \
  --bundle --platform=node --target=node24 --format=cjs --minify \
  --alias:@src="$pkg/lib" \
  --legal-comments=external --log-level=warning \
  --metafile="$meta" \
  --outfile="$target_dir/ansible-language-server.cjs"

# exact package list of the bundle (from the metafile) with license check
python3 -I scripts/als-licenses.py "$meta" "$target_dir/ansible-language-server.cjs.packages.txt"
rm -f "$meta"

if [[ "${1:-}" == "--check" ]]; then
  for f in ansible-language-server.cjs ansible-language-server.cjs.LEGAL.txt ansible-language-server.cjs.packages.txt; do
    if ! diff -q "$target_dir/$f" "$out_dir/$f" >/dev/null; then
      echo "src-tauri/resources/als/$f is out of date: run scripts/bundle-als.sh and commit the result" >&2
      exit 1
    fi
  done
  echo "ALS bundle is up to date"
fi
