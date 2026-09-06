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
- [x] Run the post-Phase-2 architecture review, reaffirm the one-package
  decision, and hide implementation modules behind three executable library
  entry points.
- [x] Correct runtime-state ownership before adding more interaction commands.
  Persist Codex's native thread status (`notLoaded`, `idle`, `systemError`, or
  `active` with all `activeFlags`) as the thread-runtime truth; retain only
  CoCo-owned provisioning/task lifecycle separately; derive the concise public
  phase instead of maintaining a competing thread state machine. Remove
  method-name-based wait-state guesses and cover mixed wait flags, stale
  observations, restart, and schema migration.
  - [x] Map the transitional phase writes, App Server status sources, storage
    schema, CLI projection, and affected tests.
  - [x] Add the separated lifecycle/native-status domain and v3 migration.
  - [x] Move Store and Coordinator transitions onto owned facts and make
    server requests observational only.
  - [x] Update the derived CLI/API projection and regression coverage.
- [x] Verify the pinned Codex 0.147.0 remote-TUI exit path. A normal `/quit` or
  `/exit` sends `thread/unsubscribe` and closes only the remote client
  WebSocket; it does not send `turn/interrupt`. The App Server keeps an active
  thread loaded, and `cocod` remains its independent subscriber.
- [x] Harden `coco jump`'s close-without-cancel contract after the state-model
  correction. Add a pinned contract/smoke test for normal TUI exit and abrupt
  transport loss while a turn is active, prove daemon event projection keeps
  running, and make the UX distinction explicit: leaving the TUI detaches;
  an explicit Codex interrupt cancels the turn.
  - [x] Exercise a second authenticated remote client in the process harness
    and prove its disconnect does not end daemon observation or the turn.
  - [x] Cover the child-process exit contract and document the shipped user
    behavior without exposing App Server internals publicly.
- [x] After the state-model and `jump` slices, stop implementation and review
  all findings and open work with the user. Reprioritize daemon recovery,
  cross-platform IPC, Git write policy, decision handling, hooks, MCP
  isolation, detached worktrees, and release work before selecting the next
  slice.
- [x] Recover persisted `ready` tasks after daemon restart by resuming their
  existing Codex threads with the stored worktree and unchanged profile
  overlay. Refresh native status only from a validated response, isolate
  per-task failures, and retain truthful interruption of unfinished turns.
  - [x] Extend the worker/store seams without leaking Codex JSON into the
    coordinator or weakening the profile secret boundary.
  - [x] Cover successful recovery, profile drift, isolated resume failure, and
    daemon-level restart in automated tests.
  - [x] Update public restart guidance and canonical recovery semantics, then
    create a dedicated checkpoint commit.
- [x] Add an explicit opt-in compatibility smoke test against the installed,
  pinned Codex executable. Verify the reported version, generated resume
  schema, authenticated WebSocket startup, persistent thread creation, and
  resume through a fresh App Server process without consuming a model turn.
- [x] After recovery and compatibility are complete, stop and discuss the Git
  administrative write policy with the user before implementing it. Retain
  native shared worktrees and Codex approvals rather than adding a custom
  commit service or per-workspace Git database.
- [x] Define the multi-repository CLI contract: implicit `.` or an explicit
  leading repository path, explicit `--all-repos`, global opaque workspace IDs,
  repository-scoped names, deterministic ambiguity errors, no hidden
  persistent selection, and safe slash-separated names such as `feat/login`.
- [x] Decide the stable vocabulary and creation UX: a CoCo `workspace` is the
  durable aggregate around one repository binding, Git worktree, Codex thread,
  and configuration snapshot. External tickets remain optional references,
  not CoCo workspaces. Replace `new` with `create` and support explicit
  composable `--send <message>` and `--jump` post-actions.
- [x] Rename the existing `task` model cleanly to
  `workspace` across Rust types/modules, SQLite with a lossless migration,
  daemon methods and DTOs, operation/event names, CLI rendering, MCP schemas,
  tests, and internal documentation. Replace `coco new` with `coco create` and
  update public docs only when that behavior ships.
  - [x] Rename the Rust domain, Store/Coordinator modules, RPC methods and
    fields, CLI output, event names, errors, and control-MCP tools.
  - [x] Add the lossless SQLite v4 vocabulary migration and bump the CLI JSON
    envelope to schema version 3.
  - [x] Verify the code migration through the full unit and process suite.
  - [x] Update canonical internal knowledge and the public site before marking
    the slice complete.
- [x] Add the create convenience pipeline and process coverage:
  `coco create <name>` prepares only; `--send <message>` starts the first turn;
  `--jump` opens the existing thread; both run create, send, then jump. Preserve
  a successfully created workspace when a later action fails, and leave an
  accepted turn running when TUI launch fails.
  - [x] Implement the ordered create/send/jump execution and reject blank
    messages before workspace creation.
  - [x] Prove through the process smoke that a failed TUI child leaves both the
    workspace and accepted initial turn active, with a truthful CLI error.
  - [x] Complete the requested command-versus-flag UX review with the user and
    add the agreed independent `-s`/`-j` short options.
