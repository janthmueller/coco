---
type: Engineering Decision
title: Rust source architecture and code-health strategy
description: Records the measured source-layout problems, target module boundaries, crate-split criteria, and enforceable Rust hygiene checks for CoCo.
tags: [architecture, rust, modules, cargo, quality, dependencies, testing]
status: stable
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

The current `codex-coordinator` package already builds four crates: the `coco`
library plus the `coco`, `cocod`, and `coco-mcp` binary crates. Files such as
`src/codex.rs` may declare children stored at `src/codex/process.rs` and
`src/codex/websocket.rs`; no additional `Cargo.toml` is needed. This is the
modern file layout described by the
[Rust Book](https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html).
Cargo workspaces are intended for multiple packages managed together, not as
the default answer to large files; see the
[Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html).

## Measured baseline

Measured on 2026-09-05 at commit `b9fb561` plus this documentation work:

| Area | Finding |
| --- | --- |
| Rust size | 8,618 lines across library and binaries; approximately 6,584 production and 1,977 in-module test lines in the library |
| Largest files | `coordinator.rs` 1,714; `store.rs` 1,617; `codex.rs` 1,535; `git.rs` 845 lines |
| Long functions | Clippy flags `cli::run` (117), shared App Server spawn (127), workspace creation (115), and Codex notification projection (106) over its default 100-line threshold |
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

Phase 1 completed on 2026-09-05. `protocol.rs` now owns the closed daemon
method set and typed request/result contracts, the RPC client derives method
and result types from those requests, and `daemon/handler.rs` is the only
RPC-to-coordinator translation boundary. The coordinator no longer imports
RPC, and the CLI no longer imports the Codex adapter. The measured boundary
leaks above remain as the historical pre-refactor baseline.

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
    workspace.rs
    repository.rs
    event.rs

  coordinator.rs                 # Coordinator and stable application API
  coordinator/
    workspace.rs                 # register/create/list/status/diff use cases
    retirement.rs                # checked close/reopen/delete sagas and recovery
    retirement/
      safety.rs                  # path, descendant activity, and context dependency guards
      deletion.rs                # combined open/closed deletion saga and recovery
      deletion/
        plan.rs                  # resource ownership and loss/retention planning
    turn.rs                      # turn start and idempotency
    codex_events.rs              # App Server event projection
    error.rs
    tests.rs                     # shared Coordinator fake and fixture
    tests/
      workspace.rs              # creation, status, scope, and diff behavior
      retirement.rs             # close/reopen/delete policy and recovery behavior
      retirement_safety.rs      # worktree, runtime, TUI, and recovery safety
      retirement_confirmation.rs # acknowledged target and resource-plan identity
      retirement_dependencies.rs # prepared context references and deletion races
      retirement_deletion.rs     # direct deletion, retention, file and commit loss policy
      retirement_deletion_recovery.rs # failed preparation and interrupted open deletion
      context.rs                # fork, compact, attach, and activation behavior
      operations.rs             # turn-start idempotency and ambiguous dispatch
      decisions.rs              # approvals and structured-input behavior
      events.rs                 # native runtime projection behavior

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
    workspaces.rs
    operations.rs               # durable control-operation ledger
    events.rs
    tests.rs

  git.rs                         # Git adapter facade and public result types
  git/
    command.rs                   # bounded subprocess execution
    repository.rs
    worktree.rs
    retirement.rs                # checked removal, restoration, owned-branch deletion
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
    prompt.rs                    # interaction contract, selection state, and line input
    prompt/
      terminal.rs                # owned inline frame and terminal-mode cleanup
      tests.rs                   # input/layout tests and opt-in interactive probes
    tests.rs

  mcp.rs                         # keep until production code grows materially
  profile.rs
  paths.rs
  daemon.rs
  daemon/
    handler.rs                   # RPC-to-coordinator translation
    worker.rs                    # WorkerRuntime adapter around CodexClient
    execution.rs                 # lazy workspace executor lifecycle
    execution/
      containment.rs             # platform selection and systemd/cgroup ownership
      resources.rs               # platform-specific ephemeral observation

  bin/                           # thin executable entry points only
