---
type: Working Document
title: "main: repository foundation and first architecture"
description: Tracks repository bootstrap, the Rust baseline, and early CoCo architecture decisions on main.
tags: [work, branch, bootstrap, rust, mcp, architecture]
status: active
branch: main
updated: 2026-09-07
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
- [x] Record the detached-worktree direction, then implement it as an explicit
  Git-binding choice while retaining new-branch creation as the default.
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
- [x] Unify the collection and cross-repository CLI vocabulary before release.
  - [x] Make `list` the documented spelling and `ls` a visible alias for
    workspace, repository, and model collections.
  - [x] Initially reserved `--all-repos`/`-a` for collection/status overviews
    and added `--global`/`-g` for resolving one named workspace. The interactive
    slice below supersedes the duplicated status overview and reserves `-a`
    for `list` alone.
  - [x] Initially made workspace-free status mirror the list overview. The
    interactive slice below supersedes it: status now always targets one
    explicit or interactively selected workspace.
  - [x] Update parser/dispatch/process coverage, public docs, canonical product
    knowledge, and the exact CLI help contract.
- [x] Add one consistent interactive selection layer for incomplete human CLI
  invocations without weakening deterministic/scripted behavior.
  - [x] Keep `list`/`ls` collection commands non-interactive and make targeted
    workspace arguments optional only for terminal-assisted use.
  - [x] Support immediate arrow-key/`j`/`k` cursor movement, Enter confirmation,
    direct one-key selection for 1 through 9, and cancellation through one
    reusable picker also used by `decide` options.
  - [x] Let `create` prompt only for a missing name/repository, and let
    `status`, `send`, `jump`, and `diff` select a missing workspace in the
    resolved repository or global `-g` scope.
  - [x] Preserve `send <workspace> <message>` as the fully explicit form;
    prompt for the message only when its second positional value is absent.
  - [x] Never prompt for JSON, non-terminal, or explicit no-input execution;
    update parser/process tests, public docs, and canonical CLI knowledge.
- [x] Implement the initial durable pending-decision slice and
  `coco decide <decision-id>`; its SQLite ownership was superseded by the
  generation-local native-first registry below while retaining the CLI flow.
  - [x] Persist supported native decision requests before presentation, bound
    to the exact App Server generation, thread, turn, and request ID.
  - [x] Project pending decisions through workspace status without inventing
    a second thread state machine.
  - [x] Render native choices as numbered options and accept a number;
    user-input requests may accept free text where the native schema permits
    it. Cursor-driven selection was initially deferred and is completed by the
    shared interactive slice above.
  - [x] Resolve through the original live App Server request, handle stale or
    already-resolved requests safely, and cover restart/orphan behavior.
  - [x] Update public and canonical internal documentation only for behavior
    proven by the completed implementation and tests.
- [ ] Design workspace annotations and external references as a deliberate future
  feature. Decide typed versus free-form values, mutation/audit semantics,
  privacy and display rules, fork/handoff inheritance, and explicit projection
  into Codex before adding any CLI or RPC field.
- [x] Implement the original coupled context-transfer slice, later generalized
  by the independent creation model below: native same-repository workspace
  fork with optional child compaction.
  - [x] Add typed CLI/RPC source selection and record immutable fork
    provenance without adding a fourth context mode.
  - [x] Bind native `thread/fork` to the destination worktree and configuration.
  - [x] Wait for child-only `thread/compact/start` completion and cover ordering,
    failure retention, idempotency, and recovery.
  - [x] Pin the relevant Codex schemas, update public docs for shipped behavior,
    run the full gates, and create a checkpoint commit.
- [x] Add App-Server-backed model discovery and an explicit per-workspace model
  override without duplicating Codex configuration resolution.
  - [x] Add native model discovery over the daemon-owned App Server's
    `model/list` result; the current public spellings are `coco model list`
    and `coco model ls`.
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
  - [x] Build and smoke-test release archives for all currently supported host
    platforms before allowing a release, without claiming Windows support.
  - [x] Keep automatic publication opt-in until the remaining public-release
    blockers are deliberately resolved. The user selected MIT and supplied a
    local crates.io token for secure GitHub-secret upload; never record it.
  - [x] Ship an installable default Nix package with `coco`, `cocod`, and
    `coco-mcp`, then document the verified flake path publicly.
  - [ ] Replace the truthful Cargo Git installation with an explicit registry
    alpha version only after the first crates.io package actually exists;
    prereleases are not selected by a bare `cargo install`.
  - [ ] Run the complete sequential verification gates and record the final
    release recommendation.
- [x] Re-audit every public page against the shipped CLI, the pinned Codex
  executable, and current official Codex documentation before the first alpha.
  - [x] Replace the legacy `[profiles.<name>]` interpretation with CoCo's
    explicit `$CODEX_HOME/<name>.config.toml` complete-overlay files across
    runtime, tests, help, and documentation; do not describe that storage
    convention as native `codex --profile` behavior.
  - [x] Use `alpha` consistently instead of the less precise `early preview`.
  - [x] Keep `cocod` startup explicit for the first alpha and record an
    opt-in, cross-platform user-service integration as separate follow-up;
    installing a package must not silently start a background process.
- [x] Verify the newly merged experimental Codex worktree surfaces from
  official source and a containing release, then reassess CoCo's product
  boundary feature by feature. Do not defend worktree creation as sufficient
  differentiation if upstream now owns it; compare durable daemon ownership,
  multi-workspace and multi-repository control, status/decisions, reattachment,
  context transfer, MCP, and automation before recommending the first alpha.
- [x] Decide with the user whether CoCo should continue as a narrower headless
  orchestration/control layer. The user selected the native-first reduction;
  do not publish to crates.io unless its multi-client product proof is
  compelling enough to justify the compatibility and maintenance burden.
- [x] Complete a hard product-reduction audit before that decision.
  - [x] Inventory every current CLI/RPC/MCP operation and persisted authority.
  - [x] Compare the implementation with both the pinned Codex 0.147.0 schemas
    and the current official App Server, SDK, CLI, project, and worktree
    surfaces.
  - [x] Separate CoCo-owned cross-system bindings and operation idempotency from
    Codex-owned thread, turn, status, content, and model state.
  - [x] Define one concrete headless multi-client workflow and a release
    go/no-go gate without implementing or publishing anything.
- [x] Design a native-first architecture-overhaul proposal that removes
  duplicated Codex state while preserving CoCo's multi-repository,
  multi-client control layer. Evaluate per-command, daemon-owned, and
  externally supervised App Server lifetimes; reduce SQLite to the bindings
  and operation facts that neither Codex nor Git owns.
- [x] Confirm the native-first overhaul with the user before changing runtime
  behavior, persistence, canonical architecture, or public documentation.