- [x] Prove with pinned real Codex that an ordinary `git add`/`git commit`
  in a linked workspace worktree follows the native approval protocol and
  advances only the bound workspace branch without a blanket writable Git
  common directory.
  - [x] Add an explicit opt-in test that requests one exact, allowlisted
    file/add/commit command, observes and answers Codex's native command
    approval, and never auto-approves an unexpected request.
  - [x] Verify that only the bound workspace branch advances, the source
    branch stays fixed, the linked worktree is clean, and the App Server emits
    the matching request-resolution and terminal turn events.
  - [x] Extend the pinned schema compatibility set, run all non-networked
    gates, and record the separately authorized live proof.
- [x] Implement the confirmed multi-repository CLI ergonomics: `repo list`,
  optional leading-path scope, `--all-repos`/`-a`, global workspace-ID lookup,
  helpful local-miss/ambiguity diagnostics, and slash-separated workspace
  names with secure path/ref collision handling. Keep the MCP adapter fixed to
  its launch-time repository.
  - [x] Implement safe nested worktree paths and Git ref-prefix collision
    detection for slash-separated workspace names.
  - [x] Complete the daemon protocol, coordinator, CLI, and human/JSON output.
  - [x] Add multi-repository, ambiguity, explicit-scope, and process coverage.
  - [x] Reconcile canonical knowledge and public user documentation with the
    behavior that actually ships.
- [x] Implement durable pending decisions and `coco decide <decision-id>` as
  the selected next v0 slice.
  - [x] Persist supported native decision requests before presentation, bound
    to the exact App Server generation, thread, turn, and request ID.
  - [x] Project pending decisions through workspace status without inventing
    a second thread state machine.
  - [x] Render native choices as numbered options and accept a number;
    user-input requests may accept free text where the native schema permits
    it. Defer cursor-driven selection and other TUI polish.
  - [x] Resolve through the original live App Server request, handle stale or
    already-resolved requests safely, and cover restart/orphan behavior.
  - [x] Update public and canonical internal documentation only for behavior
    proven by the completed implementation and tests.
- [ ] Design workspace annotations and external references as a deliberate future
  feature. Decide typed versus free-form values, mutation/audit semantics,
  privacy and display rules, fork/handoff inheritance, and explicit projection
  into Codex before adding any CLI or RPC field.
- [x] Implement the agreed first context-transfer slice: native same-repository
  workspace fork from an idle, clean source at its committed `HEAD`, with
  optional explicit compaction of the child before any initial send.
  - [x] Add typed CLI/RPC source selection and record immutable fork
    provenance without adding a fourth context mode.
  - [x] Bind native `thread/fork` to the destination worktree and configuration.
  - [x] Wait for child-only `thread/compact/start` completion and cover ordering,
    failure retention, idempotency, and recovery.
  - [x] Pin the relevant Codex schemas, update public docs for shipped behavior,
    run the full gates, and create a checkpoint commit.
- [x] Add App-Server-backed model discovery and an explicit per-workspace model
  override without duplicating Codex configuration resolution.
  - [x] Add `coco models [--json]` over the daemon-owned App Server's
    `model/list` result.
  - [x] Add `coco create --model/-m <model-id>` and pass it as the explicit
    `thread/start` or `thread/fork` model override alongside the selected
    profile configuration.
  - [x] Preserve the effective model across daemon recovery, cover the exact
    wire contract, and update public/internal documentation for shipped
    behavior.
- [ ] Review whether the current product is ready for a first public release,
  distinguishing a supervised alpha from a stable or production-ready claim.
  - [x] Port Wuf's tested-revision semantic-release pattern to the Rust
    package: Conventional Commits, alpha versions, synchronized Cargo metadata,
    generated changelog, tag, and GitHub Release.
  - [ ] Build and smoke-test release archives for all currently supported host
    platforms before allowing a release, without claiming Windows support.
  - [x] Keep automatic publication opt-in until the remaining public-release
    blockers are deliberately resolved. The user selected MIT and supplied a
    local crates.io token for secure GitHub-secret upload; never record it.
  - [ ] Run the complete sequential verification gates and record the final
    release recommendation.
- [ ] Keep handoff deferred as a separate artifact-design task. Treat authoring
  and consumption independently; consider agent-generated material, existing
  Markdown, direct CLI input, ticket or other external references, and an
  optional plan without fixing one automatic prompt. Define provenance,
  redaction, freshness, size limits, review/edit behavior, and reference
  semantics before enabling `handoff`.
- [ ] Design user-configurable lifecycle hooks as a separate future feature.
  Before defining CoCo hooks, inventory the pinned Codex CLI and App Server's
  native hooks, notifications, and lifecycle events so CoCo can expose or
  extend existing signals instead of duplicating them. Cover workspace creation,
  thread/turn start, agent state transitions and terminal outcomes, then decide
  execution context, filtering, ordering, retries, timeouts, failure policy,
  secret handling, auditability, and platform behavior.

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
- 2026-09-06 — Reaffirm one Cargo package after the Phase-2 review. No
  component has an independent consumer/release or measured isolation need.
  Export only `run_cli_from_env`, `run_daemon_from_env`, and
  `run_mcp_from_env`; keep all implementation modules private until a real
  client boundary justifies a small protocol crate.
