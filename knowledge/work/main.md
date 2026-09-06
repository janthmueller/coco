---
type: Working Document
title: "main: repository foundation and first architecture"
description: Tracks repository bootstrap, the Rust baseline, and early CoCo architecture decisions on main.
tags: [work, branch, bootstrap, rust, mcp, architecture]
status: active
branch: main
updated: 2026-09-06
---

# main — repository foundation

## Intended outcome

Establish the repository, documentation workflow, and first executable and
architectural baseline for CoCo.

## Active work

- [x] Initialize the repository on `main`.
- [x] Review Wuf's public/internal documentation boundary and agent guidance.
- [x] Establish a required working document for every branch and task.
- [x] Inspect Orca's public documentation and record the intended site stack
  and visual qualities.
- [x] Read and classify the CoCo product handoff.
- [x] Switch the implementation baseline from TypeScript to Rust at the user's
  direction.
- [x] Establish the initial Rust domain, persistence, Git, RPC, Codex client,
  profile, CLI, and control-MCP modules.
- [x] Evaluate per-thread worker MCP selection and Agentgateway.
- [x] Document CoCo-owned MCP registry/bindings, native Codex as the initial
  runtime, and Agentgateway as a deferred optional adapter.
- [x] Record static export and GitHub Pages as mandatory constraints for the
  eventual user-facing documentation.
- [x] Complete the first Rust vertical slice through repository registration,
  worktree creation, and Codex thread/turn startup.
- [x] Scaffold the public Next.js/Fumadocs site and its initial user-facing
  guides, concepts, reference, and integration pages.
- [x] Verify the complete static export at both `/` and the GitHub Pages
  project subpath, including browser-side search and the public-only boundary.
- [x] Add Nix apps and development-shell tooling for installing, checking,
  building, serving, and developing the documentation site.
- [x] Record the planned detached-worktree creation mode without presenting it
  as current behavior.
- [x] Replace the first public site draft: it exposed internal architecture and
  roadmap material and did not meet the strict user-facing boundary.
- [x] Separate task preparation from execution: `coco new` must not require or
  send an instruction, while `coco send` starts the first and later turns.
- [x] Replace the overlapping `show`/`watch` UX with a concise one-shot
  `coco status` and an explicit live `coco status --follow` mode.
- [x] Add `coco jump` so an operator can open the existing Codex thread in its
  managed worktree through the same App Server observed by `cocod`.
- [x] Replace the temporary Unix-only shared App Server listener with an
  authenticated loopback-WebSocket endpoint that `coco jump` can use on Linux,
  macOS, and Windows.
- [ ] Keep CoCo's CLI-to-daemon transport behind a local-IPC boundary: Unix
  sockets on Linux/macOS and a Windows named-pipe backend, without duplicating
  protocol or coordinator logic.
- [x] Create safe checkpoint commits as the foundation and subsequent vertical
  slices become independently buildable.
- [x] After the current App Server and CLI UX slice, research and propose a
  maintainable Rust module/workspace architecture. Evaluate when large modules
  should become submodules or separate crates, and select enforceable hygiene
  checks for code complexity, dependency health, layering, and dead code before
  performing any structural refactor.
- [x] Measure the current Rust baseline: module and function size, public
  surface, top-level dependencies, test placement, dependency use/duplication,
  and missing CI coverage.
- [x] Record the staged target and tool policy in
  `knowledge/engineering/rust-architecture.md` and route future agents to it.
- [x] Implement Phase 0 of the Rust architecture plan as a behavior-preserving
  safety-net commit before moving modules.
- [x] Implement the typed daemon seam and remove coordinator-to-RPC and
  CLI-to-Codex dependency leaks.
- [x] Split the measured hot modules without mixing in product behavior.
  - [x] Coordinator responsibilities and tests.
  - [x] Store migrations, rows, transactions, events, and tests.
  - [x] Codex process, JSONL, WebSocket, and tests.
  - [x] Git command, repository, worktree, diff, and tests.
  - [x] CLI arguments, commands, output, and tests.
