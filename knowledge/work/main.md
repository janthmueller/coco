---
type: Working Document
title: "main: repository foundation and first architecture"
description: Tracks repository bootstrap, the Rust baseline, and early CoCo architecture decisions on main.
tags: [work, branch, bootstrap, rust, mcp, architecture]
status: active
branch: main
updated: 2026-09-11
---

# main — repository foundation

## Intended outcome

Establish the repository, documentation workflow, and first executable and
architectural baseline for CoCo.

## Active work

- [ ] Revisit the read-only CLI vocabulary after `usage` has practical use:
  compare the separate `list`, `status`, and `usage` commands with a possible
  shared `show` namespace without changing the current surface prematurely.
- [ ] Before exposing workspace-owned deltas, budgets, or alerts, run the
  model-consuming fork/compact/TUI/offline attribution cases recorded in the
  workspace-usage decision; current output deliberately reports native thread
  totals only.
- [x] Implement the approved native workspace-usage slice.
  - [x] Persist focused Codex token snapshots with thread-binding identity,
    provenance, and freshness rather than conversation history.
  - [x] Add typed `workspace.usage.get`/collection protocol paths and the
    optional native per-thread cost provider without a local price catalog.
  - [x] Add `coco usage [workspace]`, current-repository/all-repository scope,
    `--follow`/`-f`, concise human output, and complete one-shot JSON.
  - [x] Keep usage following passive, terminal-stable, and separate from
    status/list while recording their possible later CLI consolidation.
  - [x] Cover exact binding, notification regression, restart/freshness,
    nullable cost, scoping, rendering, full fake-process behavior, and the
    selected Codex compatibility contract; keep model-consuming attribution
    claims in the explicit follow-up above. Update public and canonical docs.
- [x] Record the idle-runtime priority decision and assess native workspace
  token/cost accounting without implementing a product surface.
  - [x] Verify token notifications, per-thread billing estimates, replay
    limitations, and OpenTelemetry against Codex 0.154.0.
  - [x] Probe the current authenticated per-thread billing route without
    issuing a model turn.
  - [x] Separate raw usage, current context, and optional cost; record staged
    persistence, attribution, compatibility, and CLI recommendations.
- [x] Finish portable workspace resource policies and Linux enforcement.
  - [x] Reassess whether Linux cgroup v2 creates avoidable platform lock-in.
  - [x] Define the portable capability model and separate genuinely portable
    limits from Linux-, Windows-, and optional container-specific controls.
  - [x] Agree persistence, CLI/config, live-update, failure, and recovery
    semantics before implementing enforcement.
  - [x] Implement revisioned workspace policy, CLI/local RPC, fail-closed
    activation, verified cgroup launch/live updates, rollback, and restart
    staging without adding a second platform backend.
  - [x] Finish public documentation and the complete verification gates.
- [x] Implement the first Linux cgroup-v2 workspace containment slice.
  - [x] Add the instance/workspace-pool hierarchy and rootless systemd scope
    capability selection without enabling limits.
  - [x] Launch and stop each workspace executor as one complete scope, with
    instance-safe stale-runtime cleanup after daemon loss.
  - [x] Replace ancestry estimates with truthful cgroup-v2 memory, CPU,
    process/task, and controller-event observations when contained.
  - [x] Preserve the process-tree fallback and validate unit, process, real
    systemd, real Codex, public documentation, and static-export behavior.
- [x] Design the first Linux cgroup-v2 workspace containment slice.
  - [x] Audit the current executor launch, ownership, resource sampling, and
    shutdown boundaries.
  - [x] Select a rootless cgroup delegation/launch strategy and define
    capability detection plus non-Linux fallback.
  - [x] Define cgroup identity, lifecycle/recovery, accounting, configurable
    CPU/memory/PID limits, errors, and public CLI/config behavior.
  - [x] Record the implementation slices, tests, risks, and next concrete
    action in canonical runtime knowledge before changing product behavior.
- [x] Track workspace token usage and estimated cost as a deferred,
  separately designed observability capability.
- [x] Clarify destructive-command help without changing behavior.
  - [x] Describe `close` and its dry-run in user language rather than internal
    retirement terminology.
  - [x] Give both `--yes` options the same contract: skip confirmation without
    authorizing either class of data loss.
  - [x] Rebuild the CLI help and run the focused parsing tests.
- [x] Clarify the two deletion-safety axes after user review.
  - [x] Rename the overly broad `--discard-commits` policy to
    `--discard-unretained-commits` throughout CLI, wire contract, persistence,
    tests, and documentation.
  - [x] State plainly that close removes the worktree, so uncommitted files
    require explicit discard even though the branch and thread are retained.
  - [x] Re-run focused behavior and documentation gates, then update handoff.
- [x] Make dirty-source creation non-blocking with a clear omitted-changes warning.
- [x] Implement the approved close/delete redesign: direct deletion, owned-resource
  defaults, explicit retention, separate loss approvals, combined guards,
  recoverable partial effects, and public/internal documentation.
- [x] Refine the public documentation layout from the deployed-site review.
  - [x] Put the header brand and `Docs` link on one intentional content axis.
  - [x] Replace overly tight display typography with readable heading spacing
    and sizing across the landing page.
  - [x] Reduce the oversized document-page header region and align its title,
    description, content, and right-hand outline.
  - [x] Audit public hook terminology so users see hooks and guards rather than
    an unexplained internal `reaction` category.
  - [x] Verify responsive/static output and document the visual decisions.
- [x] Repair the confirmed CLI interaction bugs and make resource observation
  an explicit status view.
  - [x] Replace save/restore-based follow rendering with a bottom-edge-safe
    inline frame shared with the established picker mechanics; cover resize,
    changing height, and real tmux behavior.
  - [x] Resolve an explicit send target through the daemon before prompting
    for a missing message, while retaining turn-start's final race-safe check.
  - [x] Add `status --resources`/`-r` for human detail and collection views,
    add `--follow`/`-f`, and keep JSON status output complete without either
    presentation flag.
  - [x] Keep human resource output minimal: RSS, process count, and CPU only;
    retain backend, scope, PID, and timestamps in JSON.
  - [x] Update the shipped CLI contract and public docs, then run focused,
    full Rust, real-terminal, docs, and Nix verification sequentially.
- [x] Prove and integrate Codex's native per-workspace execution boundary.
  - [x] Inventory every thread start/resume/fork/turn and interactive-attach
    path that must carry an App Server environment selection.
  - [x] Add a lazily managed `codex exec-server` process per active workspace,
    register it through `environment/add`, and keep the shared App Server as
    CoCo's control plane.
  - [x] Preserve a clean fallback or actionable compatibility error when the
    selected Codex build does not support the experimental environment API.
  - [x] Cover environment registration, sticky selection, process cleanup,
    restart behavior, and workspace isolation with focused fake and real-Codex
    tests before treating this as shipped behavior.
  - [x] Once the execution root is proven, add truthful per-workspace resource
    observation against that process tree; keep hard limits and containers as
    separate follow-up contracts unless their prerequisites are demonstrable.
  - [x] Update canonical engineering knowledge, and update public docs only for
    behavior that is actually available and verified.
- [x] Assess per-workspace resource observation and containment without
  implementing a runtime change.
  - [x] Compare current Orca accounting and concurrency behavior against its
    fresh source rather than relying on product wording.
  - [x] Identify the attribution boundary imposed by CoCo's one shared Codex
    App Server and distinguish monitoring, admission control, and hard limits.
  - [x] Record a staged direction and the unresolved containment choice.
- [x] Extend the first hook slice with explicit operator lifecycle controls and
  synchronous guards.
  - [x] Add offline hook configuration validation and atomic daemon reload
    without replacing a valid active registry on failure.
  - [x] Add bounded, command-backed `workspace.close` and `workspace.delete`
    guards that can only allow or deny the exact checked action.
  - [x] Keep guards synchronous, non-retrying, non-mutating, and distinct from
    durable post-commit hook deliveries; record their timeout and error policy.
  - [x] Cover signal-filtered reactions, reload behavior, guard decisions,
    recovery boundaries, CLI presentation, and public/internal documentation.
- [x] Reassess CoCo's public product presentation against the implemented CLI.
  - [x] Inventory the actual user workflows and distinguish core value from
    supporting controls and advanced integrations.
  - [x] Audit the README, landing page, navigation, guides, and reference for a
    coherent first-time-user story and verified claims.
  - [x] Rewrite the public entry points around one clear product promise,
    preserving the strict public/internal knowledge boundary and static site.
  - [x] Verify public documentation and record remaining positioning questions.
- [x] Validate CoCo against the locally installed `codex-cli 0.154.0`.
  - [x] Advance the intentionally pinned, model-free App Server compatibility
    fixture from `0.153.4` only after confirming the active executable.
  - [x] Run both real-Codex compatibility contracts sequentially and record
    any upstream protocol changes or required CoCo adaptations.
- History-maintenance checkpoint: the user requested consolidating the nine
  `main` commits from September 8–9 into one freshly dated commit, preserving
  the earlier parent `1580fae` and the verified implementation. Local recovery
  refs and other worktrees remain unchanged; only `main` is rewritten. Remote
  replacement uses an exact expected-head force lease. This maintenance push
  skips CI rather than changing workflow settings or rerunning unchanged code.
  The user subsequently requested the complete Actions-history cleanup: all 26
  completed runs were deleted, and the GitHub API confirms zero remaining runs
  and zero artifacts. No logs/artifacts were backed up, no workflow settings
  changed, and no new commit or push accompanies this cleanup note. Repository
  privacy and disabled Pages remain unchanged. This is history maintenance, not
  a guarantee that all GitHub/external traces or dates in documents disappear.
- The verified signal slice is complete on 2026-09-09. The user requested its
  local commit and fast-forward integration into `main`, without a push.
  [`feature/signals`](feature/signals.md) retains the detailed implementation,
  design and verification record, including the file-defined 2020-12 catalog.
  Next product discussion is the first external consumer; hooks, automatic
  reactions, ticket workflows and public release remain separate decisions.
- [x] Propose the next bounded CoCo work sequence after confirming external
  ticket ownership. Keep current validation debt visible, make signals the
  next functional slice, and defer runtime work until the user confirms it.
- [x] Record the user's product boundary: CoCo owns agent work environments
  and the communication/control surface; a separate task system owns tickets
  and their workflows. Keep direct integration versus a separate composing
  service open, with no runtime/public-doc changes.
- [x] Compare the proposed signal contract with `stablyai/orca` using current
  primary sources, without implementing anything or expanding into a general
  orchestrator survey.
  - [x] Trace agent-to-orchestrator reporting, delivery/subscription,
    persistence, and payload validation in Orca's public documentation/source.
  - [x] Explain the actual overlap and differences, distinguish code from
    documentation, and record useful implications for the CoCo proposal.
- [x] Evaluate user-proposed agent-emitted signals as a CoCo orchestration
  extension, without implementing or committing a runtime change.
  - [x] Inspect existing control MCP, event storage, and deferred hooks.
  - [x] Recommend a bounded first contract for discovery, optional payload
    validation, provenance, persistence, and subscription; distinguish proposal
    from accepted behavior and record outstanding product choices.
- [x] Refine picker selection to the user's confirmed selected-only `›`.
  - [x] Keep a blank marker column on inactive rows and cyan/bold selection;
    remove the now-redundant no-color label brackets.
  - [x] Check fixed alignment, single-marker navigation, and terminal cleanup;
    update the public guidance and canonical selection contract.
- [x] Repair shared terminal selection and simplify CoCo confirmations.
  - [x] Replace CoCo yes/no choice menus with a line-based `y/N` prompt;
    default to No and preserve explicit destructive-action authorization.
  - [x] Reproduce and fix picker layout at the bottom of a full terminal,
    including repeated final lines, short windows, and redraw/cleanup.
  - [x] Highlight the selected option and hide/restore the terminal's actual
    cursor during selection; marker placement is refined in the follow-up above.
  - [x] Add focused terminal/confirmation regression tests, update canonical
    and public guidance, and verify sequentially without live user workspaces.
- [x] Review the implemented retirement lifecycle for destructive-operation
  safety, crash recovery, concurrent activation, and CLI contract gaps. Record
  reproducible findings and remaining limitations before changing product code.
- [x] Resolve the six retirement-review findings below before considering the
  feature ready: managed-path identity, running descendants, confirmed target
  identity, prepared context dependencies, attached-TUI send admission, and
  archive safety during recovery.
  - [x] Reject redirected managed paths before native or Git effects and
    repeat the identity check at removal/recovery boundaries.
  - [x] Check descendant worktree activity for ordinary close, and recheck
    archive safety during compensation and restart recovery.
  - [x] Bind CLI confirmation to the previewed workspace ID and resource plan.
  - [x] Protect both native-thread and workspace references used by prepared
    context forks, including recovery of a pending deletion.
  - [x] Separate exclusive fresh adoption from renewable bound-TUI presence;
    allow send on an idle thread and independent attached clients.
  - [x] Promote the review probes into focused tests, update contracts/docs,
    and run the required sequential verification gates.
- [x] Design a safe workspace-retirement surface without implementing it.
  - [x] Separate CoCo workspace state, the managed Git worktree and branch,
    and the native Codex thread by ownership and recovery semantics.
  - [x] Verify the supported App Server schema and current official lifecycle
    contract for thread unsubscribe, archive, unarchive, delete, and descendant
    effects.
  - [x] Define a reversible default, explicit data-loss acknowledgements,
    concurrency guards, and a staged implementation proposal for user review.
- [x] Implement the confirmed workspace-retirement lifecycle.
  - [x] Add crash-recoverable `closing`/`closed`/`reopening` persistence and
    coherent active-versus-closed lookup/list semantics.
  - [x] Add verified Git worktree close/reopen operations with tracked,
    untracked, ignored, lock, and detached-commit safety.
  - [x] Reuse native Codex unload/archive/unarchive/delete and descendant
    inspection through the worker boundary.
  - [x] Add typed daemon methods and CLI `close`, `reopen`, and closed-only
    `delete`, including `-t/-b/-n/-y`, `--discard-changes`, prompts, dry-run,
    and no destructive collection scope.
  - [x] Cover unit, protocol, CLI, store migration, fake-process, and real
    model-free compatibility behavior; update canonical knowledge and public
    user documentation only after behavior passes.
- [x] Make `status --follow` one consistent live-observation contract.
  - [x] Keep both explicit-workspace and collection followers alive until the
    operator interrupts them; reaching `Ready`, a wait state, or an error must
    not end observation.
  - [x] Replace the current terminal frame in place while retaining an ANSI-free
    append-only transition log when stdout is redirected or piped.
  - [x] Cover renderer and process behavior, then update the public command
    guidance and canonical status contract.
- [x] Repair real Codex context-fork activation through `coco jump`.
  - [x] Reproduce the failure against the installed 0.153.4 daemon and recover
    the exact App Server rejection from the daemon log.
  - [x] Opt the daemon connection into the experimental field CoCo deliberately
    uses to defer inherited goal continuation, and expose actionable Codex RPC
    failures without leaking secrets or raw protocol payloads.
  - [x] Add regression coverage for the initialization capability and a real,
    model-free context fork; verify the original `test/1` activation path.
  - [x] Revisit silent single-candidate selection separately from the fork
    failure and keep the resulting CLI behavior explicit.
- [x] Refine human terminal output without changing JSON or protocol behavior.
  - [x] Audit command success output, lists, status/follow, errors, and the
    shared picker for redundant or low-value presentation.
  - [x] Confirm the compact visual hierarchy and Codex-aligned color policy
    with the user.
  - [x] Replace generic rendering with command-specific typed output, add
    terminal-aware styling, update tests/docs, and run the full gates.
- [x] Replace the provisional source-specific creation flags with one
  `--context`/`-c` reference resolved by the daemon, plus child-only
  `--compact-context`/`-C` and the `-Cc <reference>` shorthand cluster.
- [x] Separate state observation from turn output in the CLI.
  - [x] Make targetless `status` show the selected repository and
    `status --all-repos` show every registered repository; retain explicit
    workspace details and remove the status picker.
  - [x] Let `status --follow` follow all workspaces in its selected collection
    and retain explicit single-workspace follow, without printing Codex chat
    messages in either mode.
  - [x] Add `send --wait` for waiting on the exact accepted native turn and
    printing only that turn's final agent response.
  - [x] Update focused/process tests, internal contracts, and user-facing docs
    together, then run the full Rust, real-Codex, docs, and Nix gates.
- [x] Validate CoCo against the latest published Codex CLI (`0.153.4`) with
  the real, model-free App Server compatibility test and a reviewed schema
  diff; do not infer support from a successful version launch alone.
- [x] After repairing native first activation, move CoCo's pinned baseline,
  protocol checks, tests, and user-facing compatibility wording together to
  the now-passing `0.153.4` contract.

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
  into Codex before adding any CLI or RPC field. Keep references optional and
  leave ticket workflow and authoritative ticket/workspace mappings with the
  external task system or its integration.
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
  - [x] Replace the completed-message compatibility event without accepting an
    unbounded or experimental native-history dependency: make status
    state-only and keep the exact `send --wait` result bounded and
    generation-local.
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
      are projected in memory, retain provisioning/failure events, and remove
      the final completed-output event write after `send --wait` owns output.
    - [ ] After the product proof, remove the now-read-only legacy status,
      turn, and decision schema in a separate physical-cleanup revision.
  - [ ] Run the two-repository/multi-client product proof and revisit the
    private-alpha go/no-go decision with the user.
