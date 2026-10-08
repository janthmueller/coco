"""Regression tests for release selection and exact-revision publication guards."""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


def _load_checks():
    path = Path(os.environ.get("COCO_RELEASE_CHECKS", Path(__file__).with_name("release_checks.py")))
    spec = importlib.util.spec_from_file_location("release_checks", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load release checks")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseChecksTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = _load_checks()
        self.sha = "a" * 40
        self.environment = {
            "GITHUB_EVENT_NAME": "workflow_dispatch",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_SHA": self.sha,
            "RELEASE_CHANNEL": "alpha",
            "PUBLISH_REQUESTED": "false",
        }
        self.run = {
            "headSha": self.sha,
            "headBranch": "main",
            "status": "completed",
            "conclusion": "success",
        }
        self.automatic_environment = self.environment | {
            "GITHUB_EVENT_NAME": "workflow_run",
            "COCO_RELEASE_ENABLED": "true",
            "GITHUB_SHA": "b" * 40,
            "RELEASE_CHANNEL": "alpha",
            "PUBLISH_REQUESTED": "true",
        }
        self.upstream_run = {
            "head_sha": self.sha,
            "head_branch": "main",
            "event": "push",
            "conclusion": "success",
        }

    def test_automatic_alpha_uses_the_tested_source_not_the_callback_sha(self) -> None:
        parameters = self.module.request_parameters(self.automatic_environment, self.upstream_run)
        self.assertEqual(parameters, {
            "target_sha": self.sha, "channel": "alpha", "prerelease": "true",
            "publish_requested": "true",
        })
        with self.assertRaises(ValueError):
            self.module.check_revision(parameters["target_sha"], self.sha, "b" * 40)

    def test_automatic_alpha_requires_the_exact_enable_switch(self) -> None:
        for value in ("", "false", "TRUE", "1"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.module.request_parameters(
                    self.automatic_environment | {"COCO_RELEASE_ENABLED": value}, self.upstream_run,
                )

    def test_automatic_alpha_rejects_bad_runs_and_non_alpha_overrides(self) -> None:
        for field, value in (
            ("head_sha", ""), ("head_sha", None), ("head_sha", "not-a-commit"),
            ("head_branch", "feature/release"), ("conclusion", "failure"),
            ("conclusion", "cancelled"), ("event", "pull_request"),
            ("event", "workflow_dispatch"),
        ):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                self.module.request_parameters(self.automatic_environment, self.upstream_run | {field: value})
        for run in (None, [], {}, "invalid"):
            with self.assertRaises(ValueError):
                self.module.request_parameters(self.automatic_environment, run)
        for override in ({"RELEASE_CHANNEL": "stable"}, {"PUBLISH_REQUESTED": "false"}):
            with self.assertRaises(ValueError):
                self.module.request_parameters(self.automatic_environment | override, self.upstream_run)

    def test_alpha_and_stable_rehearsals_do_not_request_publication(self) -> None:
        for channel, prerelease in (("alpha", "true"), ("stable", "false")):
            with self.subTest(channel=channel):
                parameters = self.module.request_parameters(self.environment | {"RELEASE_CHANNEL": channel})
                self.assertEqual(parameters["publish_requested"], "false")
                self.assertEqual(parameters["prerelease"], prerelease)
                self.assertEqual(parameters["target_sha"], self.sha)

    def test_publication_requires_an_explicit_true_for_each_channel(self) -> None:
        for channel in ("alpha", "stable"):
            parameters = self.module.request_parameters(self.environment | {
                "RELEASE_CHANNEL": channel, "PUBLISH_REQUESTED": "true",
            })
            self.assertEqual(parameters["publish_requested"], "true")

    def test_unapproved_events_other_branches_and_invalid_inputs_are_rejected(self) -> None:
        for name, values in {
            "GITHUB_EVENT_NAME": ("push", "workflow_run", "pull_request", ""),
            "GITHUB_REF": ("refs/heads/feature/release", "refs/tags/v0.1.0", ""),
            "GITHUB_SHA": ("a" * 39, "not-a-commit", ""),
            "RELEASE_CHANNEL": ("beta", "stable\npublish_requested=true", ""),
            "PUBLISH_REQUESTED": ("1", "TRUE", "", "false\ntrue"),
        }.items():
            for value in values:
                with self.subTest(name=name, value=value):
                    with self.assertRaises(ValueError):
                        self.module.request_parameters(self.environment | {name: value})

    def test_the_requested_checked_out_and_current_revision_must_match(self) -> None:
        self.module.check_revision(self.sha, self.sha, self.sha)
        for head, current in (("b" * 40, self.sha), (self.sha, "b" * 40)):
            with self.assertRaises(ValueError):
                self.module.check_revision(self.sha, head, current)

    def test_only_latest_completed_success_for_the_exact_main_revision_passes(self) -> None:
        self.module.check_rust_run([self.run], self.sha)
        for field, value in (
            ("headSha", "b" * 40), ("headBranch", "feature/release"),
            ("status", "in_progress"), ("status", "queued"),
            ("conclusion", "failure"), ("conclusion", "cancelled"), ("conclusion", None),
        ):
            with self.subTest(field=field, value=value):
                with self.assertRaises(ValueError):
                    self.module.check_rust_run([self.run | {field: value}], self.sha)
        for runs in (None, {}, [], [None], [{}], [self.run, self.run]):
            with self.assertRaises(ValueError):
                self.module.check_rust_run(runs, self.sha)

    def test_request_command_emits_validated_outputs_only_after_all_checks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            environment = self.environment | {"GITHUB_OUTPUT": str(output)}
            with patch.dict(os.environ, environment, clear=True), patch.object(
                self.module, "_output", side_effect=["", self.sha, self.sha, json.dumps([self.run])],
            ) as command:
                self.assertEqual(self.module.main(["request"]), 0)
            self.assertEqual(output.read_text(), (
                f"target_sha={self.sha}\nchannel=alpha\nprerelease=true\npublish_requested=false\n"
            ))
            self.assertIn("--limit", command.call_args_list[-1].args[0])
            self.assertNotIn("--status", command.call_args_list[-1].args[0])

    def test_automatic_request_reads_native_event_and_checks_its_exact_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"workflow_run": self.upstream_run}))
            environment = self.automatic_environment | {
                "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_PATH": str(event),
            }
            with patch.dict(os.environ, environment, clear=True), patch.object(
                self.module, "_output", side_effect=["", self.sha, self.sha, json.dumps([self.run])],
            ) as command:
                self.assertEqual(self.module.main(["request"]), 0)
            self.assertEqual(output.read_text(), (
                f"target_sha={self.sha}\nchannel=alpha\nprerelease=true\npublish_requested=true\n"
            ))
            self.assertIn(self.sha, command.call_args_list[-1].args[0])
            self.assertNotIn("b" * 40, command.call_args_list[-1].args[0])

    def test_stale_automatic_request_never_queries_ci_or_emits_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"workflow_run": self.upstream_run}))
            environment = self.automatic_environment | {
                "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_PATH": str(event),
            }
            with patch.dict(os.environ, environment, clear=True), patch.object(
                self.module, "_output", side_effect=["", self.sha, "b" * 40],
            ) as command:
                with self.assertRaises(ValueError):
                    self.module.main(["request"])
            self.assertEqual(command.call_count, 3)
            self.assertFalse(output.exists())

    def test_failed_request_does_not_emit_partial_publication_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            output.write_text("existing=unchanged\n")
            environment = self.environment | {"GITHUB_OUTPUT": str(output), "PUBLISH_REQUESTED": "true"}
            with patch.dict(os.environ, environment, clear=True), patch.object(
                self.module, "_output", side_effect=["", self.sha, self.sha, json.dumps([])],
            ):
                with self.assertRaises(ValueError):
                    self.module.main(["request"])
            self.assertEqual(output.read_text(), "existing=unchanged\n")

    def test_subprocess_failures_do_not_expose_raw_stderr(self) -> None:
        for error in (
            subprocess.CalledProcessError(1, ["gh"], stderr="Bearer private-test-sentinel"),
            subprocess.TimeoutExpired(["gh"], 30, stderr="Bearer private-test-sentinel"),
        ):
            with patch.object(self.module.subprocess, "run", side_effect=error):
                with self.assertRaises(ValueError) as raised:
                    self.module._output(["gh", "run", "list"])
                self.assertNotIn("private-test-sentinel", str(raised.exception))

    def check_version(self, channel, version, *, tag=None, manifest=None, roots=None):
        self.module.check_version(
            channel, version, f"v{version}" if tag is None else tag,
            {"package": {"name": "codex-coordinator", "version": version}} if manifest is None else manifest,
            {"package": [{"name": "codex-coordinator", "version": version}] if roots is None else roots},
        )

    def test_synchronized_alpha_and_stable_metadata_are_accepted(self) -> None:
        self.check_version("alpha", "0.1.0-alpha.11")
        self.check_version("stable", "0.1.0")
        self.check_version("stable", "0.2.0")

    def test_channel_crossovers_and_malformed_versions_are_rejected(self) -> None:
        for channel, version in (
            ("alpha", "0.1.0"), ("stable", "0.1.0-alpha.11"),
            ("alpha", "0.1.0-beta.1"), ("alpha", "0.1.0-alpha.01"),
            ("stable", "00.1.0"), ("stable", "0.1.0+local"), ("beta", "0.1.0"),
        ):
            with self.subTest(channel=channel, version=version):
                with self.assertRaises(ValueError):
                    self.check_version(channel, version)

    def test_tag_manifest_and_root_lock_version_must_all_match(self) -> None:
        for overrides in (
            {"tag": "v0.2.0"}, {"manifest": {}},
            {"manifest": {"package": {"name": "other", "version": "0.1.0"}}},
            {"manifest": {"package": {"name": "codex-coordinator", "version": "0.2.0"}}},
            {"roots": []},
            {"roots": [{"name": "codex-coordinator", "version": "0.2.0"}]},
            {"roots": [{"name": "codex-coordinator", "version": "0.1.0"}] * 2},
            {"roots": [{"name": "codex-coordinator", "version": "0.1.0", "source": "registry"}]},
        ):
            with self.subTest(overrides=overrides):
                with self.assertRaises(ValueError):
                    self.check_version("stable", "0.1.0", **overrides)


if __name__ == "__main__":
    unittest.main()