- 2026-09-06 — Treat Codex's native thread status and complete active-flag set
  as authoritative for thread runtime. CoCo owns preparation and task
  lifecycle, pending-decision correlation, Git state, and a derived display
  summary; it must not infer a second thread state from server-request method
  names.
- 2026-09-06 — Use the stock remote Codex TUI's normal exit behavior as
  `coco jump` detach semantics rather than inventing a keybinding. In pinned
  0.147.0, the user-exit path unsubscribes the TUI connection without issuing
  `turn/interrupt`; `/quit` and `/exit` therefore mean leave the UI, while an
  explicit interrupt remains the unambiguous cancel action. Contract-test this
  experimental upstream surface before relying on it in a release.
- 2026-09-06 — Name the unified approval/user-input response command
  `coco decide <decision-id>`. Start with numbered native options and optional
  free-text input; defer cursor navigation until the simpler workflow has
  proven insufficient. This records the preferred shape only, not the next
  scheduled implementation slice; priority is reconsidered after state and
  `jump` work.
- 2026-09-06 — Put an explicit planning checkpoint after the thread-state and
  `jump` slices. Report their findings and reprioritize the remaining backlog
  with the user instead of automatically continuing into `decide`.
- 2026-09-06 — Replace persisted `TaskPhase` with schema-v3 facts owned by
  their sources: CoCo stores `TaskLifecycle`, Codex supplies the complete
  native `ThreadStatus`, each observation records runtime generation, time,
  and freshness, and turns retain their own phase. Keep `phase` and
  `waitReasons` as read-time projections only; bump CLI JSON documents to
  schema version 2 because the task shape now exposes `lifecycle` and
  `threadRuntime`.
- 2026-09-06 — Treat every correlated App Server request as an observational,
  sanitized `server_request.received` event until the pending-decision model
  is deliberately implemented. Only `thread/start.thread.status` and
  `thread/status/changed` may refresh native runtime state; request method
  names never do.
- 2026-09-06 — Treat a successfully launched remote TUI as an ordinary client
  attachment, not as ownership of the turn. Normal unsubscribe and unexpected
  transport loss must leave the daemon connection and active turn intact;
  cancellation requires Codex's separate explicit interrupt operation.
- 2026-09-06 — Implement the first recovery level without claiming process
  survival: a new App Server resumes persisted `ready` threads after validating
  their ID, worktree, and immutable named-profile provenance. Turns unfinished
  at daemon loss remain interrupted; making an active turn survive an App
  Server crash would require a separately supervised worker lifetime.
- 2026-09-06 — Keep the real-Codex compatibility test both ignored by default
  and guarded by `COCO_RUN_REAL_CODEX_COMPAT=1`. It uses isolated homes, pins
  the executable and relevant generated schemas, and deliberately never sends
  `turn/start`, so a maintainer cannot consume a model turn by running it.
- 2026-09-06 — Do not negotiate Codex's broad `experimentalApi` capability
  merely to repeat the task `cwd` as `runtimeWorkspaceRoots`; omit that guarded
  field and rely on Codex's documented default. After validating a new thread,
  issue the stable, model-free `thread/name/set` operation with the task name
  before binding it, because the pinned App Server does not otherwise
  materialize an empty thread's rollout for later resume.
- 2026-09-06 — Keep ordinary Git work inside native linked worktrees. Agents
  may commit on their bound task branch through Codex's existing approval
  protocol; CoCo will not allocate separate Git databases, proxy commits, or
  grant the complete shared Git directory as an unconditional writable root.
  Validate the binding after Git-changing activity and treat drift as an
  observable error rather than repairing refs automatically.
- 2026-09-06 — Model CLI repository scope explicitly. An omitted leading path
  means `.`, a path selects one registered repository, and `--all-repos`
  requests daemon-wide listing or unique workspace-name resolution. Full workspace IDs
  resolve globally; local name lookup never falls through silently to another
  repository. Do not persist a process-global repo selection or combine paths
  and names into a colon-delimited identifier.
- 2026-09-06 — Admit conventional slash-separated workspace names such as
  `feat/login`, producing `coco/feat/login`, while validating every component
  before path/ref use and rejecting Git ref-prefix collisions explicitly.
- 2026-09-06 — Rename CoCo's durable aggregate from `task` to `workspace`
  before extending the CLI. A workspace owns the repository/worktree/thread
  binding and configuration snapshot; a future ticket or issue is a linked
  external reference. Replace `new` with `create`. Let `--send <message>` and
  `--jump` compose in the deterministic order create, send, jump; later-stage
  failure never rolls back a successfully created workspace or accepted turn.
