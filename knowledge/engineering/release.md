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
   tags the selected release channel. The currently implemented workflow
   selects alpha unconditionally; the approved transition below is not yet
   implemented.
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

The repository, GitHub Pages site, release archives, and alpha crate are
public. Repository visibility and package publication remain operationally
sensitive changes even though the initial publication decision is complete.

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

Changing `COCO_RELEASE_ENABLED`, release channels, or registry credentials is
an operational release decision, not an ordinary code change. Until the
approved explicit-channel transition is implemented, enabling the variable
continues to publish alpha releases after qualifying tested pushes.

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

During the current channel, release commits stamp `0.1.0-alpha.N`; local and
flake-built executables report that exact source version. Semantic Release
advances the prerelease number from qualifying commits until the explicit
stable-channel transition is implemented.

The approved post-alpha direction is to promote the tested `0.1` line to a
suffix-free `0.1.0`, not to claim `1.0.0`. A final `0.1.0-alpha.N` may be used
as a bounded release candidate, but alpha numbering must not substitute for
ordinary `0.x` evolution indefinitely. After promotion, compatible fixes use
`0.1.x`; meaningful feature or breaking changes before 1.0 advance the minor
line (`0.2.0`, `0.3.0`, and so on). A future minor may have explicit
`0.2.0-alpha.N` previews when external testing is useful, but every minor does
not require an alpha train or a beta phase.

Stable package publication is deliberate rather than an automatic consequence
of every qualifying commit. Ordinary pushes run verification and documentation
delivery. The release workflow must expose an explicit preview-versus-stable
choice: preview publication carries the `alpha` suffix and prerelease channel,
while stable publication is suffix-free and becomes the normal GitHub/crates.io
release. The current workflow still forces `alpha`; changing that automation
and completing one explicit release checklist are prerequisites for `0.1.0`.

`1.0.0` is reserved for an intentional compatibility promise covering at least
the CLI, control MCP surface, configuration, persisted-state migrations, and
workspace recovery behavior. A suffix-free `0.x` release communicates a usable
normal release without making that broader promise.

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

Until a suffix-free version exists on crates.io, Cargo installation examples
must name an exact alpha version because a bare install does not select a
prerelease. After the first stable `0.x` publication, verify the registry and
archives before changing the public guide to the unversioned install form.
