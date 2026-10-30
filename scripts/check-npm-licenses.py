#!/usr/bin/env python3
"""Checks licenses of shipped npm packages against supply-chain/npm-licenses.toml."""
import datetime
import json
import pathlib
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
cfg = tomllib.loads((ROOT / "supply-chain" / "npm-licenses.toml").read_text())
allowed = set(cfg.get("allow", []))
exceptions = {e["name"]: e for e in cfg.get("exception", [])}
out = subprocess.run(
    ["pnpm", "licenses", "list", "--prod", "--json"],
    cwd=ROOT, check=True, capture_output=True, text=True,
).stdout
problems = []
total = 0
for license_id, packages in json.loads(out).items():
    for pkg in packages:
        total += 1
        if license_id in allowed:
            continue
        exc = exceptions.get(pkg["name"])
        if exc and exc.get("license") == license_id and exc.get("expires", datetime.date.min) >= datetime.date.today():
            continue
        problems.append(f"{pkg['name']}@{','.join(pkg['versions'])}: {license_id}")
if problems:
    print("npm packages with licenses outside the allowlist:\n  " + "\n  ".join(problems), file=sys.stderr)
    sys.exit(1)
print(f"{total} shipped npm packages, all licenses allowed")
