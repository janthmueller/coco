---
type: Knowledge Bundle Log
title: Project knowledge update log
description: Records significant structural and semantic updates to the project knowledge bundle.
tags: [knowledge, maintenance]
status: stable
---

# Project knowledge update log

## 2026-09-06

- **Codex named-profile contract corrected**: Aligned execution profiles with
  Codex 0.147.0: `default` sends an empty per-thread overlay over the App
  Server's base configuration, while another name loads the complete
  `$CODEX_HOME/<name>.config.toml` document. CoCo keeps the overlay in memory,
  persists only named-file provenance and redacted effective settings, and
  rejects profile drift during daemon recovery.
- **Foreground daemon alpha policy**: Kept package installation separate from
  process activation. Alpha users start `cocod` explicitly in the foreground;
  an opt-in cross-platform user service remains gated on graceful SIGTERM and
  defined, tested App Server child-failure and recovery semantics.
- **Installable Nix flake**: Added a version-synchronized default Nix package
  containing all three executables, with `coco` as the default app and named
  daemon/control-MCP apps. The public installation guide now supports the
  flake while retaining Cargo Git and local-checkout paths; a registry command
  remains intentionally absent until the first irreversible publication exists.
- **Guarded semantic alpha releases**: Ported Wuf's tested-main release pattern
  to CoCo with Conventional Commit versioning, generated changelog and release
  commit, synchronized Cargo metadata, immutable action pins, a reviewed
  advisory/license/source policy, and native Linux and macOS archives for all
  three executables. The crates.io package is named
  `codex-coordinator`, while the product, library, and command remain CoCo and
  `coco`; its minimal package boundary excludes internal knowledge and site
  sources. Binary and registry dry-run smoke builds now belong to the complete
  Rust workflow. Manual dispatch rehearses the release build unless an
  operator explicitly chooses publication, and automatic publication remains
  gated by `COCO_RELEASE_ENABLED` until the first-release documentation is
  settled.
- **Native model discovery and selection**: Added daemon-backed `coco models`
  with human and versioned JSON output, using the Codex App Server's complete
  visible `model/list` catalog. `coco create --model`/`-m` now carries an
  explicit model separately from the named profile overlay through fresh
  start, native fork, idempotency, persistence, and recovery. Codex remains
  authoritative for configuration precedence; CoCo records only the requested
  override and Codex-reported non-secret effective settings.
- **Native workspace fork delivered**: Added same-repository
  `coco create --fork-from <workspace> [--compact]`. The destination derives
  from an idle, clean source workspace's committed `HEAD` and native Codex
  history; compaction applies explicitly to the child and finishes before an
  initial send or jump. Immutable provenance, destination binding context,
  failure retention, exact App Server calls, and static user docs are covered.
  Compaction remains fork provenance rather than a fourth context mode, while
  handoff stays deferred until its plan and artifact/reference model is
  designed.
- **Handoff design kept open**: Separated producing transfer material from
  attaching and consuming it. A future handoff may be agent-authored, supplied
  as Markdown or CLI input, or incorporate an external ticket/reference and
  optional plan; it is not fixed to one automatic summary prompt. Recorded
  that an ephemeral thread fork sharing `cwd` does not isolate filesystem
  writes, so safe generation must remain read-only and persist returned text
  outside the source checkout, or use a separate throwaway worktree.
- **Interactive decision closure**: Added SQLite schema v5 and the global
  `coco decide <decision-id>` flow for native command approvals, file-change
  approvals, and structured user input. Decisions retain exact private
  generation/thread/request correlation, expose only bounded presentation
  data, transition atomically through pending/submitted/resolved or orphaned,
  and are never auto-approved or replayed after process loss. Secret answers
  use a cross-platform no-echo prompt and are never stored; unsupported future
  request shapes fail closed instead of delegating to a late `jump` client.
- **Native Git approval proof**: Added and executed a model-consuming,
  explicitly opted-in Codex 0.147.0 test. Under the deterministic `untrusted`
  policy it accepts only one fully validated temporary file/add/commit command,
  observes native request resolution and terminal events, and proves that only
  the linked workspace branch advances. `on-request` remains intentionally
  model-discretionary after a sandbox denial.
- **Context-transfer modes reaffirmed**: Consolidated the original `fresh`,
  `fork`, and `handoff` plan instead of adding another metadata concept.
  `fork` will derive full history through native `thread/fork`; `handoff` will
  start fresh from a bounded, reviewable transfer artifact; neither mode
  copies uncommitted code implicitly.
