---
type: Knowledge Bundle Log
title: Project knowledge update log
description: Records significant structural and semantic updates to the project knowledge bundle.
tags: [knowledge, maintenance]
status: stable
---

# Project knowledge update log

## 2026-09-10

- **Public layout rhythm and terminology**: Aligned the landing navigation to
  the page grid, relaxed oversized display type, and removed compounded
  Fumadocs/custom spacing from documentation introductions. Public integration
  language now uses only hooks and guards rather than presenting reactions as
  a third concept. Restart guidance now states explicitly that an in-flight
  turn is interrupted while its workspace, worktree, and saved conversation
  remain available.

- **Minimal resource status and robust live frames**: Made human resource
  telemetry explicit through `status --resources`/`-r` for either one
  workspace or a collection, with `-f` for follow and clusterable short flags.
  JSON status always includes the available generation-local observation while
  ordinary lists avoid sampling. Human output omits executor backend details.
  Follow now uses relative CRLF-backed frame replacement that remains correct
  after a first draw at the terminal bottom margin, and explicit `send`
  targets are validated before an omitted-message prompt opens.

- **Public alpha publication**: At the user's explicit direction, made the
  repository public, activated its static GitHub Pages deployment, and enabled
  crates.io releases. Published `v0.1.0-alpha.1` with Linux and macOS binaries
  and `codex-coordinator 0.1.0-alpha.1` on crates.io. The live landing-page
  review added a visible `Docs` destination and replaced the overly tight,
  negative hero with a concise positive product statement.

- **Per-workspace Codex execution and observation**: Kept one shared App Server
  as CoCo's control plane and added one lazy `codex exec-server` per activated
  workspace. Fresh threads, normal turns, and TUI turns select the matching
  environment; detailed status reports generation-local process state and
  Linux process-tree RSS/CPU observations. Normal close and daemon shutdown
  stop owned executors. Hard limits, containers, history, and unsupported
  resume/fork/review/compact routing remain explicit separate contracts.

- **Hook lifecycle controls and retirement guards**: Added offline
  configuration validation, atomic daemon reload with last-known-good
  preservation, and a shared visible registry for post-event reactions and
  synchronous guards. `workspace.close` and `workspace.delete` guards receive
  the already checked plan before any effect, run once in stable ID order, and
  can only allow or deny under an explicit fail-open/fail-closed policy.
  Dry-runs and recovery do not execute them; delivery history remains scoped to
  durable post-event reactions.

- **Durable CoCo hook slice**: Added a versioned, operator-owned local command
  reaction layer for accepted signals and successful workspace create, close,
  reopen, and delete transitions. Matching delivery rows commit atomically with
  their source fact, retry with bounded at-least-once semantics, recover after
  daemon restart, and expose definition/history views without commands or
  payloads. Kept prompt, tool, permission, compaction, subagent, and session
  lifecycle policy in native Codex hooks; a model-free App Server test against
  0.154.0 proves that boundary at the first ordinary turn.

- **Public positioning correction**: Reframed CoCo's README, site landing page,
  overview, quickstart, package description, and CLI tagline around its durable
  local control-plane role. Named workspaces remain addressable outside the
  initiating client and across CLI, native TUI, and MCP; Git worktrees are the
  supporting separation mechanism rather than the primary product claim.
  Moved MCP and signals into an integration section and made the quickstart
  demonstrate detached observation instead of beginning with a blocking wait.

- **Codex 0.154.0 compatibility baseline**: Advanced the selected native
  compatibility pin after both model-free real-process contracts passed against
  the installed release. Preparation/adoption, restart and resume, context
  fork, retirement, per-thread MCP isolation, profile restoration, and signal
  attribution required no CoCo protocol change.

## 2026-09-09

- **File-defined signal catalog**: Selected JSON Schema 2020-12 as the fixed
  dialect and replaced the draft per-type registration CLI with explicitly
  selected `NAME@VERSION.json` files at control-MCP startup. Catalog loading is
  atomic, accepted versions remain immutable, and `emitAllowed` separates
  definitions from exact-version grants. Running snapshots and retained history
  do not change when files are edited or removed; no watcher or auto-grant.

- **Signal slice and MCP capability correction**: Added the small operator-owned
  signal contract, native `_meta.threadId` attribution, optional inline schema
  validation, count-bounded retention, and independent cursor replay. Kept ticket
  interpretation and automatic reactions outside CoCo. The combined real-MCP
  process test exposed a default-router bug that bypassed `--allow-send`; the
  actual handler now uses its filtered instance. Separate model-free Codex
  0.153.4 evidence covers actual signal calls, scoped profiles, forks, and restart.