- 2026-09-06 — Treat the vocabulary change as a deliberate prerelease wire and
  storage break: daemon methods are `workspace.*`, control-MCP tools are
  `workspaces.*`, event kinds are `workspace.created` and
  `workspace.completed`, and CLI JSON is schema version 3. SQLite schema v4
  migrates existing records and correlations in place rather than discarding
  prerelease state.
- 2026-09-06 — Use `-a` as the short spelling of `--all-repos`. A duplicate
  global workspace name is never guessed: the error exposes bounded matching
  repository paths and IDs, after which the operator selects with
  `coco <repository-path> <command> <name>` or a globally resolved workspace
  ID. Treat the scope switch as a global CLI option so it works both before
  and after a subcommand. Do not add a path/name composite selector.
- 2026-09-06 — Exercise Git approvals with Codex's native `untrusted` policy
  in the compatibility proof. `on-request` deliberately leaves escalation to
  the model and therefore cannot deterministically prove the callback. Never
  broaden the Git common directory or approve a model-rendered string alone;
  require exact agreement among executable argv, displayed command, parsed
  action, thread, turn, cwd, and the fixed test allowlist.
- 2026-09-06 — Preserve the original context-mode distinction rather than
  introducing a second context-transfer feature. `fork` inherits complete
  native thread history into a new workspace; `handoff` starts a fresh thread
  from a bounded, reviewable artifact; `resume` remains continuation of the
  same thread. None of them implicitly copies dirty code.
- 2026-09-06 — Deliver native fork before handoff. The first fork is restricted
  to an idle, clean source in the same repository and bases the destination on
  the source worktree's committed `HEAD`. Optional compaction applies only to
  the new child, must complete before `--send`, and is stored as a fork
  modifier rather than a fourth context mode. Handoff stays deferred until its
  plan, document, CLI-input, and external-reference model is settled.
- 2026-09-06 — Use a separate opaque CoCo decision ID in public status and
  `coco decide`; keep the native App Server request ID and exact response value
  private. Persist `pending` before presentation, compare-and-set to
  `submitted` before the response write, and accept `resolved` only from the
  matching current-generation notification. Never replay an unresolved
  callback into a replacement App Server process.
- 2026-09-06 — Support command approvals, file-change approvals, and structured
  user input through one numbered decision flow. Parse only fully understood
  native choices and permission/file-change shapes; an unknown extension is
  observational instead of enabling a blind approval. Secret answers use a
  cross-platform no-echo terminal reader and are never persisted.
- 2026-09-06 — Discover models through the daemon-owned App Server rather than
  a CoCo catalog or hard-coded list. Pass an explicit `--model` as the native
  top-level thread override alongside, but never merged into, the selected
  profile `config`. Codex owns final configuration precedence; CoCo persists
  the requested override and returned non-secret effective settings only so
  restart recovery can make the same request.
- 2026-09-06 — Do not advertise `coco jump` as replay for an already pending
  request. Codex 0.147.0's TUI retains only request IDs delivered through that
  client's own event stream, so attaching later cannot reliably answer the
  daemon's outstanding callback.
- 2026-09-06 — Retain CoCo as the product, repository, library, and executable
  identity while publishing the single Cargo package as
  `codex-coordinator`; the shorter registry name is already owned by an
  unrelated project. License the project under MIT and keep the first release
  on an explicitly enabled alpha channel.
- 2026-09-06 — Make dependency policy part of the release gate. Deny known
  advisories, unknown registry or Git sources, wildcard requirements, and
  licenses outside the reviewed set; report ecosystem duplicate versions for
  review without pretending all transitive duplicates can currently be
  eliminated.
- 2026-09-06 — Make a manual Release workflow dispatch rehearse versioning and
  the configured build by default. Require the operator to choose
  `publish=true` before any release commit, tag, GitHub Release, binary upload,
  or crates.io publication can occur; the exact successful Rust revision is
  required in either mode.

## Findings

- The implementation is suitable for a first supervised developer alpha once
  the public GitHub run proves native Linux/macOS packaging and Pages
  deployment. It is not a stable or production-ready release: Windows IPC is
  absent, Codex compatibility is pinned to 0.147.0, the daemon still requires
  explicit supervision, active turns do not survive daemon/App Server loss,
  and workspace cleanup/completion/merge remain outside the current surface.
- The reviewed dependency policy finds no advisory, source, license, wildcard,
  or explicit-ban violation. It reports eight pairs of transitive duplicate
  versions, primarily the WebSocket SHA-1 path versus direct SHA-2 and normal
  ecosystem version transitions; none currently represents an actionable
  safety failure.
- The pinned App Server's `model/list` is cursor-paginated and distinguishes a
  catalog entry's stable `id`, exact thread selector `model`, display metadata,
  default marker, reasoning choices, modalities, and personality support.
  `thread/start`, `thread/fork`, and `thread/resume` each accept a separate
  optional `model` beside their generic `config` object, so CoCo has no reason
  to reproduce Codex's configuration merge rules.