- [ ] Implement the confirmed native-first overhaul without expanding product
  scope or publishing a release.
  - [x] Prove stable non-loading thread reads and optional native history
    hydration against the pinned published Codex 0.147.0 process.
  - [x] Cut passive workspace list/status/follow projections over to validated
    native reads without deleting compatibility data.
  - [ ] Replace the completed-message compatibility event only when a stable,
    bounded native history read is available. Pinned 0.147.0 exposes only an
    unbounded stable read; its bounded pagination methods require the
    `experimentalApi` capability and are not an accepted core dependency.
  - [x] Replace eager startup resume with per-workspace activation for `send`
    and the internal attach preflight used by `jump`; native context creation
    reads and forks its exact source independently.
  - [x] Reduce native notifications and generation-bound decisions to the
    minimal live in-memory path while retaining equivalent client behavior.
  - [x] Introduce the minimal operation ledger and stop the remaining
    compatibility writes after crash and migration gates pass.
    - [x] Add an append-only-compatible schema-v6 `operations` table for
      `turn_start` intent/result correlation; migrate only legacy rows that
      already carry a client operation ID.
    - [x] Persist `prepared` before dispatch and `dispatching` before the
      App Server write. Record `accepted` only with a proven native turn ID;
      reconcile an unconfirmed dispatch to `uncertain` and never retry it
      automatically.
    - [x] Keep the current-generation active-operation guard in memory so a
      transient native `idle` read cannot authorize a second turn. Let native
      status—not the durable operation state—remain user-visible runtime truth.
    - [x] Stop production writes to legacy turn rows, `active_turn_id`, and
      turn start/completion events; retain the old schema and migration tests
      through a separate physical-cleanup checkpoint.
    - [x] Stop initial thread-status snapshot writes once creation responses
      are projected in memory, while retaining provisioning/failure events and
      the temporary bounded completed-output event.
    - [ ] After the product proof, remove the now-read-only legacy status,
      turn, and decision schema in a separate physical-cleanup revision.
  - [ ] Run the two-repository/multi-client product proof and revisit the
    private-alpha go/no-go decision with the user.
- [x] Split the 2,500-line Coordinator test module in a separate structural
  checkpoint after the current ownership change is committed. Keep one shared
  fixture/fake worker and group behavior by workspace, context/activation,
  turns/events, and decisions; do not alter product behavior during the move.
- [x] Split the 2,000-line process-smoke harness into scenario, fake-App-Server,
  and process-support modules without changing its two integration scenarios.
- [ ] Keep handoff deferred as a separate artifact-design task. Treat authoring
  and consumption independently; consider agent-generated material, existing
  Markdown, direct CLI input, ticket or other external references, and an
  optional plan without fixing one automatic prompt. Define provenance,
  redaction, freshness, size limits, review/edit behavior, and reference
  semantics before enabling `handoff`.
- [x] Revisit creation inputs and context transfer after the native-first
  architecture overhaul; discuss the CLI and invariants with the user before
  implementation.
  - [x] Verify the current official split: Codex-managed app worktrees are a
    Git/filesystem concern, while App Server `thread/fork` copies stored
    conversation history and accepts an independent destination `cwd`.
  - [x] Draft independent code-base, context-source, worktree-binding, and
    dirty-state-transfer axes without changing runtime behavior.
  - [x] Confirm the proposed CLI names, branch-backed initial default, and the
    first supported dirty-state boundary with the user.
  - [x] Separate the Git starting point from the Codex context source with
    independent `--base`/`--base-workspace` and
    `--context-workspace`/`--context-thread` options.
  - [x] Allow the context source to be either a same-repository CoCo workspace
    or an exact readable native Codex `thread.id`; validate its native state
    without loading it and persist bounded source provenance.
  - [x] Implement explicit tracked and ordinary-untracked carry, the
    `-d`/`--dirty` shorthand, and Codex-compatible `.worktreeinclude` for
    selected ignored files without mutating the source checkout.
- [ ] Design user-configurable lifecycle hooks as a separate future feature.
  Before defining CoCo hooks, inventory the pinned Codex CLI and App Server's
  native hooks, notifications, and lifecycle events so CoCo can expose or
  extend existing signals instead of duplicating them. Cover workspace creation,
  thread/turn start, agent state transitions and terminal outcomes, then decide
  execution context, filtering, ordering, retries, timeouts, failure policy,
  secret handling, auditability, and platform behavior.

## Proposed workspace-creation model

Status: confirmed and implemented in the working tree on 2026-09-07. Runtime,
protocol, schema, focused tests, and public documentation are aligned; final
whole-tree verification and a local checkpoint remain.

The former public `--fork-from` was overloaded: one workspace supplied both
the Git `HEAD` used for the destination worktree and the Codex thread used for
conversation history. It is now replaced by four independent choices:

| Axis | Proposed request | Meaning |
| --- | --- | --- |
| Code base | `--base <revision>` or `--base-workspace <workspace>` | Resolve only the immutable destination `base_sha`; default to `HEAD` of the invoked checkout. |
| Conversation context | fresh, `--context-workspace <workspace>`, or `--context-thread <thread-id>` | Start a new native thread or call `thread/fork`; never selects code. |
| Worktree binding | default new branch, `--branch <name>` for another new branch, `--checkout <branch>` for an existing branch, or `--detached` | Decide whether creation allocates, reuses, or omits a Git branch; never selects context. |
| Local changes | none, `--carry-changes`, optional `--carry-untracked`, and `.worktreeinclude` | Snapshot explicitly selected source-checkout state onto the new worktree; never changes conversation history. |

Offer `-d`/`--dirty` as a CLI-only shorthand for
`--carry-changes --carry-untracked`. Normalize it into that typed local-state
selection before constructing the daemon request; do not persist a `dirty`
worktree or context mode. Worktree binding remains independent:
`-D`/`--detached` may be used alone or combined with `-d`, while branch-backed
creation may also carry the same dirty state. A different explicit `--base`
remains subject to the snapshot base constraint below. Worktree-local extras
remain an orthogonal repository convention rather than another flag.

Do not auto-detect whether a string is a workspace reference or Codex thread
ID. Separate mutually exclusive CLI options keep typos deterministic. A direct
Codex source means `thread.id`, not `thread.sessionId`: the latter identifies a
fork tree's root and can point at a different conversation branch. CoCo should
validate the supplied thread with non-loading `thread/read` and always create a
new child with `thread/fork`; it must not silently adopt or move the source
thread. A readable inactive thread may come from another repository because
its history and the destination code are intentionally independent. Persist
its exact thread ID and source `cwd` as provenance, while the destination
thread is bound only to the newly created worktree. `thread.sessionId` is not
part of CoCo's creation contract.

The first dirty-state boundary should be conservative:

- no transfer remains the default and preserves today's clean-check rule;
- `--carry-changes` captures tracked staged and unstaged changes from the
  checkout through which `coco create` was invoked, preserving their index vs
  worktree placement with separate binary patches;
- `--carry-untracked` additionally captures ordinary non-ignored untracked
  files and is valid only when tracked changes are also being carried. It is
  independent of `.worktreeinclude` because the two file sets are disjoint;
- every managed local worktree honors a repository-root `.worktreeinclude`,
  using the same `.gitignore`-style allowlist convention documented by Codex
  for ignored local setup files. This applies independently of
  `--carry-changes`, skips source symlinks, never overwrites checkout files,
  and uses path/count/byte limits. Other ignored files are not copied;
- the source is never stashed, reset, staged, or otherwise mutated. Initial
  implementation should require the chosen base SHA to equal that checkout's
  `HEAD`; applying a dirty snapshot across a different base needs a separately
  designed conflict/rebase mode.

Snapshot contents exist only in owner-process memory, not SQLite, events, logs,
or context metadata. Persist only source/base identity, bounded manifest
metadata, counts, sizes, and a category-separated hash. A process crash cannot
resume from vanished contents: startup preserves a diagnosable failed
provisioning record and must never recapture a possibly changed source tree
silently. The creation operation fingerprint includes all four selections.

