---
type: Engineering Practice
title: CoCo release process
description: Defines semantic versioning, tested-revision guards, Cargo metadata synchronization, and supported binary artifacts.
tags: [engineering, release, ci, semver, packaging]
status: draft
---

# CoCo release process

## Release shape

CoCo uses the same release control pattern as Wuf, adapted to one Rust package:

1. Conventional Commits on `main` determine the next semantic version.
2. The complete `Rust` workflow, including native binary smoke builds, must
   pass. This includes the reviewed `cargo deny` policy for advisories,
   licenses, sources, wildcard requirements, and duplicate-version reporting.
3. The release guard verifies that the tested commit is still the current tip
   of `main`.
4. Python Semantic Release stamps `Cargo.toml`, synchronizes the root package
   entry in `Cargo.lock`, updates `CHANGELOG.md`, creates a release commit, and
   tags an alpha version.
5. A GitHub Release is created from that tag.
6. Linux and macOS runners build and smoke-test `coco`, `cocod`, and
   `coco-mcp`, then attach one archive and SHA-256 checksum per platform.
7. The same tagged package is published to crates.io as
   `codex-coordinator`.

The project remains one version and one Cargo package. All three executables
ship together because they implement one product and share one protocol and
state schema.

The root Nix flake exposes the same package independently of registry releases:
its default package installs all three executables, its default app runs
`coco`, and named `cocod` and `coco-mcp` apps support direct execution. The
package version is read from `Cargo.toml`, so Semantic Release stamps Cargo,
Nix, registry, and executable output from one source value.

## Daemon service policy

Alpha packages install the `cocod` executable but never start or enable it as a
background service. The supported lifecycle is an explicit foreground
invocation, independent of whether installation used Cargo, a release archive,
or Nix. Package installation must not create a surprising persistent process.

A supervised user service is a later, opt-in, cross-platform feature. Do not
ship a Linux-only unit as the product contract. First route SIGTERM through the
orderly shutdown path and define/test App Server child-exit handling, generation
replacement, stale-state reconciliation, eligible thread recovery, logs, and
restart limits. Only then may platform adapters such as systemd user services,
launchd agents, and the eventual Windows equivalent enable supervision.

## Publication guard

Automatic releases after a successful push are disabled unless the repository
variable `COCO_RELEASE_ENABLED` is exactly `true`. This permits the pipeline to
be tested and reviewed without making repository setup itself publish a
release. A manual dispatch defaults to a non-publishing version/build rehearsal;
the operator must explicitly set its `publish` input to create the release.
Both paths still require a successful `Rust` workflow for the exact current
`main` commit.

The release workflow never publishes an older successful commit after `main`
has advanced. It uses a non-cancelling release concurrency group and rechecks
the remote branch immediately before versioning. Branch protection must either
permit the repository token to push the generated release commit and tag or
provide the narrowly scoped `GH_PAT` secret used by the existing Wuf pattern.

Do not enable automatic publication until the public repository identity,
license, supported Codex version statement, and installation documentation are
ready for the first alpha. Enabling the repository variable is an operational
release decision, not an ordinary code change.

## Version and changelog policy

`releaserc.toml` is the release authority. The Cargo package is named
`codex-coordinator`; its library crate and installed executable remain `coco`.
The package version in
`Cargo.toml` is the sole source stamp; `.github/scripts/sync_cargo_lock.py`
updates only the matching source-less root package in `Cargo.lock` and fails
closed on malformed, duplicate, or inconsistent metadata. The lockfile is
committed as a release asset so a tag always passes `cargo --locked`.

While CoCo is in alpha, the workflow forces the `alpha` prerelease
token even though the `main` branch configuration itself remains stable-ready.
Moving to stable releases therefore requires an explicit workflow decision,
not a branch rename or an accidental commit type.

Before the first published tag, the source tree uses `0.1.0-alpha.0` as an
explicit unreleased baseline. Local and flake-built executables therefore do
not present themselves as stable; the first Semantic Release advances that
baseline to `0.1.0-alpha.1`.

Commit subjects follow Conventional Commits. `feat` causes a feature bump,
`fix` causes a patch bump, and an explicit breaking-change marker causes the
corresponding breaking bump under the configured pre-1.0 policy. Documentation,
test, refactor, CI, and chore commits remain in history and may appear in the
generated changelog, but do not create a release by themselves unless their
parser semantics explicitly request one.

## Binary contract

Release archives are built natively on Ubuntu 22.04 and macOS 15 with the exact
toolchain from `rust-toolchain.toml`. Building directly on the hosted runner,
rather than inside the Nix development shell, avoids distributing executables
whose dynamic loader or libraries point into the Nix store. Each archive must:

- contain `coco`, `cocod`, `coco-mcp`, the MIT license, and the public README;
- report the exact tag version from every executable;
- render `--help` successfully for every executable; and
- have a sibling SHA-256 checksum.

Windows is intentionally absent until the named-pipe CLI-to-daemon backend and
Windows tests exist. The reusable binary workflow supports non-uploading builds
from an arbitrary commit so packaging changes can be proved before a release;
upload mode accepts only the exact tagged commit.

The package manifest explicitly permits only crates.io and includes production
source, Cargo metadata, the public README, and the license. It excludes tests,
the internal knowledge bundle, site sources, and repository automation from
the registry artifact. CI runs `cargo publish --dry-run --locked` before a
revision can release; the tagged release job reads its credential only from
the `CARGO_REGISTRY_TOKEN` GitHub secret.

Source installation stays authoritative until the first registry version and
archives have actually been produced and the public guide has been updated
from those observed artifacts.
