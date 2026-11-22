"""Enforce the reviewed bounds of Cargo Deny exceptions before running it."""

from __future__ import annotations

import argparse
import sys
import tomllib
from datetime import date, datetime, timezone
from pathlib import Path

# This is an explicit approval record, not an automatic waiver mechanism.
# Remove the record and deny.toml entry together when upstream removes chrono.
# A version change or extension of the UTC deadline requires another review.
APPROVED_EXCEPTIONS = {
    "RUSTSEC-2026-0543": ("chrono", "0.4.45", date(2026, 12, 22)),
}
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"


def check(policy: dict, locked: dict, today: date) -> None:
    advisories = policy.get("advisories", {})
    if not isinstance(advisories, dict) or advisories.get("unmaintained", "all") != "all":
        raise ValueError("all-scope unmaintained checking must remain enabled")
    entries = advisories.get("ignore", [])
    if not isinstance(entries, list):
        raise ValueError("advisory ignores must be a list of justified records")
    ignores = {}
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != {"id", "reason"}:
            raise ValueError("each advisory ignore must contain only id and reason")
        identifier, reason = entry["id"], entry["reason"]
        if not isinstance(identifier, str) or identifier not in APPROVED_EXCEPTIONS:
            raise ValueError("unapproved advisory ignore")
        if identifier in ignores:
            raise ValueError(f"duplicate advisory ignore: {identifier}")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"missing advisory justification: {identifier}")
        ignores[identifier] = reason
    if ignores.keys() != APPROVED_EXCEPTIONS.keys():
        raise ValueError("remove unused approval records together with their advisory ignores")

    packages = locked.get("package", [])
    if not isinstance(packages, list) or not all(isinstance(p, dict) for p in packages):
        raise ValueError("Cargo.lock must contain package records")
    for identifier, (name, version, expires) in APPROVED_EXCEPTIONS.items():
        if today >= expires:
            raise ValueError(f"advisory exception expired at {expires} 00:00 UTC: {identifier}")
        if expires.isoformat() not in ignores[identifier]:
            raise ValueError(f"advisory justification must state expiry {expires}: {identifier}")
        matches = [package for package in packages if package.get("name") == name]
        if len(matches) != 1 or (
            matches[0].get("version") != version
            or matches[0].get("source") != REGISTRY_SOURCE
        ):
            raise ValueError(f"exception requires exactly registry {name} {version}: {identifier}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, default=Path("deny.toml"))
    parser.add_argument("--lock", type=Path, default=Path("Cargo.lock"))
    args = parser.parse_args(argv)
    try:
        policy = tomllib.loads(args.policy.read_text(encoding="utf-8"))
        locked = tomllib.loads(args.lock.read_text(encoding="utf-8"))
        check(policy, locked, datetime.now(timezone.utc).date())
    except (OSError, ValueError) as error:
        print(f"Dependency exception policy failed: {error}", file=sys.stderr)
        return 1
    print("Dependency exception bounds verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
