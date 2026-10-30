#!/usr/bin/env bash
# Builds the frontend and checks the bundle for things that must not ship.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! build_log=$(pnpm --silent build 2>&1); then
  echo "$build_log" >&2
  exit 1
fi

# jschardet (LGPL-2.1+) is replaced by stubs/jschardet (DESIGN.md §14 #15). Markers are
# property names from the real jschardet sources that survive minification.
markers=(mTypicalPositiveRatio precedenceMatrix keepEnglishLetter)
for m in "${markers[@]}"; do
  if grep -rlq -- "$m" dist; then
    echo "bundle contains jschardet code (marker '$m'):" >&2
    grep -rl -- "$m" dist >&2
    exit 1
  fi
done
# the stub chunks themselves must stay tiny
while IFS= read -r -d '' f; do
  size=$(wc -c <"$f")
  if (( size > 1024 )); then
    echo "unexpectedly large jschardet chunk $f ($size bytes)" >&2
    exit 1
  fi
done < <(find dist -name 'jschardet-*.js' -print0)
echo "bundle check passed (no jschardet code)"