- **Task-management boundary**: Confirmed CoCo as the agent working-environment
  and control/communication layer, with tickets and business workflow owned by
  a separate product. Direct task-system integration and a separate composing
  service remain alternatives. The signal implementation is a separate slice;
  hooks and external references remain unimplemented, not an embedded ticket
  model.
- **Terminal interaction repair**: Replaced CoCo's yes/no menus with `y/N`
  line input and a safe No default. The shared picker reserves real scrollable
  lines, fits short terminals, measures Unicode display width, and marks only
  the selected row with `›` and cyan/bold highlighting. Inactive rows retain
  blank marker space; selection stays clear without color or a blinking cursor.
  Cleanup restores the terminal on acceptance, cancellation, and errors.
  Added an isolated real-terminal probe alongside focused Rust regressions.
- **Checked workspace retirement**: Implemented reversible `close`/`reopen`
  with native thread retention by default, plus closed-only record deletion
  and explicit native-thread/owned-branch opt-ins. Follow-up review tightened
  managed-path identity, pinned confirmation plans, descendant worktree
  activity, prepared context-source protection, and recovery checks. Bound
  TUI presence protects against retirement without excluding `send` on an
  idle thread or additional TUI clients; only fresh adoption remains exclusive.

## 2026-09-08

- **Persistent live status view**: Unified targeted and collection
  `status --follow` as observation that ends only on Ctrl-C. A capable terminal
  replaces one saved live region in place, while redirected or piped stdout
  remains ANSI-free and appends only changed snapshots. Ready, wait, unloaded,
  unavailable, and failure states stay visible without ending the follower;
  conversation output remains exclusive to `send --wait`.
- **Large native context-fork repair**: Negotiated Codex's experimental API
  capability because CoCo deliberately uses deferred goal continuation, and
  requested metadata-only fork/resume responses so a large retained history
  does not exceed the shared WebSocket frame limit. The thread still inherits
  or loads its full native context. Bounded Codex RPC rejection messages now
  reach the CLI without exposing structured error data, the fake server
  enforces the negotiated contract, and the model-free real-Codex test covers
  first activation of an inherited-context workspace. Omitted interactive
  workspace arguments now show the picker even when only one candidate exists.
- **Concise Codex-style terminal presentation**: Replaced generic object
  dumps and tab-separated UUID-heavy collections with command-specific success
  summaries and bounded aligned tables. Human lists retain names, states,
  branches, and repository paths while opaque IDs remain in JSON and
  actionable recovery/decision hints. The shared picker now uses a cyan `›`
  selection, bold headings, and dim detail without persistent key help or
  post-selection echoes. Styling follows Codex's standard cyan/green/red/
  magenta palette, is terminal-only, honors `NO_COLOR`, sanitizes dynamic
  one-line fields, and never touches JSON, raw patches, or exact agent output.
- **Unified context reference**: Replaced source-specific creation options
  with `--context`/`-c`. The daemon resolves a destination-repository workspace
  first and otherwise validates an exact native thread ID; `workspace:` and
  `thread:` prefixes disambiguate deliberately. `--compact-context`/`-C`
  modifies only the child fork and composes as `-Cc <reference>`. Git-base
  selection remains independent.
- **State/output CLI separation**: Made targetless `coco status` the current
  repository overview and `status -a` the all-repository overview; either can
  follow collection state changes until Ctrl-C, while an explicit workspace
  retains detailed and bounded follow behavior. Status no longer opens a
  workspace picker or prints conversation messages. Added `coco send --wait`
  to wait for the exact accepted client operation and print its last final
  agent response. Each response is bounded to 1 MiB and the current-generation
  cache to 256 entries/8 MiB; none is written to SQLite.
  Direct-response/notification races remain safe, including completion
  that overtakes `turn.start`; `event.list` remains only as an unconsumed
  compatibility method.
- **Native first activation and Codex 0.153.4 baseline**: Changed workspace
  creation to persist only the verified Git worktree and derive a new public
  `prepared` phase while no native thread is bound. The first fresh `send`
  binds the exact `thread/start` result atomically with the accepted real turn;
  inherited context materializes on first activation. A first fresh `jump`
  now launches the official TUI through a one-use authenticated correlation
  relay, leaves an empty candidate unbound, and adopts only an exact candidate
  with a non-empty native rollout before subscribing the daemon. Bound jumps
  continue through exact `resume --remote`; a liveness heartbeat keeps a fresh
  jump exclusive while its TUI remains open, and TUI exit does not interrupt
  an accepted turn. The model-free real-process gate now passes against released
  `codex-cli 0.153.4`, including the empty/durable distinction, history, daemon
  and App Server restart, model selection, and exact resume. Process coverage
  additionally proves profile/model/cwd propagation and detach behavior. CLI
  JSON schema 6 records the new `prepared` value; public compatibility and usage
  documentation moved to the same baseline.