- [x] Reduce the measured production function-size findings and enforce the
  selected `too_many_lines` plus `excessive_nesting` Clippy gates.
- [ ] Design task annotations and external references as a deliberate future
  feature. Decide typed versus free-form values, mutation/audit semantics,
  privacy and display rules, fork/handoff inheritance, and explicit projection
  into Codex before adding any CLI or RPC field.
- [ ] Design user-configurable lifecycle hooks as a separate future feature.
  Before defining CoCo hooks, inventory the pinned Codex CLI and App Server's
  native hooks, notifications, and lifecycle events so CoCo can expose or
  extend existing signals instead of duplicating them. Cover task creation,
  thread/turn start, agent state transitions and terminal outcomes, then decide
  execution context, filtering, ordering, retries, timeouts, failure policy,
  secret handling, auditability, and platform behavior.
- [ ] Revisit `coco jump` detach semantics. Verify with the pinned Codex TUI
  whether closing a remote-attached UI cancels an active turn, and whether
  Codex already exposes a detach action or configurable keybinding. If CoCo
  needs its own UX, define an explicit shortcut or command that disconnects
  the TUI while the daemon-owned turn continues, together with clear cancel,
  reattach, signal, and accidental-exit behavior.

## Decisions

- 2026-09-05 — Use a progressively disclosed `knowledge/` bundle for durable
  maintainer context, keeping public documentation focused on users.
- 2026-09-05 — Mirror full branch names below `knowledge/work/` so every task
  record is unambiguous and parallel branches normally touch different files.
- 2026-09-05 — Use Next.js, Fumadocs, MDX, and Tailwind for the eventual public
  site. This follows the structure and visual restraint of Orca's docs rather
  than Wuf's Astro Starlight implementation.
- 2026-09-05 — Supersede Orca as the visual reference with nuqs' open-source
  Fumadocs package: use its monochrome notebook layout, top navigation,
  centered search, flat sidebar, reading width, and typographic rhythm while
  retaining CoCo's branding and content.
- 2026-09-05 — Defer the actual site scaffold, package versions, navigation,
  and branding to a separately scheduled documentation milestone after the
  first public behavior is stable.
- 2026-09-05 — Use a single Rust crate with Tokio instead of the initial
  TypeScript/Node direction.
- 2026-09-05 — Keep worker execution profiles and MCP capability profiles as
  separate immutable snapshot dimensions, with optional presets joining them
  for user-facing convenience.
- 2026-09-05 — CoCo owns the future MCP catalog and per-thread bindings. The
  first runtime projection uses native Codex MCP configuration; CoCo will not
  build a custom MCP proxy.
- 2026-09-05 — Preserve an adapter boundary for Agentgateway, but do not add it
  as a dependency or roadmap item yet.
- 2026-09-05 — The Fumadocs application must use Next.js static export and be
  deployed through GitHub Actions to GitHub Pages. It may not require a Node.js
  server at runtime.
- 2026-09-05 — Spell repository registration `coco repo add [path]`; the nested
  namespace leaves room for later `repo list`, `repo show`, and `repo remove`
  commands without an ambiguous top-level `init`.
- 2026-09-05 — Resolve a named execution profile from
  `[profiles.<name>]` in `$CODEX_HOME/config.toml` and project that overlay into
  `thread/start.config`. The full overlay stays in memory; SQLite receives only
  its identity/hash and the App Server's allowlisted effective settings.
- 2026-09-05 — Use cursor-based `event.list` polling for the first `watch`
  implementation. Live subscription remains a compatible later transport
  optimization rather than a prerequisite for durable observation.
- 2026-09-05 — Position CoCo concretely as the coordinator of Codex tasks and
  their Git working areas. Codex App Server owns threads and turns in a caller-
  supplied `cwd`; CoCo owns repository registration, worktree lifecycle, the
  durable task binding, and client-independent orchestration.
