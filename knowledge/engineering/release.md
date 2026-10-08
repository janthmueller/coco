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
   tags the explicitly selected manual alpha/stable channel.
   A rehearsal first stamps and checks the metadata without committing,
   tagging, pushing, or creating a release.
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

Packages install the `cocod` executable but never start or enable it as a
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

The repository, GitHub Pages site, release archives, and crate are
public. Repository visibility and package publication remain operationally
sensitive changes even though the initial publication decision is complete.

Publication is deliberately dispatched for both regular releases and optional
alpha previews. `COCO_RELEASE_ENABLED=false` disables the previous automatic
main-push alpha train; ordinary pushes run verification and documentation
delivery without publication. This switch was disabled for the first regular
release on 2026-10-08 and must not be re-enabled incidentally.

The workflow retains the tested optional automatic-alpha path for a separately
authorized operational decision. It requires `COCO_RELEASE_ENABLED` to be exactly
`true` and successful `Rust` completion after a main push. It always selects
alpha and can never promote stable. Keeping this capability does not make it
the current release policy.

Manual dispatch remains available independently of the automatic switch and
defaults to `channel=alpha` and `publish=false`: a non-publishing version/build
rehearsal. The operator must explicitly select `stable` to promote a normal
release and set `publish=true` to publish either manually selected channel.

Both rehearsals and publication require the selected SHA, checked-out commit,
and current remote `main` tip to match. The latest `Rust` run for that exact
revision must be completed and successful, including its binary-smoke jobs;
an older successful run does not override a newer queued, running, failed, or
cancelled run. The selected channel, proposed tag, and versions stamped in
`Cargo.toml` and the source-less root `Cargo.lock` entry must agree before
publication. `.github/scripts/release_checks.py` owns these fail-closed checks.
Automatic requests use `workflow_run.head_sha`, not the callback's
`GITHUB_SHA`; manual requests use the explicitly dispatched `GITHUB_SHA`.
An old successful callback cannot publish a newer untested main tip.

The release workflow never publishes an older successful commit after `main`
has advanced. It uses a non-cancelling release concurrency group and rechecks
the remote branch immediately before versioning. Branch protection must either
permit the repository token to push the generated release commit and tag or
provide the narrowly scoped `GH_PAT` secret used by the existing Wuf pattern.

The exact-main and successful-test checks run again immediately before
publication, and Semantic Release retains its own final upstream guard before
pushing. Changing the automatic-release switch, selecting channels, executing
a publishing dispatch, or changing registry credentials remains an operational
release decision. Preparing this workflow locally does not authorize any of
those operations.

## Version and changelog policy

`releaserc.toml` is the release authority. The Cargo package is named
`codex-coordinator`; its library crate and installed executable remain `coco`.
The package version in
`Cargo.toml` is the sole source stamp; `.github/scripts/sync_cargo_lock.py`
updates only the matching source-less root package in `Cargo.lock` and fails
closed on malformed, duplicate, or inconsistent metadata. The lockfile is
committed as a release asset so a tag always passes `cargo --locked`.
The synchronization build command uses `python3`, consistently with the other
automation checks; no shell alias for `python` is part of the release contract.

The `main` branch configuration is stable-capable. Manually selected alpha uses
Semantic Release's `prerelease` action input and `alpha` token; stable selection
leaves the normal-version calculation intact. Both use the same Conventional
Commit history and Cargo lock synchronization.
No branch rename, manual version edit, or incidental commit type changes the
selected channel.

The first suffix-free `0.1.0` was published through the guarded stable channel
on 2026-10-08 after the exact-revision Rust workflow and a non-publishing stable
rehearsal passed. Release commits stamp the selected version; local and
flake-built executables report that exact source version. Optional alpha
previews keep the `-alpha.N` suffix through an explicit alpha request. Preparing
or rehearsing automation does not change the source repository version.