- Codex 0.147.0 represents a file update kind as a tagged object such as
  `{"type":"update","move_path":null}`, not a bare string. Decision
  presentation now parses that exact shape, preserves move targets, bounds the
  aggregate patch, and neutralizes terminal control characters. Unknown
  fields in approval choices, permission grants, or file-change objects fail
  closed rather than being hidden behind an approval label.
- A TUI attached after a server request is pending is not a safe response
  fallback. The pinned Codex TUI correlates only request IDs delivered through
  its own event stream, and the App Server exposes no pending-request replay
  call. `coco decide` therefore reads `isSecret` answers through `rpassword`'s
  cross-platform no-echo console path and sends them through the original
  daemon connection without storing their values.
- The context-transfer request was already present in the original handoff and
  in the stored `ContextMode::{Fresh,Fork,Handoff}` values. Native
  `thread/fork` accepts a source thread plus new `cwd` and configuration;
  CoCo now uses `turn/start.additionalContext` to repeat the destination Git
  binding on its own turns after a native fork. Whether the same field should
  deliver a future structured handoff remains deliberately unselected.
- Native `thread/fork` can retain a source goal and automatically continue it.
  CoCo sets `deferGoalContinuation: true` so `coco create` preserves the
  established prepare-without-starting contract. It passes the destination
  worktree and selected profile directly to the child, persists the returned
  child/source relation, and never mutates the source thread or worktree.
- `thread/compact/start` acknowledges before compaction finishes. CoCo therefore
  keeps the child lifecycle at `starting`, consumes its compaction-only
  `turn/started`, `contextCompaction` item, and terminal `turn/completed`
  notifications without creating a user turn, and exposes `ready` only after
  successful completion. A request error, terminal failure, disconnect, or
  timeout leaves the child thread and worktree bound to a failed workspace for
  diagnosis.
- Handoff is an artifact boundary, not necessarily an automatic summary. Its
  producer may be an agent prompt, an existing Markdown document, direct CLI
  input, or an external reference, while its destination consumes a reviewed
  snapshot in a fresh thread. A safe generated variant could prompt a read-only
  ephemeral source-thread fork and persist its final message outside the
  source checkout. Merely forking a thread does not isolate file writes when
  both threads use the same worktree.
- The real approval request uses a shell-rendered `command`, a parsed inner
  `commandActions` entry, and a proposed argv amendment. Its ordered
  `availableDecisions` are request-specific—the observed request offered
  `accept`, an exec-policy amendment, and `cancel`—so a future `decide` client
  must render what Codex supplied rather than assume a fixed menu.
- With `on-request`, the same sandboxed compound command can write its ordinary
  workspace file, fail at `git add`, and finish without asking to escalate.
  With `untrusted`, the pinned App Server reliably requests approval before
  execution. Accepting the exact callback advances only the linked workspace
  branch by one commit, leaves `main` fixed, and finishes with a clean
  worktree.
- Repository scope now travels as a typed daemon value rather than being
  inferred independently in clients. The control-MCP adapter always constructs
  a fixed single-repository scope; only CLI callers can request the explicit
  daemon-wide scope.
- Slash-separated workspace names require both filesystem and Git namespace
  checks. A valid nested worktree path can still collide with an existing ref
  such as `coco/feat`, so exact, ancestor, and descendant ref collisions are
  rejected before Git mutation and nested parent directories are canonicalized
  without following symlinks.
- `workspace.list` rows now carry compact repository identity even in local
  JSON output, and `repository.list` exposes only ID, display name, and root
  path. This intentional consumer-visible shape change advances CLI JSON from
  schema version 3 to 4.
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
- Official App Server documentation now explicitly supports attaching the
  stock terminal UI with `codex --remote`. It also specifies that the last
  unsubscribe only permits unloading after both zero subscribers and zero
  thread activity for 30 minutes. Inspection of tagged Codex 0.147.0 confirms
  the normal user-exit path calls `thread/unsubscribe`, then closes the remote
  WebSocket client; only the separate interrupt path sends `turn/interrupt`.
- The thread returned by native `thread/start` already contains an exact
  `ThreadStatus`, while `turn/start` returns a native turn rather than a new
  thread-status snapshot. The durable model therefore needs to store the
  former and later `thread/status/changed` observations, while deriving a
  short display phase from lifecycle, status freshness, flags, and the
  separately correlated active turn.
- The pre-migration event projection mutated task phase from server-request
  method names and labeled every request as an approval. Codex exposes several
  request families beyond approvals and user input, so schema v3 now keeps
  requests observational until the dedicated correlated decision model is
  scheduled.
- Schema v3 maps native-like v2 phases into diagnostic snapshots but marks
  them stale, maps bound old tasks to lifecycle `ready`, and fails only
  unfinished task preparation. On restart an unfinished turn becomes
  interrupted while the task remains recoverable; without `thread/resume`,
  its honest public phase is `unavailable`.
- The executable coordinator now serializes creation and turn-start operations
  per repository, resolves client paths back to a registered Git common
  directory, and treats Git/Codex effects as persisted saga steps.