- 2026-09-05 — Make the public/internal documentation boundary an explicit
  acceptance gate. Every rendered page must help a user evaluate, operate, or
  troubleshoot shipped behavior; architecture, rationale, roadmaps, gateway
  evaluation, and agent work remain exclusively under `knowledge/`.
- 2026-09-05 — Use one local Codex App Server listener over a private Unix
  socket for both `cocod` and interactive `coco jump` clients. A standalone
  `codex resume` process would bypass CoCo's event projection; sharing the App
  Server lets the TUI own interaction while the daemon remains subscribed to
  thread and turn notifications. Pin and contract-test the Codex version
  because the remote transport is currently documented as experimental.
- 2026-09-05 — Supersede the Unix-only App Server endpoint with an
  authenticated loopback WebSocket. Codex CLI supports the same `--remote`
  endpoint on native Windows, Linux, and macOS, whereas Codex does not expose a
  Windows named-pipe listener. Keep the capability token in private local
  runtime state and pass it to interactive clients through an environment
  variable rather than command-line arguments.
- 2026-09-05 — Do not force one transport across unrelated boundaries. CoCo's
  own privileged local RPC should use OS-native local IPC through one abstract
  interface (Unix sockets or Windows named pipes); the shared Codex App Server
  uses its cross-platform loopback-WebSocket surface because Codex TUI must
  connect to it directly.
- 2026-09-05 — Make preparation explicit: `new` creates the worktree and Codex
  thread and returns it `idle`; it accepts no instruction or open-ended
  metadata field. `send` is the only non-interactive command that starts the
  first or a later turn.
- 2026-09-05 — Remove the prerelease `goal` field instead of renaming it to a
  generic `metadata` bag. Preserve old local values only in hidden
  compatibility storage and design annotations/references separately before
  exposing them through CLI, RPC, or prompts.
- 2026-09-05 — Keep one Cargo package through the next refactor. Rust child
  modules provide package-like source organization without premature crate
  APIs; reconsider a workspace only for independent consumers, releases,
  dependency isolation, or recurring boundaries that visibility cannot enforce.
- 2026-09-05 — Make a typed local daemon protocol the first structural seam.
  Coordinator use cases must stop parsing RPC JSON, CLI must stop importing a
  Codex adapter DTO, and file splitting follows those dependency corrections.
- 2026-09-05 — Use concrete lint/dependency gates rather than aggregate
  complexity scores. `cargo machete` currently passes; Clippy identifies four
  functions over 100 lines and no findings for the separately enabled nesting,
  argument-count, or type-complexity lints.
- 2026-09-05 — Replace the overlapping `show` and `watch` commands with
  `status` and `status --follow`. `jump` launches the official Codex TUI in the
  recorded worktree and resumes the recorded thread through the daemon-owned
  App Server rather than creating a parallel conversation.
- 2026-09-05 — Make `protocol.rs` authoritative for the closed daemon method
  set and typed request/result pairs. Keep serialization and stable RPC error
  mapping in `daemon/handler.rs`; coordinator use cases receive and return
  domain-aware values, while CLI and MCP share the same protocol DTOs.
- 2026-09-05 — Use the Git-aware Nix reference `.` for repository commands and
  run heavy Nix/Cargo checks sequentially. `path:.` copied the ignored
  multi-gigabyte `target/` tree and parallel invocations created avoidable CPU
  and I/O pressure.
- 2026-09-05 — Keep `coordinator.rs` as the application facade and move its
  task commands, turn startup, Codex event projection, errors, and worker port
  into responsibility-based child modules. Put the concrete Codex worker under
  the daemon composition root so the worker contract no longer depends on the
  Codex client implementation.
- 2026-09-06 — Keep the Coordinator's shared fake worker and cross-use-case
  behavior tests together in `coordinator/tests.rs`. Their value is testing
  orchestration across task, turn, event, Git, and store boundaries; splitting
  individual cases across production modules would duplicate the fixture.