The post-alpha baseline is suffix-free `0.1.0`, not a `1.0.0` compatibility
promise. Alpha numbering must not substitute for ordinary `0.x` evolution
indefinitely. After promotion, compatible fixes use
`0.1.x`; meaningful feature or breaking changes before 1.0 advance the minor
line (`0.2.0`, `0.3.0`, and so on). A future minor may have explicit
`0.2.0-alpha.N` previews when external testing is useful, but every minor does
not require an alpha train or a beta phase.

Stable package publication is deliberate rather than an automatic consequence
of every qualifying commit. Ordinary pushes run verification and documentation
delivery without publication under the current policy. The release
workflow exposes an explicit preview-versus-stable choice: preview publication
carries the `alpha` suffix and prerelease channel,
while stable publication is suffix-free and becomes the normal GitHub/crates.io
release. The first normal publication is complete; the same checklist applies
to subsequent releases.

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

The normal crate is available on crates.io, so public Cargo installation
examples use `cargo install --locked codex-coordinator`. Optional alpha
installation must name an exact preview version because a bare install selects
the normal release. Verify registry and archive availability before updating
public installation/status claims for a newly published line.

## Release checklist

Before either channel:

1. Review the complete intended source, including newly added modules. Obtain
   authorization for checkpoint/push and intended publication. Confirm the
   automatic-alpha switch is off under the regular-release policy. If separately
   re-enabled, a qualifying tested push can publish alpha. Local preparation
   does not change that switch or authorize either a push or publication.
2. Require the exact current `main` revision to pass `Rust`: format, Clippy,
   all-target tests, dependency use/policy, actual registry-package dry-run,
   Git-flake checks, and native Linux/macOS binary smoke. Static documentation
   checks must also pass when public content changes. A local snapshot can
   validate pending source, but cannot substitute for the exact hosted SHA.
   Generated release commits carry `[skip ci]`; if the current main tip is such
   a commit, use the Rust workflow's manual dispatch before rehearsing or
   promoting it. A successful test of its parent is not exact-tip proof.
3. For a manual release, dispatch the selected channel with `publish=false`,
   inspect the calculated version/changelog, and verify the stamped Cargo/lock/tag
   checks pass. For
   the first stable promotion, the expected normal version is `0.1.0`; do not
   publish an unexpected bump merely because it is suffix-free.
   Automatic alpha runs the same non-publishing rehearsal and metadata checks
   inside CI before its publishing step.
4. With publication authorization, either use enabled automatic alpha after a
   qualifying tested push, or dispatch the same tested current revision and
   selected channel with `publish=true`. Stable always requires manual dispatch.
   Verify the resulting GitHub tag/release, checksummed archives, and crates.io
   version.
5. Only after confirming registry and archive availability, update applicable
   public install examples and alpha wording. For the first stable release,
   change exact-alpha Cargo examples to the normal unversioned install.

Local tests cover enabled automatic alpha, callback/source SHA separation,
rejected non-push/failed/non-main callbacks, disabled automation, forbidden
automatic stable selection, manual default rehearsals and explicit publication,
stale SHA, latest CI state, channel/version/tag crossovers, and mismatched or
duplicate Cargo lock roots. Nix tooling checks run these alongside actionlint
and the lockfile synchronizer tests. The
workflow requires an authorized exact-SHA hosted rehearsal before publication;
its Docker action cannot be represented by static lint alone. The first stable
promotion proved that native hosted action path, in addition to local fixtures.

The pinned Semantic Release [action input contract](https://github.com/python-semantic-release/python-semantic-release/blob/39dd2052f2ce8282a5d932c31d58a2ca06d2550e/action.yml)
and [input-to-CLI mapping](https://github.com/python-semantic-release/python-semantic-release/blob/39dd2052f2ce8282a5d932c31d58a2ca06d2550e/src/gh_action/action.sh)
define the native prerelease and no-publication behavior rather than a custom
version calculator in CoCo.