## 2026-09-07

- **Codex 0.153.4 compatibility finding**: Replaced the real-process test's
  byte-for-byte generated-schema snapshot gate with validation of the concrete
  App Server behavior CoCo consumes. The model-free probe then exposed a real
  incompatibility: a prepared empty legacy thread remains visible to
  metadata-only `thread/read` but is no longer materialized before its first
  user message, so a fresh App Server rejects `thread/resume`. CoCo retains
  0.147.0 as its supported baseline while native-thread creation is redesigned
  to occur on first `send` or `jump`; no hidden synthetic turn is accepted.
- **Terminal-gated CLI interaction**: Added one reusable picker for omitted
  repository/workspace targets and native decision options. Arrow keys and
  `j`/`k` move immediately, Enter confirms, keys 1 through 9 select directly,
  and Escape/`q`/Ctrl-C cancel. `create` can collect a missing name and
  repository, while `status`, `send`, `jump`, and `diff` can collect a missing
  workspace; `send` separately collects a missing message. Collection commands,
  JSON, non-terminal execution, and explicit `--no-input` remain deterministic.
  `--choice` provides a non-interactive one-based approval response. The
  previous workspace-free status overview was removed in favor of canonical
  `list`/`ls`, leaving status as one detailed target locally or with `--global`.
- **Consistent collection and repository-scope CLI**: Made `list` the visible
  canonical collection verb and `ls` its visible alias for workspaces,
  repositories, and models. Split the former overloaded `--all-repos` scope;
  the later terminal-interaction entry above records the final refinement that
  reserves `--all-repos`/`-a` for `list`, uses `--global`/`-g` for one named or
  interactively selected workspace, and removes the duplicate status overview.
  The compatibility-only top-level `models` spelling remains hidden for one
  revision.
- **Independent workspace creation axes**: Replaced the public coupled
  `--fork-from` contract with independent Git base
  (`--base`/`--base-workspace`), native context
  (`--context-workspace`/`--context-thread`), Git binding
  (new branch, existing branch, or detached), and local-state selections.
  `--context-thread` means exact Codex `thread.id`, never
  `thread.sessionId`. Schema v7 persists the typed worktree mode and
  descriptor v3 keeps bounded resolved provenance; the former coupled option
  remains hidden for one compatibility revision.
- **Explicit local-state carry**: Added staged/unstaged tracked patch carry,
  opt-in ordinary non-ignored untracked files, and the CLI-only `-d`/`--dirty`
  preset. `-D` remains the independent detached selector, so `-dD` composes
  both. Every managed worktree honors Codex's `.worktreeinclude` convention
  only for Git-confirmed ignored paths, with automatic ignored root
  `AGENTS.override.md`, symlink/overwrite refusal, count/byte bounds, private
  destination permissions, memory-only contents, and no source mutation.
- **Process-smoke module split**: Replaced the 2,028-line integration-test file
  with a small crate root and focused lifecycle, native-fork, fake-App-Server,
  and process-support modules without changing the two scenarios.
- **Schema-v6 operation ledger and turn stop-write**: Added a minimal durable
  `turn_start` state machine with `prepared`, `dispatching`, `accepted`, and
  `uncertain` states. Intent commits before the App Server call; only its
  direct correlated response proves acceptance, because `turn/started` does
  not echo the client message ID. Unconfirmed dispatches are never retried
  automatically. Production stopped writing local turns, persisted
  `active_turn_id`, start/resume status snapshots, and turn start/completion
  events; legacy rows remain for one reversible migration revision. The CLI
  now surfaces and accepts an operation ID for exact send replay after an
  interrupted client response.
- **Coordinator test responsibility split**: Kept the shared fake worker,
  fixture, and Git helpers in `coordinator/tests.rs`, and moved the unchanged
  behavior cases into focused workspace, context/activation, decision, and
  native-event child modules. The structural checkpoint changes no production
  code or assertions and preserves all 30 Coordinator tests.