- **Workspace vocabulary and creation UX**: Selected `workspace` as CoCo's
  durable aggregate around a repository binding, Git worktree, Codex thread,
  and configuration snapshot; external tickets remain optional references.
  Implemented the lossless `task`-to-`workspace` SQLite v4 migration, replaced
  `new` with `create`, and shipped composable `--send`/`-s` and `--jump`/`-j`
  post-actions with create-send-jump ordering and non-destructive partial
  failure semantics.
- **Multi-repository CLI contract**: Kept workspace names repository-scoped and
  opaque workspace IDs globally unique. Repository-aware commands default to
  `.`, accept an explicit leading path, and use `--all-repos`/`-a` for an
  intentional daemon-wide list or unique name lookup. `repo list`, global ID
  resolution, bounded local-miss suggestions, and deterministic ambiguity
  reporting are implemented; the CLI keeps no hidden selected-repository
  state.
- **Conventional workspace names**: Selected safe slash-separated names such as
  `feat/login`, mapping to `coco/feat/login`, with component-level path/ref
  validation, secure nested worktree directories, and explicit Git ref-prefix
  collision handling now enforced before worktree creation.
- **Native Git write policy**: Closed the shared-Git decision in favor of
  ordinary linked worktrees and Codex's native approvals. CoCo will not build a
  per-task Git database or commit proxy, nor grant the whole common Git
  directory as an unconditional writable root; the pinned opt-in
  approval/commit proof now passes.
- **Pinned Codex compatibility**: Added an explicit opt-in process smoke test
  for Codex 0.147.0 covering generated schemas, authenticated startup,
  model-free persistent thread preparation, and resume through a fresh App
  Server. The test exposed and removed an unnegotiated experimental field and
  established `thread/name/set` as the empty-thread durability step.
- **Daemon thread recovery**: A fresh `cocod` now resumes each persisted
  `ready` Codex thread with its verified ID, worktree, and unchanged in-memory
  profile overlay. Successful responses refresh native status; profile drift
  or one resume failure leaves only that task unavailable, while unfinished
  turns remain truthfully interrupted.
- **TUI detach contract**: Locked down `coco jump` as an attachment to the
  existing Codex thread: both `/quit`/`/exit` and abrupt remote-client loss
  leave active work running under daemon observation, while explicit Codex
  interruption remains the separate cancellation action.
- **Runtime-state ownership**: Migrated SQLite to schema v3, separated the
  CoCo task lifecycle from exact generation-aware Codex thread status and turn
  state, made public phase/wait reasons read-time projections, and stopped
  treating server-request method names as state transitions.
- **Post-Phase-2 architecture review**: Reaffirmed the single-package design,
  hid twelve implementation modules behind three executable entry points, and
  used the narrower facade to remove previously masked dead code.
- **Structural lint gates**: Removed the remaining production function-size
  findings and now deny Clippy's `too_many_lines` plus `excessive_nesting`
  lints, retaining one explicit exception for the ordered process smoke test.
- **CLI module boundary**: Reduced the public CLI facade to parsing and
  delegation, and separated Clap arguments, typed command handlers, status
  following, authenticated TUI jump, output rendering, and contract tests.
- **Git adapter layers**: Centralized bounded, environment-hardened Git process
  execution and split repository identity, worktree lifecycle, diff
  observation, and native-Git tests out of the public adapter facade.
- **Codex adapter layers**: Split App Server child lifecycle, JSONL framing and
  request correlation, authenticated shared WebSocket transport, and adapter
  tests out of the public client facade without changing its API or behavior.
- **Store transaction modules**: Split Task/Turn lifecycle operations from
  Event/Audit persistence, placed shared read lookups with row decoding, and
  moved cross-module persistence tests out of the facade while preserving the
  exact SQLite transaction used for atomic state-plus-event changes.
- **Store migration and row boundaries**: Extracted schema upgrades and legacy
  migration policy into `store/migrations.rs`, and centralized stable select
  lists plus SQLite row decoding in `store/rows.rs`; transactional writes and
  public behavior remain unchanged.
- **Coordinator test boundary**: Moved the shared fake worker, fixture, and
  cross-use-case orchestration tests out of the production facade and into
  `coordinator/tests.rs` without changing assertions or behavior.

## 2026-09-05

- **Coordinator module boundary**: Reduced the Coordinator production facade
  to composition and shared invariants, extracted task commands, turn startup,
  Codex event projection, errors, and the worker port, and moved the concrete
  Codex-backed worker into the daemon adapter layer without changing behavior.