- [x] Repair first activation for the published `codex-cli 0.153.4` without
  relying on the unreleased `codex --worktree` surface or fabricating a turn.
  - [x] Keep `coco create` as Git-only preparation and represent an unbound
    workspace explicitly as `prepared`.
  - [x] Materialize and bind fresh context atomically with the first real
    `send`; retain native fork/compact materialization for inherited context.
  - [x] Make the first fresh `jump` launch the official TUI in remote start
    mode through a one-use correlation relay, then adopt the exact native
    thread only after Codex has materialized it.
  - [x] Preserve normal `resume --remote` for already bound threads and prove
    that TUI exit leaves an accepted turn owned and observed by `cocod`.
  - [x] Renew a live fresh-jump lease without busy adoption polling, while
    preserving bounded expiry after CLI/relay loss.
  - [x] Cover competing activation, empty-TUI exit, relay failure, daemon
    restart, profile/model propagation, and the real published App Server
    contract before updating public documentation.
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
    independent `--base`/`--base-workspace` and unified `--context` options.
  - [x] Allow the context source to be either a same-repository CoCo workspace
    or an exact readable native Codex `thread.id`; validate its native state
    without loading it and persist bounded source provenance.
  - [x] Implement explicit tracked and ordinary-untracked carry, the
    `-d`/`--dirty` shorthand, and Codex-compatible `.worktreeinclude` for
    selected ignored files without mutating the source checkout.
- [x] Design and implement the first user-configurable CoCo hook slice.
  Before defining CoCo hooks, inventory the pinned Codex CLI and App Server's
  native hooks, notifications, and lifecycle events so CoCo can expose or
  extend existing signals instead of duplicating them. Cover workspace creation,
  thread/turn start, agent state transitions and terminal outcomes, then decide
  execution context, filtering, ordering, retries, timeouts, failure policy,
  secret handling, auditability, and platform behavior.
  - [x] Leave in-session lifecycle policy with native Codex hooks and prove the
    App Server execution boundary against installed `codex-cli 0.154.0`.
  - [x] Add a narrow durable outbox and trusted asynchronous command reactions
    for accepted signals and successful workspace lifecycle transitions.
  - [x] Bound configuration, process environment, concurrency, timeout,
    retries, retention, restart recovery, and delivery observability.

## Proposed next CoCo work sequence — 2026-09-09

The user asked what the confirmed product boundary means for the next work.
This is prioritization only, not authorization to implement, run model turns,
remove legacy data, commit, push, or release.

Recommended order:

1. Close the already-open combined two-repository/multi-client proof of the
   existing core. Exercise CLI preparation/send, MCP inspection and explicitly
   enabled continuation, exact TUI attach/detach, restart/recovery, and repeated
   operation IDs in isolated test state. Existing focused tests are not absent;
   it is the combined product acceptance scenario that remains unchecked in
   the plan. Keep public-release approval separate. Physical legacy-schema
   cleanup remains its own reviewed/migration-tested checkpoint after the proof,
   not a hidden prerequisite for designing the signal contract.
2. Make a complete, small signal path the next functional slice. First settle
   operator-owned registration/storage, optional schema revisioning, emitting
   workspace identity and explicit grants, and retention/closed-deleted
   workspace behavior. Then implement type discovery, validated/idempotent
   MCP emit, durable service acknowledgement, bounded MCP/CLI reads, and CLI
   follow with resumable reader cursors. The first example can be
   `review.requested`; its business interpretation is external.
   The key technical preflight is workspace-bound sender attribution: the
   current MCP adapter fixes only a repository, not an emitting worker identity.
   Test its attachment/resume/fork lifecycle without inventing a broader worker
   MCP registry or hard-coding a task system. Do not claim an exact native turn
   source unless the binding proves it.
   Exit: a real bound agent can emit an allowed signal, invalid payloads fail
   before persistence, independent readers see the record, accepted retries
   do not duplicate it, and restart/reconnect preserves replay within retention.
3. After the signal slice is proven, discuss the first consuming integration.
   Decide whether its actual need is external notifications/hooks, directed
   agent mail, or missing control API coverage. The current MCP surface is
   list/status/diff plus opt-in send; it does not yet offer workspace creation
   or full CLI parity. Expose only deliberately authorized operations needed
   by that consumer, rather than enabling all mutating CLI capabilities at once.

Remain deferred: automatic model wakeup/turn routing, command-executing hooks,
acknowledged consuming mailboxes, ticket scheduler/state/mandatory references,
handoff design, broad MCP catalog/gateway work, and a third integration service.
External annotations stay optional future correlation, not a ticket database.

Verification: re-read the open checklist, canonical native-first acceptance
gate/product ownership, and actual MCP tool declarations; `git diff --check`
passes. Only this working record changes in this turn; no runtime tests or
implementation ran. Next action awaits confirmation of the proposed work order.

## Separate task-management product boundary — 2026-09-09

The user confirmed a future separate task-management tool. CoCo supplies
agent work environments and communication/control capabilities; ticketing is
owned entirely by the other product. Whether that product calls CoCo directly
or a third component integrates both remains deliberately open.

Recorded this durable direction in the product specification and engineering
architecture, and clarified the existing external-reference follow-up:

- CoCo remains independently usable. Workspace, thread, and ticket identities
  are different; an idle/completed native turn or closed workspace is not a
  ticket workflow transition.
- CoCo owns operational safety and execution controls. Priorities, dependencies,
  acceptance criteria, scheduling business work, and interpreting agent reports
  as ticket progress belong to the task system/integration.
- The integrating component owns ticket/workspace mappings. Optional future
  references in CoCo can aid correlation without copying ticket authority or
  forcing a mandatory ticket field into workspace creation.
- The proposed signals remain domain-neutral. An integration can eventually
  define the types/schemas it needs and decide how to react, without making
  CoCo understand a particular ticket system. This does not approve a full
  signal API, hook executor, or peer-messaging implementation yet.

No separate integration service was selected or scaffolded. Public docs and
runtime code are unchanged; previous discussion notes are preserved. Reviewed
canonical ownership/non-goals for consistency and ran `git diff --check`;
runtime tests are unnecessary for this knowledge-only change. Next work remains
discussion of the first signal/consumer and its detailed contract, not building
the task system or starting an unrequested implementation. No commit or push.

## Agent-emitted signals assessment — 2026-09-09

Status: the user welcomed the direction and requested an Orca comparison;
the detailed feature contract and implementation remain unapproved. The user
asked whether typed, optionally validated agent-to-CoCo signals
with discovery, persistence, and subscriptions would be useful. Assessment is
against checkpoint `d90d82d`; no runtime, public docs, commit, or push changes.

Current evidence:

- `src/mcp.rs` exposes repository-scoped `workspaces.list/status/diff` and
  opt-in `workspaces.send`. It advertises tools only; no signal registry,
  agent emission, subscription, or workspace-bound sender identity exists.
- `event.list` is a compatibility method requiring a workspace. Its current
  implementation hydrates native status and reads an unbounded historical
  suffix; it is not a suitable ready-made public signal subscription API.
- SQLite already holds CoCo-owned lifecycle/context and control-audit facts.
  The native-first architecture deliberately stopped native status/history
  mirroring. It reserves durable delivery/outbox work for a concrete consumer.
- Lifecycle hooks, annotations/external references, and handoff remain deferred.
  The multi-repository/multi-client product proof and legacy physical-schema
  cleanup remain open; signals are an orchestration extension, not a newly
  declared blocker for the existing CLI.

Recommended direction, pending user discussion:

- Use “signal” for an explicit domain message emitted by an agent, not a new
  native runtime state or an OS signal. Keep three concepts distinct: a signal
  type/contract, an immutable recorded emission, and a hook/consumer reaction.
- Start with an operator-controlled, repository-scoped type catalog containing
  a name, description of meaning/when to emit, immutable revision, and optional
  JSON Schema. Even schema-free types remain registered; unknown types fail
  clearly rather than silently creating typoed channels. Workers discover and
  emit allowed types, but cannot register or weaken their own validation policy.
- Use a small shared MCP surface for type discovery and `signals.emit`, plus
  bounded emission reads and CLI follow. Names remain provisional. Validate
  the selected payload schema in `cocod`, not only a generic tool envelope.
  Type revision, source binding, ID, recording time, and sequence are service
  facts, not user-supplied payload fields.
- Bind the emitting adapter/capability to a workspace and validate its grant;
  do not trust a model-provided workspace/thread name as sender provenance.
  Attach native thread/turn correlation only when proven. Fresh remote-TUI
  activation, forks/resume, and external operator clients need explicit binding
  tests before claiming reliable agent identity. Local same-user binding is
  capability scoping, not hostile-process isolation.
- Persist accepted emissions in a dedicated, bounded signal log within the
  existing SQLite store. Acknowledge only after commit; use scoped producer
  idempotency keys so the same logical emission can be retried after a lost
  response. Store schema revision with each record; schema edits do not change
  the meaning of history. Do not revive the full native-event mirror or add a
  mandatory broker, MCP proxy, Agentgateway, or automatic action executor.
- Begin observation with cursor-based paginated reads and follow polling.
  Resume from a saved cursor within an explicit retention window, and report
  expired cursors rather than silently skipping history. Slow observers do not
  block writers. Replay may repeat delivery, so consumers deduplicate by ID;
  this is not an exactly-once side-effect guarantee.
- A subscription notifies an observing client, not automatically an idle model.
  Starting another turn, invoking a command, or contacting an external system
  is a later, separately authorized hook action. Bound retries, recursion,
  permissions, concurrency, and cost before enabling automatic reactions.
- Schema validity proves shape, not truth: an agent's `review.requested` or
  `tests.passed` payload is a claim. Do not use it alone for merge/deploy approval,
  infer it from prose, or rely on the model to emit every lifecycle transition.
  Set payload/rate/storage limits; reject remote schema references and keep
  secrets and arbitrary payload text out of routine logs.

