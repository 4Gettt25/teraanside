#!/usr/bin/env python3
"""Fails if an advisory ignore has no reason or an expired / missing expiry date.

Sources: deny.toml ([advisories].ignore, expiry as "expires: YYYY-MM-DD" in the reason)
and supply-chain/pnpm-audit-ignores.toml (expires = YYYY-MM-DD).
"""
import datetime
import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
today = datetime.date.today()
problems = []
count = 0

deny = tomllib.loads((ROOT / "deny.toml").read_text())
for entry in deny.get("advisories", {}).get("ignore", []):
    count += 1
    if isinstance(entry, str):
        problems.append(f"deny.toml: {entry} has no reason")
        continue
    m = re.search(r"expires:\s*(\d{4}-\d{2}-\d{2})", entry.get("reason", ""))
    if not m:
        problems.append(f"deny.toml: {entry.get('id')} has no 'expires: YYYY-MM-DD' in its reason")
    elif datetime.date.fromisoformat(m.group(1)) < today:
        problems.append(f"deny.toml: ignore of {entry.get('id')} expired on {m.group(1)}")

pnpm = tomllib.loads((ROOT / "supply-chain" / "pnpm-audit-ignores.toml").read_text())
for entry in pnpm.get("ignore", []):
    count += 1
    if not entry.get("reason", "").strip():
        problems.append(f"pnpm-audit-ignores.toml: {entry.get('id')} has no reason")
    expires = entry.get("expires")
    if not isinstance(expires, datetime.date):
        problems.append(f"pnpm-audit-ignores.toml: {entry.get('id')} has no expiry date")
    elif expires < today:
        problems.append(f"pnpm-audit-ignores.toml: ignore of {entry.get('id')} expired on {expires}")

if problems:
    print("\n".join(problems), file=sys.stderr)
    sys.exit(1)
print(f"{count} advisory ignores, all with reason and valid expiry")