- Supported App Server requests are now surfaced as durable, generation-bound
  decisions and are never auto-approved. Unsupported request families remain
  redacted observations rather than being guessed or answered blindly.
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
- The post-Phase-2 measurement finds 10,093 Rust lines and the same one Cargo
  package with one library plus three binary crates. The top-level import graph
  remains acyclic. Hiding the twelve implementation modules surfaced fourteen
  masked dead-code warnings; removing unused helpers, limiting fixtures to test
  builds, and retaining only the reasoned approval-response seam leaves a
  warning-free build and exactly three Rustdoc-visible entry points.
- The process harness can model independent authenticated App Server clients
  without launching an interactive terminal: one remote client performs the
  normal resume/unsubscribe path, a later client reattaches and drops its
  transport abruptly, and the original daemon connection still receives the
  terminal turn events. The built `coco jump` path is exercised separately in
  the same scenario to lock down its command, worktree, and secret boundary.
- Recovery can use the same persisted Codex thread through a completely new
  App Server process. A response supplies the only fresh native status for the
  new runtime generation; unsuccessful tasks remain unavailable and do not
  prevent the daemon from recovering other threads.
- The first run of the real compatibility test found that Codex 0.147.0 rejects
  `thread/start.runtimeWorkspaceRoots` unless the client declares the broad
  experimental API capability. The same generated schema says omission
  defaults that value to `cwd`, so the field had no benefit for CoCo.
- The pinned App Server returns an ID and future rollout path for a persistent
  empty thread but does not create the rollout file at `thread/start`. A
  model-free `thread/name/set` creates the durable record; after that operation
  a fresh App Server resumes the exact ID successfully.
- The vocabulary migration must cover more than a Rust type rename: SQLite
  owns table/foreign-key/index names, durable event and audit strings need
  translation, the daemon and MCP each expose independent method namespaces,
  and CLI JSON consumers need an explicit schema-version break.
- `jump` and `send` remain useful after creation, while their create flags are
  one-time post-actions. Replacing the standalone verbs with global `-j`/`-s`
  flags would make required workspace/message positionals ambiguous and weaken
  per-action help. The clean candidate is to retain the verbs and, if desired,
  add local `-j`/`-s` spellings only to `coco create`; this remains pending the
  user's requested UX review before public documentation is changed.

## Verification

- Release preparation passes `actionlint` for every workflow, the version-sync
  script's three unit tests, Python Semantic Release's no-operation version
  calculation to `0.1.0-alpha.1`, `cargo publish --locked --dry-run`, and
  inspection of the resulting 56-file package boundary. The package verifies
  as `codex-coordinator` and contains only production Rust source, Cargo
  metadata, README, and MIT license.
- The release-readiness run passes Rustfmt, all-target/all-feature Clippy with
  warnings denied, all 85 library tests (84 passed, one explicitly
  model-consuming test ignored), both process smokes, `cargo machete`, and the
  reviewed `cargo deny` policy. The socket-dependent suite required execution
  outside the restricted sandbox and then passed without failures.
- Documentation TypeScript, Oxlint, and Prettier checks pass. A production
  `DOCS_BASE_PATH=/coco` build verifies 88 static files, eight pages, local
  search, project-subpath routing, and the public-only boundary. The complete
  `nix flake check . --no-write-lock-file --max-jobs 1` passes locally; native
  macOS archive and hosted Pages proof remain pending the first GitHub run.
- The model-selection slice passes all 85 library tests (84 passed and the
  model-consuming approval proof ignored), both daemon/CLI process smokes, and
  the separately enabled turn-free real-Codex 0.147.0 compatibility test. The
  real test compares the generated model-list schemas, selects the advertised
  default, prepares an empty thread with that explicit model, restarts the
  daemon/App Server, and resumes with the same persisted override without
  starting a turn. Fake-process coverage proves complete two-page catalog
  traversal and that profile `config` plus top-level `model` remain separate
  for start, fork, and resume. Rustfmt, all-target/all-feature Clippy with
  warnings denied, `cargo machete`, and `git diff --check` pass.
- `nix run .#docs-check` passes after the public models/profile guide and CLI
  reference update. The final `DOCS_BASE_PATH=/coco nix run .#docs-build`
  verifies 88 static files across eight pages, client-side search,
  project-subpath routing, and the public-only boundary. `nix flake check .
  --no-write-lock-file --max-jobs 1` also passes.
- The completed decision slice passes 73 library tests plus the real
  daemon/CLI fake-App-Server process smoke with one build job and one test
  thread. The process test observes a native command approval, finds its opaque
  decision ID through `status`, drives `coco decide` with a numbered choice,
  verifies the exact original JSON-RPC response, receives native resolution,
  and returns the workspace to active execution. The two explicitly opt-in
  real-Codex/model tests compile and remain ignored.
- Decision-focused tests cover structured policy choices, schema-shaped file
  add/update/move data, bounded and control-safe presentation, unknown-field
  rejection, additional filesystem/network permissions, secret no-echo input,
  answer-free persistence, compare-and-set submission, native resolution,
  generation mismatch, disconnect, and restart orphaning.