Standards checked: MCP's
[2026-07-28 tools contract](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
uses JSON Schema for tool inputs; the
[JSON Schema object reference](https://json-schema.org/understanding-json-schema/reference/object)
covers property types, required fields, and additional-property constraints.
MCP subscription transports are version-dependent (see the official
[2026-07-28 SDK migration](https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28));
these are transport capabilities, not proof of CoCo retention, replay, or
automatic model wakeup. No installed Codex/MCP subscription compatibility was
tested or claimed in this discussion.

Open choices: first concrete signal and consumer; catalog authoring location;
schema/version policy; binding and opt-in grants; retention/export and workspace
deletion behavior; later hook and cross-workspace delivery semantics. Next step
is to discuss those choices with the user before canonical contract changes or
implementation. Verification was read-only source/knowledge inspection and
primary-specification review; only this working record changed, so runtime
tests were not rerun.

## Orca messaging comparison — 2026-09-09

Scope: compare the above proposal with `stablyai/orca`, not a broader competitor
survey or an implementation task. GitHub reported `main` at
`ed9d76178de14c7f220cf83f06af0d23c2717cfc`, committed 2026-09-09 10:59:50 UTC.
Fetched the relevant documentation and source at that exact revision. Search
results included older monolithic code; current code has moved into focused
messaging modules, so conclusions below use the pinned files instead.

Verified findings:

- Orca has actual agent-to-orchestrator messaging, not only passive status
  notifications. Its
  [CLI guide](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/docs/site/content/docs/cli/orchestration.mdx)
  describes Runs, Tasks, Dispatches, worker reports, questions, and gates.
  Agents use `orca orchestration send/check/ask/reply`; lifecycle reports
  include the exact task/dispatch and success/failure outcome. The
  [worker contract](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/skill-guides/orchestration/references/worker-contract.md)
  supplies sender/capability context and explicitly directs workers to read
  follow-ups at checkpoints. Agent-reported completion and liveness have
  different meanings.
- Message kinds are a
  [fixed code enum](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/main/runtime/orchestration/types.ts):
  `status`, `dispatch`, `worker_done`, `merge_ready`, `escalation`, `handoff`,
  `decision_gate`, `question`, and `heartbeat`. The
  [RPC schema](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/main/runtime/rpc/methods/orchestration/schemas.ts)
  validates that enum with Zod; `payload` is a string, parsed as JSON by
  specialized paths. Lifecycle handlers perform additional ownership/outcome
  checks. A JSON payload is therefore not evidence of a user-registered,
  versioned payload-schema catalog. No such catalog was found in the inspected
  messaging and plugin contracts; this is not an exhaustive whole-repo absence
  claim.
- Messages are persisted with source, destination, type, body/payload, sequence,
  timestamps, and read/delivery state in the
  [database schema](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/main/runtime/orchestration/db/schema/create-core-tables-sql.ts).
  [Insertion](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/main/runtime/orchestration/db/messages/message-insert.ts)
  also supports ordinary terminal mail outside an explicit Run through an
  unbound Run; do not incorrectly claim every Orca message requires a Task.
- Coordinator consumption is a FIFO inbox with a durable Delivery, replayed
  until `check --ack`; `check --wait` can wait for incoming mail. The
  [delivery guide](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/skill-guides/orchestration/references/messaging-and-gates.md)
  and [Run reader](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/main/runtime/rpc/methods/orchestration/messaging/check-run.ts)
  distinguish consuming, peek/history, and exclusive current-consumer behavior.
  Type filters govern waking, not skipping older actionable mail. Successful
  send proves durable enqueue; wake/nudge does not prove read, a started turn,
  or accepted steering. This is addressed mailbox delivery, not independent
  observer cursors over a general signal log.
- A separate
  [plugin event contract](https://github.com/stablyai/orca/blob/ed9d76178de14c7f220cf83f06af0d23c2717cfc/src/shared/plugins/plugin-events.ts)
  defines bounded, validated `worktree.created`, `worktree.removed`, and
  `agent.status.changed` payloads. These are fixed runtime/plugin events, not
  user-defined agent-emitted business signals.

Implications for CoCo, still a proposal:

- The agent-reporting/persistence problem is shared and demonstrably useful;
  do not present it as unique to CoCo. Orca already goes further in supervised
  task execution, addressed mail, acknowledgements, questions, and gates.
- CoCo's proposed difference is a small domain-neutral signal layer with an
  operator-controlled optional-schema catalog, workspace provenance, and
  independent readers. It does not require Orca's Run/Task/Dispatch model or
  immediately interpret an emitted signal as a lifecycle mutation.
- Preserve durable acceptance versus processing as separate guarantees.
  Cursor observation need not inherit Orca's consuming-inbox acknowledgement
  model. Future effectful hooks may need delivery/ack/retry records of their
  own; multiple observers should not consume one another's signals.
- Sender attribution, retry idempotency, and stale-origin rejection deserve
  attention from the start. Exact native thread/turn binding still requires a
  CoCo-specific design; an event payload alone cannot prove origin or outcome.

Verification: current public source and documentation inspected read-only;
Orca was not installed or executed. Only this branch working document changed;
no product/public-doc edits, runtime tests, commits, or pushes. Next step is
user discussion of a first concrete signal/consumer and the remaining contract
choices recorded above, not automatic implementation.

## Terminal interaction repair — 2026-09-09

Scope: replace CoCo's binary confirmation menus with ordinary `y/N` input,
repair picker rendering in a full terminal, and apply the user's preference
for row highlighting instead of a blinking hardware cursor. Initially every
option had a marker; the user subsequently confirmed a selected-only marker.
Runtime protocol, workspace data, live Codex sessions, releases, and deployments
remain outside this task. The user subsequently authorized a checkpoint commit
and push on 2026-09-09.

Findings and decisions:

- Reproduced both reported artifacts in an isolated 80×14 tmux pane with the
  original renderer: initial drawing at the bottom showed only option nine;
  one `j` navigation then displayed the menu with option nine duplicated.
  `MoveToNextLine` emits CSI E (cursor movement), which does not allocate
  scrollable lines. The old cleanup assumed all requested rows existed.
  The [XTerm control-sequence reference](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
  and the installed Crossterm implementation informed the checked distinction.
- The new owned inline frame writes real CRLF line feeds, keeps a blank row
  below it, rewinds/clears the previous region, and buffers each redraw. It
  limits choices to `min(9, height - 2)`, uses that window size for page keys,
  and rejects unusably small terminals safely. No alternate-screen takeover.
- The initial all-row `›` design used cyan/bold selection and `NO_COLOR` label
  brackets. On review, the user confirmed `›` only on the selected row, keeping
  cyan/bold styling and a blank marker column elsewhere. This makes selection
  explicit without color and keeps labels aligned, so the bracket fallback
  and its unused palette accessor were removed. Hardware cursor visibility,
  wrapping, and raw input mode are restored on exit, cancellation, or errors.
- Added `unicode-width 0.2.2` without its CJK feature to budget columns for wide
  characters and combining sequences; no unrelated dependency moved. Its
  [documented display-width API](https://docs.rs/unicode-width/0.2.2/unicode_width/)
  avoids treating every Unicode character as one terminal column.
- `Interaction::confirm` is separate from `select`: normal line input accepts
  case-insensitive y/yes and n/no after Enter, defaults blank input to No,
  re-prompts invalid input, and rejects EOF. `--yes` behavior and acknowledged
  deletion plans stay unchanged. Codex decision options remain choice menus.
- Input/layout tests moved into `prompt/tests.rs`; the frame and terminal-mode
  guard live in `prompt/terminal.rs`. Two explicitly ignored interactive probes
  can be driven by `python3 tests/terminal_smoke.py <library-test-executable>`
  after `cargo test -j1 --locked --lib --no-run`. The script uses only its own
  disposable tmux server and normalizes the test shell environment/history.
  It waits for a completed redraw and idle shell, rather than assuming that a
  sent key or resize has already been rendered.

Verification: focused prompt tests pass (15 automated); the full suite passes
223 library tests and five process tests, with model/interactive probes opted
out by default. The real-terminal probe passes full-screen first draw,
navigation without duplicated rows, shrinking/growing height and width,
numeric selection, color/NO_COLOR, q/Escape/Ctrl-C cleanup, restored cursor and
wrapping, y/n/YES, blank-No, invalid-answer retries, and EOF rejection. Rustfmt,
all-target/all-feature Clippy, `cargo machete`, and `cargo deny check` pass;
deny reports only the already-reviewed duplicate dependencies. Documentation
type generation, TypeScript, Oxlint, and Prettier pass. The `/coco` static
export verifies 93 files, nine pages, search, routing, and the public-only
boundary. `nix flake check . --no-write-lock-file --max-jobs 1` passes,
including the x86_64-linux package and tooling; other platforms were not run.
The isolated test terminals are closed. No required implementation work
remains for the original repair; the selected-marker follow-up is recorded below.

Selected-marker follow-up: implementation, focused assertions, real-terminal
probe expectations, and affected guidance now reflect selected-only `›`.
Verified the follow-up sequentially:

- `cargo test -j1 --locked --lib cli::prompt:: -- --test-threads=1`: 16 pass,
  two interactive probes skipped by default.
- `python3 tests/terminal_smoke.py target/debug/deps/coco-e80ddf0e19909fd8`:
  both real-terminal probes pass, including single-marker movement in color
  and `NO_COLOR`, bottom-margin redraws, resizing, and cursor/wrap restoration.
- `cargo test -j1 --locked --all-targets --quiet -- --test-threads=1`: 224
  library and five process tests pass; live Codex/model checks remain opt-in.
- Rustfmt, all-target/all-feature Clippy with warnings denied, and
  `cargo machete` pass. `cargo deny --frozen check` passes using the cached
  advisory database, with only previously reviewed duplicate-version warnings.
- `pnpm --dir docs run check` and `git diff --check` pass. The earlier static
  export and Nix results apply to the original repair; those heavier builds
  were not repeated for this marker-only follow-up.

No required work remains for the terminal repair or selected-marker follow-up.
No user workspace or live Codex thread was touched, and the isolated tmux test
server has been closed.

Checkpoint handoff — 2026-09-09: the user requested one commit and push for the
verified terminal-interaction changes. The source and tests are unchanged since
the verification above; only this handoff record was updated. Remote preflight
confirmed the repository is private, has no Pages site, keeps the Documentation
workflow `disabled_manually`, and sets `COCO_RELEASE_ENABLED=false`. The intended
checkpoint is `fix: repair interactive terminal prompts`; no release or Pages
deployment is part of the push. No further product changes are queued for this
scope; the next task awaits user direction.

## Proposed workspace-retirement model

Status: lifecycle direction and short options confirmed by the user on
2026-09-09; implementation, focused real-process verification, and final
whole-tree gates passed. The subsequent 2026-09-09 review found six
correctness/safety issues below. All six fixes and their regression tests are
implemented; the corrected tree passes the final sequential verification gates.

A CoCo workspace record, its Git worktree, its optional branch, and its native
Codex thread are distinct resources. Arbitrary independent deletion of those
resources would permit incoherent live workspaces. Prefer a two-stage model:

1. `coco close <workspace>` is the normal, reversible operation. It first
   proves that no turn, pending decision, TUI attachment, or background terminal
   is using the workspace, removes the managed worktree, and retains the CoCo
   record, branch, and Codex thread. The workspace becomes `closed`, disappears
   from normal active lists and pickers, and remains addressable for explicit
   status inspection and reopening.
2. `coco reopen <workspace>` recreates the exact managed worktree from the
   retained branch or detached commit and retains the same Codex thread. When
   CoCo archived that thread during close, reopen unarchives it before returning
   the workspace to `open`.
3. A separate `coco delete <workspace>` is permanent and is valid only for a
   closed workspace. It makes thread and branch retention explicit, presents a
   complete impact plan, and requires confirmation.

The first delivery implements all three commands together. A safe close is
quietly executable when the worktree is branch-backed, unchanged (including no
ordinary untracked or ignored local files), and its native thread is proven
`idle` or `notLoaded`. Otherwise:

- an active/waiting thread, pending decision, attached TUI, background terminal,
  unavailable native state, invalid Git binding, or Git worktree lock blocks
  close rather than being bypassed by a generic force flag;
- tracked, untracked, or ignored local files require an interactive summary and
  explicit confirmation, or `--discard-changes --yes` in non-interactive use;
- a detached worktree whose `HEAD` differs from its immutable base is refused
  until it is promoted to a branch, unless a future explicit
  `--discard-unretained-commits` policy is accepted; and
- CoCo invokes `git worktree remove` against the revalidated canonical managed
  path and never recursively deletes the directory itself. Removing a worktree
  never implicitly deletes its branch.

Use `--dry-run`/`-n` to render the exact resource and risk plan and `--yes`/`-y`
to answer a confirmation; `--yes` must never imply `--discard-changes` or
another data-loss policy. Use `-t` as the consistent "include the thread in
this lifecycle action" selector: `close -t` archives the thread and
`delete -t` deletes it, while the explicit long options remain
`--archive-thread` and `--delete-thread`. Use `-b` only for
`delete --delete-branch`; `-tb` therefore requests both optional permanent
deletions and composes naturally with `-y`. Keep `--discard-changes` and any
future `--discard-unretained-commits` long-only because they authorize data loss. Do not
reuse `-a`: it already means `--all-repos`, and destructive bulk scope remains
unsupported. Omitted targets use the existing picker; `--global`/`-g` retains
its existing single-workspace meaning.

Codex owns native conversation storage. CoCo uses `thread/unsubscribe` to
release its own event subscription before removing a retained thread's working
directory. In the pinned App Server this intentionally does not unload an idle
thread immediately: the native idle grace period is 30 minutes, so waiting for
`thread/closed` would make ordinary close unusable. Optional native archival
provides immediate native teardown through `thread/archive`; restoration
must call `thread/unarchive`, and permanent removal must call `thread/delete`;
CoCo must never manipulate Codex rollout files. Both native archive and delete
can affect spawned descendants. Before either operation, enumerate descendants
through native `thread/list` across active and archived storage, refuse while
any child exists, and never silently affect another CoCo workspace or
externally created thread. Native deletion additionally checks known CoCo
context-parent references; Codex remains the final guard for external
dependencies CoCo cannot know.

Persist `closing`, the exact close-time `HEAD`, and the archive intent before
the first external side effect. Revalidate the binding, `HEAD`, lock, and full
local state under the repository lock immediately before removal. A crash after
worktree removal leaves a recoverable `closing` record that startup reconciles,
not a live-looking workspace with a missing directory. Closed records retain
the desired path, binding, base/context provenance, and thread ID; normal name
uniqueness remains unchanged so `reopen` recovers the same identity. Permanent
deletion similarly persists independent thread/branch intent in `deleting`
before applying either optional effect.

### Retirement review — 2026-09-09

Scope: review only, with production source unchanged. Seven temporary
regression probes in `/tmp/coco-retirement-review-iuxinH` reproduce six
findings against the current source using real temporary Git repositories and
the existing fake Codex worker. These are not claims of live model or App
Server fault-injection coverage.

1. **P1 — close can remove an unrelated worktree through a replaced path.**
   `prepare_close` accepts the canonical path returned by Git without checking
   it equals the stored managed destination. Moving a detached managed
   worktree, putting a symlink at its old path to another detached worktree in
   the same repository, and closing the workspace succeeds and removes the
   unrelated directory. Revalidate the managed destination and reject path
   redirection before any native or Git side effect.
2. **P1 — ordinary close ignores running descendants.** Descendant inspection
   runs only for `--archive-thread`. An idle parent with an active child using
   the same worktree passes normal close and loses that working directory.
   Inspect descendant use of the directory for every close, independently of
   whether native conversation storage is being archived.
3. **P1 — confirmation is not bound to the previewed workspace identity.**
   With an explicit workspace name, `run_delete` sends that name again after
   confirmation instead of the preview's workspace ID. If another client
   deletes and replaces that name while the prompt is open, the accepted
   request deletes the replacement. The same close preview/apply flow retains
   a mutable name. Pin the exact ID and validate the acknowledged resource
   plan before applying it.
4. **P2 — deletion misses prepared context dependencies.** Before a fork is
   materialized, its source reference lives in the stored context request,
   while `parent_thread_id` is still absent. Native-delete dependency checks
   look only at that column; default record-only deletion skips reference
   checks altogether. Both delete variants succeed for a source with a
   prepared child; default deletion then makes child activation fail with
   `WorkspaceNotFound` despite retaining the source thread. Protect pending
   source references or make activation independent of a deliberately removed
   source record before allowing that removal.
5. **P2 — the new TUI lease also blocks send and additional attachment.**
   `resume_launch` now retains the exclusive lease for the TUI's lifetime,
   while `start_turn` still interprets every lease as pending fresh-thread
   adoption. Sending to an idle bound thread fails with
   `WorkspaceAttachInProgress`. Separate live-TUI presence used for retirement
   safety from the exclusive fresh-adoption guard.
6. **P2 — recovery archives without renewed runtime/descendant checks.**
   `ensure_thread_archived` validates only ID/cwd and archive state. A pending
   reopen with no worktree, followed by an external unarchive and new
   descendant, causes startup recovery to invoke archive without inspecting
   that descendant. Apply the same native safety checks immediately before
   archive in recovery/compensation as in the foreground operation.

The existing 15 Coordinator retirement tests pass in the same temporary
source snapshot; all seven added safety expectations fail and expose the
listed gaps. Reproduction commands use one Cargo build job and one test
thread with `--locked --offline --manifest-path
/tmp/coco-retirement-review-iuxinH/Cargo.toml --lib`: filter
`retirement_review` for the probes and `coordinator::tests::retirement::` for
the existing baseline. No application fix, commit, push, or publication was
performed during that review. The following remediation records the authorized
fixes rather than treating the earlier passing baseline as sufficient.

### Retirement-review remediation — 2026-09-09

Scope: fix all six reviewed safety/correctness issues. The user additionally
confirmed that `send` should work while the TUI is open. The remediation
initially excluded commit/push; the user subsequently authorized that
checkpoint below. Publication and mutation of a real user workspace remain
out of scope.

- Managed destination identity is now checked before native effects, on
  recovery/reopen, and again by Git binding verification at removal. Stored
  root/repository/name paths cannot redirect through a replaced symlink to
  another detached worktree in the same repository.
- Ordinary close inspects native descendants using the same directory. Active
  or errored agents and loaded background terminals block removal; idle or
  unloaded children and verified independent directories remain permitted.
  Native and descendant checks repeat after unsubscribe and before removal,
  and immediately before a recovery/compensation archive.
- CLI apply requests now pin the previewed workspace ID and acknowledged
  `expectedPlan`, including `HEAD`. A changed resource/plan requires another
  review. Dirty close re-previews the explicitly selected discard policy
  before a single confirmation, without printing the plan twice. This is an
  identity/displayed-risk guard, not a filesystem-content snapshot or a lock
  against independent Git/App Server clients.
- Pending context dependencies are decoded through the existing typed stored
  creation request. Workspace-ID sources protect record deletion; workspace
  and raw-thread sources protect native deletion, even before `parentThreadId`
  exists and during restart reconciliation. A retained raw native-thread
  source still works after source record-only deletion. A repo-then-dependency
  mutex orders cross-repository source capture/persistence against deletion;
  ordinary Git provisioning does not hold that dependency mutex.
- TUI leases are now per connection. Pending fresh adoption stays exclusive;
  binding converts it to non-exclusive presence. Idle bound threads accept
  `send` and multiple TUI clients. Retirement rejects every live presence;
  releasing one client cannot clear another client's protection.
- Promoted the seven original review probes and added late-race, raw-thread,
  recovery, multiple-client, and actual CLI-wire regressions. Tests are split
  into lifecycle, safety, confirmation, and dependency modules, with Git and
  CLI-specific tests at their respective boundaries. Production descendant
  and dependency policy lives in `coordinator/retirement/safety.rs`.
- Final verification: all 37 focused retirement tests pass; the final full
  suite passes 216 library tests and five process-smoke tests (the live-model
  test remains intentionally opt-in). Rustfmt, all-target/all-feature Clippy
  with warnings denied, `cargo machete`, and `cargo deny check` pass; the last
  reports only the already-reviewed dependency duplicates. The model-free
  compatibility test passes against installed `codex-cli 0.153.4`, including
  close/reopen/archive/unarchive/delete. Docs type generation, TypeScript,
  Oxlint, and Prettier pass. The first local-socket test attempt was
  sandbox-denied, not an application failure; the same test passes with local
  socket permission. The `/coco` static export verifies 93 files, nine pages,
  search, subpath routing, and the public-only boundary. `nix flake check .
  --no-write-lock-file --max-jobs 1` passes, including the built package on
  x86_64-linux; other platforms were not executed. `git diff --check` passes.
- Kept the existing conservative detached-HEAD rule rather than expanding the
  feature into external ref-retention discovery; clarified that user guidance
  must not imply that creating some other branch automatically permits close.

Canonical behavior and safety contracts are updated in the product spec,
engineering architecture, Rust layout, and knowledge log. Public docs explain
the user's actions and safety refusals only; no internal plans or protocol
details were copied into them.

### Retirement delivery checkpoint — 2026-09-09

The user requested a commit and push of the verified implementation and all
six review fixes. Deliver the complete retirement slice as
`feat: add safe workspace retirement` on `main` to `origin/main`; do not
squash existing history, create release tags, or enable publication/Pages.
The 58-file change set matches the verified code, tests, and documentation;
only this handoff note was added after the completed verification gates.
GitHub confirms that `janthmueller/coco` remains private, the Pages workflow
is `disabled_manually`, and `COCO_RELEASE_ENABLED` is `false`. No product
changes or additional release work are included in this checkpoint.

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
| Conversation context | fresh or `--context <workspace-or-thread>` | Start a new native thread or call `thread/fork`; never selects code. |
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

The final public input is one `--context`/`-c` reference rather than separate
workspace and thread flags. The daemon first resolves it as a workspace in the
destination repository and otherwise validates it as an exact native Codex
thread ID; `workspace:` and `thread:` prefixes provide deterministic
disambiguation. A direct Codex source means `thread.id`, not
`thread.sessionId`: the latter identifies a fork tree's root and can point at a
different conversation branch. CoCo validates the supplied thread with
non-loading `thread/read` and always creates a new child with `thread/fork`; it
never adopts or moves the source thread. A readable inactive thread may come
from another repository because its history and the destination code are
intentionally independent. Persist its exact thread ID and source `cwd` as
provenance, while the destination thread is bound only to the newly created
worktree. `thread.sessionId` is not part of CoCo's creation contract.

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

## Human CLI presentation refresh

Status: confirmed and implemented on 2026-09-08.

- Human output is now command-specific rather than passing every result
  through `print_human(Value)`. The former generic path printed low-value IDs,
  null fields, raw profile objects, and nearly the same workspace block after
  both create and send.
- Collection tables prioritize workspace/repository name, native state, and
  branch. Opaque IDs remain in `--json` and ambiguity diagnostics rather than
  every normal row; one bounded renderer replaces tab stops.
- A repository-scoped workspace picker omits the redundant repository path;
  the global picker retains it as a discriminator.
- The picker keeps its numbered choices and immediate arrow/`j`/`k` behavior,
  colors only the active cursor/row, omits the permanent navigation footer,
  and clears without printing a second `Selected:` line. A sole available
  choice still opens the explicit picker instead of silently selecting an
  action the operator did not name.
- Follow Codex's own `codex-rs/tui/styles.md`: default foreground for primary
  text, bold headers/names, dim secondary details, cyan selection and live
  status, green success/ready, red failure, and magenta only for Codex. Avoid
  custom colors plus blue/yellow. Text and symbols still carry all meaning.
  Styling is enabled only for the relevant TTY, disabled by `NO_COLOR`, and
  never emitted in JSON or piped output.
- Keep `send --wait` response text and raw `diff` output untouched. Progress
  belongs on stderr; structured or pipe-oriented stdout must remain clean.
- Recommended success summaries are one primary line plus at most one useful
  secondary line: registered repository path; created workspace with branch
  and worktree; accepted send with workspace/state; submitted decision with
  workspace. Do not echo base SHA, native thread ID, profile JSON, or operation
  ID after an ordinary confirmed operation.

## Decisions

- 2026-09-09 — Implement retirement as an availability state machine separate
  from provisioning lifecycle: `open -> closing -> closed`,
  `closed -> reopening -> open`, and `closed -> deleting -> absent`. Persist
  close-time `HEAD`, archive ownership, and permanent-delete intent before
  external Git/Codex effects; startup converges transitional rows from their
  observed owners before accepting RPC traffic. Availability takes precedence
  in the public phase projection, while native thread status remains Codex-
  owned.
- 2026-09-09 — Adopt a reversible `close`/`reopen` boundary before permanent
  workspace deletion. Closing removes only a safely disposable managed
  worktree by default and retains the CoCo identity, branch, and Codex thread;
  permanent record, thread, or branch deletion remains a separate confirmed
  operation. Never model an active workspace as an arbitrary bag of
  independently removable resources.
- 2026-09-08 — Define `status --follow` uniformly as a persistent observer:
  explicit and collection forms end only on Ctrl-C. On capable terminals,
  redraw one saved output region so state changes replace the prior view;
  redirected output appends only visible changes and contains no terminal
  control sequences. Status remains state-only and never owns conversation
  output.
- 2026-09-08 — Negotiate `experimentalApi` because CoCo's inherited-context
  lifecycle deliberately depends on `thread/fork.deferGoalContinuation`.
  Request `excludeTurns: true` for fork and resume: it changes only the RPC
  response shape, not the native history copied into or loaded for the thread.
  Keep experimental pagination and unrelated fields unused. Surface only the
  bounded, single-line Codex RPC code/message to CLI users; structured error
  data and non-RPC transport internals remain private.
- 2026-09-08 — Align terminal presentation with Codex's standard palette and
  hierarchy, but keep CoCo's output command-specific. Enable styling
  automatically per output stream, honor `NO_COLOR`, and defer a public
  `--color` option until a real override need exists. Omit opaque IDs from
  ordinary human output while preserving them in JSON, ambiguity diagnostics,
  pending-decision commands, and interrupted-operation recovery instructions.
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
- 2026-09-08 — Supersede the single-target status decision above. Treat state
  observation and turn output as separate CLI contracts: targetless `status`
  is a repository collection, `status -a` is the daemon-wide collection, and
  either may follow state changes; explicit status remains a detailed
  single-workspace read/follow. Status never selects a target or prints Codex
  conversation text. `send --wait` owns the exact accepted turn's final
  response instead.
- 2026-09-08 — Do not load full native history or enable Codex's experimental
  pagination to implement `send --wait`. Correlate the existing
  `item/completed` and `turn/completed` notifications with the operation guard,
  publish the result only after the direct start response confirms its native
  turn ID, bound each response to 1 MiB and the cache to 256 results/8 MiB,
  retain it only in the current daemon generation, and write none of that text
  to SQLite.
- 2026-09-08 — Replace the provisional source-specific context flags with one
  `--context`/`-c` reference. The Coordinator, not the CLI, first resolves a
  workspace in the destination repository and otherwise reads an exact native
  Codex thread ID. Workspace matches take precedence; `workspace:` and
  `thread:` force the rare ambiguous case. No option means fresh context.
  Rename the modifier to child-specific `--compact-context`/`-C`; boolean `-C`
  may safely precede value-taking `-c` in `-Cc <reference>`.

## Findings

- The public entry points correctly listed most shipped commands but framed
  CoCo primarily as a parallel worktree launcher. That boundary is now too
  weak and undersells implemented behavior: short-lived clients can start,
  observe, continue, and enter the same named workspace; repository, worktree,
  native thread, and settings stay bound across those clients and repositories.
  Worktree creation is a supporting mechanism. Signals are useful for advanced
  integrations but should not compete with that first-use story.
- The README linked only to the currently disabled GitHub Pages site, and the
  quickstart used `send --wait` before teaching `status --follow`. Relative
  source-document links now remain usable while the repository is private, and
  the quickstart demonstrates the distinguishing non-blocking workflow first.
- Exact retirement lookup cannot use default `thread/list`: Codex 0.153.4
  filters out CoCo's App-Server-origin root threads there. Exact `thread/read`
  finds both active and archived threads and returns the rollout path; the
  `archived_sessions` component supplies the same archive-state evidence Codex
  itself uses. CoCo now fails closed when that path is absent and never opens
  or edits the rollout. An externally archived thread blocks ordinary close;
  `--archive-thread` explicitly adopts the obligation to unarchive it during
  reopen.
- `thread/backgroundTerminals/list` is valid only for a loaded thread. An exact
  `notLoaded` status already rules out live in-process terminals, so retirement
  skips that endpoint only in this state and treats every other query failure
  as a blocker. Both fresh and resume `jump` paths now hold the same renewable
  attach lease so close cannot race an official TUI startup.
- Native `thread/delete` rejects a source thread that remains referenced by a
  forked history, even when the descendant listing itself is empty. CoCo can
  preflight the context-parent links it stores and name the dependent
  workspaces; Codex remains the guard for unknown external relationships. A
  native delete rejection returns the durable row from `deleting` to `closed`
  only after an exact read proves the thread remains. Verified absence completes
  the requested step; an unavailable follow-up leaves recovery pending rather
  than claiming either outcome.
- Normal close needs a second full Git check immediately before
  `git worktree remove`: a file, including an ignored file, can appear after
  the initial plan. The adapter now repeats binding, `HEAD`, lock, tracked,
  untracked, and ignored checks unless explicit discard was requested, and
  refuses the late change without removing the path.
- CoCo's supported 0.153.4 schema already includes native `thread/archive`,
  `thread/unarchive`, `thread/delete`, `thread/unsubscribe`, descendant filters
  on `thread/list`, and background-terminal inspection/cleanup. CoCo should
  delegate native conversation retention to those APIs rather than manipulate
  Codex rollout files. The current official App Server contract says archive
  and delete also affect spawned descendants, so either operation needs an
  explicit descendant impact check and must not be a default side effect of
  removing one CoCo workspace.
- The existing Git observation already distinguishes dirty state and commits
  relative to the immutable base, but safe worktree retirement must additionally
  inventory ignored files: `.worktreeinclude` may deliberately copy ignored
  local setup into a managed worktree. `git worktree remove --force` combines
  dirty and locked bypasses, so CoCo must refuse locks and expose a narrower
  discard-changes policy instead of forwarding a generic force switch.
- Physically deleting the current workspace row is not a first-step cleanup:
  operation, event, audit, turn, and legacy decision rows reference it, and the
  binding is what makes a retained thread discoverable and reopenable. A closed
  state provides a recoverable disk-cleanup boundary; permanent record deletion
  can be a separately confirmed operation after external resources have reached
  their requested terminal states.
- Targetless follow previously printed the collection table once and routed
  every later phase through a separate one-line event renderer, regardless of
  whether stdout was a terminal. That directly produced a `Working` table row
  followed by a detached `Ready` line. Explicit follow had the opposite
  lifecycle problem: it redrew one line in a terminal but exited on ready,
  waiting, unloaded, unavailable, or failure states. Neither distinction
  represented the operator's intent to keep observing live state.
- Cursor save/restore is safer than moving upward by a counted number of newline
  characters: a long detail line may occupy multiple visual terminal rows after
  wrapping. The live renderer therefore restores the exact region origin and
  clears downward before drawing the next complete frame. Pipes, files, and
  `TERM=dumb` avoid cursor controls and retain append-only output.
- The original `jump test/1` failure was a real App Server rejection:
  `thread/fork.deferGoalContinuation` requires an initialized
  `experimentalApi` capability. Once negotiated, the same roughly 50 MiB
  inherited history exposed a second issue: returning every turn exceeded the
  WebSocket implementation's 16 MiB frame limit. `excludeTurns: true` retains
  the full native child context while omitting that duplicate response body;
  the original child then materialized and resumed successfully.
- The fake fork server previously accepted experimental fields without checking
  the initialization capability and returned only tiny fixtures regardless of
  request shape. Enforcing both capability negotiation and metadata-only fork
  output prevents that double blind spot. The real compatibility test now
  activates a prepared native fork as well as testing start and resume.
- `TerminalInteraction` previously returned index zero before rendering when a
  selector had exactly one choice. This explained why targetless `coco jump`
  appeared to skip its picker; it was independent of the explicit
  `jump test/1` App Server failure.
- CoCo workspace names are constrained human labels while native Codex thread
  IDs are normally opaque, so one workspace-first context reference keeps the
  common CLI concise without losing either source. Resolution must remain in
  the Coordinator because it alone owns repository scope, bindings, and the
  native worker. A combined `CONTEXT_REFERENCE_UNRESOLVED` error reports both
  attempted interpretations without making the CLI query or guess.
- `status --follow` replayed the latest completed message because it began its
  compatibility-event cursor at zero and owned both state and transcript
  presentation. A workspace that was already idle could therefore print an
  old response immediately. Polling `workspace.get`/`workspace.list` removes
  that coupling; the no-longer-consumed completed-message row can stop being
  written without a schema migration.
- Codex can deliver both the final agent item and `turn/completed` before the
  `turn/start` future returns to the coordinator. The existing pre-dispatch
  runtime guard is the correct correlation point: it temporarily holds the
  native turn ID/output and exposes them through `turn.result` only if the
  direct response later confirms the same ID.
- Released Codex 0.153.4 returns a `path` for an empty loaded thread before the
  rollout is resumable. The same native TUI code treats a thread as resumable
  only when that path is a non-empty regular file. Exact `thread/read` plus
  filesystem metadata is therefore the adoption gate; `thread/list` is not a
  reliable exact-thread materialization check and is no longer used for it.
- Git-only preparation removes the incompatibility without inventing activity.
  A first real `send` binds the native thread and accepted turn in one store
  transaction. A first fresh `jump` needs a one-use relay because the official
  root TUI starts its own thread; correlating its exact request/response avoids
  latest-thread heuristics, while an expiring lease excludes competing first
  activation. Once adopted, the daemon subscribes before the TUI leaves.
- A fixed thirty-second lease without renewal would reject the first action
  after an operator left a fresh TUI open. The one-use relay now sends a
  ten-second liveness heartbeat: a live TUI keeps exclusivity indefinitely,
  while relay or CLI loss still makes the lease recoverable within thirty
  seconds.
- Additive generated-schema drift is unsuitable as a byte-for-byte release
  gate because CoCo owns narrow stable projections. The opt-in test now pins
  the executable version and verifies concrete consumed behavior instead.

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
  remain. At that checkpoint, final follow text still came from the
  compatibility event stream. The 2026-09-08 state/output split supersedes
  that seam: status is state-only, completed output is generation-local for
  `send --wait`, and no Codex conversation text is written to SQLite.
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
- The first Codex 0.153.4 probe exposed that a fresh thread is not resumable
  before its rollout file exists: after a complete App Server restart,
  `thread/resume` returned `no rollout found for thread id`, even though a
  metadata-only `thread/read` could still see the transient thread. That
  finding invalidated CoCo's former eager-empty-thread design. It is now
  resolved by Git-only workspace preparation and exact native materialization
  on first `send` or the first action inside `jump`; the repaired model-free
  compatibility test passes against 0.153.4.
- The checked-in App Server JSON schema snapshot does not generate or compile
  any production Rust. Exact snapshot equality was removed from the real
  compatibility gate because additive upstream definitions blocked the
  behavioral probe. Compatibility is instead decided by the concrete methods
  and stable response fields CoCo actually exercises; removal or replacement
  of the now-unreferenced snapshot directory remains cleanup work.
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

- The public-positioning revision passes Rustfmt, all 28 focused CLI tests,
  `git diff --check`, and the documentation typecheck, Oxlint, and Prettier
  gate. The production static export passes with 98 files, all ten pages,
  search, `/coco` project-path routing, and the public-only boundary. The
  README also passes the documentation toolchain's Prettier check; the rebuilt
  CLI exposes the new tagline, and `nix flake show . --no-write-lock-file`
  accepts the aligned flake metadata. No release, commit, push, Pages change,
  or deployment was performed.
- The selected real-Codex gate now targets installed `codex-cli 0.154.0`.
  Both model-free compatibility tests pass sequentially against the actual
  executable: preparation/adoption, native reads and history, daemon/App Server
  restart, resume, context fork, retirement, per-thread MCP isolation, profile
  restoration, and signal attribution remain compatible. The first sandboxed
  attempt failed before reaching Codex because loopback binding was forbidden;
  the identical test command passed with local loopback permission. No model
  turn or user workspace was used. Rustfmt, `git diff --check`, and the public
  documentation typecheck/lint/format gate also pass. No release, commit, push,
  or deployment was used. The subsequent locked, single-job,
  all-target/all-feature suite passes 244 of 247 regular tests with three
  deliberate ignores, plus all five process smokes; the two real-Codex tests
  remain ignored in that normal run because they were already exercised
  explicitly against 0.154.0.
- The workspace-retirement slice passes Rustfmt, locked Cargo check, and
  all-target/all-feature Clippy with warnings denied. The complete serialized
  Rust suite passes 200 of 201 library tests (the remaining test is the
  deliberately ignored model-consuming Git proof) plus all five daemon/CLI
  process smokes. The separately enabled model-free compatibility test passes
  against installed `codex-cli 0.153.4` and covers exact archived-thread
  lookup, archive, unarchive, reopen, and native deletion. Focused coverage
  additionally proves crash recovery before and after Git removal, ambiguous
  native-delete reconciliation, external archive ownership, late ignored-file
  refusal, detached restoration, branch ownership/head protection, and v1-to-
  v9 migration defaults. `cargo machete` finds no unused dependencies and
  `cargo deny check` passes with only informational duplicate-version output.
  Public docs pass Next type generation, TypeScript, Oxlint, and Prettier; the
  production `/coco` build verifies 93 static files, all nine pages, search,
  project-subpath routing, and the public-only boundary. After marking the new
  source modules as Git intent-to-add so the Git-backed flake includes them,
  `nix flake check . --no-write-lock-file --max-jobs 1` and
  `git diff --check` pass. No commit, push, release, deployment, or publication
  was run.
- The persistent-follow slice passes Rustfmt, locked all-target/all-feature
  Clippy with warnings denied, and the complete Rust suite: 176 of 177 library
  tests pass with the deliberate live-model proof ignored, and all four
  daemon/CLI process smokes pass. Process coverage keeps both collection and
  already-ready explicit followers alive beyond the former automatic-stop
  boundary and terminates them only by SIGINT. Focused renderer tests prove
  cursor-based frame replacement for a terminal, ANSI-free append-only output
  when redirected, and the `TERM=dumb` fallback.
- Manual PTY checks against the existing `test/1` workspace keep the collection
  table as one live view and keep an explicit `Ready` workspace spinning until
  SIGINT; both exit successfully without affecting its thread. Public docs pass
  TypeScript, Oxlint, Prettier, and a production static `/coco` export with 93
  files, nine pages, search, routing, and the public-only boundary.
  `cargo machete`, `git diff --check`, and the single-job Nix flake check pass.
  No release, deployment, or publication was run for this slice.
- The context-fork repair passes locked all-target/all-feature Clippy with
  warnings denied, Rustfmt, all 175 library tests (174 passed and the deliberate
  live-model proof ignored), and all four daemon/CLI process smokes. The fake
  App Server now rejects an unnegotiated deferred continuation and requires
  metadata-only fork/resume requests. The separately opted-in, model-free
  real-Codex compatibility test passes against installed `codex-cli 0.153.4`
  and activates a prepared inherited-context workspace through the same
  `workspace.attach` path used by `jump`.
- The original `test/1` production fixture was verified manually against the
  rebuilt daemon. Its roughly 50 MiB context fork bound a new native child
  after `excludeTurns` removed the oversized response; after another daemon
  restart, status truthfully showed the thread as unloaded, explicit
  `jump test/1` resumed the exact binding with exit status zero, and the final
  status was `Ready`. Targetless `jump` displayed the one-candidate picker and
  accepted Enter instead of selecting silently. `cargo machete` and
  `git diff --check` also pass. No public docs, release, deployment, or
  publication was run for this repair.
- The terminal-presentation slice passes Rustfmt; locked all-target/all-feature
  Clippy with warnings denied; and `cargo test --locked --all-targets` with 171
  library tests and all four daemon/CLI process scenarios passing. The live
  model proof and separately opted-in real-Codex test remain deliberately
  ignored. Process coverage confirms that captured repository/workspace output
  is ANSI-free and omits opaque IDs while JSON stays unchanged; focused tests
  cover the Codex-style palette, `NO_COLOR` capability policy, bounded tables,
  picker cleanup/color, control-character neutralization, and untouched raw
  diff output. `cargo machete` reports no unused dependency. Public docs pass
  Next type generation, TypeScript, Oxlint, Prettier, and a production static
  `/coco` export with 93 files, all nine pages, search, routing, and the
  public-only boundary. The first Nix attempt correctly omitted two completely
  untracked child modules from its Git source; after staging only those files,
  `nix flake check . --no-write-lock-file --max-jobs 1 --cores 1` passes.
  Cached and uncached `git diff --check` both pass. No commit, push, release,
  deployment, or publication was run.
- The unified-context slice passes Rustfmt, locked all-target/all-feature
  Clippy with warnings denied, and the complete locked Rust suite: 161 of 162
  library tests pass with the deliberate model-consuming proof ignored; all
  four real daemon/CLI process scenarios pass, including compacted context
  creation through the unified option. The opt-in real-Codex compatibility
  test remains deliberately ignored because this slice changes no native wire
  call. `cargo machete` finds no unused dependency. The earlier one-off daemon
  lock failure did not reproduce in its focused retry or the final full suite.
  Public docs pass Next type generation, TypeScript, Oxlint, and Prettier; the
  production `/coco` export verifies 93 static files, all nine pages, search,
  project-subpath routing, and the public-only boundary. The single-job Nix
  flake check and `git diff --check` pass. No commit, push, release,
  deployment, or publication was run for this slice.
- The 2026-09-08 state/output split passes Rustfmt, locked all-target/all-feature
  Clippy with warnings denied, all 158 library tests (plus one deliberately
  ignored model-consuming proof), all four daemon/CLI process tests, and
  Doc-tests. `cargo machete` finds no unused dependencies; `cargo deny check`
  passes advisories, bans, licenses, and sources with only the existing
  informational duplicate-version warnings. The guarded model-free real-Codex
  test passes against the installed pinned CLI. Public docs pass Next type
  generation, TypeScript, Oxlint, and Prettier; their production `/coco` export
  verifies 93 static files, all nine pages, search, project-subpath routing,
  and the public-only boundary. `nix flake check . --no-write-lock-file
  --max-jobs 1 --cores 1` and `git diff --check` pass with the complete new
  source tree tracked in the index. No push, release, deployment, or
  publication was run for this slice before its local checkpoint.
- The completed 2026-09-08 native-first tree passes `cargo fmt --all --
  --check`, all-target/all-feature Clippy with warnings denied, and
  `cargo test --locked --all-targets`: 155 library tests pass, the explicitly
  model-consuming Git proof remains ignored, and all four daemon/CLI process
  scenarios pass. `cargo machete` finds no unused dependency; `cargo deny
  check` passes advisories, bans, licenses, and sources with only the reviewed
  informational duplicate-version warnings.
- `pnpm --dir docs run check` passes Next type generation, TypeScript, Oxlint,
  and Prettier. The production `/coco` build verifies 93 static files, all nine
  pages, search, project-subpath routing, and the public-only boundary. A
  direct dirty-tree Flake build correctly omitted the four still-untracked new
  Rust modules; an exact temporary repository mirror with the complete
  production tree tracked passes `nix flake check`. The real repository index
  was not changed by that mirror. No release, publication, or deployment was
  run.
- Against locally installed `codex-cli 0.153.4`,
  `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked --test real_codex_compat
  -- --ignored --nocapture` passes without a model turn. It proves Git-only
  preparation, an empty remote candidate remaining unbound, one exact
  model-free shell-action candidate becoming durable and adopted, persisted
  native history, passive `notLoaded` reads, exact resume after daemon/App
  Server restart, and preservation of the requested catalog model. This
  supersedes the initial failure recorded in Findings: that failure correctly
  exposed that the former eager empty-thread design was incompatible.
- A focused coordinator test covers valid lease renewal and rejects a foreign
  lease ID. The four-process suite exercises the same internal protocol while
  proving empty fresh-TUI exit, exact action-triggered adoption, detach without
  interruption, and release after relay startup failure. The relay heartbeat
  is ten seconds against a thirty-second daemon expiry, so it does not add the
  former 250 ms adoption polling while the TUI is idle.
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

## Signal and hook boundary review — 2026-09-10

Status: the user approved the narrow hook/reaction MVP. Runtime implementation,
canonical/public documentation, and all sequential quality gates are complete
in the existing dirty `main` working tree. Release, commit, push, and deployment
remain outside this scope.

- Signals are a major orchestration primitive even though the public
  navigation currently places them under integrations. The implemented half is
  intentionally passive: a bound agent can publish a typed, schema-validated,
  durable, replayable claim, while readers independently inspect or follow it.
  It does not yet cause an external effect.
- Current Codex provides its own lifecycle-hook framework for command and MCP
  tool handlers around session, prompt, tool, permission, compaction,
  subagent, stop, interrupt, and session-end events. Codex can use synchronous
  handlers to influence its own loop and asynchronous handlers for advisory
  work. CoCo must reuse or expose that native facility for in-session policy
  and context rather than recreating its event vocabulary or control behavior.
- A distinct CoCo reaction layer remains useful for durable, cross-workspace
  and cross-repository facts that Codex does not own: accepted signals and a
  deliberately small set of committed CoCo lifecycle changes. It must not
  mirror all App Server notifications, infer ticket semantics, wake models by
  default, or grow into a workflow engine.
- Implemented one versioned CoCo event envelope and a narrow transactional
  outbox for `signal.emitted`, `workspace.created`, `workspace.closed`,
  `workspace.reopened`, and `workspace.deleted`. Loaded exact definitions
  select delivery in the same transaction as the source fact. Signal
  idempotency retries enqueue nothing twice; effect failure cannot roll back
  coordinator state.
- `cocod` loads a version-1 operator file from the standard config path, rejects
  unsafe/invalid configuration, and executes absolute command arrays without a
  shell. It clears the environment except `PATH`, sends compact JSON on stdin,
  discards process output, caps concurrency at four, enforces 1–300 second
  timeouts and 1–5 attempts, recovers interrupted rows, and cancels deliveries
  whose exact definition hash no longer exists after restart.
- Added daemon-wide `coco hook list|ls` and `coco hook
  history|deliveries`. These expose definitions and bounded delivery outcomes
  while deliberately hiding command arguments and event payloads. Terminal
  event groups retain the newest 10,000 completed groups without pruning
  pending or running work.
- The exact native boundary was checked in a fresh matching upstream clone at
  `/tmp/codex-upstream-0.154.0` and through the installed binary. Source shows
  `SessionStart` is queued when the session is created and executed at the
  first ordinary turn; `thread/start` alone and `thread/shellCommand` do not run
  it. The ignored real-App-Server proof passes using an isolated unreachable
  provider and test-only hook-trust bypass; production CoCo never bypasses
  native hook trust.
- Focused Rust hook tests, lifecycle/retirement process tests, and all three
  installed-Codex 0.154.0 compatibility proofs pass. The process tests cover
  signal/workspace envelopes, exact idempotency, successful history, and every
  initial workspace transition. `cargo clippy --locked --all-targets
  --all-features -- -D warnings`, all `258` library tests plus `5` process
  scenarios, `cargo machete`, and `cargo deny check` pass. Dependency-policy
  output contains only the already accepted duplicate-version warnings.
- Final review moved stdin writing under the hook timeout so a child that never
  reads its event cannot block dispatch indefinitely, included the canonical
  config working directory in definition identity, and added explicit tests
  for v10-to-v11 migration, oversized config, unsafe permissions, symlinks,
  and a full pipe backpressure timeout. Delivery is now serialized in commit
  order per hook ID, including retries, while distinct hooks retain bounded
  parallelism.
- Public docs now explain the native/CoCo hook choice, one complete command
  configuration, event shape, retries, idempotency, security, and CLI
  observability. TypeScript, Oxlint, Prettier, and the production static export
  pass; the export verifies `103` static files, `10` pages, search, `/coco`
  project-subpath routing, and the public/internal boundary. `nix flake check .
  --no-write-lock-file` and `git diff --check` pass. New files are marked only
  with Git intent-to-add so the dirty Flake source includes them; no file
  content is staged.
- Product sequencing recommendation: complete the signal-to-reaction loop
  first, harden daemon/App Server supervision before presenting it as
  unattended automation, and validate the boundary through one real external
  consumer. Detached branch promotion, handoff artifacts, Windows named pipes,
  and worker MCP registry/gateway work remain independent follow-ups.

Official source reviewed: [Codex hooks](https://learn.chatgpt.com/docs/hooks).

## Hook lifecycle controls and guards — 2026-09-10

Status: complete in the existing dirty `main` working tree. No commit, push,
release, deployment, or external integration was performed.

- Kept the existing post-event reaction contract intact. A reaction selects
  `signal.emitted` and may add an exact `NAME@VERSION` filter, or it can select
  one of the committed workspace lifecycle events. These deliveries remain a
  durable, retrying, at-least-once outbox after the source fact succeeds.
- Added `guards` to the same version-1 `hooks.json`, but kept them a distinct
  synchronous policy mechanism. The first bounded actions are
  `workspace.close` and `workspace.delete`; a guard cannot subscribe to a
  signal, mutate a retirement request, or replace CoCo's built-in safety
  checks.
- The coordinator resolves and validates the current retirement plan, rejects
  built-in blockers, and verifies any client-confirmed plan before running a
  matching guard. It still holds the repository operation lock, and no Store,
  Git, or Codex effect has begun. A dry-run returns before guards. Once a saga
  reaches `closing` or `deleting`, recovery converges it without rerunning an
  external policy check that could strand recovery.
- Matching guards use one coherent registry snapshot, run synchronously in
  stable ID order, and short-circuit on the first explicit denial. Commands
  receive bounded JSON through stdin and must return strict JSON allow/deny;
  deny requires a sanitized, bounded reason, while allow forbids one. They do
  not retry. Each definition must explicitly choose `onError: allow|deny` for
  spawn, output, exit, or timeout failures.
- `coco hook validate` now performs the complete safety and syntax check
  offline without constructing a daemon client. `coco hook reload` validates a
  replacement before atomically swapping the shared hook/guard snapshot; a
  rejected reload retains the last known good snapshot. There is deliberately
  no file watcher. `hook list` exposes both definition kinds without commands;
  `hook history` remains only the durable post-event delivery history.
- Guard outcomes are visible through daemon logs, and denials/fail-closed
  failures return `GUARD_DENIED` or `GUARD_FAILED_CLOSED` with bounded
  `guardId`, `action`, and `reason` metadata. They are not persisted as a new
  audit/history stream in this slice. Add that only for a concrete consumer and
  retention contract.
- The command target remains trusted same-user code rather than an
  authorization sandbox. HTTP/MCP targets, model wakeup, workflow chaining,
  declarative repository/workspace selectors, manual delivery retry, more
  guarded actions, and durable guard history remain unscheduled extensions.

Verification:

- `cargo fmt --all -- --check` passes.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passes.
- `cargo test --locked --all-targets --quiet` passes: `271` Rust tests pass,
  `3` are intentionally ignored, all `5` process scenarios pass, and the `3`
  opt-in real-Codex tests remain intentionally ignored in the ordinary suite.
- Focused hook/guard coverage passes `22` tests, including exact signal
  filtering, safe configuration, atomic reload, stable guard ordering,
  short-circuit denial, fail-open/fail-closed behavior, bounded output and
  timeouts. Four coordinator tests prove the dry-run, pre-effect, error, and
  recovery boundaries. The retirement process scenario proves daemon-free
  validation, last-known-good reload, and exactly one guard invocation per
  applied operation.
- `cargo machete` passes. `cargo deny check` passes all advisories, bans,
  licenses, and source policy with only the already accepted duplicate-version
  warnings.
- Public docs pass Next type generation, TypeScript, Oxlint, and Prettier. The
  production static export verifies `103` files, `10` pages, search, `/coco`
  routing, and the public/internal boundary.
- `nix flake check . --no-write-lock-file` passes on `x86_64-linux`.

## Workspace resource governance assessment — 2026-09-10

Status: research and design recommendation only. No runtime, CLI, schema,
configuration, public documentation, dependency, or persistence change was
made.

- Current CoCo owns one daemon and one shared Codex App Server for all bound
  workspaces. The App Server process itself is therefore shared overhead and
  cannot be truthfully divided between workspaces. Parent-process traversal
  alone also cannot separate commands for concurrent threads when their common
  ancestor is that shared server.
- Native Codex 0.154.0 does not expose a local-thread/workspace CPU, RAM, or
  process quota through the CLI, App Server request surface, or documented
  `config.toml`. Its sandbox limits filesystem/network access, MCP timeouts
  limit tool calls, and `agents.max_concurrent_threads_per_session` limits
  spawned-agent concurrency; none is an OS resource budget for one CoCo
  workspace. The source does contain an experimental code-mode-host capability
  named `session-cell-execution-resource-limits`, but its two fields are a
  JavaScript-cell yield ceiling and heap-size ceiling. Production Codex opens
  these sessions with defaults, the local in-process host discards the heap
  limit, and the mechanism does not govern normal shell commands or App Server
  threads. It is therefore not a reusable solution for this requirement.
- The experimental App Server background-terminal API is useful but does not
  change that conclusion. It can list, clean, and terminate the long-running
  unified-exec terminals retained by one thread. Although the protocol already
  declares `osPid`, `cpuPercent`, and `rssKb`, Codex 0.154.0 currently fills all
  three with `null`; the core record only carries the logical process ID,
  command, and working directory. It also does not cover transient commands or
  shared App Server/MCP cost. Treat this surface as a future native observation
  and cleanup seam, not as a complete process-tree or containment boundary.
- Current Orca source has a Resource Manager that performs a host process-table
  sweep, walks each registered PTY's descendant tree, and aggregates CPU and
  memory by session and worktree. It reports unattributed terminals, host
  pressure, and bounded history; its orchestrator separately defaults to four
  concurrent workers. No per-worktree cgroup, Job Object CPU/memory quota, or
  equivalent hard-limit path was found at the reviewed commit. Its Windows Job
  Object support is presently process-lifecycle containment, not a resource
  budget.
- CoCo must distinguish three contracts: observation reports what was measured
  and explicitly labels shared/unattributed cost; admission control decides
  whether another turn may start under host pressure or a concurrency cap; hard
  containment makes the OS enforce a budget. A sampled process scan is useful
  observability but is not a security or enforcement boundary.
- Recommended first slice, if scheduled, is truthful on-demand observation plus
  global safety admission: host available memory/load, CoCo/App Server shared
  cost, attributable descendants where evidence exists, process count, and an
  explicit unknown/unattributed bucket. Do not put a high-frequency time series
  in SQLite initially, and do not make ordinary `status` perform an expensive
  host sweep. A bounded live resource view can later emit a durable
  `resource.pressure` signal only after its threshold and deduplication contract
  is designed.
- A conservative global guard can protect the machine sooner than pretending
  to enforce per-workspace limits: cap simultaneously active turns and stop
  admitting new work below a configured available-memory threshold. It should
  warn or queue first; automatic killing needs a separate explicit policy and
  failure/recovery semantics.
- Real per-workspace CPU, memory, and process limits require a process
  containment boundary that CoCo can place every descendant into. Linux cgroup
  v2 is the natural first backend; Windows Job Objects are the corresponding
  process-tree primitive. The present shared-App-Server topology does not
  provide that boundary. A global daemon/App-Server cgroup is feasible but
  limits CoCo as a whole; a separate App Server or isolated execution host per
  workspace could provide strict attribution and enforcement at the cost of a
  material lifecycle/attachment architecture change. macOS should remain
  best-effort monitoring/admission unless a container or VM backend supplies
  hard isolation.
- One fallback is to lazily launch an App Server process inside one OS
  containment scope per active workspace. All descendants that Codex launches
  within that scope can then be attributed, stopped, and limited together, but
  this duplicates App Server/configuration overhead and complicates endpoint,
  event-routing, supervision, and TUI-attachment lifecycle. It is no longer the
  preferred containment design after the native exec-server finding below.
- A local idle-cost probe used two separately initialized Codex 0.154.0 App
  Servers with experimental API support enabled and no loaded thread or turn.
  Both sampled at `0.0%` CPU while idle. Their resident sets varied between
  roughly `112` and `142 MiB`, but much of that is shared file-backed code: the
  combined proportional set size was about `148 MiB`, or approximately
  `74 MiB` per instance, while private memory varied from about `38` to `68
  MiB`. This makes a few active isolated runtimes reasonable, but argues against
  keeping one resident server for every persisted workspace. Use lazy startup
  and idle retirement if this backend is scheduled.
- The better native boundary would be a shared App Server control plane with an
  explicit per-thread or per-execution resource scope: Codex would place every
  spawned command and owned descendant into the supplied cgroup/Job Object,
  while the shared server stayed outside. Codex 0.154.0 has no CPU/RAM/PID
  policy field, but it does already have a more important experimental
  execution boundary: one App Server can register multiple `codex exec-server`
  environments, and `thread/start`/`turn/start` can select sticky environment
  IDs with environment-relative working and workspace roots. The executor owns
  process, filesystem, sandbox, and supported environment-scoped MCP execution;
  current Codex tests explicitly prove two exec servers with mutually isolated
  workspace-write roots. `externalSandbox` is not this router—it only tells
  Codex that the server process is already externally sandboxed.
- The preferred first strict-resource spike is consequently one shared host
  App Server plus one host `codex exec-server` per active CoCo workspace,
  launched directly inside a workspace cgroup. The exec server is already a
  distinct process root, so its commands and descendants inherit exact Linux
  accounting and CPU/memory/PID enforcement without requiring an OCI runtime.
  CoCo would register that endpoint as an experimental environment and bind the
  environment to the workspace thread. Keep the shared App Server in a
  separate global safety scope while leaving `cocod` outside so it can report
  and recover failures. A local idle probe of `codex exec-server` 0.154.0
  measured about `48 MiB` RSS, `23 MiB` PSS, `10 MiB` private memory, and `0.0%`
  CPU—materially lighter than duplicating the full App Server.
- Treat an OCI container as an optional stronger execution backend behind the
  same environment/exec-server contract, not as a prerequisite for resource
  governance. It adds filesystem and network namespaces, image-defined
  toolchains, more portable quota controls, and a stronger accidental-escape
  boundary, but also introduces a runtime dependency, image lifecycle, mount
  and credential policy, slower cold starts, and awkward host-toolchain reuse.
  The existing host Git worktree can be bind-mounted rather than copied. Git
  linked worktrees remain compatible only if the worktree's `.git` indirection
  and the repository common Git directory are both visible at consistent
  container paths; mounting only the worktree directory commonly breaks Git.
  An independent clone is the stronger-isolation alternative but changes
  CoCo's established worktree semantics and should not be introduced merely to
  obtain resource limits.
- This environment surface is experimental and only partly described by the
  public App Server documentation. `environment/add`, thread/turn environment
  selection, and `codex exec-server` are present in the installed generated
  protocol/source and integration tests, but there is no resource-policy field
  or public stability promise. A CoCo spike must therefore capability-check the
  selected Codex build, pin compatibility, prove start/resume/fork/jump and MCP
  behavior, bind the endpoint locally and safely, and fall back without
  corrupting an existing workspace. It must also resolve container image/tool
  provisioning, credential projection, UID ownership, and Git linked-worktree
  paths before this becomes a product contract.
- Upstream issue `#11523` requested per-session/global memory governance and was
  closed `not planned`; an OpenAI contributor recommended container-level
  containment and said this likely does not belong in the agent harness. Open
  reports `#38909` and `#35433` request bounded Linux/Windows shell-process
  trees after host-exhaustion incidents, while `#43256` reports an App Server
  PID storm and demonstrates the value of an external daemon cgroup. No
  announced per-thread cgroup roadmap or active matching implementation PR was
  found in the targeted search. Treat that as current public evidence, not
  proof that OpenAI has no internal plan.
- In the future DAG/CMMN system, scheduling policy belongs to that task system:
  it decides which ready case gets a slot and may request a resource class.
  CoCo should own measurement, host-capacity admission, and enforcement for the
  workspace/runtime it launches. Do not move ticket or workflow state into this
  layer.

The implementation decision made after this assessment was to prove the native
per-workspace exec-server boundary and add on-demand observation in the same
slice. Global admission, hard limits, and an alternative execution backend
remain separate contracts rather than being inferred from sampled metrics.

Primary comparison source was fresh Orca commit
`f2d5711b2d32e9f11277cd63805c76b0b5f9ddf7`: [process statistics
types](https://github.com/stablyai/orca/blob/f2d5711b2d32e9f11277cd63805c76b0b5f9ddf7/src/shared/process-stats-types.ts),
[collector](https://github.com/stablyai/orca/blob/f2d5711b2d32e9f11277cd63805c76b0b5f9ddf7/src/main/memory/collector.ts),
[diagnostics command](https://github.com/stablyai/orca/blob/f2d5711b2d32e9f11277cd63805c76b0b5f9ddf7/src/cli/specs/diagnostics.ts),
and [coordinator concurrency](https://github.com/stablyai/orca/blob/f2d5711b2d32e9f11277cd63805c76b0b5f9ddf7/src/main/runtime/orchestration/coordinator.ts).
Platform primitives reviewed: [Linux cgroup
v2](https://docs.kernel.org/admin-guide/cgroup-v2.html), [Windows Job
Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
and [Windows CPU rate
control](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information).

## Native workspace execution and resource observation — 2026-09-10

Status: implementation and documentation complete in the working tree. The
checkpoint commit `ce8bc02` predates this slice; no commit or push was requested
for the runtime work yet.

- Default daemon execution now keeps one shared Codex App Server as the control
  plane and lazily launches one host `codex exec-server` for each workspace that
  first needs execution. Merely creating, listing, or observing an inactive
  workspace does not launch its executor. `COCO_WORKSPACE_EXECUTION=shared`
  remains an explicit compatibility fallback.
- The daemon assigns an opaque stable environment ID, accepts only a nonzero
  loopback WebSocket endpoint from the child, registers it with
  `environment/add`, and proves connectivity with `environment/info`. Startup
  output is bounded, child ownership uses kill-on-drop, and normal workspace
  close or daemon shutdown stops the owned executor. Reopening and reattaching
  re-registers the stable ID against the replacement endpoint.
- Fresh `thread/start` and every ordinary `turn/start` select the exact
  workspace environment. Context fork and thread resume first ensure the
  destination executor exists, then the following turn reselects it because
  Codex 0.154.0 exposes no `environments` field on `thread/fork` or
  `thread/resume`. `jump` places a loopback relay between the native TUI and the
  App Server so fresh and resumed interactive `thread/start`/`turn/start`
  requests receive the same selection without reimplementing the TUI.
- The exact upstream boundary remains visible: `thread/compact/start`,
  `review/start`, and `thread/shellCommand` have no environment selector in
  0.154.0. Consequently pre-turn compact/review immediately after resume or
  fork may use Codex's local default, and the TUI `!command` shortcut remains
  host-local. Normal agent shell tool calls after `turn/start` use the selected
  workspace executor.
- Detailed workspace status now carries an ephemeral `runtimeResources`
  projection and the public JSON envelope advances from schema version 7 to 8.
  It reports inactive, running, or exited state without starting an inactive
  process. Linux walks the current `/proc` descendant tree to aggregate process
  count and RSS and calculates CPU across consecutive samples; the first sample
  truthfully has no CPU percentage. Other hosts expose only evidence available
  from the root process. Collection status/list avoids process scans.
- The measured idle cost of one real Codex 0.154.0 exec server in this Linux
  environment was approximately 48 MiB RSS, 23 MiB PSS, 10 MiB private memory,
  and 0% CPU. These are observations, not quotas or capacity promises.
- No container backend, cgroup/Job Object policy, hard resource limit, admission
  controller, or resource-history persistence was added. A hard-killed daemon
  can leave an exec server behind because Codex 0.154.0 rejects
  `--exit-on-stdin-close` with a local listener; normal close and shutdown are
  covered. The ephemeral loopback executor endpoint also has no independent
  CoCo token and remains suitable only for the existing same-user local trust
  model.
- Unit and coordinator tests cover mode parsing, endpoint validation, opaque
  IDs, environment injection, status rendering, error sanitization, and stop
  delegation. The normal process suite continues to exercise the explicit
  shared fallback instead of maintaining a fake implementation of the
  experimental Codex environment protocol. The opt-in real-Codex suite proves
  registration and readiness, fresh and resumed attachment, normal turn
  selection, two simultaneous workspaces with distinct executor PIDs,
  same-daemon close/reopen/re-registration, workspace-close cleanup, and daemon
  shutdown cleanup against exactly `codex-cli 0.154.0`.
- Final verification after the close/reopen strengthening passed on
  2026-09-10: formatting, Clippy with warnings denied, all `285` ordinary Rust
  tests plus all `5` process scenarios, all `3` opt-in real-Codex tests against
  installed `codex-cli 0.154.0`, the static `/coco` export (`103` files and ten
  pages), and `nix flake check .`. The compatibility timeout is deliberately
  60 seconds because a cold native resume after archive/unarchive takes roughly
  25 seconds in both CoCo's direct path and a fresh standalone App Server
  connection; production RPC has no corresponding timeout.
- Public entry points now describe CoCo positively through what users can do.
  Product and ownership exclusions remain internal knowledge; negative wording
  in public material is reserved for concrete operational or safety limits that
  prevent surprise.
- Clarification: this is strictly a public-presentation rule for `README.md`
  and `docs/`. Internal product and engineering documents should retain clear
  statements about what CoCo does not own when those boundaries guide design.
- A strengthened real-Codex retirement test initially exposed a 20-second test
  timeout during cold resume after archive/unarchive. A fresh secondary App
  Server connection behaved identically and returned after roughly 25 seconds,
  ruling out the CoCo connection and workspace executor as the cause. The
  direct CoCo reopen/attach path passes with a 60-second compatibility-test
  budget; production RPC does not impose the discarded 20-second deadline.
- The final diff audit removed a diagnostic-only lifecycle deviation: close
  continues to unsubscribe explicitly before optional native archive, then
  proves quiescence and stops the workspace executor before Git removes the
  worktree. This preserves the established subscription semantics even when a
  thread was already archived elsewhere.
- The original Codex repository was fast-forwarded through commit `196964ef10`
  on 2026-09-10 and rechecked. Its current protocol still offers environment
  selection on `thread/start` and `turn/start`, not on `thread/resume` or
  `thread/fork`; the documented routing boundary remains accurate for the
  pinned `codex-cli 0.154.0` behavior suite.

## Public documentation coherence review — 2026-09-10

Status: complete after checkpoint `c14bda4`; this is a documentation-only
working-tree slice and has not been committed.

Scope: re-read every rendered MDX page and `README.md`, compare command claims
with the generated CLI help, and make the workspace/runtime mental model
consistent without exposing internal architecture. Preserve the existing
static site structure unless a page fails to answer a user question.

Initial findings:

- Public entry points correctly lead with durable, addressable Codex work, but
  several still describe a workspace only as worktree plus thread/settings.
  Since detailed status now exposes it, the minimal user model should also say
  that the first activating action starts one dedicated `codex exec-server`,
  reuses it for later turns, and stops it on close or normal daemon shutdown.
- Generic claims about "resource use" overstate cross-platform behavior. The
  execution process is visible on supported hosts; process-count, RSS, and CPU
  measurements are currently Linux-specific.
- The README phrase "a lightweight Codex execution process only when a
  workspace first needs it" is ambiguous. Name the process and the activating
  commands explicitly instead of asking the reader to infer whether normal
  workspace execution uses it.
- Generated help for the root command and every public subcommand was compared
  with the reference. Command names and core semantics match; the compact
  workspace table should expose the existing global and confirmation flags
  more consistently.
- The official Codex hooks page remains the correct target for session-level
  hooks. Keep CoCo's saved lifecycle/signal reactions and retirement guards
  clearly separate in user language.

Outcome:

- README, landing page, overview, quickstart, installation, workspace guide,
  profiles, MCP, signals, hooks, CLI reference, and troubleshooting were all
  reviewed as one public journey. The existing ten-page navigation remains
  small and every page still has a concrete user purpose.
- The entry points now explain one consistent workspace model: `create`
  prepares the worktree; the first `send` or `jump` starts one dedicated
  `codex exec-server` in the default mode; CoCo reuses it; `close` or orderly
  daemon shutdown stops it; and reopen activates a replacement lazily. Fresh
  jump's thread remains bound only after the first interactive action.
- Resource claims now distinguish the visible executor from Linux-only process
  count, RSS, and CPU sampling. The environment-specific 48 MiB observation was
  removed from public installation prose and remains internal engineering
  evidence.
- Agent signals now have their own overview next step rather than being folded
  into the hooks card. MCP prose directs worker-side tools to the selected
  Codex profile, while the hooks guide retains the verified official Codex
  hooks link and keeps both hook systems distinct.
- The compact CLI table now includes the shipped global and confirmation flags
  consistently. All command semantics were checked against generated help,
  not inferred from earlier prose.

Verification:

- `pnpm --dir docs run check` passes type generation, TypeScript, Oxlint, and
  Prettier.
- The production build with `DOCS_BASE_PATH=/coco` and the repository URL
  passes. It statically exports 103 files and all ten pages, verifies search
  and project-subpath routing, and proves the public-only boundary.
- Root `README.md` separately passes Prettier and `git diff --check` reports no
  whitespace errors.

## Public alpha activation and repository cleanup — 2026-09-10

Status: GitHub publication switches activated at the user's explicit request;
the local cleanup is complete and authorized for the public-alpha commit and
push.

Scope and findings:

- A locked `cargo publish --dry-run` against crates.io packaged 117 files
  (1.3 MiB, 247.8 KiB compressed), compiled the packaged crate successfully,
  and stopped before upload. The public crates.io API reports that
  `codex-coordinator` does not yet exist.
- GitHub still reports `janthmueller/coco` as private. Pages now exists with
  `build_type=workflow`, HTTPS enforcement, and the intended
  `https://janthmueller.github.io/coco/` URL. The Documentation workflow was
  re-enabled. No deployment was dispatched from the older remote revision.
- The `crates.io` environment contains the named `CARGO_REGISTRY_TOKEN`, and
  the repository variable `COCO_RELEASE_ENABLED` was changed from `false` to
  `true`. No release or push was started in this slice.
- Remote Actions history is empty and local `main` remains three commits ahead
  of `origin/main`; therefore the current candidate has no hosted CI result or
  Pages artifact until it is pushed.
- `schema/` contained 361 generated experimental Codex App Server files
  totaling 4.4 MiB. No Rust code, build, test, package, or workflow consumes
  them; the narrow hand-owned adapter and real-process compatibility suite are
  authoritative. The tracked snapshot was removed, `/schema/` is now ignored,
  and the architecture record says to generate a disposable local snapshot
  only for an upgrade review.
- Root `handoff.md` was removed before publication because it still specified
  TypeScript, task/new vocabulary, and the superseded original architecture.
  Its durable decisions already live in current canonical knowledge.

Verification:

- `cargo package --locked --list` succeeds and contains only the declared
  crate inputs.
- `cargo publish --dry-run --locked` completes packaging and verification.
- Both staged and unstaged `git diff --check` pass after schema removal.
- A repository-wide reference scan finds no live consumer of the removed
  schema snapshot; the remaining canonical reference is its on-demand
  generation command, plus an accurate historical entry in this log.
- Gitleaks 8.30.1 reports no leaks across all 48 commits and approximately
  7.46 MB of Git history. A separate directory scan of the exact next-commit
  file set, including the unstaged documentation changes and staged schema
  removal, also reports no leaks across approximately 2.30 MB.
- No current or historical path resembles a tracked environment file,
  credential, private key, database, or local authentication file. Historical
  filename matches for `Token` are generated App Server protocol type names,
  not credential material. The ignored browser, build, and Nix artifacts are
  absent from Git.

## Public alpha release and landing-page refinement — 2026-09-10

Status: complete. Commit `e4c3f07` is pushed, Documentation run `34525149861`
deployed it successfully, and the corrected landing page is live.

Outcome:

- The user explicitly authorized public visibility and publication. The
  repository is public, GitHub Pages deploys from the active Documentation
  workflow, and the first release is tagged `v0.1.0-alpha.1` with Linux and
  macOS artifacts.
- The first crates.io attempt reached the registry but was rejected because
  the account email was not yet verified. After verification, retrying only
  the failed job published `codex-coordinator 0.1.0-alpha.1` successfully; no
  second version or tag was created.
- A screenshot of the first live landing page exposed two presentation issues:
  the header had no visible documentation destination, and the oversized,
  tightly tracked negative headline collapsed word boundaries. The shared
  navigation now includes a `Docs` link, while the hero positively states
  `Codex work, coordinated.` and uses more restrained type with explicit word
  spacing.
- The supporting copy was shortened to the minimum workspace model needed on
  the landing page. No internal architecture or roadmap material was added.
- Canonical documentation knowledge now reflects the authorized public-alpha
  state instead of the superseded private-incubation state.

Verification:

- `pnpm --dir docs run check` passes type generation, TypeScript, Oxlint, and
  Prettier.
- The production build with `DOCS_BASE_PATH=/coco` and the public repository
  URL exports and verifies 103 static files, ten pages, search, project-subpath
  routing, and the public-only boundary.
- Headless Chrome renders the production export at `1678x873` and `390x844`.
  The desktop header exposes `Docs`, the headline has clear word boundaries,
  and both layouts retain their intended hierarchy without clipping.
- The successful Pages deployment was fetched from its public URL and contains
  both `href="/coco/docs/">Docs` and `Codex work, coordinated.`.

## Public documentation layout refinement — 2026-09-10

Status: complete in the working tree.

Outcome:

- Fumadocs' home layout used a 1,400-pixel navigation width while the landing
  content used a 74-rem container. The home layout now accounts for its own
  horizontal padding so the brand, `Docs` link, hero, and lower sections sit on
  one intentional grid.
- Landing display headings use a smaller ceiling, lighter weight, gentler
  tracking, and a readable line height. The mobile heading retains the same
  word separation without overflowing its viewport.
- The notebook description's built-in bottom margin had compounded with the
  custom body margin and flex gap. The page introduction now uses one compact
  rhythm; its title, description, divider, prose, and outline remain aligned.
- Public terminology now presents post-event commands as hooks and pre-action
  checks as guards. `Reaction` remains an internal delivery classification,
  not a third user concept.
- Restart wording now distinguishes an interrupted in-flight turn from the
  workspace, worktree, thread, and saved conversation that remain recoverable.

Verification:

- `pnpm --dir docs run check` passes TypeScript generation/checking, Oxlint,
  and Prettier.
- The production `/coco` static export verifies 103 public-only files, ten
  pages, static search, and project-subpath routing.
- Headless Chrome renders the exported landing and overview pages at
  `1690x900` and `390x844`. The desktop navigation shares the landing content
  edge, headings retain visible word boundaries, and the compact overview
  introduction remains responsive without clipping.

## CLI interaction edge-case audit — 2026-09-10

Status: implementation and verification complete in the working tree.

Outcome:

- The picker terminal primitive is now a reusable inline frame. Status follow
  rewinds relative to the rows it allocated instead of restoring a coordinate
  invalidated by terminal scrolling, and it restores wrapping and cursor state
  on exit.
- An explicitly named send target is resolved through the daemon before CoCo
  asks for an omitted message. Turn start still performs the final lookup to
  protect against a race.
- `status --resources`/`-r` makes RSS, process count, and CPU explicit in human
  output for either a workspace or collection; JSON status includes available
  observations without that presentation flag. `--follow` now also accepts
  `-f`, and short flags can be clustered.

Original findings:

- Interactive `status --follow` incorrectly relies on terminal save/restore
  coordinates. When its first multi-line frame starts at the bottom margin,
  writing the frame scrolls the viewport but does not relocate the saved
  coordinate. Every later restore therefore starts at the bottom again and
  appends another visible frame. A later invocation can appear healthy when it
  happens to begin with enough rows below the cursor.
- The existing `FollowOutput` unit test proves only that the expected escape
  bytes were emitted into a `Vec<u8>`; it cannot model terminal scrolling and
  therefore misses this bug. An isolated 70-by-8 tmux probe rendered one final
  frame when started near the top but three copies when started at the bottom.
- Rewinding by the number of allocated CRLF rows, as the picker already does,
  left exactly one final frame in the same bottom-edge probe. That evidence led
  to the shared inline-frame primitive now used by both selection and follow,
  including changing frame height, resize, wrapping, cursor restoration, and
  targeted or collection status.
- `coco send WORKSPACE` also has an ordering bug when the message is omitted.
  `resolve_workspace_input` validates picker-derived targets through
  `workspace.list`, but for an explicit reference it merely constructs a scope
  and returns the unverified string. `run_send` then opens `Message:` before
  the first daemon request, so an invalid or ambiguous workspace is rejected
  only after the user has typed a message.
- The implemented send correction deliberately reuses non-loading
  `workspace.get` before secondary input, returns the canonical workspace ID,
  and retains the real `turn.start` resolution as the final race-safe check.
  Repository/name resolution remains owned by the coordinator rather than
  being duplicated in the CLI.

Verification:

- Focused library and process tests cover conditional resource sampling,
  collection cells, JSON completeness, early send validation, and status CLI
  forms.
- The isolated real-tmux smoke test starts follow at the terminal bottom
  margin, observes changing frame sizes, and confirms one live/final frame plus
  restored terminal state. It does not touch the user's tmux server.
- `cargo check --all-targets --locked`, all library/all-target tests, Clippy
  with warnings denied, `cargo machete`, `cargo deny check`, package/publish
  dry-runs, documentation checks/export, and the flake check complete without
  a new product failure. The expected duplicate-version warning remains in the
  crates.io dry run.

## Open questions and handoff

- `janthmueller/coco` is now public, its Pages site and Documentation workflow
  are active, and `COCO_RELEASE_ENABLED` remains true at the user's explicit
  direction. Release `v0.1.0-alpha.1`, its Linux/macOS artifacts, and
  `codex-coordinator 0.1.0-alpha.1` on crates.io are published.
- The release workflow still requires the Rust result for the exact candidate
  SHA but not the separate Documentation result. With automatic release now
  enabled, a successful Rust run can publish while Documentation is still
  running or has failed. Close this gate gap if documentation success must be
  mechanically required rather than checked operationally.
- The earlier crates.io pause is resolved: the narrower headless,
  multi-repository control-plane direction, native-first reduction, dedicated
  workspace runtime, and released-Codex behavioral proof are complete. The
  user explicitly authorized both automatic alpha publication and public
  repository visibility.
- The user withdrew the follow-up issue search and upstream-comment idea after
  the private checkpoint was completed. Do not pursue or post either unless
  explicitly requested again.
- Native Codex per-thread MCP isolation across start, resume, and fork is
  proven against 0.154.0. The broader worker registry remains unscheduled.
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
- Codex's current native lifecycle extensibility and CoCo's first durable
  reaction slice are implemented and documented above. Preserve the
  distinction between native Codex hooks, durable CoCo signals/events, and
  executable external effects. HTTP targets, automatic model wakeup, workflow
  chaining, and broader App Server event mirroring remain unapproved.
- The reorganized public site now passes a production export under the `/coco`
  GitHub Pages project subpath with ten total routes. Keep that static-export
  check when changing its routing or deployment workflow.
- The earlier Rust module-layout Phase 2 and its review are complete. Do not
  split a workspace now. If cross-platform support is scheduled next, begin
  that architecture's Phase 3 by moving the existing Unix RPC backend behind
  the common transport API, then add Windows named pipes with native CI before
  claiming Windows support. Otherwise prefer a user-visible capability over
  more structural movement.
- Track per-workspace token usage and estimated cost as a later observability
  task. First audit the released App Server's authoritative per-turn/thread
  counters, then define fork/compaction/retry attribution, persistence, JSON
  output, and versioned model pricing. Raw token evidence and derived monetary
  estimates must remain distinguishable. This does not expand the first cgroup
  containment slice.

## Non-blocking dirty-source creation

Status: complete in the working tree.

Scope:

- Allow ordinary `coco create` from a dirty source checkout without adding a
  redundant ignore flag.
- Warn before creation, preserve the source checkout unchanged, and create the
  destination from its independently selected committed base without copying
  tracked or ordinary untracked changes.
- Keep strict rejection and ignore as distinct typed daemon policies so the
  CLI can preflight safely and non-CLI clients remain explicit.
- Continue honoring explicitly declared `.worktreeinclude` files independently
  from tracked and ordinary-untracked state.
- Cover wire representation, Git behavior, coordinator behavior, the real CLI
  retry path, and user-facing documentation.

Decision:

- The CLI sends the strict request first. Because dirty rejection occurs before
  persistence, hooks, worktree creation, or thread activation, it can print a
  warning and safely retry the same operation with `changes: ignore`.
- Default omission must not imply `HEAD` or otherwise alter base resolution.
  Explicit carry modes remain unchanged, and no interactive confirmation is
  introduced for this non-destructive operation.

Verification:

- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --locked -- -D warnings` passes.
- `cargo test --locked --all-targets` passes outside the filesystem sandbox:
  292 library tests and all 5 process tests pass; only the 7 explicitly manual
  or live-Codex tests remain ignored.
- The focused real daemon/CLI lifecycle test proves a dirty source produces the
  warning, retries without duplicate creation effects, and completes the
  existing send/jump flow.
- `pnpm --dir docs run check` passes.
- The production static export with `DOCS_BASE_PATH=/coco` and the public
  repository URL passes, verifying 103 files and all 10 public pages.
- `git diff --check` passes, and a repository-wide wording scan found no stale
  claim that ordinary CLI creation requires a clean checkout.

Follow-up kept out of this slice:

- Direct deletion of an open workspace remains a separate lifecycle change.
- Fine-grained omission such as carrying tracked changes while explicitly
  leaving ordinary untracked files behind remains unchanged and can be
  reconsidered independently.

## Close/delete behavior and CLI review

Status: complete. The user approved the breaking alpha CLI change; the
implementation and final verification are recorded below. Review findings
describe the original behavior, before this change.

Completed implementation checklist:

- [x] Make deletion accept open/closed and safely inspect failed provisioning;
  delete owned resources by default, with `--keep-thread`/`--keep-branch`.
- [x] Preview all effects together and run both applicable guards before effects.
  Persist one deletion intent and recover without replaying a destructive
  worktree discard after a crash.
- [x] Protect unretained commits independently from uncommitted files; preserve
  adopted branches and expose retained branch/thread identities.
- [x] Keep close reversible, correcting detached commit checks to actual Git
  reachability rather than a comparison to the original base.
- [x] Update focused lifecycle/recovery/CLI tests and affected documentation;
  preserve and verify the preceding dirty-source fix in the same worktree.

Scope:

- Compare the current close/delete defaults with the workspace resource model
  and the expected meaning of permanently deleting a workspace.
- Evaluate deleting an open workspace directly, retention flags, and handling
  dirty files, existing branches, detached commits, dependent context, hooks,
  and interrupted operations.
- Report concrete recommendations before changing lifecycle behavior. Preserve
  the completed, uncommitted dirty-source creation fix.

Findings:

- Ordinary close stops the managed executor, removes the exact verified
  worktree, and retains the workspace record, branch, and conversation.
  Archive is a separate opt-in. Interactive close already offers an explicit
  discard confirmation for local changes without requiring the flag first;
  scripts need `--discard-changes`, and `--yes` alone never authorizes loss.
- Delete currently requires closed availability in both coordinator and store.
  Its default removes the workspace record and associated operational rows;
  thread and branch deletion require separate flags. The selector likewise
  shows only closed workspaces. This resembles deregistration more than the
  whole-workspace deletion a user expects.
- Branch deletion checks CoCo ownership, current ref identity, and absence of
  another checkout, then uses `git branch -D`. It does not check whether
  commits remain reachable from other branches/tags. A destructive default
  must expose/protect that additional loss boundary.
- The detached close blocker compares HEAD to the creation base. It also
  blocks commits retained elsewhere; it is not a reachability check. A
  retained SHA in SQLite alone is not a durable Git retention reference.
- Close requires ready provisioning state; delete requires closed state. A
  failed provisioning record therefore has no normal cleanup route through
  these commands. A generalized deletion planner should inspect and report
  the resources actually present in failed/prepared/open/closed workspaces.
- Signals deliberately survive record deletion under their existing retention
  policy, as the signal-store test confirms. Resource deletion must not claim
  to erase emitted history or external effects.

Recommended product contract:

- Close means park for reopening: stop execution, remove the clean worktree,
  preserve branch, thread, and binding. Keep native archive optional; current
  archive checks are stricter for descendants than ordinary close.
- Delete accepts open or closed workspaces and removes the managed worktree,
  executor, record, bound thread, and a CoCo-created branch by default.
  `--keep-thread` and `--keep-branch` select retention. An existing branch
  supplied with `--checkout` remains external and is explicitly shown as kept;
  detached workspaces have no branch selection.
- Plan all selected effects and blockers before any mutation. For open
  workspaces, include both close and delete guards before removal; use one
  confirmation and one durable operation intent, with effect-time rechecks
  and recoverable partial progress. Merely calling today's CLI close and then
  delete would discover delete blockers/guards too late.
- Dirty tracked/untracked/ignored files remain a distinct discard choice.
  Show commits at risk separately; keep a retaining branch/ref or explicitly
  approve discarding those commits. Ordinary `--yes` must not expand either
  loss policy. Preserve active-turn, attachment, descendant, dependency,
  worktree-lock, path, and branch-identity checks.
- Make kept resources discoverable by reporting branch names and thread IDs.
  Do not silently reverse the meaning of existing `-t`/`-b` options. Changing
  deletion defaults is a breaking CLI change, especially for old `delete -y`
  scripts; retain explicit wire policies and plan migration before shipping.

Verification:

- Reviewed the current CLI argument/confirmation flows, coordinator/store
  transitions, Git removal/ref checks, hooks/guards, context dependencies,
  and existing retirement and signal-history tests.
- No lifecycle code changes, runtime operations, Git deletions, or new test
  runs were needed for this source-based review. The prior completed
  dirty-source changes remain untouched.

### Approved implementation and verification

Implemented on 2026-09-11, following the user's explicit approval of the
breaking alpha change:

- `delete` selects open and closed workspaces, including safe cleanup of
  failed preparation. It removes the managed worktree/runtime, record, bound
  native thread, and owned branch by default. `--keep-thread`/`--keep-branch`
  retain resources and print exact IDs/names. Adopted branches always remain;
  a planned branch name alone cannot establish ownership after a collision.
- The CLI uses one combined plan and one yes/no confirmation. Discarding files
  and unretained commits are separate choices; scripts must explicitly select
  `--discard-changes`/`--discard-unretained-commits`. `--yes` alone grants neither. The old
  delete `-t`/`-b` and long removal flags are rejected, not silently reversed.
  The RPC keeps required explicit deletion booleans, avoiding implicit loss
  when a client omits a policy.
- Close keeps its reversible contract and optional `-t` archive. Detached
  commits are allowed when another branch/tag/remote ref retains them, rather
  than comparing HEAD to the initial base. Git loss checks run again before
  effects; owned-branch removal uses expected-object compare-and-delete.
- Open deletion checks both close/delete guards before any lifecycle effects,
  then revalidates the complete plan. Success emits only `workspace.deleted`.
  Active work, attachments, decisions, terminals, descendants, dependencies,
  path/binding drift, and locks remain protected.
- Schema v12 persists deletion origin and explicit commit-discard policy.
  Recovery never replays a destructive file discard on a surviving worktree:
  it restores open availability for a fresh confirmation. Once the worktree
  is gone, verified remaining thread/branch effects can finish. Partial failure
  explicitly reports `WORKSPACE_DELETION_INCOMPLETE` without leaking private
  native error data. Signal history retains its existing policy.
- The expanded deletion saga and planner have focused modules under
  `coordinator/retirement/`; new policy, recovery, and migration tests have
  separate files. This is the boundary needed by the combined lifecycle,
  not a package-wide layout refactor. Public docs describe user choices;
  canonical product, architecture, hook, and Rust-layout knowledge hold the
  internal contract. The preceding dirty-source fix remains included.

Verification:

- `cargo fmt --all -- --check` and
  `cargo clippy --all-targets --locked -- -D warnings` pass.
- `cargo test --locked --all-targets -- --test-threads=2` passes: 309 library
  tests and all 5 process tests; the 4 manual/live library tests and 3 real
  Codex integration tests remain deliberately ignored. Local IPC tests ran
  outside the filesystem sandbox. No live user workspace was deleted.
- Coverage includes open/closed/detached deletion, adopted and retained refs,
  loss cancellation, `--yes` safety, native rejection/ambiguity, dependency
  races, both guards before open deletion, failed provisioning, safe restart,
  schema-v11 migration, and the actual CLI/daemon workflow.
- `cargo machete` finds no unused dependencies. `cargo deny --offline check`
  passes against the existing advisory cache, with existing duplicate-version
  warnings. The latter needed permission to acquire its cache lock; no
  dependency version changed.
- `pnpm --dir docs run check` passes. The `/coco` production export verifies
  103 static files, all 10 pages, search/routing, and the public-only boundary.
- Built `coco close --help` / `coco delete --help` match the new documentation.
  `git diff --check` passes.

Handoff:

- Implementation complete locally. No commit, push, release, live workspace
  retirement, or remote configuration change was requested or performed in
  this slice.
- A future commit should include the new untracked deletion/planner/test and
  migration files together with the tracked changes. Both this slice and the
  preceding dirty-source fix are still uncommitted.

### Destructive-option naming follow-up

After reviewing the safety model, the user selected the more precise
`--discard-unretained-commits` name. The rename is complete in the Clap field,
typed RPC (`discardUnretainedCommits`), guard input, stored schema-v12 intent,
planner/execution code, tests, and public/internal documentation. The former
`--discard-commits` and `discardCommits` spellings are deliberately rejected;
there is no compatibility alias in this unreleased alpha change.

Public guidance now states the close-time loss boundary explicitly: close
removes the worktree, so tracked edits and untracked/ignored files do not live
in the retained branch and require `--discard-changes` if not first committed,
stashed, or moved. `--discard-unretained-commits` remains delete-only and
applies only when another branch, tag, or remote-tracking branch will not retain
the commits after the selected ref/worktree is removed.

Follow-up verification:

- `cargo fmt --all -- --check` and Clippy with warnings denied pass.
- The full library suite passes with 309 tests and 4 explicit manual/live
  ignores. The focused retirement suite also passes all 51 selected tests.
- Protocol and CLI tests assert the new names and reject the old spellings.
- Public documentation check and the production `/coco` static export pass;
  the export again verifies 103 files and all 10 pages.
- A clean-target build presents the final long option and its
  branch/tag/remote-retention description. The repository's existing default
  dev fingerprint initially (and incorrectly) reused the pre-change binary;
  rebuilding the package with incremental compilation disabled refreshed the
  local executable, whose `close --help` and `delete --help` now match source.
  This was a local build-cache inconsistency rather than a second CLI path.
- The five process-level CLI/daemon tests pass after the final rename. Their
  first sandboxed run could not bind the fake App Server's loopback port; the
  required out-of-sandbox rerun passed all five tests.
- `git diff --check` passes.

### Destructive-command help audit

The generated help now describes `close` as keeping the workspace available
to reopen and calls `--dry-run` output a close plan rather than exposing the
internal retirement term. Both `--yes` descriptions state the same contract:
confirmation is skipped, but no discard policy is granted. The delete text
names both protected classes, local changes and unretained commits.

The focused CLI parsing/help tests pass (3 selected), library Clippy passes
with warnings denied, the rebuilt `close --help` and `delete --help` output was
inspected, and `git diff --check` remains clean. No command behavior changed.

### Cgroup-v2 containment design checkpoint

The pre-design product checkpoint is local commit `699ebaa` and has not been
pushed. Workspace token usage and estimated cost are now a distinct deferred
observability item in the canonical runtime knowledge; native usage evidence,
attribution, pricing-version, retention, and unknown-model behavior must be
designed before it becomes output.

The current executor boundary, lifecycle, storage, process-tree sampler, and
single-daemon lock were audited. The selected Linux direction is a transient
rootless systemd user scope per lazy workspace exec server, under an opaque
data-directory-specific slice. This respects systemd's single-writer cgroup
contract and places the executor plus descendants inside the boundary from
process creation. Direct cgroupfs ownership and containers are not part of the
first resource-only slice.

Scope and slice names derive only from hashes of the canonical CoCo data path
and stable workspace ID. A clean daemon shutdown or workspace close stops the
whole scope. A hard daemon death leaves no reusable endpoint, so the next
daemon generation must stop only its own stale namespace after acquiring the
data-directory lock; it must never sweep a broad `coco-*` pattern. Any partial
launch must also stop its exact deterministic unit before returning an error.

The first implementation slice contains containment lifecycle and exact
cgroup accounting, but no user limit flags. `memory.current` remains distinct
from fallback summed RSS; `cpu.stat` provides interval CPU, `cgroup.procs` and
`pids.current` distinguish processes from tasks, and controller event counters
show limit pressure. Auto-detection can choose today's process-tree fallback
before spawn, but once a systemd launch begins CoCo will not silently weaken a
failed activation. Configured limits later make containment mandatory.

Local disposable probes on this host confirmed a unified cgroup-v2 hierarchy,
rootless transient user scopes, readable CPU/memory/PID counters, immediate
runtime changes to memory/CPU/task properties, and whole-scope stop. Official
kernel and systemd contracts back the accepted design. `git diff --check`
passes; no product code, public documentation, live CoCo workspace, remote
state, or release was changed during the design.

Next implementation order:

1. add the focused execution-containment module, capability selection,
   instance-scoped stale cleanup, scope launch, and whole-scope shutdown;
2. add truthful cgroup-v2 measurements and controller events while retaining
   the process-tree fallback;
3. observe real behavior, then jointly design durable global/workspace limits
   and dynamic updates; and
4. only afterward consider a shared pool, admission, and idle runtime stop.

### Cgroup-v2 containment implementation

Implemented the approved first slice without adding limits or changing thread
routing. On a compatible Linux user session, every lazy workspace exec server
now starts in an opaque transient systemd scope beneath a stable per-data-dir
workspace slice. The instance and workspace components use 128-bit SHA-256
prefixes. Runtime activation verifies both the supervisor's unified cgroup
membership and the exact workspace-slice/unit ancestry before registration.
Startup cleanup accepts only the exact lower-hex unit shape for the current
instance, so it cannot broaden into another CoCo data directory's scopes.

Normal close, delete, daemon shutdown, registration failure, and partial
startup stop the complete scope. The final audit also made shutdown retain its
direct-process fallback and retry exact scope cleanup if the first systemd
stop fails. `auto` selects this backend only after cgroup-v2/controller and
user-manager probes; `systemd` makes it mandatory and `process-tree` preserves
the explicit compatibility path. A systemd activation failure never silently
falls back after launch has begun.

Status now reads `memory.current`, `cpu.stat`, `cgroup.procs`, `pids.current`,
and memory/PID/CPU event counters from the verified boundary. Human output
uses the neutral `MEMORY` heading and says `memory` for cgroup-charged bytes,
while fallback detail retains the truthful `RSS` label. Schema-v10 JSON keeps
charged memory and RSS in distinct fields and additionally exposes task count,
cumulative CPU, event counters, and the opaque unit. Inactive and unsupported
measurements remain absent. Public documentation explains only the user-facing
behavior and fallback; topology, safety rationale, and future limit policy
remain canonical internal knowledge.

Final verification on 2026-09-11:

- `cargo fmt --all -- --check` and Clippy with all targets and warnings denied
  pass after the final safety audit.
- The full Rust suite passes with 319 library tests and all 5 process tests;
  5 deliberate manual/live library tests and the opt-in real-Codex tests remain
  ignored in that ordinary run.
- The focused live systemd test passes and proves descendant accounting plus
  exact next-generation scope cleanup.
- The model-free real Codex compatibility suite passes all 3 tests against the
  pinned 0.154.0 executable, including real workspace-pool placement,
  measurements, and shutdown cleanup. No model turn or token usage occurred.
- The explicit process-tree fallback had already passed its real compatibility
  path before the final systemd-only argument hardening.
- Public docs typecheck, lint, and format checks pass. The `/coco` production
  build verifies 103 static files, all 10 pages, search, routing, and the
  public-only boundary.
- `git diff --check` passes after the implementation. The cgroup slice is ready
  for its own local checkpoint after `699ebaa`. Nothing was pushed, released,
  or changed remotely.

The next product decision is deliberately not hidden inside this slice:
observe real workspace costs first, then design persisted global defaults and
workspace overrides for `MemoryLow`/`MemoryHigh`/`MemoryMax`, `CPUWeight`/
`CPUQuota`, and `TasksMax`. A whole-CoCo cap would additionally require the
daemon and shared App Server to run in a sibling control-plane scope beneath
the already reserved instance parent. Token/cost accounting remains a separate
observability task.

### Cross-platform containment checkpoint

The pre-policy review found no single host-native cross-platform equivalent to
cgroup v2. Linux cgroup v2 provides hierarchical aggregate control and
accounting. Windows Job Objects provide a strong native peer: process-tree
membership, nested jobs, aggregate accounting, whole-job termination, memory
and process limits, and CPU weights or hard rate caps. macOS process groups can
provide whole-group signalling, while inherited `setrlimit` and launchd job
limits are per-process/job facilities rather than a directly equivalent,
dynamic nested resource hierarchy. They must not be presented as equivalent
aggregate enforcement without a separate proof.

An OCI/container runtime is the closest common operational abstraction, but on
macOS and ordinary Windows development it adds a Linux VM or different
container mode plus worktree mounts, credentials, toolchains, networking, and
filesystem-performance semantics. It is therefore a useful future opt-in
strict-isolation backend, not the transparent default for host-native CoCo
workspaces.

The recommended direction is a platform-neutral CoCo containment/policy port
with capability-reporting backends: current Linux systemd/cgroup v2, future
Windows Job Objects, a macOS lifecycle/observation backend that claims only
what it can prove, and an optional container backend. Common policy fields may
be accepted only where their semantics genuinely map; Linux-specific controls
such as `MemoryLow` must remain explicit rather than being given misleading
portable names. Any requested hard guarantee must fail closed when the chosen
backend cannot enforce it. The user accepted this direction, and it is now the
canonical runtime extension boundary.

### Portable policy and Linux enforcement implementation

Implemented a versioned portable policy with exact byte-valued memory high and
maximum controls, a CPU maximum expressed in millicores, relative CPU weight,
and a task/thread maximum. The policy and monotonic desired revision live in a
schema-v13 workspace child table. No row remains the revision-zero empty
default, and workspace deletion cascades the row. Backend capability reporting
uses portable semantic fields rather than systemd names; the current Linux
systemd/cgroup-v2 backend advertises all five while process-tree/shared
execution advertises none. A non-empty desired policy therefore fails closed
both when changed and again before any later workspace activation.

Added `coco limits show`, `set`, and `reset` with ordinary local/global
workspace resolution. The CLI parses exact decimal and IEC byte units, CPU
capacity to three fractional core digits, typed per-field clears, and stable
JSON. Human output separates desired policy from its applied runtime snapshot.
No limit is configured implicitly.

The Linux backend supplies configured systemd properties at scope creation and
uses runtime property updates for live scopes, then reads the kernel cgroup
files back before claiming the revision was applied. `MemoryMax` below current
charged use is rejected before mutation. Live testing exposed an upstream
systemd behavior where clearing `CPUQuota` reports success without resetting
`cpu.max`; CoCo does not kill a workspace to hide that mismatch. It persists
the desired reset, retains the stricter applied cap, and reports that it will
apply after the next runtime start. Other enforcement failures roll durable
intent back with a new monotonic revision; incomplete rollback stops execution
on a best-effort basis and fails explicitly.

Final verification on 2026-09-11:

- all-target Clippy with warnings denied passes;
- the ordinary all-target Rust suite passes with 334 library tests, all 5
  process tests, and 6 deliberate ignored library/live probes;
- both the existing live containment test and the new live policy mutation
  test pass against the real systemd user manager;
- canonical runtime, product, and persistence knowledge has been brought to
  schema v13 and the implemented policy contract;
- the complete model-free compatibility suite passes all 3 tests against the
  installed Codex 0.154.0, including policy application before launch, a live
  update, daemon recovery, and a staged CPU-cap reset without killing the
  active runtime;
- public docs checks pass, and the static `/coco` production export verifies
  103 files, all 10 pages, search, routing, and the public-only boundary;
- `nix flake check .` passes after the new module files were added to Git's
  index so the flake source could include them; its first run correctly failed
  because Git-backed flakes exclude untracked source files;
- the rebuilt executable exposes the documented `limits show`, `set`, and
  `reset` surface. An initially stale `target/debug/coco` was traced to the
  local incremental compiler cache; a non-incremental rebuild produced the
  current source behavior and did not require a product change.

No Windows Job Object, macOS aggregate-enforcement, container, shared-pool,
admission-control, or automatic runtime-stop backend was added. Those remain
separate capability-backed extensions behind the portable runtime interface;
the current implementation makes no unsupported cross-platform guarantee.

### Idle-runtime and workspace-usage assessment

The first real idle cgroup observation showed approximately 14 MiB of charged
memory, one process, and no measurable interval CPU for a Codex 0.154.0
workspace executor. This is one local observation rather than a benchmark, but
it does not justify automatic idle shutdown as the next feature. The canonical
runtime decision now keeps auto-stop lower priority until aggregate scale or
host-pressure evidence warrants its lifecycle complexity. `Ready` alone is
not a safe stop condition because pending decisions, TUI relays, and surviving
workspace children must also be ruled out.

The token/cost investigation found two distinct stable-schema surfaces in the
installed release. `thread/tokenUsage/updated` supplies exact thread/turn
identity, cumulative and latest token breakdowns, and model-context size. An
optional `threadId` on `account/usage/read` can return backend-estimated
credits, optional USD, and model/reasoning/speed/token groups. The latter is
route-dependent and eventually consistent; a read-only probe of an existing
workspace returned `threadUsage: null` through the current authentication, so
cost must be optional rather than synthesized as zero.

OpenTelemetry was also audited. Codex exports turn token metrics and related
response fields, but that path requires operator exporter/collector setup and
is designed for external observability. It would duplicate the already owned
App Server stream and is not CoCo's local source of truth. Local tokenization,
rollout/SQLite parsing, goal counters, account-wide totals, internal-only raw
response events, and model-traffic proxying were rejected as primary sources.

The proposed implementation order is native raw usage first, with a compact
per-binding checkpoint and explicit freshness/provenance, followed by a
separate optional native cost query. Fresh, tool-using, compacted, restarted,
forked/imported, TUI-driven, and externally driven thread cases need
real-process attribution tests before CoCo labels a delta as workspace-owned.
The likely CLI is a dedicated `coco usage [workspace]` surface rather than
adding cumulative billing noise to default status, but its scoping and history
semantics remain a user decision. No product code, public docs, model turn, or
remote state changed during this assessment.

Verification on 2026-09-11:

- generated schemas from the installed `codex-cli 0.154.0` were inspected for
  both usage contracts;
- exact released Codex source was checked for resume/fork replay and telemetry
  behavior;
- the current per-thread account route was probed read-only and returned an
  unavailable estimate;
- canonical runtime and new usage knowledge were updated; and
- `git diff --check` passes for the documentation-only change.

## Native workspace usage implementation — 2026-09-11

Implemented the approved dedicated read surface without changing default
status output. `coco usage` lists open workspaces in the current repository,
`-a` selects all repositories, and an explicit workspace supports ordinary
local lookup or `-g`. Both a single workspace and a collection support
terminal-aware `--follow`/`-f`; redirected output appends only changed frames,
and `--json` remains a complete one-shot projection. No targetless picker was
introduced. A later comparison of `list`/`status`/`usage` with a possible
shared `show` namespace is recorded as follow-up rather than folded into this
slice.

The App Server event adapter now consumes focused
`thread/tokenUsage/updated` fields. The coordinator resolves the exact bound
thread and schema-v14 storage keeps one schema-v1 checkpoint containing native
cumulative and latest breakdowns, context-window size, turn/thread identity,
observation time, and daemon generation. A lower cumulative total cannot
replace a higher one, a different thread binding is rejected, and workspace
deletion cascades the checkpoint. After daemon restart the retained value is
exposed as last seen until a notification in the new generation arrives. This
is native thread evidence only; no conversation content, event history,
workspace-attributable delta, or budget was added.

The typed worker boundary also exposes optional per-thread native cost. The
Codex adapter decodes credits, optional USD, and model/reasoning/speed groups
from `account/usage/read`; null, unsupported, and failed reads produce an
explicit unavailable value without hiding token usage. Results are cached in
memory for 15 seconds and invalidated by new token evidence. They are never
persisted, and CoCo does not maintain a price table. Usage reads do not invoke
`thread/read`, `thread/resume`, resource sampling, or workspace-executor
activation.

Verification on 2026-09-11:

- Rustfmt and `git diff --check` pass.
- All-target Clippy passes with warnings denied; `cargo machete` reports no
  unused dependencies.
- The complete non-incremental all-target Rust suite passes: 346 library tests and all 5
  process tests, with 6 deliberate manual/live library probes ignored. The
  process scenario covers notification ingestion, exact JSON and concise human
  output, local/all/global scope, both follow forms, native cost, persistence,
  and passive stale reads after daemon restart.
- The focused lifecycle process smoke test passes again after adding an exact
  assertion that repeated usage projections share one cached native
  `account/usage/read` request for the bound thread.
- All 3 model-free real-Codex compatibility tests pass against the installed
  0.154.0 executable. No model turn was issued; fork/compact/TUI/offline usage
  attribution remains explicitly gated before any future delta claim.
- Public docs typecheck, lint, and format checks pass. The production build
  verifies 103 static files, all 10 pages, search, `/coco` routing, and the
  public-only boundary.
- A stale incremental local `target/debug/coco` initially exposed the previous
  help despite current tests. A non-incremental binary rebuild produced the
  expected `usage`, `limits`, and current lifecycle help; this was a local
  build-artifact issue, not a source change.

This slice is included in the requested local checkpoint commit. No push,
release, model-consuming request, or remote mutation was made.

## Hermetic guard-output test repair — 2026-09-11

The first Rust workflow for the consolidated feature commit failed only in
`hooks::runner::tests::guard_rejects_invalid_or_oversized_output`. The guard
fixtures emitted output and exited without consuming stdin, leaving a race
between the asynchronous request write and process exit. Depending on
scheduling, the same test could observe either the intended invalid-output
error or an earlier broken-pipe write error.

Both invalid and oversized-output fixtures now consume the complete guard
request before emitting their controlled response. This keeps the test focused
on the output boundary it claims to verify without weakening production guard
handling.

Verification:

- `nix run .#fmt` passes.
- The focused guard-output test passes.
- The exact CI test command `nix run .#test -- --all-targets` passes all 346
  ordinary library tests and all 5 process tests; 9 deliberate manual/live
  probes remain ignored.

The next CI run exposed the same scheduling race in two additional static
guard fixtures. The fixture-only repair was therefore incomplete. The durable
fix now lives at the process boundary: after a child exits successfully, CoCo
accepts an early stdin close only when the write error is `BrokenPipe`; guards
must still return valid bounded JSON, and every other input, exit, output, or
timeout failure keeps its existing behavior. The same rule applies to hooks,
which may legitimately ignore an event payload. Large-input regression tests
force this path for both command kinds, while the original ordering and
open-delete tests continue to exercise small static guards.

Verification after moving the fix to the process boundary:

- all 16 guard-filtered tests and the static-hook regression pass;
- the CI-identical Clippy command passes with warnings denied; and
- `nix run .#test -- --all-targets` passes 348 ordinary library tests and all
  5 process tests, with 9 deliberate manual/live probes ignored.
- The remaining Rust workflow gates pass: dependency usage, dependency policy,
  crates.io publish dry-run, and `nix flake check .`. The local package dry-run
  required its standard dirty-tree override because this repair had not yet
  been committed; the packaged contents and upload simulation both completed.

## Outcome-led public documentation — 2026-09-11

Active scope: implement the approved editorial restructuring. Lead README and
overview with parallel agents, navigation, signals/hooks, action guards, and
resource controls. Split everyday guides by reader task and retain the public
configuration contracts needed by integration authors in dedicated reference.
Keep architecture and rationale internal. Preserve existing page routes where
practical, check internal links and the static Pages export, and record results.
No runtime changes, release, commit, or push are part of this task.

Completed the editorial slice. README and overview now lead with parallel
agents and five practical benefits. Quickstart demonstrates two concurrent
workspaces. Preserved all ten existing page routes and added focused context,
cleanup, resource, and guard guides plus signal, automation, and accounting
references. Navigation groups these into getting started, everyday work,
automation, and reference.

Updated the canonical documentation policy and docs agent instructions:
signals/hooks are visible product benefits; integration contracts remain
public reference material, while architecture and rationale stay internal.
Kept guards limited to close/delete and resource enforcement explicitly Linux
and capability dependent. Installation distinguishes current Git builds from
published alphas, so examples do not silently assume unreleased features exist
in an older package.

The hook example now sends an actual review webhook, with a configurable
endpoint. The guard example checks branch reachability in Git. The automation
reference specifies EOF-delimited JSON; the prior shell example could fail on
the missing trailing newline under `set -e`.

Verification: Astro source diagnostics pass with no errors or warnings.
The final `/coco` static build verifies 61 files, all 17 pages, Pagefind,
internal link destinations, and the public-only boundary. An additional check
validated section-fragment links and README file links. All ten JSON examples
parse. Executed the documented Python guard with real temporary Git history:
merged work allows; unmerged, detached, and missing branches deny. No agent
turn or webhook request was issued. Runtime Rust tests are unnecessary for
this documentation-only change.

User approved the result and requested a docs commit, rebase onto the current
origin/main, and push. The verified documentation slice is ready for that
handoff; no runtime changes accompany it.
