---
type: Engineering Decision
title: Rust source architecture and code-health strategy
description: Records the measured source-layout problems, target module boundaries, crate-split criteria, and enforceable Rust hygiene checks for CoCo.
tags: [architecture, rust, modules, cargo, quality, dependencies, testing]
status: proposed
---

# Rust source architecture and code-health strategy

## Decision summary

CoCo should remain one Cargo package for the next refactor. Its source should
become a hierarchy of focused Rust modules inside the existing library crate,
with the three binaries remaining thin entry points. A multi-package Cargo
workspace would add manifests, dependency plumbing, and cross-crate API
commitments without yet separating independently released or reused products.

The first architectural change is not file movement. It is a typed local
daemon protocol and an application boundary that removes RPC serialization
from the coordinator. Once that seam exists, the large files can be split
without simply spreading the same coupling across more paths.

Quality gates should favor compiler-aware, actionable checks. Rustfmt, Clippy,
tests, dependency policy, and unused-dependency checks belong in the normal
workflow. Aggregate “maintainability” or cognitive-complexity scores are useful
for exploration at most and must not become release gates.

## Rust vocabulary and the Python analogy

Rust provides the nested source organization expected from Python packages,
but the terms differ:

| Rust concept | Practical role | Rough Python analogy |
| --- | --- | --- |
| Cargo package | one `Cargo.toml`, dependency/version unit | distribution/project |
| crate | one compiled library or executable target | importable library or executable application |
| module | namespace and privacy boundary inside a crate | module or package namespace |
| workspace | related Cargo packages sharing lockfile and build output | monorepo containing multiple distributions |

The current package already builds four crates: the `coco` library plus the
`coco`, `cocod`, and `coco-mcp` binary crates. Files such as `src/codex.rs` may
declare children stored at `src/codex/process.rs` and
`src/codex/websocket.rs`; no additional `Cargo.toml` is needed. This is the
modern file layout described by the
[Rust Book](https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html).
Cargo workspaces are intended for multiple packages managed together, not as
the default answer to large files; see the
[Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html).

## Measured baseline

Measured on 2026-09-05 at commit `b9fb561` plus this documentation task:

| Area | Finding |
| --- | --- |
| Rust size | 8,618 lines across library and binaries; approximately 6,584 production and 1,977 in-module test lines in the library |
| Largest files | `coordinator.rs` 1,714; `store.rs` 1,617; `codex.rs` 1,535; `git.rs` 845 lines |
| Long functions | Clippy flags `cli::run` (117), shared App Server spawn (127), task creation (115), and Codex notification projection (106) over its default 100-line threshold |
| Other structural lints | The current code produces no `excessive_nesting`, `too_many_arguments`, or `type_complexity` findings when those lints are enabled |
| Module graph | eleven flat, public top-level library modules; the observed top-level dependency graph is acyclic |
| Boundary leaks | coordinator imports RPC envelopes/handler; CLI imports a Codex adapter DTO merely to discover the shared endpoint |
| Public surface | 78 top-level `pub` declarations, in addition to public methods; much is package-internal rather than an intentional library API |
| Test shape | 44 library tests plus one Unix process integration test that drives the built daemon and CLI against a fake App Server |
| Automation | GitHub Actions runs pinned format, Clippy, all-target tests, unused-dependency, and Flake checks; the static docs retain their separate workflow |
| Dependencies | 18 normal direct dependencies, one dev dependency, and 172 package entries in `Cargo.lock` |
| Dependency use | `cargo machete 0.9.2` reports no unused dependency |
| Duplicate versions | `cargo tree --duplicates` reports two digest stacks through SHA-1/WebSocket and SHA-2, two `syn` majors, and two `getrandom` lines; these require review, not a blanket failure rule |

The flat layout is not inherently unidiomatic. The problem is that several
files now combine policy, transport, parsing, lifecycle, persistence, and test
fixtures, while accidental public visibility makes dead-code detection less
effective.

## Target dependency direction

```text
domain <---------------- store
   ^  <---------------- git
   ^  <---------------- profile
   |
protocol <-------------- cli ---------> local RPC client
   ^  <---------------- MCP ----------> local RPC client
   |
coordinator <----------- daemon RPC handler
   ^                         |
   |                         +--------> store / git / profile
worker contract <------- Codex worker adapter
                                  |
                                  +----> Codex App Server client

daemon composition root depends on all concrete adapters
```

Rules:

1. Domain types import no CLI, daemon, RPC, SQLite, Git-process, MCP, or Codex
   transport code.
2. The local protocol owns method names and typed request/response DTOs. It
   does not open sockets or execute use cases.