- **Typed daemon seam**: Centralized the closed daemon method set and typed
  request/result contracts, made the RPC client infer wire methods and response
  types, moved dispatch/error translation into a daemon handler, and removed
  coordinator-to-RPC plus CLI-to-Codex dependency leaks before splitting hot
  modules.
- **Architecture refactor safety net**: Added pinned Rust CI, forbade unsafe
  application code, made unused-dependency checks reproducible, and added a
  binary-level daemon/CLI test that exercises task preparation and a complete
  fake App Server turn before any source modules are moved.
- **Rust architecture and hygiene plan**: Audited the first vertical slice,
  kept one Cargo package, selected nested modules plus a typed daemon seam as
  the next refactor, defined objective crate-split triggers, and classified
  Clippy, dependency, coverage, mutation, and structural-analysis tools by
  whether they should gate changes or remain diagnostic.
- **Prepared-task and interactive CLI flow**: Split task preparation from
  execution so `coco new` creates an idle worktree/thread without an implicit
  instruction, `coco send` starts the first or later turn, `coco status`
  replaces the overlapping show/watch commands, and `coco jump` opens the same
  thread and worktree in the official Codex TUI. The daemon-owned App Server is
  now shared through a capability-token-protected IPv4-loopback WebSocket;
  CoCo's own Windows client transport remains a named-pipe follow-up.
- **Task metadata boundary**: Removed the prerelease `goal` field from the CLI,
  daemon contract, and task projection rather than treating one vague string as
  both intent and metadata. A future annotation/reference model is tracked for
  deliberate design; retained legacy database values stay internal and are not
  sent to Codex.
- **Public documentation boundary and presentation**: Made README and rendered
  docs strictly user-only, removed architecture and roadmap pages from the
  public tree, and selected the nuqs notebook-style Fumadocs layout as the
  primary visual reference while keeping CoCo's own branding and prose.
- **First executable Rust slice**: Wired `cocod`, `coco`, and `coco-mcp` to the
  coordinator. Repository registration, native worktree/task creation, Codex
  thread and turn startup, durable events, observation, retry protection, and
  failure retention now run through the local daemon boundary.
- **Repository command**: Selected `coco repo add [path]` and the
  `repository.register` daemon method instead of the ambiguous `coco init`.
- **Codex execution profiles (superseded source shape)**: The first profile
  slice read named tables from the base Codex configuration while persisting
  only a redacted effective snapshot per task. The 2026-09-06 named-file
  contract above replaces that source interpretation.
- **Static public documentation**: Fixed GitHub Pages as the deployment target
  for the future Next.js/Fumadocs site. The site must use a fully static
  export, client-side static search, project-subpath-safe assets and routes,
  and a GitHub Actions Pages deployment of only the generated `out/` artifact.
- **Worker MCP architecture**: Made CoCo authoritative for the future MCP
  catalog, capability profiles, and immutable thread bindings. Selected native
  Codex MCP configuration as the first runtime projection and explicitly
  excluded building a custom CoCo proxy.
- **Agentgateway boundary**: Documented Agentgateway as a compatible future
  data-plane adapter for federation and independent policy enforcement. It is
  not a current dependency or planned delivery item.
- **Rust migration**: Replaced the initial TypeScript/Node implementation
  direction with a single Rust crate using Tokio while retaining SQLite,
  native Git worktrees, and the Codex App Server boundary. The later shared-TUI
  slice moved that boundary from private `stdio` to authenticated loopback
  WebSocket.
- **CoCo v0 contract**: Added the concrete product specification, including
  task/thread/worktree invariants, CLI and MCP contracts, lifecycle states,
  normalized events, safety requirements, acceptance tests, and explicit
  assumptions.
- **Architecture**: Added the headless `cocod` topology and boundaries between
  CLI/MCP adapters, SQLite, Git worktrees, and the Codex App Server.
- **Implementation baseline (superseded)**: Initially recorded Node.js 24,
  pnpm, Nix flakes, SQLite, generated local App Server bindings, and a first
  local RPC direction; the Rust migration later the same day superseded the
  language/runtime portion of this entry.
- **Initialization**: Established the internal knowledge bundle and separated
  public product documentation from maintainer and agent context.
- **Branch workflow**: Required one working document per branch, with the full
  branch name mirrored below `knowledge/work/`.
- **Public documentation**: Selected a customized Next.js, Fumadocs, MDX, and
  Tailwind direction inspired by Orca's public documentation, explicitly
  excluding Astro Starlight.
