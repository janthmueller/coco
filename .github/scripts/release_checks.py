"""Fail-closed checks for automatic alpha and deliberate stable releases."""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tomllib
from collections.abc import Mapping
from pathlib import Path

_NUMBER = r"(?:0|[1-9][0-9]*)"
_NORMAL_VERSION = rf"{_NUMBER}\.{_NUMBER}\.{_NUMBER}"
_CHANNEL_VERSIONS = {
    "alpha": re.compile(rf"{_NORMAL_VERSION}-alpha\.{_NUMBER}"),
    "stable": re.compile(_NORMAL_VERSION),
}


def request_parameters(
    environment: Mapping[str, str], workflow_run: object = None,
) -> dict[str, str]:
    if environment.get("GITHUB_REF") != "refs/heads/main":
        raise ValueError("releases require the main branch")
    event = environment.get("GITHUB_EVENT_NAME")
    if event == "workflow_dispatch":
        channel = environment.get("RELEASE_CHANNEL", "")
        if channel not in _CHANNEL_VERSIONS:
            raise ValueError("select the alpha or stable release channel")
        publish = environment.get("PUBLISH_REQUESTED", "")
        if publish not in ("true", "false"):
            raise ValueError("publication must be explicitly true or false")
        sha = environment.get("GITHUB_SHA", "")
    elif event == "workflow_run":
        if environment.get("COCO_RELEASE_ENABLED") != "true":
            raise ValueError("automatic alpha publication is disabled")
        if (
            not isinstance(workflow_run, dict)
            or workflow_run.get("conclusion") != "success"
            or workflow_run.get("event") != "push"
            or workflow_run.get("head_branch") != "main"
        ):
            raise ValueError("automatic alpha requires a successful Rust push run on main")
        if environment.get("RELEASE_CHANNEL", "") not in ("", "alpha"):
            raise ValueError("automatic publication may only select alpha")
        if environment.get("PUBLISH_REQUESTED", "") not in ("", "true"):
            raise ValueError("automatic publication requires its explicit enable switch")
        channel, publish = "alpha", "true"
        sha = workflow_run.get("head_sha", "")
    else:
        raise ValueError("unsupported release event")
    if not isinstance(sha, str) or re.fullmatch(r"[0-9a-f]{40}", sha) is None:
        raise ValueError("the selected revision is not a commit SHA")
    return {
        "target_sha": sha,
        "channel": channel,
        "prerelease": "true" if channel == "alpha" else "false",
        "publish_requested": publish,
    }


def check_revision(requested: str, checked_out: str, current_main: str) -> None:
    if requested != checked_out or requested != current_main:
        raise ValueError("the selected revision is no longer the current main tip")


def check_rust_run(runs: object, sha: str) -> None:
    if not isinstance(runs, list) or len(runs) != 1 or not isinstance(runs[0], dict):
        raise ValueError("the current main revision needs a completed successful Rust workflow")
    run = runs[0]
    if (
        run.get("headSha") != sha
        or run.get("headBranch") != "main"
        or run.get("status") != "completed"
        or run.get("conclusion") != "success"
    ):
        raise ValueError("the latest Rust workflow for this main revision has not passed")


def check_version(
    channel: str,
    version: str,
    tag: str,
    manifest: dict,
    lock: dict,
) -> None:
    pattern = _CHANNEL_VERSIONS.get(channel)
    if pattern is None or pattern.fullmatch(version) is None:
        raise ValueError("the proposed version does not match the selected release channel")
    if tag != f"v{version}":
        raise ValueError("the proposed release tag does not match its version")
    package = manifest.get("package", {})
    roots = [
        item
        for item in lock.get("package", [])
        if item.get("name") == "codex-coordinator" and "source" not in item
    ]
    if (
        package.get("name") != "codex-coordinator"
        or package.get("version") != version
        or len(roots) != 1
        or roots[0].get("version") != version
    ):
        raise ValueError("the proposed version is not synchronized in Cargo.toml and Cargo.lock")


def _output(command: list[str]) -> str:
    try:
        return subprocess.run(
            command, check=True, capture_output=True, text=True, timeout=30
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError("release verification could not complete its Git/GitHub check") from error


def main(arguments: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if arguments is None else arguments
    if arguments == ["request"]:
        workflow_run = None
        if os.environ.get("GITHUB_EVENT_NAME") == "workflow_run":
            event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
            if not isinstance(event, dict):
                raise ValueError("the workflow event is not an object")
            workflow_run = event.get("workflow_run")
        parameters = request_parameters(os.environ, workflow_run)
        _output(["git", "fetch", "origin", "main"])
        sha = parameters["target_sha"]
        check_revision(sha, _output(["git", "rev-parse", "HEAD"]), _output(["git", "rev-parse", "origin/main"]))
        runs = json.loads(_output([
            "gh", "run", "list", "--workflow", "rust.yml", "--commit", sha,
            "--branch", "main", "--limit", "1", "--json", "status,conclusion,headSha,headBranch",
        ]))
        check_rust_run(runs, sha)
        with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
            for name, value in parameters.items():
                output.write(f"{name}={value}\n")
    elif arguments == ["version"]:
        manifest = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))
        lock = tomllib.loads(Path("Cargo.lock").read_text(encoding="utf-8"))
        check_version(
            os.environ.get("RELEASE_CHANNEL", ""),
            os.environ.get("RELEASE_VERSION", ""),
            os.environ.get("RELEASE_TAG", ""),
            manifest,
            lock,
        )
    else:
        raise ValueError("usage: release_checks.py (request | version)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError):
        # Do not expose subprocess stderr, credentials, or unexpected API payloads.
        print("Release verification failed; check channel, current main, Rust CI, and Cargo version consistency.", file=sys.stderr)
        raise SystemExit(1) from None