- `cargo fmt --all -- --check`, all-target/all-feature Clippy with warnings
  denied, `cargo machete`, and `git diff --check` pass. `nix run .#docs-check`
  and both root plus `DOCS_BASE_PATH=/coco` static builds pass; the verifier
  finds 88 files across eight public pages with client-side search and no
  internal knowledge. `nix flake check . --no-write-lock-file --max-jobs 1`
  also passes.
- `git rev-parse --is-inside-work-tree` returned `true`.
- `git branch --show-current` returned `main`.
- `codex --version` reports the pinned `codex-cli 0.147.0`; its help exposes
  `resume --remote` and authenticated remote App Server attachment.
- The tagged 0.147.0 TUI and app-server-client sources were inspected for the
  exact exit path: normal exit unsubscribes and closes the client connection,
  while the explicit turn-interrupt request is separate.
- `git diff --check` passes for the state-ownership, `jump`, deferred `decide`,
  and post-`jump` planning-checkpoint updates; no production code changed in
  this research slice.
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
- After narrowing the library facade, all 49 library tests and the process
  smoke test pass sequentially; all-target/all-feature Clippy is warning-free,
  `cargo machete` finds no unused dependency, and `cargo doc --no-deps`
  exposes exactly the three intended executable entry points. The
  resource-limited `nix flake check . --no-write-lock-file` run also passes
  against the staged Git source.
- The documentation TypeScript, Oxlint, and Prettier checks pass. Both the
  root and `/coco` builds export 88 static files across eight pages with static
  search, valid local links, no server artifact, and no internal knowledge.
- After the runtime-ownership migration, the complete resource-limited Rust
  suite passes 52 library tests and the process smoke test with one build job
  and one test thread. All-target/all-feature Clippy with warnings denied,
  rustfmt, and `cargo machete` pass. `nix run .#docs-check` and
  `nix run .#docs-build` pass; the latter verifies 88 static files across eight
  public pages.
- After hardening `jump`, the complete resource-limited suite again passes all
  52 library tests and the process smoke test. The smoke test runs the built
  launcher, then exercises authenticated normal unsubscribe and abrupt loss as
  two independent remote clients while the turn stays active; the daemon still
  receives completion and projects `idle`. All-target/all-feature Clippy with
  warnings denied, rustfmt, `cargo machete`, `nix run .#docs-check`,
  `nix run .#docs-build`, and `nix flake check . --no-write-lock-file
  --max-jobs 1` pass. The static verifier again finds 88 files across eight
  public pages.
- After adding daemon recovery, the complete resource-limited suite passes all
  55 library tests and the expanded process smoke test. The latter shuts down
  `cocod`, starts a fresh daemon and fake App Server against the same database,
  verifies an authenticated `thread/resume` with the stored ID/worktree/config,
  and observes a fresh `idle` snapshot without `thread/start`. All-target and
  all-feature Clippy with warnings denied, rustfmt, `cargo machete`,
  `nix run .#docs-check`, `nix run .#docs-build`, and
  `nix flake check . --no-write-lock-file --max-jobs 1` pass.
- The explicit real-Codex command
  `COCO_RUN_REAL_CODEX_COMPAT=1 CARGO_BUILD_JOBS=1 cargo test --locked --test real_codex_compat -- --ignored --test-threads=1 --nocapture`
  passes against `codex-cli 0.147.0`. It compares the generated experimental
  start/resume/name schemas with the committed bundle, runs the built daemon
  and CLI against isolated state, and verifies the same idle thread and
  worktree after a new daemon/App Server generation with no active turn.
- After the compatibility corrections, the normal suite passes all 55 library
  tests and the fake process smoke while compiling and skipping the guarded
  real test. All-target/all-feature Clippy with warnings denied, rustfmt,
  `cargo machete`, `nix run .#docs-check`, `nix run .#docs-build`, and
  `nix flake check . --no-write-lock-file --max-jobs 1` pass.
- The workspace code/schema checkpoint passes all 55 library tests plus the
  fake daemon/CLI process smoke with one build job and one test thread outside
  the Unix-socket-restricted sandbox. The run verifies v1 task data migrating
  through SQLite schema v4, the new `workspace.*` and `workspaces.*` surfaces,
  `coco create`, and CLI JSON schema version 3. The all-target/all-feature
  Clippy gate with warnings denied also passes.
- The provisional create-action pipeline passes the same 55 library tests and
  fake process smoke sequentially, plus all-target/all-feature Clippy with
  warnings denied. The smoke invokes create, send, and jump as one ordered
  command, forces the TUI child to exit 23, observes the retained workspace and
  active turn, and then successfully reattaches with standalone `coco jump`.
- The repository-scope, slash-name, and native Git policy record passes
  `git diff --check` and a targeted stale-decision scan. No Rust code or public
  documentation changed in this decision-only slice.
- The confirmed workspace/create migration and composable post-action plan
  passes `git diff --check`; this planning update likewise changes no Rust code
  or public documentation.
