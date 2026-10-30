#!/usr/bin/env bash
# Supply-chain checks (CI job "supply-chain" on PRs/pushes and weekly; needs network):
# advisory ignores not expired, cargo-deny (advisories, licenses, sources, bans),
# pnpm audit, npm license allowlist. Expects cargo-deny on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

step "advisory ignores have reasons and valid expiry dates"
python3 -I scripts/check-audit-ignores.py

step "cargo deny (advisories, licenses, sources, bans)"
cargo-deny --manifest-path src-tauri/Cargo.toml --config deny.toml --locked check

step "pnpm audit"
ignores=()
while IFS= read -r id; do ignores+=(--ignore "$id"); done < <(
  python3 -I -c 'import tomllib,sys; [print(e["id"]) for e in tomllib.load(open(sys.argv[1],"rb")).get("ignore",[])]' \
    supply-chain/pnpm-audit-ignores.toml
)
pnpm audit --audit-level low "${ignores[@]}"

step "npm license allowlist"
python3 -I scripts/check-npm-licenses.py

printf '\n\033[1;32mSupply-chain checks passed.\033[0m\n'