A workspace name is always a CoCo routing label, not a Git-branch identity.
Branch-backed creation defaults to a new `coco/<workspace>` branch, while
`--branch <name>` selects another new safe branch name. The explicit
`--checkout <branch>` variant instead binds the worktree to an already existing
local branch at that branch's current `HEAD`; it does not accept `--base` or
`--base-workspace`, and must fail if Git reports that branch checked out in
another worktree. CoCo never bypasses that Git safety check with `--force`.
Using the same value for workspace name and checked-out branch is valid, for
example `coco create feat/login --checkout feat/login`, although the managed
filesystem path remains CoCo-owned. `--detached` conflicts with both branch
options and allocates no branch. Persist a typed worktree mode and permit
`branch_name = null`; verify detached `HEAD` rather than `symbolic-ref`.
Detached creation is supported, but durable workflow completeness still needs
an explicit atomic create-branch or promotion operation to anchor its commits
before manual worktree removal. Keep the new-branch default until that
operation and cleanup safeguards exist.

Recommended implementation order:

1. [x] Replace the flat `context_mode`/`fork_from` request combination with a
   tagged context request and typed workspace-vs-thread source; migrate the old
   persisted fork descriptor losslessly.
2. [x] Make `--base` legal with either context source and add
   `--base-workspace`; remove source-worktree cleanliness as a context
   requirement.
3. [x] Add and verify new-branch, existing-branch, and detached worktrees. Keep
   atomic branch promotion as a separate follow-up.
4. [x] Add `.worktreeinclude`, explicitly selected tracked/untracked snapshots,
   and the normalized `--dirty` preset with crash, path, size, symlink, overwrite, and
   secret-boundary tests.
5. [x] After the combinations are covered, retire `--fork-from` from public
   help; it may remain a hidden compatibility alias for one schema revision.

## Proposed native-first architecture overhaul

Status: confirmed by the user on 2026-09-07. It deliberately schedules no
hooks, handoff, worker MCP registry, Agentgateway, Windows, or release work.

### Target ownership

| Authority | Owned truth |
| --- | --- |
| Codex App Server | threads, turns, history/items, runtime status and active flags, models, effective execution behavior, and native server requests |
| Git | repositories, refs, commits, current worktree contents, and dirty/ahead state |
| CoCo | curated repository registry, human workspace identity, expected worktree/base binding, Codex thread binding, creation provenance, provisioning saga, mutation idempotency, and routing of requests that are live in the current daemon generation |

CoCo must project native data rather than copy it into a competing thread,
turn, or event model. Unknown active flags remain lossless at the adapter edge.
An unknown thread `status.type` currently fails the native read and projects
the workspace as unavailable; CoCo must not guess its meaning from an App
Server method name.

### Runtime topology

Retain one long-lived `cocod` as the default broker and let it continue to
spawn one shared `codex app-server`. This is already how the implementation
works. Starting a new App Server for every `coco` invocation is rejected for
mutating operations: when the command exits it would either terminate the
runtime or have to leave a hidden supervisor behind, and a later CLI process
has no documented way to reclaim a connection-bound pending approval or input
request. It would therefore remove the headless background workflow or merely
recreate `cocod` less explicitly.

The daemon's reduced responsibilities are:

1. own or connect to one compatible App Server runtime;
2. serialize CoCo's cross-system mutations and SQLite writes;
3. keep subscriptions needed for background turns and route live server
   requests to `coco decide`;
4. expose the same typed local control surface to CLI and MCP; and
5. publish the authenticated endpoint used by the official TUI for `jump`.

An externally supervised App Server is a later optional lifecycle mode, not a
prerequisite for this overhaul. It needs explicit endpoint/authentication,
version and capability checks, reconnection semantics, and proof for requests
that become pending while CoCo is disconnected. The experimental
`codex remote-control` command is not selected as CoCo's local protocol host.
Do not implement simultaneous direct and daemon mutation paths.

After the reduction and child-failure/SIGTERM behavior are reliable, normal
`coco` commands may lazily ensure that `cocod` is running. That is a user-
experience improvement, not removal of the broker; `cocod` remains available
in the foreground for diagnostics.

### Delivery phases

#### 0. Freeze and prove the native contract

- Select a published supported Codex build; do not base the cutover only on
  rolling documentation or unreleased worktree changes.
- Extend the generated-schema and model-free real-Codex tests for stable
  `thread/read`, `thread/list`, `thread/loaded/list`, `thread/resume`, native
  status/active flags, and `thread/read(includeTurns)` history hydration.
- Prove how `clientUserMessageId` appears in stored history before relying on
  it to reconcile a turn accepted immediately before a crash.
- Retain direct tests for approval and structured-input request correlation,
  remote-TUI unsubscribe, and App Server restart.
- Do not make experimental `thread/turns/list`, `thread/items/list`, or
  undocumented section APIs a core dependency.

Exit: every native read used by the redesign passes against the selected
published executable. Otherwise stop the reduction at the unsupported seam.

#### 1. Introduce native read projections without deleting data

- Separate `WorkspaceBinding` and `ProvisioningState` from a read-only
  `ThreadProjection` and `GitProjection` in the typed application boundary.
- Extend the existing earned `WorkerRuntime` port with only the required
  thread reads; keep App Server JSON translation in `daemon/worker.rs`.
- Make workspace list/status use cases capable of joining local bindings with
  native Codex and Git observations.
- Shadow the existing SQLite projection first and report mismatches in tests
  or diagnostics without changing CLI/JSON behavior.

Exit: the compiler and contract tests prevent a native thread status from
being treated as a CoCo provisioning lifecycle.

#### 2. Cut reads and recovery over to Codex and Git

- Build `ls` and `status` from the small CoCo binding plus current native
  `thread/read`/`thread/list` data; missing App Server data projects honestly
  as unavailable rather than falling back to a stale stored status.
- Make `status --follow` hydrate a native snapshot and then observe live
  notifications. A polling-first implementation is acceptable; a later local
  stream must subscribe before hydration and deduplicate the buffered race.
- Replace the stored `agent.message.completed` copy only after the selected
  stable App Server exposes a bounded way to retrieve the latest response.
  Never load an unbounded complete thread merely to render follow output, and
  do not enable an experimental capability for this core path.
- Stop eagerly resuming every ready thread on daemon startup. Passive list and
  status reads must not load a thread. `send` resumes on demand; `jump` first
  asks the daemon to load and subscribe before attaching the TUI so later
  decisions still reach CoCo.
- Treat an externally deleted/archived thread or removed/mismatched worktree
  as a broken binding; never create an implicit replacement.

Exit: no current-status read needs stored native status or turn lifecycle.
The completed-message compatibility event may remain solely for follow output
until the bounded-history gate above is met.

#### 3. Reduce the live event path

- Keep notification handling only for an in-memory current snapshot/wakeup,
  compaction synchronization, live pending-request routing, and runtime
  disconnect monitoring.
- Move actionable decisions to a generation-bound in-memory registry. Current
  SQLite rows are already unusable after a daemon/App Server restart, so
  persistence does not provide request recovery.
- Stop persisting user messages, plans, diffs, native errors, turn completion,
  and thread-status notifications. Retain completed agent messages only while
  `status --follow` lacks a stable bounded native replacement.