- 2026-09-06 — Split Store code along persistence responsibilities rather than
  SQL statement size. Schema creation/upgrades belong to `store/migrations.rs`;
  stable select lists and fallible SQLite-row decoding belong to
  `store/rows.rs`. Keep multi-record task/turn/event writes together until the
  next slice can preserve their existing transaction scope explicitly.
- 2026-09-06 — Keep Task/Turn state writes in `store/tasks.rs` and Event/Audit
  persistence in `store/events.rs`, but share the caller-owned SQLite
  transaction for state-plus-event operations. Centralize read lookups in
  `store/rows.rs` so the sibling modules do not depend on each other in both
  directions. Keep the cross-module atomicity/recovery fixture in
  `store/tests.rs`.
- 2026-09-06 — Keep the Codex client as one public facade while separating
  transport responsibilities underneath it: process startup and monitoring,
  JSONL framing and correlation, and authenticated WebSocket/runtime-file
  handling. Keep cross-transport contract tests together in `codex/tests.rs`
  because they exercise the same client state machine through both transports.
- 2026-09-06 — Keep Git subprocess hardening in one internal command runner,
  then separate repository identity, worktree mutation, and diff observation
  as policy modules. Re-export the existing task-name validator and retain all
  public methods on `Git`, so callers do not inherit the internal layout.
- 2026-09-06 — Keep `cli.rs` as the public parse-and-delegate entry point.
  Separate Clap definitions, typed command handlers, status following, TUI
  jump setup, output formatting, and contract tests without changing command
  names, JSON schema, human output, or process behavior.
- 2026-09-06 — Deny `clippy::too_many_lines` and
  `clippy::excessive_nesting` package-wide after removing every production
  finding. Keep one local, reasoned exception for the single process-level
  lifecycle scenario; split tests that contain separable assertions instead.

## Findings

- Wuf separates its concise README and user site from an internal OKF knowledge
  bundle, with `AGENTS.md` acting as a short router into that material.
- The relevant Orca reference is `stablyai/orca`. Its public docs render with
  Next.js and Fumadocs conventions and use MDX-oriented, grouped navigation.
- The installed `codex-cli 0.147.0` App Server does not accept CLI
  `--profile`; CoCo must resolve named profiles into per-thread configuration
  overlays when sharing one App Server process.
- Locally generated App Server schemas expose `config` on thread start,
  resume, and fork. Runtime isolation of different MCP selections still needs
  a pinned-version contract test before implementation depends on it.
- Agentgateway can federate MCP targets and filter tools by tool/target and
  authenticated claims, but strict per-thread policy on one endpoint requires
  a thread-distinguishing identity.
- Fumadocs documents a static search/build path, while Next.js emits the
  deployable `out/` tree with `output: "export"`. A GitHub project Pages site
  also requires build-time subpath handling rather than root-relative
  assumptions.
- The official Codex SDK/App Server surface starts, resumes, and forks thread
  history in a caller-supplied `cwd`; the documented managed-worktree creation,
  handoff, snapshot, and cleanup behavior belongs to the desktop application.
- The initial Fumadocs draft violated the repository's existing audience
  boundary by exposing architecture and future-integration material. A clean
  static build did not make that content suitable for users; the public
  information architecture and link verification both require correction.
- Native Codex CLI now supports Windows directly. The present CoCo executable
  remains Unix-only because its RPC listener is implemented with Tokio Unix
  sockets; the daemon concept itself is not platform-specific.
- The installed Codex CLI accepts a capability-token-protected loopback
  WebSocket and can initialize successfully with an isolated temporary Codex
  home. This proves the shared transport without consuming a model turn.
- The executable coordinator now serializes creation and turn-start operations
  per repository, resolves client paths back to a registered Git common
  directory, and treats Git/Codex effects as persisted saga steps.
- App Server requests are surfaced as redacted durable events and are never
  auto-approved. The approval response workflow remains intentionally open.