```

This is a direction, not a requirement to create every file immediately.
Children should be extracted only when they own a coherent responsibility;
empty taxonomy directories make navigation worse.

## Intentional public API

The package is published as `codex-coordinator` so Cargo can install the three
product binaries together. Its `coco` library exists primarily so those binary
crates can share code; publication does not make that deliberately narrow
library facade a promised general-purpose SDK. `lib.rs` should expose only
stable executable entry points and, where needed, protocol types used by future
clients. Everything else should prefer private or `pub(crate)` visibility.

This end state is now implemented: Rustdoc exposes only
`run_cli_from_env`, `run_daemon_from_env`, and `run_mcp_from_env`; all twelve
implementation modules are private. Integration tests exercise binaries
through `CARGO_BIN_EXE_*`, while in-crate unit tests use module privacy rather
than expanding the external library API for convenience.

## Refactor sequence

### Phase 0 — safety net and repeatable gates

Completed on 2026-09-05. The process test uses the built `cocod` and `coco`
binaries with isolated temporary state and a fake authenticated App Server; it
does not invoke a model.

1. Add a Rust GitHub Actions workflow for format, Clippy, and all tests using
   the pinned toolchain.
2. Add one process-level test harness with a fake App Server executable. Cover
   daemon startup, repository registration, workspace preparation, explicit send,
   status, and clean shutdown without a model call.
3. Forbid unsafe code in CoCo unless a later platform adapter has a reviewed,
   narrowly scoped exception.
4. Add `cargo machete` as a fast dependency-use gate. Introduce `cargo deny`
   only with an explicit source/license/advisory policy.

Exit: the current behavior has an automated refactor safety net; no source
movement is required yet.

### Phase 1 — typed daemon seam

Completed on 2026-09-05 without changing the versioned wire envelope or
user-facing command behavior.

1. Create the closed method enum and typed parameter/result DTOs.
2. Make RPC serialization generic over those types while retaining the
   versioned envelope.
3. Move dispatch and error mapping from `Coordinator` into
   `daemon/handler.rs`.
4. Make CLI and MCP construct the same typed protocol values.
5. Move the shared endpoint descriptor out of the Codex adapter so CLI no
   longer imports `codex`.

Exit: no coordinator import of `rpc`, no CLI import of `codex`, and contract
tests cover every method name and wire field. Achieved: CLI and MCP construct
the same request types, while daemon dispatch and stable error mapping live in
the dedicated handler.

### Phase 2 — split the hot modules

The first coordinator slice completed on 2026-09-05: its production facade is
about 150 lines, with workspace/repository commands, turn startup, Codex event
projection, error policy, and the worker port in focused child modules. The
concrete Codex-backed worker moved to `daemon/worker.rs`, so the coordinator's
worker contract no longer imports the Codex client. On 2026-09-06, the shared
Coordinator fixture and its behavior tests moved unchanged into
`coordinator/tests.rs`, leaving `coordinator.rs` as a production-only facade.
After that suite grew beyond 2,500 lines during the native-first cutover, its
shared fake worker and fixture stayed in `coordinator/tests.rs`, while behavior
cases moved without assertion changes into focused `tests/{workspace,context,
decisions,events}.rs` children on 2026-09-07. The later schema-v6 slice added
`tests/operations.rs` for dispatch/idempotency behavior without regrowing the
shared fixture.
The first Store slice also completed on 2026-09-06: schema creation and the
v1-to-v2 migration live in `store/migrations.rs`, while stable select lists and
all SQLite-row-to-domain decoding live in `store/rows.rs`. Transactional write
operations were then separated into `store/workspaces.rs` and `store/events.rs`
without weakening their atomic boundaries. Schema v6 adds
`store/operations.rs` as the narrow durable turn-start intent/dispatch/result
state machine; schema v7 extends workspace rows with the typed worktree mode.
Schema v8 adds durable open/closing/closed/reopening availability and
close-time binding evidence; schema v9 adds recoverable thread/branch deletion
intent. Legacy turn helpers remain test-only for migration coverage.
Cross-module Store tests live in `store/tests.rs`; `store.rs` is now the
connection, repository, shared-type, filesystem-safety, and facade layer.
The Codex adapter split completed next: `codex/process.rs` owns child startup,
initialization, stderr capture, and termination; `codex/jsonl.rs` owns framing,
request correlation, and server-event dispatch; and `codex/websocket.rs` owns
the authenticated shared transport and private runtime files. The public
client state and close contract remain in the roughly 240-line `codex.rs`
facade, while its unchanged transport tests live in `codex/tests.rs`.
The Git adapter followed on 2026-09-06. `git/command.rs` is now the only place
that spawns Git and bounds stdout/stderr; repository identity, worktree
lifecycle, explicit local-state carry, and diff/observation policy live in
their corresponding child modules. `git.rs` retains the error and shared data
types plus adapter construction. Worktree-mode and local-state cases now live
under focused `git/tests/` children while the shared native-Git fixture
remains in `git/tests.rs`.
The final physical split completed with the CLI: the facade now only parses
and delegates, while Clap arguments, typed command execution, the status
follow-loop, authenticated TUI jump, output rendering, and tests have focused
child modules. The later terminal-input slice keeps reusable TTY detection,
raw-mode restoration, picker state, and line-input behavior in
`cli/prompt.rs`; commands and native-decision presentation depend on that one
adapter instead of implementing separate input loops. The original `cli::run`
size finding is gone. A final cleanup
extracted shared App Server process startup, prepared-workspace persistence, and
terminal-turn projection, removing every production `too_many_lines` finding.
Both `too_many_lines` and the separately reviewed `excessive_nesting` lint are
now denied package-wide. The process harness later grew beyond 2,000 lines and
was split into lifecycle/fork scenarios, fake-App-Server behavior, and process
support; the long lifecycle scenario retains one local, reasoned function-size
exception. The oversized protocol test was split by assertion responsibility
instead.

Extract coherent child modules in this order:

1. coordinator commands and Codex event projection;
2. store migrations, row mapping, and workspace/event repositories;
3. Codex process, JSONL, and WebSocket layers;
4. Git command runner, worktree operations, and diff observation;
5. CLI argument parsing, commands, and output.

Move tests with the responsibility they cover. Reduce the four currently
flagged functions below 100 lines, then enable `clippy::too_many_lines` as a
project lint. Review `excessive_nesting` separately; do not enable the complete
Clippy restriction group.

Exit achieved on 2026-09-06: parent modules are readable facades, boundaries
match the dependency rules, behavior is unchanged, and the selected structural
lints are enforced.

### Post-Phase-2 review

The required review trigger ran on 2026-09-06 after commit `567140a`.

- The tree contains 10,093 Rust lines across production, unit tests, binaries,
  and the process harness. Growth from the historical baseline includes the
  shipped CLI/App Server slice and more boundary tests, not a return to flat
  hot modules.
- The formerly hot parents are now small facades: Coordinator about 150 lines,
  Store about 300, Codex about 230, Git about 100, and CLI under 20. Larger
  child files own one responsibility, and the enforced function/nesting lints
  provide a more actionable guard than raw file length.
- A direct import audit leaves the top-level graph acyclic and preserves the
  staged dependency direction. Codex notification projection remains the
  deliberate App Server edge described by the target layout; local daemon
  envelopes do not leak into Coordinator use cases.
- Cargo metadata still reports one package with one library and three binary
  crates. Nothing is independently released or consumed, and no measured
  dependency/build problem calls for crate isolation. The workspace split is
  therefore rejected again.
- Hiding the implementation modules exposed fourteen previously masked
  dead-code findings. Unused accessors/builders were removed, test fixtures
  became test-only, and the explicit Codex response seam now carries the
  generation-bound decision workflow. The normal warning-denied build is
  clean.
- Generated library documentation contains exactly the three executable entry
  points and no internal module API. A future native client remains the first
  likely reason to extract `coco-protocol` as a separate package.

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
| selected `too_many_lines` and `excessive_nesting` lints | actionable structural pressure | denied package-wide; exceptions must be local and reasoned |
| `cargo test --all-targets` | behavior and contract safety | required on every change |
| `cargo machete` | fast unused direct-dependency detection | required; document justified false positives |
| `cargo deny` | advisories, sources, licenses, and duplicate/banned crates | required; the reviewed root policy denies advisories, unknown sources, unapproved licenses, and wildcard requirements while reporting duplicate versions |
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

The post-Phase-2 review reaffirmed this decision. Revisit it next when native
Windows work begins, another Rust client needs the daemon protocol, or a
measured build/distribution cost demonstrates a real crate boundary need.
