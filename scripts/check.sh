#!/usr/bin/env bash
# Quality gate (CLAUDE.md): must be green before every commit and in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

step "cargo fmt --check"
(cd src-tauri && cargo fmt --check)

step "cargo clippy --all-targets -- -D warnings"
(cd src-tauri && cargo clippy --locked --all-targets -- -D warnings)

step "cargo test (also regenerates TypeScript bindings)"
rm -rf src/lib/generated
(cd src-tauri && cargo test --locked)

step "generated bindings are up to date"
if [[ -n "$(git status --porcelain -- src/lib/generated)" ]]; then
  git status --short -- src/lib/generated
  echo "src/lib/generated differs from the Rust types: run 'pnpm gen:types' and commit the result." >&2
  exit 1
fi

step "pnpm lint"
pnpm lint

step "pnpm format:check"
pnpm format:check

step "pnpm typecheck"
pnpm typecheck

step "pnpm test"
pnpm test

printf '\n\033[1;32mQuality gate passed.\033[0m\n'