- Rust modules can use nested directories without becoming separate Cargo
  packages. CoCo currently has one package, one library crate, and three binary
  crates; the flat module tree is normal but its largest files now mix enough
  responsibilities to justify child modules.
- The top-level module graph is acyclic, but coordinator currently depends on
  RPC and CLI depends on a Codex adapter type. These are the first seams to
  correct before physical source movement.
- The pinned Nixpkgs input contains the selected dependency/analysis tools.
  `cargo-modules --acyclic` currently produces a false self-cycle for an
  inherent method, so its graph is diagnostic rather than a CI architecture
  gate.
- The Phase 0 process harness can fake the Codex executable without mocking
  CoCo internals: the fake executable exposes the daemon-selected WebSocket
  address, while the test owns the authenticated App Server peer and controls
  exactly when a turn completes.
- The Phase 1 seam removes both measured dependency leaks: Coordinator has no
  RPC imports, CLI has no Codex imports, daemon method strings are centralized,
  and MCP/CLI calls receive typed results instead of traversing arbitrary JSON.
- After the first Phase 2 extraction, `coordinator.rs` contains roughly 150
  production lines; the extracted task and Codex event modules are each about
  320 lines, and the turn, error, and worker-port modules are smaller focused
  units. Its shared integration-style unit fixture now lives separately in
  `coordinator/tests.rs`.
- `store.rs` fell from roughly 1,617 to 1,339 lines after extracting 155 lines
  of migration policy and 145 lines of row decoding. No transaction was moved
  or split in this first persistence step.
- The completed Store split leaves a roughly 315-line facade, 483-line
  Task/Turn transaction module, 157-line Event/Audit module, 224-line query and
  row-mapping module, 155-line migration module, and 333-line shared test
  module. The parent no longer mixes schema SQL, row decoding, lifecycle
  transactions, event persistence, and tests.
- The completed Codex split leaves a 238-line facade, 437-line JSONL state
  machine, 341-line process lifecycle module, 270-line authenticated WebSocket
  adapter, and 287-line shared test module. Public methods remain on
  `CodexClient`; the child modules are private implementation boundaries.
- The completed Git split leaves a 109-line facade, 180-line bounded command
  runner, 111-line repository module, 212-line worktree module, 143-line diff
  observer, and 135-line native-Git test fixture. Only the command module
  touches `std::process::Command` in production; all child modules remain
  private.
- The completed CLI split leaves a 17-line facade, 90-line argument module,
  155-line typed command module, 94-line status follower, 102-line jump module,
  151-line output module, and 109-line test module. A targeted Clippy
  measurement confirmed the old 117-line dispatcher was gone and identified
  the final three production functions plus two tests for focused cleanup.
- Extracting shared-process startup, prepared-task persistence, and terminal
  turn completion removed the final production `too_many_lines` findings. The
  protocol test naturally split into wire-field and defaulting assertions; the
  process smoke test remains one ordered cross-process scenario and carries
  the only local exception. `excessive_nesting` has no current findings.

## Verification

- `git rev-parse --is-inside-work-tree` returned `true`.
- `git branch --show-current` returned `main`.
- All expected local documentation targets exist.
- Every non-index knowledge document has the required frontmatter and a
  non-empty `type`.
- The MCP and Rust architecture documents are linked from the engineering
  index and routed from `AGENTS.md`; targeted trailing-whitespace checks are
  clean.