- Replace internal `event.list` polling once `status --follow` has an equal or
  better native-backed path. Future hooks must earn a focused redacted outbox;
  they do not justify retaining a copy of Codex history today.

Exit: cross-client `decide`, `jump`, `send`, and follow behavior remains intact
within a daemon generation, while native conversation data has one owner.

#### 4. Shrink SQLite through a reversible bridge migration

Keep three responsibilities, whether represented as three tables or an
equivalent normalized schema:

- `repositories`: stable repository ID, canonical root/common directory, and
  registration time;
- `workspaces`: ID/name/repository, expected worktree/branch/base, bound native
  thread, CoCo-only provisioning/error state, and minimal profile/context
  provenance; and
- `operations`: operation kind and ID, versioned request fingerprint, workspace
  correlation, accepted native ID, outcome, and timestamps. It stores no raw
  prompt.

Remove after the read cutover: `thread_status_*`, `active_turn_id`, the local
turn history, native event payloads, persisted decisions, `legacy_goal`, the
unused completed lifecycle, and the write-only MCP audit log unless a concrete
consumer is added first. Retain named-profile source path/hash, model override,
and fork/source provenance only where recovery or audit behavior actually uses
them; remove copied effective native settings.

Use an additive bridge migration first: validate and back up the v5 database,
populate new records in one transaction, keep old tables read-only for one
revision, and cover migrations from every existing schema with golden
databases. A later migration physically drops the legacy tables. Rollback
restores the pre-migration database and reports any worktrees/threads created
after it instead of silently orphaning them.

#### 5. Simplify the daemon and prove the product

- Keep managed-child App Server ownership as the zero-configuration default;
  define child-exit, SIGTERM, logging, and restart behavior before adding
  daemon auto-start or an OS user service.
- Consider an explicit external-endpoint mode only after the default path is
  complete and only if surviving a `cocod` restart is a demonstrated need.
- Run the complete two-repository/two-client proof: create and send from CLI,
  let the invoker exit, observe and send through MCP, answer a native request
  from a separate CLI, attach/detach the same thread with `jump`, restart and
  reconcile, and replay operation IDs without duplicate worktrees or threads.
- Verify that the reduced database contains no user or agent message bodies.

Exit: reconsider public alpha release only when that workflow passes against a
published Codex build. If real use collapses to one person calling
`create -> jump`, reduce CoCo to private glue rather than maintaining a
parallel control plane.

### Implementation progress (2026-09-07)

- Phase 0 now has a model-free real-process proof against the published pinned
  `codex-cli 0.147.0`: `thread/read` preserves the exact ID, `cwd`, name, native
  `notLoaded` status, and completed turn history across App Server restarts;
  reads do not load the thread. The generated read/list/loaded-list schemas are
  part of the compatibility comparison.
- The same proof found that `thread/list` does not discover CoCo's prepared
  empty thread under either the reported `vscode` source or `appServer`, even
  with the exact `cwd`. CoCo therefore must keep its explicit thread binding
  and use `thread/read`; native listing is not a recovery/discovery authority.
- `thread/read(includeTurns)` does not expose `clientUserMessageId` in the
  selected schema. Turn-start crash reconciliation must consequently retain an
  `uncertain` operation outcome or find a separately proven native correlation;
  it must not guess that retrying is safe.
- Phase 1 read projection is implemented in the working tree: passive workspace
  list/status/event reads consult `thread/read`, validate ID and `cwd`, derive
  phase from native status in memory, and project failures as unavailable
  without overwriting the stored compatibility snapshot. `status --follow`
  still obtains completed text from the existing normalized event stream; a
  stable second terminal poll closes the native-status/notification race.
- A real pinned-process probe rejected `thread/turns/list` and
  `thread/items/list` without the `experimentalApi` initialization capability.
  The stable `thread/read(includeTurns: true)` does hydrate completed history,
  but returns the entire thread in one response. CoCo's transport deliberately
  caps a JSON frame at 8 MiB, so automatic full-history hydration could take
  down the shared App Server channel for a long thread. The attempted bounded
  adapter was removed rather than silently enabling experimental API surface.
- Phase 2 activation is implemented in the working tree: daemon startup no
  longer resumes every `ready` workspace. `send` and the internal
  `workspace.attach` used before `jump` validate the binding and resume only
  when the workspace thread is `notLoaded` or the current daemon connection
  lacks a subscription. Native context creation reads and forks the exact
  source thread without coupling it to Git-base or source-worktree activation.
  Start, fork, and resume establish a generation-local subscription marker;
  globally loaded state is not mistaken for ownership by this connection.
- A prolonged shadow-only checkpoint was deliberately skipped for this read
  slice. The pinned published 0.147.0 real-process test proves non-loading
  reads, exact ID/`cwd`, restart persistence, and optional history hydration;
  focused fake-runtime tests cover mismatch, failure, phase, follow-output
  stabilization, and activation behavior. The cutover remains reversible
  because no table or existing record was deleted. Eager recovery
  status/failure writes were deliberately removed with the startup-resume path.
  This exception does not weaken the pending operation-ledger, remaining
  stop-write, migration, or crash-reconciliation gates.
- Phase 3 decision/event reduction is implemented in the working tree.
  Supported approval/input requests now live only in a generation-local
  registry with lock-protected single submission, native resolution,
  disconnect orphaning, and no durable prompt, answer, or native correlation.
  Sent prompts and native status, plan, diff, error, and unsupported-request
  observations no longer create events. Local turn rows and start/completion
  events, start/resume status snapshots, provisioning events, and completed
  agent text remain until the operation-ledger and bounded-output gates pass.
- The roughly 2,500-line Coordinator suite is now split without behavior
  changes. Its shared fake worker, fixture, and Git helpers remain in
  `src/coordinator/tests.rs`; workspace, context/activation, decisions, and
  native-event cases live in focused child modules.

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
- 2026-09-05 — Superseded on 2026-09-06: resolve a named execution profile from
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
  free-text input; cursor navigation was deferred at this checkpoint and later
  completed through the shared CLI picker recorded below. This records the
  original priority sequence after state and `jump` work.
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
- 2026-09-06 — **Superseded on 2026-09-07.** Initially treat every correlated
  App Server request as an observational, sanitized
  `server_request.received` event until the pending-decision model is
  implemented. Request method names never change runtime state. The native-
  first reduction later removed that generic persisted observation and made
  `thread/read` the runtime authority.
- 2026-09-06 — Treat a successfully launched remote TUI as an ordinary client
  attachment, not as ownership of the turn. Normal unsubscribe and unexpected
  transport loss must leave the daemon connection and active turn intact;
  cancellation requires Codex's separate explicit interrupt operation.
