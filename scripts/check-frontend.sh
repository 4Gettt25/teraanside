#!/usr/bin/env bash
# Frontend part of the quality gate (CI job "frontend").
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

step "pnpm lint"
pnpm lint

step "pnpm format:check"
pnpm format:check

step "pnpm typecheck"
pnpm typecheck

step "pnpm test"
pnpm test

step "bundle check"
scripts/check-bundle.sh

step "ansible-language-server bundle is reproducible"
scripts/bundle-als.sh --check

