"""Synchronize CoCo's release version into Cargo.lock."""

from __future__ import annotations

import os
import re
import sys
import tempfile
import tomllib
from pathlib import Path

_SEMVER = re.compile(
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)"
    r"(?:-(?:[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+(?:[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
_VERSION_LINE = re.compile(r'version = "[^"]+"(?P<newline>\r?\n)?\Z')
_PACKAGE_NAME = "codex-coordinator"


def synchronize_version(repository: Path, requested_version: str) -> None:
    """Update the root package's Cargo.lock entry after Cargo.toml is stamped."""

    if _SEMVER.fullmatch(requested_version) is None:
        raise ValueError(f"invalid release version: {requested_version!r}")

    manifest_path = repository / "Cargo.toml"
    lock_path = repository / "Cargo.lock"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    configured_version = str(manifest["package"]["version"])
    if configured_version != requested_version:
        raise ValueError(
            f"release version {requested_version!r} does not match "
            f"Cargo.toml version {configured_version!r}"
        )

    lines = lock_path.read_text(encoding="utf-8").splitlines(keepends=True)
    matching_blocks = _root_package_blocks(lines)
    if len(matching_blocks) != 1:
        raise ValueError(
            f"expected one root {_PACKAGE_NAME} package in Cargo.lock, found {len(matching_blocks)}"
        )

    start, end = matching_blocks[0]
    version_indexes = tuple(
        index for index in range(start, end) if _VERSION_LINE.fullmatch(lines[index]) is not None
    )
    if len(version_indexes) != 1:
        raise ValueError(
            f"root {_PACKAGE_NAME} package must contain exactly one version in Cargo.lock"
        )

    index = version_indexes[0]
    newline = "\r\n" if lines[index].endswith("\r\n") else "\n"
    replacement = f'version = "{requested_version}"{newline}'
    if lines[index] != replacement:
        lines[index] = replacement
        _atomic_write(lock_path, "".join(lines))

    locked = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    root_packages = tuple(
        package
        for package in locked.get("package", ())
        if package.get("name") == _PACKAGE_NAME and "source" not in package
    )
    if len(root_packages) != 1 or root_packages[0].get("version") != requested_version:
        raise RuntimeError("Cargo.lock version synchronization did not validate")


def _root_package_blocks(lines: list[str]) -> tuple[tuple[int, int], ...]:
    starts = [index for index, line in enumerate(lines) if line.rstrip("\r\n") == "[[package]]"]
    blocks: list[tuple[int, int]] = []
    for ordinal, start in enumerate(starts):
        end = starts[ordinal + 1] if ordinal + 1 < len(starts) else len(lines)
        content = {line.rstrip("\r\n") for line in lines[start:end]}
        if f'name = "{_PACKAGE_NAME}"' in content and not any(
            line.startswith("source = ") for line in content
        ):
            blocks.append((start, end))
    return tuple(blocks)


def _atomic_write(path: Path, content: str) -> None:
    mode = path.stat().st_mode
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if argv is None else argv
    if len(arguments) != 1:
        raise SystemExit("usage: sync_cargo_lock.py VERSION")
    synchronize_version(Path(__file__).resolve().parents[2], arguments[0])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