- 2026-09-06 — Implement the first recovery level without claiming process
  survival: a new App Server resumes persisted `ready` threads after validating
  their ID, worktree, and immutable named-profile provenance. Superseded on
  2026-09-07 by passive native reads plus selected activation on demand. Turns
  unfinished at daemon loss still cannot survive an owned App Server crash;
  that would require a separately supervised worker lifetime.
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
- 2026-09-06 — Superseded on 2026-09-07: model CLI repository scope with an
  omitted path meaning `.`, a path selecting one registered repository, and
  `--all-repos` requesting both daemon-wide listing and unique workspace-name
  resolution. The later CLI-vocabulary decision splits those two meanings.
  Full workspace IDs still resolve globally; local lookup never falls through
  silently, and paths/names are never combined into a colon-delimited ID.
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
- 2026-09-06 — Superseded on 2026-09-07: use `-a` as the short spelling of an
  overloaded `--all-repos` list/lookup scope. The ambiguity behavior, bounded
  matching repository paths/IDs, leading-path selection, and globally resolved
  workspace IDs remain; unique global name lookup now uses `--global`/`-g`.
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
- 2026-09-06 — Superseded on 2026-09-07: deliver native fork before handoff
  with one workspace coupled to both code and context. The independent
  creation contract below preserves native fork/compaction but removes that
  coupling. Handoff stays deferred until its plan, document, CLI-input, and
  external-reference model is settled.
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
- 2026-09-06 — Stamp the never-published source baseline as
  `0.1.0-alpha.0`, not stable-looking `0.1.0`. Local, source, and Nix builds
  must identify as prerelease software before Semantic Release creates the
  first `0.1.0-alpha.1` tag.
- 2026-09-06 — Keep alpha package installation side-effect free. `cocod`
  remains an explicit foreground process; an opt-in cross-platform user
  service waits for graceful SIGTERM and defined App Server child-exit,
  reconciliation, logging, and restart behavior rather than shipping a
  Linux-only unit prematurely.
- 2026-09-06 — Use Cargo as the primary public installation path without
  claiming an unpublished registry artifact. Until the first crates.io alpha,
  install the package from the Git repository with its lockfile; after
  publication, replace that command with an explicit prerelease version because
  Cargo does not select prereleases implicitly. Nix remains a supported
  secondary installation path.
- 2026-09-06 — Restructure the public site around eight product pages plus the
  landing page: overview, installation, quickstart, three task-oriented guides,
  CLI reference, and troubleshooting/limitations. Explain only the user-visible
  lifetime of `cocod`; keep service design, persistence, protocol, gateway, and
  roadmap material in `knowledge/`.
- 2026-09-06 — Correct the shipped execution-profile contract to the pinned
  Codex convention: `default` is an empty per-thread overlay and a named profile
  is the parsed complete `$CODEX_HOME/<name>.config.toml` document. Persist only
  provenance and redacted settings, reject named-overlay drift when on-demand
  activation reloads it, and disclose that changes to inherited base
  configuration are not pinned.
- 2026-09-06 — Pause the first crates.io publication after three upstream Codex
  PRs made managed worktree creation available to `codex exec`, interactive
  startup, and TUI new/fork flows. A merge to Codex main is not a released
  compatibility target, but CoCo must no longer claim worktree creation or a
  one-command TUI launch as sufficient differentiation. Publication now
  requires an explicit product decision after testing the first containing
  Codex release.
- 2026-09-07 — Make stable `thread/read` authoritative for passive ready-
  workspace status. Use the stored CoCo thread ID as the lookup key and
  validate both the returned ID and canonical `cwd`; never fall back to a
  persisted native snapshot after a read failure.
- 2026-09-07 — Keep completed agent text in the compatibility event stream for
  now. Pinned 0.147.0's stable history read returns the complete thread, while
  bounded turn/item pagination requires `experimentalApi`; neither is safe as
  an automatic core follow dependency. Do not enable the experimental
  capability merely to remove this one mirror.
- 2026-09-07 — Replace eager daemon-start recovery with explicit activation.
  `send` and `workspace.attach` for `jump` resume only the selected validated
  thread when it is unloaded or this daemon generation lacks a subscription.
  Track that subscription in memory because another client's globally loaded
  thread does not prove that `cocod` receives its notifications and server
  requests. Native context forks use the exact readable source thread without
  coupling it to source-worktree activation.
- 2026-09-07 — Permit the initial read cutover to bypass a prolonged shadow-only
  checkpoint because the published pinned 0.147.0 process proof and focused
  mismatch/failure tests cover the adopted stable contract, while schema v5
  retains its tables and existing records as a reversible bridge; only the
  obsolete eager-recovery status/failure writes stop in this slice. Treat the
  remaining event/decision reduction, operation-ledger design, stop-write,
  physical migration, and crash reconciliation as later gates rather than
  folding them into this checkpoint.
- 2026-09-07 — Keep actionable approvals and structured input only in the
  daemon generation that owns the native callback. Perform the pending-to-
  submitted transition under one lock before awaiting the response; never
  write its prompt, native correlation/options, or user answer to SQLite.
  Disconnect or response failure orphans it, and restart drops it rather than
  replaying an unanswerable request.
- 2026-09-07 — Superseded in part by the operation-ledger slice: stop
  normalizing user prompts, native status/plan/diff/error
  notifications, decisions, and unsupported requests into durable events.
  The later ledger removes local turn correlation writes; completed agent text
  remains only until the stable bounded-output gate is resolved.
- 2026-09-07 — Split the oversized Coordinator tests after the decision/event
  checkpoint while retaining one shared parent fixture. Group behavior cases
  by workspace, context/activation, decisions, and native events; keep the
  move behavior-neutral and separate from authority changes.
- 2026-09-07 — Use the operation ledger only for durable `turn_start`
  idempotency and native-result correlation, never as another turn-history or
  status store. Commit intent before dispatch and a dispatch marker before the
  external call. Only the direct App Server response may prove acceptance:
  `turn/started` contains the native ID but does not echo
  `clientUserMessageId`, so matching by thread would be ambiguous. Otherwise a
  crash/error becomes `uncertain`, which the same operation ID must never
  redispatch. Keep the short current-generation concurrency guard in memory
  and continue deriving visible runtime state from Codex.
- 2026-09-07 — Replace coupled `--fork-from` creation with four typed axes:
  immutable Git base, fresh/workspace/exact-native-thread context, new/existing/
  detached worktree binding, and explicit local-state carry. Keep the old flag
  hidden for one compatibility revision. `-d` means tracked plus ordinary
  untracked carry, `-D` means detached, and `-dD` composes them. Adopt
  Codex's `.worktreeinclude` convention only for ignored files, never as a
  reason to copy every ignored path. Snapshot content remains memory-only and
  the source checkout is never mutated.
- 2026-09-07 — Split the process integration harness by scenario, fake App
  Server, and process support after it exceeded 2,000 lines. Keep one
  integration-test crate and shared constants; this is a behavior-neutral
  module split rather than a new test framework.
- 2026-09-07 — Use `list` as the visible canonical collection verb and `ls` as
  its visible equivalent alias for workspaces, repositories, and models.
  This checkpoint initially reserved `--all-repos`/`-a` for collection/status
  overviews and used `--global`/`-g` only for one named workspace. The next
  decision supersedes the duplicated status overview.
- 2026-09-07 — Keep collection and target UX separate: `list`/`ls` is the only
  workspace overview and `--all-repos`/`-a` is its collection scope. `status`
  always presents one detailed workspace, selected explicitly or through a
  human-terminal picker; `--global`/`-g` expands that target lookup or picker.
  Use the same picker for `send`, `jump`, `diff`, and Codex decision options.
  Arrow keys and `j`/`k` move immediately, Enter confirms, 1 through 9 select
  directly, and Escape/`q`/Ctrl-C cancel. Non-terminal, JSON, and
  `--no-input` execution must never prompt. Add approval-only `--choice` so a
  scripted decision does not need to fake a TTY.

## Findings