3. Coordinator methods accept typed commands and return typed results. The
   coordinator does not implement `RpcHandler` and does not parse or emit
   arbitrary local-protocol JSON.
4. The daemon RPC handler is the translation boundary between envelopes and
   coordinator calls. Stable error codes are mapped there.
5. CLI and MCP depend on the local protocol and RPC client, never on
   coordinator, SQLite, Git, or Codex internals.
6. Codex JSON may remain `serde_json::Value` at the versioned App Server edge
   until generated Rust types are justified. That exception must not leak into
   CoCo's own daemon protocol.
7. Do not introduce a trait for every concrete type. `WorkerRuntime` already
   has a real fake implementation and earns its port. Store and Git ports
   should be extracted only when a second implementation, a focused unit-test
   seam, or a platform boundary needs them.

## Target source layout

Use the modern `name.rs` plus `name/child.rs` convention, retaining a small
facade in the parent file:

```text
src/
  lib.rs                         # narrow library facade for the binaries
  domain.rs                      # keep compact until its concepts truly split
  protocol.rs                    # typed daemon methods, DTOs, and error codes
  protocol/
    task.rs
    repository.rs
    event.rs

  coordinator.rs                 # Coordinator and stable application API
  coordinator/
    task.rs                      # register/create/list/status/diff use cases
    turn.rs                      # turn start and idempotency
    codex_events.rs              # App Server event projection
    error.rs
    tests.rs

  codex.rs                       # narrow App Server client facade
  codex/
    process.rs                   # child lifecycle and initialization
    jsonl.rs                     # framing/correlation
    websocket.rs                 # authenticated shared transport
    tests.rs

  store.rs                       # Store facade and transaction API
  store/
    migrations.rs
    rows.rs                      # SQL row mapping only
    tasks.rs
    events.rs
    tests.rs

  git.rs                         # Git adapter facade and public result types
  git/
    command.rs                   # bounded subprocess execution
    repository.rs
    worktree.rs
    diff.rs
    tests.rs

  rpc.rs                         # transport-neutral envelopes/client/server API
  rpc/
    unix.rs                      # Unix-domain-socket backend
    windows.rs                   # named-pipe backend when implemented

  cli.rs
  cli/
    args.rs
    commands.rs
    commands/
      status.rs
      jump.rs
    output.rs
    tests.rs

  mcp.rs                         # keep until production code grows materially
  profile.rs
  paths.rs
  daemon.rs
  daemon/
    handler.rs                   # RPC-to-coordinator translation
    worker.rs                    # WorkerRuntime adapter around CodexClient

  bin/                           # thin executable entry points only
```

This is a direction, not a requirement to create every file immediately.
Children should be extracted only when they own a coherent responsibility;
empty taxonomy directories make navigation worse.

## Intentional public API

The package is `publish = false`; its library exists primarily so three binary
crates can share code. `lib.rs` should expose only stable executable entry
points and, where needed, protocol types used by future clients. Everything
else should prefer private or `pub(crate)` visibility.

A suitable end state is a small facade such as `run_cli_from_env`,
`run_daemon_from_env`, and an MCP serve entry point, rather than eleven public
modules. Integration tests should exercise binaries through
`CARGO_BIN_EXE_*` or the intentional facade instead of making internals public
for convenience.

## Refactor sequence

### Phase 0 — safety net and repeatable gates

Completed on 2026-09-05. The process test uses the built `cocod` and `coco`
binaries with isolated temporary state and a fake authenticated App Server; it
does not invoke a model.

1. Add a Rust GitHub Actions workflow for format, Clippy, and all tests using
   the pinned toolchain.
2. Add one process-level test harness with a fake App Server executable. Cover
   daemon startup, repository registration, task preparation, explicit send,
   status, and clean shutdown without a model call.
3. Forbid unsafe code in CoCo unless a later platform adapter has a reviewed,
   narrowly scoped exception.
4. Add `cargo machete` as a fast dependency-use gate. Introduce `cargo deny`
   only with an explicit source/license/advisory policy.

Exit: the current behavior has an automated refactor safety net; no source
movement is required yet.

### Phase 1 — typed daemon seam

1. Create the closed method enum and typed parameter/result DTOs.
2. Make RPC serialization generic over those types while retaining the
   versioned envelope.
3. Move dispatch and error mapping from `Coordinator` into
   `daemon/handler.rs`.
4. Make CLI and MCP construct the same typed protocol values.
5. Move the shared endpoint descriptor out of the Codex adapter so CLI no
   longer imports `codex`.

Exit: no coordinator import of `rpc`, no CLI import of `codex`, and contract
tests cover every method name and wire field.