- `cargo test --locked --all-targets` passes all 44 library tests plus the
  process integration test, including
  native Git worktrees, legacy-goal retirement, task preparation without a
  turn, externally started TUI turns, authenticated WebSocket bridging,
  private capability files, daemon singleton locking, exact `jump` invocation,
  SQLite recovery, RPC sockets, MCP delegation, event normalization, and
  failure retention.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo machete`, Nix formatting, and
  `actionlint` pass in the pinned development environment.
- `nix flake check . --no-write-lock-file` passes and validates every app plus
  the Rust workflow. Git and the Unix signal utility are explicit test-app
  runtime inputs.
- The automated process test starts the built `cocod`, authenticates it to a
  fake IPv4-loopback App Server, verifies private runtime/state files, runs the
  built CLI through `repo add`, `new`, `ls`, `send`, and `status`, observes
  `idle -> active -> idle`, validates the emitted Codex requests, and confirms
  endpoint/token/socket cleanup after SIGINT. It performs no model call.
- After the typed daemon refactor, a resource-limited sequential run passes 48
  library tests and the process test; Clippy with warnings denied and
  `cargo machete` are also clean.
- After splitting the Coordinator production responsibilities, the same 48
  library tests plus the process test pass with one Cargo build job and one
  test thread; the all-target/all-feature Clippy run remains warning-free.
- Moving the unchanged Coordinator test body to `coordinator/tests.rs` keeps
  all 48 library tests and the process test green; an old/new content diff
  contains only two rustfmt line-wrap changes, and Clippy remains clean.
- After extracting Store migrations and row mapping, all 48 library tests and
  the process test pass sequentially; Clippy with warnings denied and
  `cargo machete` remain clean.
- After completing the Store transaction/event split and moving its tests, all
  48 library tests plus the process test pass again with one build job and one
  test thread; the all-target/all-feature Clippy gate is warning-free.
- After separating the Codex process, JSONL, WebSocket, and test layers, all 48
  library tests plus the process test pass with one build job and one test
  thread; all-target Clippy with warnings denied and `cargo machete` remain
  clean. The resource-limited `nix flake check . --no-write-lock-file` run also
  passes against the staged Git source.
- After separating Git command execution, repository identity, worktree
  mutation, diff observation, and tests, all 48 library tests plus the process
  test pass with one build job and one test thread; the all-target/all-feature
  Clippy and `cargo machete` gates remain clean. The resource-limited
  `nix flake check . --no-write-lock-file` run also passes against the staged
  Git source.
- After separating CLI parsing, command execution, status, jump, output, and
  tests, all five CLI tests, all 48 library tests, and the process test pass
  with one build job and one test thread; the normal all-target/all-feature
  Clippy gate and `cargo machete` remain clean. The resource-limited
  `nix flake check . --no-write-lock-file` run also passes against the staged
  Git source.
- With both selected structural lints denied, all 49 library tests and the
  process smoke test pass sequentially; all-target/all-feature Clippy with
  warnings denied and `cargo machete` are clean. The resource-limited
  `nix flake check . --no-write-lock-file` run builds its tooling check and
  passes against the staged Git source.
- The documentation TypeScript, Oxlint, and Prettier checks pass. Both the
  root and `/coco` builds export 88 static files across eight pages with static
  search, valid local links, no server artifact, and no internal knowledge.

## Open questions and handoff

- Prove native Codex per-thread MCP isolation across start, resume, and fork
  before scheduling the registry feature.
- Agentgateway is deliberately not scheduled. Reconsider it only when
  federation, centralized credential custody, independent enforcement, or
  gateway observability becomes an actual requirement.
- Add a safe approval-response command and durable pending-request model before
  calling v0 generally usable.
- Add daemon recovery through `thread/resume`; the first slice conservatively
  marks in-flight work interrupted after daemon loss.
- Research Codex's native lifecycle extensibility before designing CoCo hooks.
  Keep the distinction between internal normalized events and executable user
  automation explicit; hooks must not silently inherit credentials or block
  coordinator state transitions without a deliberate policy.
- Investigate detached `jump` behavior with the pinned Codex TUI before
  changing its process handling. A detached UI must not imply a cancelled
  worker turn, and an ordinary cancel must remain unambiguous and observable.
- When the public site is scheduled, validate its production export under the
  GitHub Pages project subpath before enabling deployment from `main`.
- Phase 2 is complete. Perform its architecture review trigger before starting
  platform transport work: remeasure module boundaries and public visibility,
  confirm the one-package decision, then decide whether Phase 3 (Unix socket
  backend plus Windows named pipes) or a product capability should be next.
