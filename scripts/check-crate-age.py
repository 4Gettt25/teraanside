#!/usr/bin/env python3
"""Versionsregel for Rust crates (CLAUDE.md): crate versions that are new or changed in
src-tauri/Cargo.lock compared to a base git ref must be published >= 14 days ago on crates.io.

Cargo has no built-in minimum-release-age setting, so CI runs this script.
Usage: scripts/check-crate-age.py <base-ref>
Exceptions (security fixes only): supply-chain/crate-age-exceptions.txt, one
`name@version  # advisory/CVE link` per line.
"""
import datetime
import json
import pathlib
import subprocess
import sys
import time
import tomllib
import urllib.request

MIN_AGE = datetime.timedelta(days=14)
LOCK = "src-tauri/Cargo.lock"
ROOT = pathlib.Path(__file__).resolve().parent.parent
EXCEPTIONS = ROOT / "supply-chain" / "crate-age-exceptions.txt"
USER_AGENT = "iac-studio-ci (crate age check)"


def registry_packages(lock_text: str) -> set[tuple[str, str]]:
    data = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"])
        for p in data.get("package", [])
        if p.get("source", "").startswith("registry+")
    }


def published_at(name: str, version: str) -> datetime.datetime:
    req = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}", headers={"User-Agent": USER_AGENT}
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        created = json.load(resp)["version"]["created_at"]
    return datetime.datetime.fromisoformat(created.replace("Z", "+00:00"))


def load_exceptions() -> set[str]:
    if not EXCEPTIONS.exists():
        return set()
    lines = (l.split("#", 1)[0].strip() for l in EXCEPTIONS.read_text().splitlines())
    return {l for l in lines if l}


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    base = sys.argv[1]
    try:
        old_text = subprocess.run(
            ["git", "show", f"{base}:{LOCK}"], cwd=ROOT, check=True, capture_output=True, text=True
        ).stdout
    except subprocess.CalledProcessError:
        old_text = ""  # lockfile did not exist at base
    new = registry_packages((ROOT / LOCK).read_text())
    added = sorted(new - registry_packages(old_text))
    exceptions = load_exceptions()
    now = datetime.datetime.now(datetime.timezone.utc)
    violations = []
    for name, version in added:
        if f"{name}@{version}" in exceptions:
            print(f"exception: {name}@{version}")
            continue
        age = now - published_at(name, version)
        print(f"{name}@{version}: {age.days} days")
        if age < MIN_AGE:
            violations.append(f"{name}@{version} ({age.days} days old)")
        time.sleep(1)  # crates.io crawler policy: max 1 request/s
    if violations:
        print("Crate versions younger than 14 days:\n  " + "\n  ".join(violations), file=sys.stderr)
        return 1
    print(f"{len(added)} new/changed crate versions checked, all >= 14 days old.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