### Phase 2 — split the hot modules

Extract coherent child modules in this order:

1. coordinator commands and Codex event projection;
2. store migrations, row mapping, and task/event repositories;
3. Codex process, JSONL, and WebSocket layers;
4. Git command runner, worktree operations, and diff observation;
5. CLI argument parsing, commands, and output.

Move tests with the responsibility they cover. Reduce the four currently
flagged functions below 100 lines, then enable `clippy::too_many_lines` as a
project lint. Review `excessive_nesting` separately; do not enable the complete
Clippy restriction group.

Exit: parent modules are readable facades, boundaries match the dependency
rules, and behavior is unchanged.

### Phase 3 — platform transport boundary

Move Unix socket operations behind `rpc/unix.rs`; implement Windows named
pipes in `rpc/windows.rs` under `cfg(windows)`. Envelope, coordinator, CLI, and
MCP behavior remain common. Add native platform CI before claiming support.

### Phase 4 — reconsider a workspace

Split Cargo packages only if at least one of these becomes true:

- a separately built Rust GUI/client needs a stable, low-dependency protocol
  library;
- a component is released or versioned independently;
- dependency isolation measurably improves build/distribution cost;
- compiler-enforced crate boundaries solve recurring coupling that module
  visibility cannot;
- platform adapters require genuinely different dependency sets.

The likely first extraction would be a small `coco-protocol` crate, not one
crate per directory. Until a criterion is met, a workspace is rejected.

## Tooling policy

The pinned Nixpkgs input currently provides `cargo-deny 0.20.2`,
`cargo-machete 0.9.2`, `cargo-modules 0.27.0`, `cargo-llvm-cov 0.9.0`,
`cargo-mutants 27.1.0`, and `rust-code-analysis 0.0.25`. Tools belong in the
Nix development shell/check apps, not in application dependencies.

| Tool/check | Role | Policy |
| --- | --- | --- |
| `cargo fmt --check` | deterministic formatting | required on every change |
| `cargo clippy --all-targets --all-features -- -D warnings` | compiler-aware correctness/style/performance | required on every change |
| selected `too_many_lines` and `excessive_nesting` lints | actionable structural pressure | enable individually after the current findings are fixed |
| `cargo test --all-targets` | behavior and contract safety | required on every change |
| `cargo machete` | fast unused direct-dependency detection | required; document justified false positives |
| `cargo deny` | advisories, sources, licenses, and duplicate/banned crates | required after policy/config review; do not use an unreviewed generated config |
| `cargo tree --duplicates` | explain dependency duplication | review report; not a blanket pass/fail gate |
| `cargo llvm-cov` | reveal untested boundaries | scheduled/reporting first; do not invent an initial percentage gate |
| `cargo mutants` | prove important state/migration tests detect faults | scheduled and targeted at critical modules, not every commit |
| `cargo modules` | visualize structure and find orphan files | diagnostic only for dependency cycles; its current `--acyclic` run reports a false self-cycle on an inherent method |
| `rust-code-analysis` | exploratory cyclomatic/ABC/maintainability reports | optional trend data only, never a quality verdict |

Clippy explicitly documents that its `cognitive_complexity` lint has known
problems and should not be treated as a reliable measurement; it recommends
more concrete lints such as
[`too_many_lines` and `excessive_nesting`](https://rust-lang.github.io/rust-clippy/stable/index.html#cognitive_complexity).
The complete `pedantic` or `restriction` groups must not be enabled wholesale;
the [Clippy usage guide](https://doc.rust-lang.org/clippy/usage.html) warns that
they are opinionated and may conflict. Prefer selected lints with a local,
reasoned exception where needed.

For dependency policy, `cargo deny` covers
[licenses, bans, advisories, and sources](https://github.com/EmbarkStudios/cargo-deny),
while `cargo machete` is explicitly fast but approximate and documents its
[false-positive mechanism](https://github.com/bnjbvr/cargo-machete).
Coverage and mutation answer different questions: coverage shows executed
regions, while [cargo-mutants](https://mutants.rs/) checks whether tests notice
behavioral changes. Neither replaces contract-focused tests.

## Non-goals

- no framework-driven “clean architecture” rewrite;
- no crate per adapter or feature;
- no generic repository abstraction solely to satisfy a diagram;
- no public user documentation about internal source layout;
- no hard quality score whose movement cannot be tied to a concrete code
  improvement;
- no structural move mixed with feature behavior changes in the same commit.

## Review trigger

Revisit this decision after Phase 2, when a native Windows client begins, or
when another Rust client needs the daemon protocol—whichever happens first.