- Interactive repository choice needs the daemon's canonical Git identity,
  especially from linked worktrees. The narrow `repository.resolve` read
  avoids duplicating Git discovery or opening SQLite in the CLI; selectors
  still obtain their choices from the existing deterministic repository and
  workspace list methods and return a stable workspace ID.
- Terminal assistance is gated on both stdin and the stderr prompt stream.
  Normal stdout stays reserved for command results; collection and JSON paths
  do not consult the interactive adapter.

- The old `--all-repos` flag changed two dimensions at once: `ls -a` selected
  a collection, while `status/send/jump/diff -a <workspace>` selected one
  globally resolved reference. The implementation existed and handled
  ambiguity safely, but the shared spelling made a target appear nonsensically
  required for an "all" command. Separating overview (`-a`) from reference
  lookup (`-g`) removes that ambiguity without changing the daemon protocol.
- The pinned generated schema and current official App Server documentation
  both show `clientUserMessageId` on `turn/start`, but not in the returned
  `Turn` or `turn/started` notification. A same-thread event therefore cannot
  safely resolve an ambiguous CoCo dispatch. Completion notifications may
  overtake the direct response; the runtime guard records that native ID only
  in memory and clears itself if the later response confirms the exact ID.
- The CLI previously generated a send operation ID internally but gave the
  user no way to reuse it after losing the local RPC response. `coco send` now
  includes the ID in error context and accepts `--operation-id <ID>` for an
  exact replay; a different message under that ID remains a conflict.
- Schema v6 is intentionally additive. It migrates operation-bearing legacy
  turns into the minimal ledger, clears old persisted active-turn pointers,
  and leaves the legacy status/turn/decision/event shapes readable. Physical
  deletion is deferred until after the multi-client product proof so this
  authority cutover remains independently reversible.
- Schema v7 adds only the persisted worktree mode. Existing rows migrate to
  `new_branch`; new records may store `existing_branch` or `detached`, with
  a null branch valid only for detached. Context descriptor v3 records the
  independent requested and resolved creation axes plus bounded local-state
  provenance.
- Codex's `thread.sessionId` identifies the root of a live session/fork tree,
  while `thread/read` and `thread/fork` address exact `thread.id` values.
  Accepting only the latter prevents a session-root lookup from silently
  choosing the wrong conversation branch.
- `.worktreeinclude` is a Codex convention, not a Git feature. Git supplies
  the ignore matching used to prove eligibility. Ordinary non-ignored
  untracked paths and selected ignored paths are intentionally separate sets.
- A detached CoCo worktree is a normal Git-registered worktree and is not
  deleted when `cocod` exits. Its commits remain reachable through that
  registration, but manual directory removal/pruning can eventually leave
  unreferenced commits eligible for garbage collection; branch promotion
  therefore remains a distinct product follow-up.
- The pinned Codex 0.147.0 schema bundle contains `thread/list` with a `cwd`
  filter, stable `thread/read` with optional turns and native status, plus
  experimental `thread/turns/list` and `thread/items/list`. Direct calls prove
  those pagination methods require the `experimentalApi` capability. CoCo now
  uses the stable metadata read for passive projection, while start/resume/
  fork/compact/name, turn start, model list, and continuous notifications
  remain. Final follow text still comes from the compatibility event stream.
  The adapter no longer mirrors prompts, decisions, or native status, plan,
  diff, error, and unsupported-request data. It still records local turn
  correlation and completed output pending the next gates; the storage and
  crash-model reduction is therefore not complete.
- Neither the official App Server nor SDK documents a first-class repository,
  worktree, or composite workspace API. The closest stable primitive is a
  Codex thread with `cwd`, name, fork lineage, Git metadata, history, and
  runtime status. The experimental schema also contains undocumented
  `threadSection/*` grouping methods, but they neither own a Git checkout nor
  constitute a stable project/workspace contract and must not become a CoCo
  dependency.
- Codex now has a higher managed-worktree layer in its product surfaces. The
  merged CLI changes allocate from the pool shared with Desktop, bind checkout
  ownership to the thread, and support fresh or forked interactive sessions;
  Desktop additionally provides project grouping, handoff, cleanup, and
  restore. That layer is not exposed as a documented reusable App Server or
  SDK control-plane API, so CoCo fills an API-composition gap rather than a
  capability Codex could not implement itself.
- The reduction audit identifies three substantial native-state mirrors:
  persisted thread-runtime snapshots, a second turn lifecycle/history, and
  normalized copies of selected Codex events. A narrower design should obtain
  current status and history from Codex, retain Git as authority for checkout
  state, and persist only CoCo's repository registry, workspace alias/ID,
  expected worktree/base binding, Codex thread binding, creation provenance,
  provisioning failure state, and a small idempotency ledger. Pending
  decisions still require live cross-client routing, but their current durable
  rows provide no recovery after the App Server generation disappears.
- Multi-repository listing plus worktree creation is useful convenience but is
  not sufficient product differentiation. The defensible workflow is a
  headless operator or external client creating and addressing named workspaces
  across repositories, letting the invoking client exit while the daemon owns
  active turns, observing native status from another client, routing a pending
  decision, and handing the exact thread/worktree to a human TUI without
  cancelling the turn.
- The corresponding release gate is behavioral rather than packaging-based:
  keep the repository private and crates.io disabled until a second client can
  create or take over, observe, control, and hand off that multi-repository
  workflow without duplicate worktrees or lost bindings across client and
  coordinator restart. If actual use remains one person invoking only create,
  status, and jump, reduce CoCo to scripts/a small library or stop; native Codex
  is the better product at that scope.
- Codex PRs 42652, 43069, and 43120 directly overlap CoCo's most visible
  create/send/jump and fresh/fork flows. They add experimental managed
  worktrees to new and forked `codex exec` runs, `codex --worktree`, explicit
  interactive forks, `/worktree`, and worktree choices under `/new` and
  `/fork`. The upstream implementation also binds the checkout to the Codex
  thread and loads destination configuration before the first turn.
- The latest documented Codex CLI release is 0.153.4 from 2026-09-04. The two
  interactive PRs merged on 2026-09-05 and therefore are not in that release;
  the exec PR is not named in the 0.153.x notes or current option tables. The
  new commands must be treated as unreleased, experimental main-branch behavior
  until a containing version is identified and tested.
- The native lifetime behavior currently has two distinct paths. Direct
  `codex --worktree` deliberately bypasses the implicit local daemon, rejects an
  explicit remote endpoint, and runs against an App Server embedded in the TUI;
  closing that non-daemon TUI therefore does not detach a running turn. An
  ordinary TUI attached to the experimental local App Server daemon can create
  a worktree through `/worktree`, `/new`, or `/fork` and offers **Run in
  background** on exit, which detaches without interrupting its threads. CoCo's
  remaining distinction is consequently deterministic daemon ownership and a
  stable external control surface, not the detach capability itself.
- CoCo is no longer justified as a convenience wrapper for parallel Codex
  worktrees. Its remaining possible product is a headless control layer:
  durable named workspace bindings, repository-wide and daemon-wide lookup,
  separate later send/status/decision operations, reattachment, JSON/local RPC,
  and a narrowly exposed control MCP server. That narrower thesis still needs
  a concrete workflow and user decision before publication; otherwise the
  additional daemon, database, compatibility pin, and lifecycle semantics are
  unjustified maintenance.
