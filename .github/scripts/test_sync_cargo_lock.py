"""Regression tests for the release lockfile synchronizer."""

from __future__ import annotations

import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from types import ModuleType


def _load_synchronizer() -> ModuleType:
    path = Path(os.environ.get("COCO_RELEASE_SCRIPT", Path(__file__).with_name("sync_cargo_lock.py")))
    spec = importlib.util.spec_from_file_location("sync_cargo_lock", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SynchronizeCargoLockTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.repository = Path(self.temporary.name)
        self.module = _load_synchronizer()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_fixture(self, manifest_version: str, locked_version: str) -> None:
        (self.repository / "Cargo.toml").write_text(
            f'[package]\nname = "codex-coordinator"\nversion = "{manifest_version}"\n',
            encoding="utf-8",
        )
        (self.repository / "Cargo.lock").write_text(
            """version = 4

[[package]]
name = "coco-helper"
version = "9.9.9"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "codex-coordinator"
version = """
            + f'"{locked_version}"\n'
            + 'dependencies = ["coco-helper"]\n',
            encoding="utf-8",
        )

    def test_updates_only_the_root_package(self) -> None:
        self.write_fixture("0.1.0-alpha.3", "0.1.0-alpha.2")

        self.module.synchronize_version(self.repository, "0.1.0-alpha.3")

        contents = (self.repository / "Cargo.lock").read_text(encoding="utf-8")
        self.assertIn('name = "coco-helper"\nversion = "9.9.9"', contents)
        self.assertIn('name = "codex-coordinator"\nversion = "0.1.0-alpha.3"', contents)
        self.assertNotIn('version = "0.1.0-alpha.2"', contents)

    def test_rejects_a_version_not_stamped_in_the_manifest(self) -> None:
        self.write_fixture("0.1.0-alpha.3", "0.1.0-alpha.2")

        with self.assertRaisesRegex(ValueError, "does not match Cargo.toml"):
            self.module.synchronize_version(self.repository, "0.1.0-alpha.4")

    def test_rejects_a_non_semantic_version(self) -> None:
        self.write_fixture("next", "0.1.0-alpha.2")

        with self.assertRaisesRegex(ValueError, "invalid release version"):
            self.module.synchronize_version(self.repository, "next")


if __name__ == "__main__":
    unittest.main()
