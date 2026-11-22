"""Model-free tests for the explicit maintenance-risk exception guard."""

from __future__ import annotations

import copy
import importlib.util
import io
import os
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout
from datetime import date, datetime, timezone
from pathlib import Path
from unittest.mock import patch


def load_checks():
    path = Path(os.environ.get(
        "COCO_ADVISORY_CHECKS", Path(__file__).with_name("check_advisory_exceptions.py"),
    ))
    spec = importlib.util.spec_from_file_location("advisory_checks", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class AdvisoryExceptionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.checks = load_checks()
        self.identifier = "RUSTSEC-2026-0543"
        self.entry = {
            "id": self.identifier,
            "reason": "Until 2026-12-22: reviewed maintenance-only acceptance.",
        }
        self.policy = {"advisories": {"unmaintained": "all", "ignore": [self.entry]}}
        self.package = {
            "name": "chrono", "version": "0.4.45", "source": self.checks.REGISTRY_SOURCE,
        }
        self.locked = {"package": [self.package]}
        self.today = date(2026, 11, 22)

    def test_approved_exact_package_is_accepted_until_the_day_before_expiry(self) -> None:
        for today in (self.today, date(2026, 12, 21)):
            self.checks.check(self.policy, self.locked, today)

    def test_expiry_is_exclusive_at_the_utc_date_boundary(self) -> None:
        for today in (date(2026, 12, 22), date(2026, 12, 23), date(2027, 1, 1)):
            with self.subTest(today=today), self.assertRaisesRegex(ValueError, "expired"):
                self.checks.check(self.policy, self.locked, today)

    def test_no_other_advisory_or_package_ignore_is_accepted(self) -> None:
        for identifier in ("RUSTSEC-2026-0592", "chrono", "chrono@0.4.45", "*"):
            self.entry["id"] = identifier
            with self.subTest(identifier=identifier), self.assertRaisesRegex(ValueError, "unapproved"):
                self.checks.check(self.policy, self.locked, self.today)

    def test_bare_ids_and_extra_native_fields_cannot_bypass_the_guard(self) -> None:
        for entry in (self.identifier, {"id": self.identifier}, self.entry | {"expiry": "2099-01-01"}):
            self.policy["advisories"]["ignore"] = [entry]
            with self.subTest(entry=entry), self.assertRaisesRegex(ValueError, "id and reason"):
                self.checks.check(self.policy, self.locked, self.today)

    def test_reasons_must_be_nonempty_and_state_the_actual_deadline(self) -> None:
        for reason in (None, 1, "", " ", "Until 2099-01-01: ignore forever"):
            self.entry["reason"] = reason
            with self.subTest(reason=reason), self.assertRaises(ValueError):
                self.checks.check(self.policy, self.locked, self.today)

    def test_duplicate_or_unused_ignores_require_cleanup(self) -> None:
        for entries in ([self.entry, copy.deepcopy(self.entry)], []):
            self.policy["advisories"]["ignore"] = entries
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                self.checks.check(self.policy, self.locked, self.today)

    def test_unmaintained_scope_cannot_be_weakened(self) -> None:
        for scope in ("none", "workspace", "transitive", None):
            self.policy["advisories"]["unmaintained"] = scope
            with self.subTest(scope=scope), self.assertRaisesRegex(ValueError, "all-scope"):
                self.checks.check(self.policy, self.locked, self.today)

    def test_different_or_additional_versions_need_another_review(self) -> None:
        for packages in ([], [self.package | {"version": "0.4.46"}],
                         [self.package, self.package | {"version": "0.4.44"}]):
            with self.subTest(packages=packages), self.assertRaisesRegex(ValueError, "exactly registry"):
                self.checks.check(self.policy, {"package": packages}, self.today)

    def test_other_sources_are_not_covered(self) -> None:
        for source in (None, "git+https://example.invalid/chrono", "registry+https://example.invalid"):
            self.package["source"] = source
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "exactly registry"):
                self.checks.check(self.policy, self.locked, self.today)

    def test_malformed_policy_and_lock_shapes_fail_closed(self) -> None:
        for policy in ({"advisories": None}, {"advisories": {"ignore": self.entry}}):
            with self.subTest(policy=policy), self.assertRaises(ValueError):
                self.checks.check(policy, self.locked, self.today)
        for locked in ({"package": None}, {"package": [None]}):
            with self.subTest(locked=locked), self.assertRaises(ValueError):
                self.checks.check(self.policy, locked, self.today)

    def test_command_checks_real_files_and_uses_the_utc_clock(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            policy = root / "deny.toml"
            lock = root / "Cargo.lock"
            policy.write_text(
                '[advisories]\nunmaintained = "all"\nignore = [{id = "RUSTSEC-2026-0543", '
                'reason = "Until 2026-12-22: temporary maintenance acceptance"}]\n', encoding="utf-8",
            )
            lock.write_text(
                '[[package]]\nname = "chrono"\nversion = "0.4.45"\n'
                f'source = "{self.checks.REGISTRY_SOURCE}"\n', encoding="utf-8",
            )
            args = ["--policy", str(policy), "--lock", str(lock)]
            for day, expected in ((21, 0), (22, 1)):
                with self.subTest(day=day), patch.object(self.checks, "datetime") as clock:
                    clock.now.return_value = datetime(2026, 12, day, tzinfo=timezone.utc)
                    with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                        self.assertEqual(self.checks.main(args), expected)
                    clock.now.assert_called_once_with(timezone.utc)
            lock.write_text("not valid TOML", encoding="utf-8")
            with redirect_stderr(io.StringIO()):
                self.assertEqual(self.checks.main(args), 1)

    def test_command_does_not_offer_a_clock_override(self) -> None:
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as failure:
            self.checks.main(["--today", "2026-11-22"])
        self.assertEqual(failure.exception.code, 2)

    def test_checked_in_policy_and_lock_match_the_reviewed_approval(self) -> None:
        root = Path(__file__).resolve().parents[2]
        policy = Path(os.environ.get("COCO_ADVISORY_POLICY", root / "deny.toml"))
        lock = Path(os.environ.get("COCO_ADVISORY_LOCK", root / "Cargo.lock"))
        # Nix may cache this fixture's derivation. Check its inputs against the
        # approval date here; actual expiry runs uncached in policy/release apps.
        self.checks.check(
            tomllib.loads(policy.read_text(encoding="utf-8")),
            tomllib.loads(lock.read_text(encoding="utf-8")),
            self.today,
        )


if __name__ == "__main__":
    unittest.main()