- crates.io renders the package `README.md` as its long-form description; the
  manifest `description` remains a short plain-text blurb. The current Git
  install is executable before publication and installs all three binary
  targets. A later registry command must name the alpha explicitly.
- A package manager should install `cocod` without starting it. Automatic
  service activation would be surprising and is currently unsafe: the daemon
  handles foreground interruption but lacks the complete SIGTERM and child-
  failure contract expected by systemd, launchd, or a future Windows service.
- A shared local Cargo target briefly retained an old executable despite newer
  sources compiled by overlapping checks. A fresh one-job target is the release
  authority for this slice; cached `target/debug/coco` output is not accepted as
  verification.
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
  thread-status snapshot. Start/resume snapshots remain only as bridge data;
  passive and mutating projections read current status through `thread/read`
  and retain the separately correlated accepted turn only as a short race
  guard.
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
- Supported App Server requests are surfaced as generation-local decisions and
  are never auto-approved. Unsupported request families are warned and left
  unanswered rather than persisted, guessed, or answered blindly.
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

- The native read contract was exercised with the guarded, model-free pinned
  Codex 0.147.0 compatibility test: after a full App Server restart,
  `thread/read` returned the same ID, `cwd`, name, `notLoaded` status, and (when
  requested) completed native turn history without loading the thread. Focused
  coordinator tests cover passive projection, failures, binding mismatches,
  newest-turn output, and on-demand subscription behavior. Full integration
  gates for the combined checkpoint remain to be recorded after the working-
  tree implementation stabilizes.
- `git diff --check -- knowledge/engineering/architecture.md
  knowledge/product/v0-spec.md knowledge/log.md knowledge/work/main.md` passes
  for the Phase 1/2 canonical and chronological documentation update.
- The reduction audit made no product or public-documentation changes and ran
  no release action. `codex --version` reports `codex-cli 0.147.0`; direct
  inspection of its committed generated schemas confirms the list/read/status,
  history, metadata, section, and lifecycle methods used in the comparison.
  Current official App Server, SDK, CLI, projects, and worktree documentation
  plus the three merged upstream worktree PR descriptions were compared with
  CoCo's domain, schema, coordinator, protocol, CLI, and MCP surfaces.
- The final public-content audit finds no remaining behavior, navigation,
  link, terminology, or public/internal-boundary mismatch. Direct pinned-tool
  checks pass Next route generation, TypeScript, Oxlint, and Prettier. A clean
  `DOCS_BASE_PATH=/coco` Next export verifies 93 static files across the landing
  page and eight documentation pages, static browser search, project-subpath
  routing, valid local links, no server runtime, and no internal knowledge.
- A fresh one-job Cargo target compiles the complete current source and runs 90
  library tests: 88 pass, the explicitly model-consuming Git proof remains
  ignored, and the one Unix-socket test is denied only by the managed sandbox.
  That exact test passes outside the sandbox, as do both real daemon/CLI process
  smokes. Fresh all-target/all-feature Clippy with warnings denied, Rustfmt,
  `cargo machete`, and `git diff --check` pass.
- Release preparation passes `actionlint` for every workflow, the version-sync
  script's three unit tests, Python Semantic Release's no-operation version
  calculation from the explicit `0.1.0-alpha.0` source baseline to
  `0.1.0-alpha.1`, `cargo publish --locked --dry-run`, and inspection of the
  resulting 56-file package boundary. The package verifies as
  `codex-coordinator` and contains only production Rust source, Cargo metadata,
  README, and MIT license.
- The installable Nix package builds from its restricted source fileset, and
  its `coco`, `cocod`, and `coco-mcp` outputs each report
  `0.1.0-alpha.0`. `nix run .`, `nix run .#cocod`, and
  `nix run .#coco-mcp` select the intended executable; `nix flake show .`
  exposes the default package and named apps on Linux and Darwin systems.
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
- The new public repository's first two `Rust` runs pass on GitHub, including
  native Ubuntu 22.04 and macOS 15 release builds and version/help/package
  smokes. The repeated `Documentation` run builds and deploys successfully;
  both the site root and quickstart return HTTP 200 from GitHub Pages. The
  automatic `Release` workflow is observed as skipped while the repository
  switch remains false.
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
- The alpha-packaging, profile-contract, CLI-help, and public-documentation
  checkpoint passes Rustfmt; locked all-target/all-feature Clippy with warnings
  denied; 89 library tests and two process smokes with two deliberate live
  tests ignored; `cargo machete`; the reviewed `cargo deny` policy; release
  lockfile tests; Actionlint; and a 56-file crates.io dry run with no upload.
  Documentation typechecking, Oxlint, Prettier, and the production
  `DOCS_BASE_PATH=/coco` export pass with 93 files, all nine required pages,
  static search, project-subpath routing, and no internal-knowledge leakage.
  `nix flake check .` and `git diff --check` also pass. The repository release
  switch was re-read immediately before checkpointing and remains `false`.
- The native-status/on-demand-activation checkpoint passes Rustfmt; locked
  all-target/all-feature `cargo check` and Clippy with warnings denied; the
  full Rust suite (104 passed, one deliberate live-model test ignored); both
  daemon/CLI process smokes; and the separately enabled model-free real-Codex
  0.147.0 compatibility test. The real test proves exact ID/`cwd`, non-loading
  status, restart persistence, on-demand resume, and optional stable full
  history. It also proved bounded turn/item pagination is rejected without
  `experimentalApi`, so that attempted dependency was removed. Public docs
  pass Next type generation, TypeScript, Oxlint, Prettier, and a fully static
  `/coco` export with 93 files, nine pages, search, and the public-only
  boundary. `CARGO_BUILD_JOBS=1 nix flake check .` and `git diff --check` pass.
- The generation-local decision/event reduction passes 105 library tests with
  the deliberate model-consuming test ignored plus both daemon/CLI process
  smokes outside the Unix-socket-restricted sandbox. Coverage proves no stored
  send prompt or decision row/event, concurrent at-most-once response, native
  resolution, restart loss, disconnect orphaning, native read-based wait
  states, and no new status/plan/diff/error/unsupported-request events.
  Rustfmt, all-target/all-feature Clippy with warnings denied, `cargo machete`,
  and `git diff --check` pass. Public docs pass Next type generation,
  TypeScript, Oxlint, and Prettier; the direct production Next build and export
  verifier confirm 93 static files, nine pages, `/coco` routing, search, and
  the public-only boundary.
- The behavior-neutral Coordinator test split preserves the exact set of 30
  orchestration tests. The shared parent is 530 lines; the focused context,
  decision, event, and workspace children are 724, 395, 270, and 640 lines.
  The full suite passes with 105 library tests, two process smokes, one
  deliberate live-model ignore, and one opt-in real-Codex ignore. Rustfmt,
  all-target/all-feature Clippy with warnings denied, `cargo machete`, and
  `git diff --check` pass.
- The schema-v6 operation-ledger checkpoint passes Rustfmt, locked
  all-target/all-feature Cargo check and Clippy with warnings denied, 111
  library tests, and both real daemon/CLI process smokes. The two deliberately
  opt-in live-Codex tests remain ignored. Coverage includes intent-before-
  dispatch, exact-response acceptance, ambiguous-dispatch reconciliation,
  replay/conflict handling, current-generation send exclusion, migration from
  operation-bearing legacy turns, and completion-before-response ordering.
  `cargo machete`, the direct fully static `/coco` documentation export (93
  files, nine pages, search, project-subpath routing, and the public-only
  boundary), `git diff --check`, and
  `CARGO_BUILD_JOBS=1 nix flake check . --no-write-lock-file --max-jobs 1`
  pass. No push, release, Pages deployment, or crate publication was run.