- The implemented repository-scope slice passes all 62 library tests and the
  daemon/CLI process smoke with one build job and one test thread outside the
  Unix-socket-restricted sandbox. Coverage includes two repositories sharing
  `feat/shared`, deterministic ambiguity data, local-miss suggestions, global
  ID lookup, explicit leading-path lookup, `repo list`, `-a`, nested
  `feat/process-smoke` creation, and secure ref/path collision rejection.
  All-target/all-feature Clippy with warnings denied, rustfmt, and
  `cargo machete` pass. `nix run .#docs-check` and `nix run .#docs-build` pass;
  the latter verifies the fully static public export and the retired tasks
  guide is absent. A second production export with `DOCS_BASE_PATH=/coco`
  verifies all eight pages, search assets, and GitHub Pages subpath routing.
  `nix flake check . --no-write-lock-file --max-jobs 1` also passes against the
  staged source.
- The native Git proof command
  `COCO_RUN_REAL_GIT_APPROVAL=1 CARGO_BUILD_JOBS=1 cargo test --locked codex::tests::real_git_approval::pinned_codex_approves_a_commit_only_on_the_linked_workspace_branch -- --ignored --exact --nocapture`
  passes against `codex-cli 0.147.0` with explicit authorization. It consumes
  one model turn, approves only the exact validated temporary command, observes
  `serverRequest/resolved` plus completed command/turn events, advances only
  `coco/approval-proof` by one commit, leaves `main` fixed, and leaves the
  linked worktree clean. Earlier diagnostic runs safely cancelled an
  unexpected display wrapper and established why `on-request` is not a
  deterministic trigger.
- The expanded model-free real-Codex compatibility smoke passes and now compares
  command-approval request/response, server-request resolution, turn start and
  completion, and thread-fork schemas in addition to start/resume/name. The
  complete normal suite passes 62 library tests with the live test ignored,
  plus the daemon/CLI process smoke. Rustfmt, all-target/all-feature Clippy with
  warnings denied, `cargo machete`, and `git diff --check` pass. The staged
  source also passes `nix flake check . --no-write-lock-file --max-jobs 1`.
- The native-fork slice passes all 81 library tests (80 passed and one explicitly
  model-consuming test ignored), both daemon/CLI process smokes, and the
  separately enabled model-free real-Codex 0.147.0 compatibility test. Coverage
  proves source `HEAD` selection, dirty/active rejection, immutable provenance,
  replay idempotency, child-only compaction ordering and failure retention,
  destination `additionalContext`, and the exact App Server requests. Rustfmt,
  all-target/all-feature Clippy with warnings denied, `cargo machete`, docs
  TypeScript/Oxlint/Prettier, the fully static 88-file/eight-page export,
  `git diff --check`, and `nix flake check . --no-write-lock-file --max-jobs 1`
  pass.

## Open questions and handoff

- Release code and local gates are prepared. Create and push the public
  `janthmueller/coco` repository, store the registry credential only in its
  protected `crates.io` environment, keep `COCO_RELEASE_ENABLED=false`, and
  require the first hosted Rust, macOS binary smoke, and Pages runs to pass
  before deciding whether to publish the irreversible first alpha.
- Prove native Codex per-thread MCP isolation across start, resume, and fork
  before scheduling the registry feature.
- Agentgateway is deliberately not scheduled. Reconsider it only when
  federation, centralized credential custody, independent enforcement, or
  gateway observability becomes an actual requirement.
- Thread-runtime ownership, `jump` exit/reattach behavior, and prepared-thread
  recovery are corrected. Running turns still become interrupted if the daemon
  and its owned App Server stop; surviving that boundary would require a
  separately supervised App Server lifetime.
- The generation-bound pending-decision model and numbered `coco decide` flow
  are implemented for command/file approvals and structured user input. Keep
  it separate from native thread status; consider non-interactive flags only
  after real use demonstrates a need.
- Keep context transfer under the existing reserved modes. Before implementing
  `handoff`, agree its artifact/reference model, optional relationship to a
  plan, authoring inputs, and review UX; do not treat it as generic workspace
  metadata or duplicate the source conversation into SQLite. Native fork plus
  explicit child compaction is complete; handoff remains a separate design
  task rather than the next implicit coding step.
- The workspace vocabulary/schema migration, create convenience pipeline,
  multi-repository CLI slice, native Git-approval proof, and interactive
  decision closure are complete.
- Research Codex's native lifecycle extensibility before designing CoCo hooks.
  Keep the distinction between internal normalized events and executable user
  automation explicit; hooks must not silently inherit credentials or block
  coordinator state transitions without a deliberate policy.
- The public site now passes a production export under the `/coco` GitHub Pages
  project subpath. Keep that static-export check when changing its routing or
  deployment workflow.
- Phase 2 and its review are complete. Do not split a workspace now. If
  cross-platform support is scheduled next, begin Phase 3 by moving the
  existing Unix RPC backend behind the common transport API, then add Windows
  named pipes with native CI before claiming Windows support. Otherwise prefer
  a user-visible capability such as safe approval responses or daemon recovery
  over more structural movement.