- **Generation-local decision and event reduction**: Moved actionable command,
  file-change, and structured-input requests out of SQLite into the daemon
  generation that owns their native callback. The registry preserves bounded
  presentation and exact private response correlation, changes pending to
  submitted under one lock before the native write, resolves from Codex, and
  orphans on disconnect without retaining user answers. Production also
  stopped event writes for sent prompts, native status/plan/diff/error
  notifications, decisions, and unsupported requests. The schema-v6 entry
  above records the later removal of local turn and status-snapshot writes;
  provisioning events and completed agent text remain.
- **Native read and on-demand activation cutover**: Passive workspace list,
  status, and follow polling now use stable, non-loading `thread/read`, validate
  the stored ID and `cwd`, and project failures as unavailable rather than
  serving the SQLite snapshot. Daemon startup no longer resumes every workspace;
  `send` and the internal `workspace.attach` used by `jump` resume and
  subscribe only the selected thread after profile and binding validation.
  Native context forks address the exact readable source thread without
  coupling it to source-worktree activation. At that checkpoint, schema-v5
  native notification, turn, event,
  and decision paths still received compatibility writes; eager recovery
  status and failure writes ended with eager startup resume. The later entry
  above records the next reduction.
- **Bounded-history compatibility seam**: Kept completed agent text in the
  normalized compatibility event stream after a pinned 0.147.0 process probe
  showed that bounded `thread/turns/list` and `thread/items/list` require the
  `experimentalApi` initialization capability. Stable
  `thread/read(includeTurns: true)` hydrates the full thread and can exceed
  CoCo's bounded shared transport for long histories, so it is not called by
  `status --follow`. Follow instead waits for a stable second terminal poll;
  current phase remains native while final text remains transitional.
- **Reversible direct read cutover**: Replaced the planned prolonged
  shadow-only checkpoint with a direct projection cutover after the published,
  pinned Codex 0.147.0 real-process test proved non-loading read/history behavior
  across restart and focused fake-runtime tests covered mismatches, failures,
  status projection, and follow-output stabilization. No table or existing
  record was removed; obsolete eager-recovery status/failure writes stopped,
  while the then-remaining compatibility writes kept the bridge reversible.
  The later schema-v6 entry above records completion of the operation-ledger,
  remaining turn/status stop-write, and crash-reconciliation gate.
- **Native-first ownership confirmed**: Reframed CoCo as a local headless
  control plane rather than a competing worktree launcher or Codex history
  store. Git remains authoritative for repository/worktree truth and the App
  Server for threads, turns, conversation, native status, models,
  configuration, and server-request semantics. CoCo retains only its stable
  repository/worktree/thread binding, provisioning/failure evidence,
  idempotent composite-operation correlation, and irreducible context/policy
  provenance.
- **State-reduction migration and release gate**: Classified schema-v5 native
  snapshots, turn rows, normalized events, completed messages, decision rows,
  and MCP audit history as implemented compatibility behavior rather than
  permanent authority. Recorded a released-Codex compatibility gate, shadow
  reads, native read/follow cutover, per-field removal criteria, historical
  database migration tests, and a final multi-repository CLI/MCP/`jump`
  control-plane proof. Public release remains a no-go if actual use collapses
  to `create` followed by `jump` or native Codex exposes the equivalent
  programmable binding.

## 2026-09-06

- **Private incubation restored**: Changed `janthmueller/coco` from public to
  private, deleted its GitHub Pages site, and disabled the Documentation
  workflow server-side while the upstream-overlap and product boundary are
  reviewed. The static Pages export remains a dormant, locally verifiable
  deployment target; re-enabling repository, site, or crate publication
  requires a new explicit user decision.
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
- **Native workspace fork delivered (superseded interface)**: Added same-repository
  `coco create --fork-from <workspace> [--compact]`. The destination derives
  from an idle, clean source workspace's committed `HEAD` and native Codex
  history; compaction applies explicitly to the child and finishes before an
  initial send or jump. Immutable provenance, destination binding context,
  failure retention, exact App Server calls, and static user docs are covered.
  Compaction remains fork provenance rather than a fourth context mode, while
  handoff stays deferred until its plan and artifact/reference model is
  designed. The 2026-09-07 independent-creation entry above retains this
  behavior through separate base and context selectors and hides the coupled
  flag.
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
- **Daemon thread recovery (superseded on 2026-09-07)**: The first recovery
  implementation made a fresh `cocod` resume each persisted `ready` Codex
  thread with its verified ID, worktree, and unchanged in-memory profile
  overlay. The native-first cutover replaced this eager behavior with passive
  reads and per-workspace activation on demand.
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