- The creation-axis working tree passes Rustfmt, locked all-target/all-feature
  Clippy with warnings denied, `cargo machete`, and the full serialized Rust
  suite: 134 library tests plus both real daemon/CLI process smokes pass; the
  model-consuming Git proof and opt-in real-Codex compatibility test remain
  deliberately ignored. Focused coverage includes 18 Git, 5 Coordinator
  creation, 7 CLI creation, 8 protocol, and 11 store tests. It proves
  independent base/context selection, exact native thread IDs, all three
  worktree modes, staged/unstaged carry, ordinary untracked opt-in,
  ignored-file inclusion, count/byte bounds, source immutability, and the
  independent `-d`/`-D` shorthands. The process harness was split from 2,028
  lines into a 47-line root plus focused lifecycle, fork, fake-server, and
  support modules without changing its two scenarios. Public docs pass Next
  type generation, TypeScript, Oxlint, and Prettier; the production
  `DOCS_BASE_PATH=/coco` build verifies 93 static files, all nine pages,
  search, project-subpath routing, and the public-only boundary.
  `git diff --check` also passes. No Nix build, push, Pages deployment,
  release, or publication was run for this slice.
- The collection/scope CLI checkpoint passes Rustfmt, locked all-target/all-
  feature Clippy with warnings denied, and the full serialized Rust suite:
  138 library tests and both daemon/CLI process smokes pass; the live-model
  Git proof and opt-in real-Codex compatibility test remain deliberately
  ignored. The focused CLI suite has 24 passing tests, including visible
  `list`/`ls` aliases at all three collection levels, incompatible-scope help
  and parser behavior, status overview/detail forms, and leading/trailing
  scope spellings. The process smoke proves selected-repository `status`,
  daemon-wide `status -a`, global named `status -g`, canonical model/workspace
  lists, and `repo ls` against the real RPC path. Public docs pass Next type
  generation, TypeScript, Oxlint, and Prettier; the production
  `DOCS_BASE_PATH=/coco` export verifies 93 static files, all nine pages,
  search, project-subpath routing, and the public-only boundary.
  `git diff --check` passes. No Nix build, push, Pages deployment, release, or
  publication was run for this slice.
- The interactive CLI slice passes Rustfmt, locked all-target/all-feature
  Clippy with warnings denied, `cargo machete`, and the full serialized Rust
  suite: 149 library tests contain 148 passes and one deliberately ignored
  model-consuming proof; both daemon/CLI process scenarios pass, and the
  separate opt-in real-Codex compatibility test remains deliberately ignored.
  The 37 focused CLI tests cover parser/help forms, raw picker navigation,
  direct numeric selection, cancellation, decision projection, deterministic
  approval `--choice`, and the non-terminal no-prompt contract. The real
  lifecycle smoke additionally rejects omitted status/create/send values when
  no terminal is present. The dependency policy passes with only the existing
  informational duplicate-version warnings, and the minimized Crossterm
  feature set is used directly. Public docs pass Next type generation,
  TypeScript, Oxlint, and Prettier; the production `/coco` build verifies 93
  static files, all nine pages, search, project-subpath routing, and the
  public-only boundary. The single-job Nix flake check and `git diff --check`
  pass. No push, Pages deployment, release, or publication was run.
- At the user's request, the nine local 2026-09-07 checkpoints from
  `34b5b43` through `ad34e62` are intentionally consolidated into one commit
  above the unchanged remote base `78835b2`. This is a local history rewrite;
  no remote ref, release, publication, or deployment is changed.

## Open questions and handoff

- The `janthmueller/coco` repository and protected `crates.io` environment
  exist, but the repository is private again, its Pages site is deleted, and
  the Documentation workflow is disabled server-side at the user's direction.
  The credential name is present without exposing its value, file permissions
  were narrowed before transfer, and `COCO_RELEASE_ENABLED` remains false.
  Repository, documentation, and crate publication each require a new explicit
  user decision after the product-boundary review.
- Before enabling automatic releases, make the successful Documentation run
  for the exact candidate SHA an automated release prerequisite as well as the
  existing Rust run. For the first manual alpha, inspect both hosted results
  and complete a `publish=false` rehearsal before requesting publication.
- Crates.io publication is paused. The upstream comparison shows that managed
  worktree creation and TUI launch are no longer a defensible product boundary.
  The user selected the narrower headless multi-repository control-plane
  direction; finish the native-first reduction and its behavioral proof, then
  test the first Codex release containing PRs 42652, 43069, and 43120 before
  revising public positioning or publishing an alpha.
- The user withdrew the follow-up issue search and upstream-comment idea after
  the private checkpoint was completed. Do not pursue or post either unless
  explicitly requested again.
- Prove native Codex per-thread MCP isolation across start, resume, and fork
  before scheduling the registry feature.
- Agentgateway is deliberately not scheduled. Reconsider it only when
  federation, centralized credential custody, independent enforcement, or
  gateway observability becomes an actual requirement.
- Thread-runtime ownership, `jump` exit/reattach behavior, passive native reads,
  and on-demand thread activation are implemented. Running turns still become
  interrupted if the daemon and its owned App Server stop; surviving that
  boundary would require a separately supervised App Server lifetime.
- The generation-bound in-memory pending-decision model and shared picker-based
  `coco decide` flow are implemented for command/file approvals and structured
  user input. Keep it separate from native thread status. Approval-only
  `--choice` is the deterministic scripting path; unrelated pending requests
  are not implicitly selected because `decide` still requires their opaque ID.
- Keep context transfer under the existing reserved modes. Before implementing
  `handoff`, agree its artifact/reference model, optional relationship to a
  plan, authoring inputs, and review UX; do not treat it as generic workspace
  metadata or duplicate the source conversation into SQLite. Native fork plus
  explicit child compaction is complete; handoff remains a separate design
  task rather than the next implicit coding step.
- Creation inputs are now implemented as four independent axes: code base,
  exact workspace/native-thread context, new/existing/detached Git binding, and
  bounded explicit local-state carry. The remaining Git-lifecycle question is
  an atomic detached-to-branch promotion; context handoff remains a separate
  artifact design.
- The workspace vocabulary/schema migration, create convenience pipeline,
  multi-repository CLI slice, native Git-approval proof, and interactive
  decision closure are complete.
- Research Codex's native lifecycle extensibility before designing CoCo hooks.
  Keep the distinction between internal normalized events and executable user
  automation explicit; hooks must not silently inherit credentials or block
  coordinator state transitions without a deliberate policy.
- The reorganized public site now passes a production export under the `/coco`
  GitHub Pages project subpath with nine total routes. Keep that static-export
  check when changing its routing or deployment workflow.
- The earlier Rust module-layout Phase 2 and its review are complete. Do not
  split a workspace now. If cross-platform support is scheduled next, begin
  that architecture's Phase 3 by moving the existing Unix RPC backend behind
  the common transport API, then add Windows named pipes with native CI before
  claiming Windows support. Otherwise prefer a user-visible capability over
  more structural movement.
