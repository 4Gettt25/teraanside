#!/usr/bin/env bash
# Quality gate (CLAUDE.md): must be green before every commit and in CI.
# CI runs the two parts as parallel jobs; supply-chain checks: scripts/supply-chain.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

scripts/check-rust.sh
scripts/check-frontend.sh

printf '\n\033[1;32mQuality gate passed.\033[0m\n'
