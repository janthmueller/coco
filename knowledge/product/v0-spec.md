---
type: Product Specification
title: CoCo v0 technical product specification
description: Defines the first usable Codex Coordinator slice, its command contracts, domain semantics, lifecycle, and acceptance criteria.
tags: [product, v0, cli, mcp, codex, git, orchestration]
status: draft
---

# CoCo v0 technical product specification

## Purpose

CoCo coordinates Codex workspaces and the Git working areas in which they run. Its
first release lets one operator create a separate Codex work unit from a
known Git commit, send work to it, and observe both model activity and
repository changes without mixing orchestration policy into a presentation
client.

Its durable product role is a local, headless control plane shared by CLI,
MCP, and later clients. It is not justified as a second worktree launcher or
thread-history store: native Codex already owns agent execution, while CoCo
packages the cross-system binding, safe composite operations, and human
takeover workflow.

This document is normative where it says **must** or **must not**, subject to
the classification below. It describes intended v0 behavior; it does not claim
that the behavior is already implemented.

### Boundary with task-management systems

Product direction confirmed on 2026-09-09: CoCo owns the working environment
and programmable control/communication surface for agents, not task or ticket
management. A separate product owns tickets, priorities, dependencies,
acceptance criteria, and the business workflow that decides what work to do
and when it is complete.

CoCo remains independently usable without a ticket system. A workspace is an
execution environment, not a ticket; native thread completion or workspace
closure must not imply ticket completion. External references may later help
correlate work, but must not turn CoCo into a second ticket database or make
ticket identifiers mandatory for normal workspace operations.

The task system may use CoCo directly, or a separate integration may compose
the two. That placement remains open. The integrating component owns mappings
between tickets and workspaces, decisions to start follow-up work, and the
translation of agent reports into ticket workflow changes. CoCo owns the
safety and authorization of requested workspace/runtime operations, not the
ticket-specific reason for issuing them.

Agent-emitted signals and later agent-to-agent communication fit this boundary
as domain-neutral capabilities. Ticket-specific signal definitions and
reactions belong to an integration's configuration or implementation, not
hard-coded CoCo lifecycle rules. The first signal path is implemented with
operator-selected JSON Schema 2020-12 files and immutable versions, opt-in
publication, native sender mapping, validation, retention, and independent
reads; see the [signal contract](../engineering/signals.md). The first CoCo
hook path adds trusted local command reactions to accepted signals and
committed workspace transitions without interpreting their business meaning.
Separate synchronous guards may allow or deny a fully checked workspace close
or deletion before its first effect; see the
[hook contract](../engineering/hooks.md). External reference fields and
peer-response routing remain separate, unimplemented capabilities.

## Evidence and decision classification

### Observed facts

- At the specification baseline on 2026-09-05, the repository had no commit
  and no implementation files; it contained the handoff and documentation
  foundation only.
- The locally installed `codex-cli 0.154.0` is the selected compatibility
  baseline. Its model-free real-process test passes preparation, exact native
  materialization/adoption, inherited-context fork, history reads, daemon
  restart, and exact resume.
- That baseline also passes the experimental environment contract CoCo now
  consumes: lazy per-workspace exec-server registration, fresh-thread and
  ordinary-turn selection, distinct process roots for concurrent workspaces,
  rootless Linux cgroup-v2 scope placement with process-tree fallback,
  on-demand resource observation, close-time cleanup, and daemon-shutdown
  cleanup. The resource boundary now also accepts revisioned per-workspace
  memory, CPU, and task policies, applies supported changes dynamically, and
  verifies the resulting kernel cgroup values. The same evidence shows that
  resume/fork do not accept environment selection and that host-local
  `thread/shellCommand` is not routed to a remote-only workspace executor.
- Schemas and real behavior from selected releases expose the `initialize`,
  `model/list`, `thread/start`, `thread/resume`, and `turn/start` client requests;
  thread/turn, plan, diff, item, token, error, and status notifications; and
  server-initiated approval and user-input requests.
- That observed protocol is version-specific. Narrow consumed fields and the
  real behavior test are authoritative; generated schemas remain an upgrade
  review aid and additive schema drift alone is not a compatibility failure.
- Schema v14 retains schema v6's minimal turn-start operation ledger, schema
  v7's typed worktree binding, and schema v8/v9's recoverable workspace
  retirement state and deletion intent. Schema v10 adds bounded signals and
  schema v11 adds the narrow durable hook outbox. Schema v12 adds open-delete
  origin and explicit commit-discard intent. Schema v13 adds revisioned
  workspace resource policies, and schema v14 adds one compact native
  token-usage checkpoint per workspace. It also retains the old
  native-status columns, local turns, normalized events, completed messages,
  MCP audit events, and decisions as a reversible compatibility bridge. The
  retained shapes are not evidence of long-term CoCo ownership.
- Passive `list`/`ls`, `status`, and follow polling validate the stored binding
  through stable, non-loading `thread/read` and derive current phase from the
  native response. Status output is state-only and never reads or stores Codex
  conversation text. `send --wait` instead correlates the exact accepted
  operation with completion notifications and retains at most 256 bounded
  final responses in the current daemon generation's memory. CoCo no longer
  stores sent prompt bodies, completed agent messages, status/plan/diff/error
  notifications, unsupported server requests, live decisions, local turn
  rows, `active_turn_id`, start/resume status snapshots, or turn
  start/completion events. It retains only operation dispatch/idempotency
  facts, provisioning evidence, and MCP audit records until their separate
  removal gates pass.
- Codex 0.153.4 first established, and the 0.154.0 gate confirms, that a fresh
  persistent thread is not resumable merely because
  `thread/start` returned an ID. CoCo therefore prepares only Git state, binds
  the first fresh `send` together with its accepted native turn, and adopts a
  fresh-TUI thread only after exact `thread/read` exposes a non-empty rollout.
  The real-process test proves that an empty candidate remains unbound and the
  materialized exact ID survives fresh daemon and App Server processes.
- The model-free 0.154.0 compatibility path also passes native context-fork
  cleanup, reversible worktree close with native thread archive, exact
  worktree/thread reopen, and explicit native-thread plus CoCo-created-branch
  deletion. Exact `thread/read` is used for archived-state inspection because
  default `thread/list` source filtering omits App-Server-origin threads.
- Codex 0.154.0 also exposes its own command/MCP lifecycle hooks. A direct
  App Server proof confirms that a configured `SessionStart` hook runs at the
  first ordinary turn, not merely at `thread/start`. CoCo leaves prompt, tool,
  permission, compaction, subagent, stop, interrupt, and session policy with
  that native facility; its own hooks cover only durable CoCo-owned facts.
- The 2026-09-07 review of the official
  [App Server](https://developers.openai.com/codex/app-server) and
  [Codex SDK](https://developers.openai.com/codex/sdk) documentation found no
  reusable repository/worktree/thread aggregate or worktree-lifecycle API.
  The official
  [worktree guide](https://learn.chatgpt.com/docs/environments/git-worktrees)
  documents such a product layer in the ChatGPT desktop client, including
  managed worktrees and chat handoff, but not as a client API CoCo can call.
- An explicit model-consuming test against `codex-cli 0.147.0` confirms that
  an `untrusted`, workspace-write turn in a native linked worktree produces
  `item/commandExecution/requestApproval` for the Git administrative write.
  Accepting the exact request advances only the linked workspace branch and is
  followed by `serverRequest/resolved`, completed command, and completed-turn
  events. Under `on-request`, asking for escalation remains model-discretionary
  after a sandbox failure, so that policy is not a deterministic test trigger.

### Confirmed product and architecture decisions

- The product is named **CoCo** (Codex Coordinator), with commands `coco` and
  `cocod`.
- It is implemented in Rust with Tokio.
- `cocod` owns one Codex App Server child and connects through an authenticated
  IPv4-loopback WebSocket. This same control plane lets `coco jump` attach the
  official Codex TUI without creating a second App Server process. One lighter
  `codex exec-server` is started lazily per activated workspace and selected on
  fresh-thread plus ordinary-turn requests; it is not a second App Server.
- Starting `cocod` does not eagerly resume every persisted workspace thread.
  Passive reads remain non-loading; `send`, native-fork source preparation, and
  `jump` activate and subscribe only the selected thread after validating its
  exact ID, worktree, profile provenance, and explicit model override.
- Alpha packages install the executables but do not enable a background
  service. The operator starts `cocod` explicitly in the foreground. A later
  user service must remain opt-in and cross-platform, and is gated on graceful
  SIGTERM handling plus defined App Server child-failure and recovery behavior.
- Git isolation uses native Git worktrees and persistence uses SQLite.
- Git is the product authority for files, refs, commits, and current worktree
  condition. The Codex App Server is the product authority for threads, turns,
  conversation contents, native runtime status, models, effective Codex
  configuration, and server-request semantics.
- CoCo durably owns only the repository/worktree/thread binding, provisioning
  and retained failure evidence, idempotent composite-operation correlation,
  and CoCo-specific context or policy provenance that cannot be read from Git
  or Codex. Native facts may be cached for presentation but must not form a
  second authoritative history.
- Multi-repository lookup and worktree creation are useful parts of the
  workflow but are not sufficient product differentiation by themselves.
  CoCo remains relevant only as a programmable, multi-client orchestration
  layer with reliable human takeover of the exact thread and worktree.
- The CLI and a local CoCo MCP server are v0 client adapters. Later TUI and
  local web clients are equal peers and must use the same headless
  orchestrator behavior rather than reimplementing it.
- CoCo itself must be usable as an MCP server. Its initial external transport
  is local `stdio`; the MCP adapter must not contain orchestration policy.
- A workspace, Codex thread, worktree, branch, base commit, context provenance,
  and profile are distinct concepts even when one v0 operation creates them
  together.
- Normal CLI creation warns and continues from the selected committed base
  when its source is dirty; the wire contract retains explicit reject and
  ignore policies. An explicit local-state
  selection may copy tracked changes and ordinary non-ignored untracked files
  into the destination; CoCo never silently stashes, resets, stages, or mutates
  the source checkout.
- Worktrees live outside the registered repository by default.
- CoCo never performs destructive branch or worktree cleanup automatically.
  Explicit `close` and `delete` operations expose and validate
  their effects before applying them.
- One daemon may register multiple repositories. Workspaces remain
  repository-owned, workspace names are unique only within that repository,
  and opaque workspace IDs are globally unique.
- Repository-aware CLI commands use an optional leading repository path that
  defaults to `.`. `--all-repos`/`-a` expands workspace collection to every
  repository, while `--global`/`-g` resolves or interactively selects one
  workspace across repositories. CoCo keeps no hidden, persistent "selected
  repository" for CLI sessions.
- Workspace names may use conventional slash-separated components such as
  `feat/login`. A new `coco/<workspace-name>` branch is the default Git
  binding, but creation may instead select another new branch, an existing
  local branch, or detached HEAD.
- The stable user-facing name for CoCo's durable aggregate is **workspace**,
  not task or session. Earlier prerelease `task` records are migrated
  losslessly; the CLI, daemon protocol, MCP surface, and current storage model
  use `workspace` consistently.
- `coco create` prepares the Git workspace without allocating an empty native
  thread. Its phase is `prepared`. `--send <message>` materializes context and
  starts its first turn; `--jump` either resumes the bound thread or lets the
  official TUI create the first fresh thread through a one-use correlated
  relay. The two options compose in the fixed order create, send, jump. Failure
  of a later post-action does not roll back a successfully created workspace or
  accepted turn.
- `coco create` selects code, conversation context, Git binding, and optional
  local changes independently. `--base-workspace` selects committed code;
  `--context`/`-c` selects native Codex history by workspace reference or
  exact thread ID. A context source must be idle or unloaded. Optional
  `--compact-context`/`-C` applies only to the child and completes before
  `--send` or `--jump` runs.
- `coco model list`/`coco model ls` exposes the visible catalog reported by the daemon-owned Codex
  App Server. `coco create --model/-m` is an explicit per-workspace model
  override. CoCo passes the named profile overlay in `config` and the explicit
  model in the App Server's separate `model` field; it does not reimplement
  Codex configuration precedence.
- `coco close` is the reversible worktree-retirement boundary. It retains the
  workspace record, branch, and thread by default; optional native archive is
  undone by `coco reopen`. Permanent `coco delete` accepts an open or closed
  workspace and deletes its owned worktree, executor, record, thread, and
  CoCo-created branch. `--keep-thread` and `--keep-branch` opt into retention;
  an adopted branch is always retained. No lifecycle action has a
  destructive collection scope, and `--yes` never implies a discard policy.
- Native worktrees intentionally share their repository's Git object and ref
  storage. CoCo does not proxy ordinary worker commits or allocate a separate
  Git database per workspace.
- Supported Codex approvals and structured user-input requests are projected
  as generation-bound decisions and answered only through an explicit
  `coco decide` operation; CoCo never auto-approves them. Their bounded prompt,
  private native correlation, choices, and state live only in the daemon
  generation that owns the App Server request. They are not written to the
  schema-v11 retained legacy decision table, and raw answers are never retained.
- v0 has no publicly reachable network listener. Its App Server endpoint is
  capability-token protected and bound only to `127.0.0.1`.
- Status JSON and human `--resources` may expose a generation-local observation
  of each selected workspace executor. A compatible Linux cgroup-v2 user
  manager reports memory charged to the complete execution group, process and
  task counts, cumulative and interval CPU use, and controller event counters.
  The Linux compatibility backend reports descendant count, aggregate RSS,
  and interval CPU instead. Samples exclude shared App Server cost, are never
  persisted, and are distinct from configured resource policy. Desired limits
  and their revision are durable; the applied snapshot is live runtime
  evidence.
- `coco usage` exposes the latest cumulative native Codex token report for one
  workspace or an open-workspace collection without loading conversations or
  starting executors. The complete native breakdown and provenance are
  available in one-shot JSON; human output keeps cumulative tokens, latest
  context occupancy, and optional backend-estimated cost distinct. One compact
  checkpoint is durable, becomes stale across daemon generations, and never
  becomes a CoCo billing authority. Native cost is optional, cached briefly in
  memory, and unavailable is distinct from zero.
- Worktrees may use a new branch, an existing local branch, or detached HEAD.
  Detached worktrees remain registered Git worktrees. CoCo can close one only
  while its `HEAD` still equals its base; detached-to-branch promotion remains
  unavailable.

### Draft assumptions

These choices make the contract implementable but were not explicitly fixed
by the handoff. They are reversible and require confirmation before a stable
release:

- The current executable supports a single local operator on Linux and macOS.
  Its daemon protocol uses a Unix domain socket; native Windows support will
  use the same protocol and coordinator behind a named-pipe transport.
- `fresh` and same-repository `fork` are executable context modes.
  `handoff` remains reserved and returns a clear unsupported-mode error until
  its transfer-artifact contract is deliberately designed.
- v0 uses the App Server's base Codex configuration by default, represented by
  an empty per-thread overlay. A non-default `--profile <name>` loads the
  complete `$CODEX_HOME/<name>.config.toml` document as that thread's overlay.
  Schema v11 retains its name, source path, parsed-configuration hash, explicit
  model override, and non-secret effective settings, never the complete
  overlay. The native-first target keeps only the request provenance proven
  necessary to verify resume. Profile CRUD is not part of v0. Future MCP
  capability profiles are a separate dimension rather than an unstructured
  extension of this execution profile.
- CLI and MCP adapter connect to one user-scoped `cocod`; the daemon owns one
  App Server child process at a time in v0.
- The MCP adapter is repository-scoped at launch and read-only by default. An
  explicit startup capability may add the narrowly scoped `workspaces.send` tool.

### Recommendations adopted by this draft

- Represent CoCo provisioning/binding lifecycle, native Codex thread status,
  operation correlation, and Git condition separately. `phase` and
  `waitReasons` are a read-time summary; `dirty` and `ahead_of_base` remain
  independently calculated Git facets. A single stored enum would permit
  contradictory or lossy state.
- Retain only profile/context provenance required to verify recovery and explain
  an operation. Codex resolves effective configuration; CoCo must not persist
  a broader effective snapshot merely to create its own audit history.
- Make every mutating client request carry a client-generated operation ID.
  This lets the daemon reject or replay duplicate requests from CLI or MCP
  without creating a second branch, worktree, thread, or turn. For turn start,
  persist intent before dispatch, mark the dispatch boundary before writing to
  App Server, and never automatically retry an unconfirmed operation.
- Add the minimal `coco decide <decision-id>` response command before calling v0
  generally usable; merely displaying an approval can otherwise leave a turn
  permanently blocked.

## v0 outcome

An operator can register a Git checkout, prepare a workspace from an exact
commit without creating native conversation state, send the first or a later turn,
inspect or follow its current state, enter the same thread with the official
Codex TUI, answer a supported approval or question, and inspect all workspace
changes relative to the fixed base commit. It can then close and reopen that
binding without changing its identity, or explicitly retire a closed record
while selecting branch and native-thread retention independently.
A local MCP host can inspect the same workspace projections and, when the
operator explicitly enables the capability, send a turn through the same
daemon use case.

The smallest proof slice is successful when it demonstrates, end to end:

1. `cocod` starts and initializes a Codex App Server child.
2. CoCo creates a native worktree with the selected new-branch,
   existing-branch, or detached binding at a fully resolved base SHA.
3. Creation returns phase `prepared` with no Codex thread ID and no model turn.
4. A first `send` creates a non-ephemeral native thread with the worktree as
   `cwd` and immediately dispatches the real text turn.
5. One SQLite transaction binds that exact fresh thread and accepts the
   operation only after the native turn response proves both IDs.
6. Alternatively, a first fresh `jump` adopts only the exact TUI-created
   thread after its first action has produced a durable native rollout; an
   empty exit remains prepared.
7. Native status and terminal output reach a CLI client, and a later read
   reconstructs current state from the durable CoCo binding plus Git and the
   App Server rather than a CoCo-owned conversation history.
8. A supported native server request is registered before presentation and an
   explicit numbered response reaches that exact live request exactly once.

The MCP adapter is required for the complete v0 but does not need to be in this
first proof slice; it is added once the daemon methods it delegates to are
stable.

## Worktree binding direction

Creation represents Git binding independently from conversation context.
`new_branch` creates `coco/<name>` by default or another explicitly named
branch; `existing_branch` checks out an existing local branch only when Git
reports it is not checked out elsewhere; `detached` creates a regular
registered worktree at the resolved base SHA without allocating a branch.

Detached work remains available across daemon restarts because Git, not CoCo's
process, owns the worktree registration. CoCo never cleans it automatically;
explicit close requires its commits to remain reachable from another local
branch, tag, or remote-tracking ref. Delete can discard unretained detached
commits only with separate explicit approval.
A later promotion design must atomically
create and verify a branch before changing persisted binding metadata; until
then, branch-backed workspaces are the durable default.

## Domain vocabulary and invariants

### Repository

A registered, canonical Git worktree from which CoCo resolves refs and creates
workspace worktrees. Registration does not mean CoCo owns or may delete the
repository.

### Workspace

A stable CoCo work unit identified independently of the Codex thread. A
workspace owns exactly one repository binding, worktree mode, optional branch,
base SHA, worktree path,
context descriptor, minimal execution-request provenance, and Codex thread
binding. Schema v11 currently exposes a broader redacted profile snapshot. In v0
a workspace acquires at most one Codex thread. Its first instruction belongs to
a turn, not to workspace creation.

### Thread and turn

The Codex thread owns model conversation history. A turn is one execution in
that thread. The App Server remains authoritative for contents, history,
native lifecycle, and status. CoCo stores only operation correlation needed to
prevent duplicate external actions and to bind the native IDs to its workspace;
legacy turn and event rows are a migration source, not product authority.

### Code and context provenance

- `base_sha` is a full commit object ID and defines the immutable code origin.
- The worktree mode, optional branch, and worktree define the workspace's
  current code state.
- The tagged context request and its resolved descriptor define how model
  context was obtained.
- The local-state manifest records whether explicitly selected tracked,
  ordinary untracked, or ignored setup files were copied at creation without
  becoming the authority for their later contents.

These dimensions must not be inferred from one another. Forking context does
not imply copying uncommitted files, and sharing a base SHA does not imply
sharing conversation history.

The original product handoff already reserves three context-transfer modes;
they are one deferred feature, not a second metadata mechanism:

- `fresh` creates a new thread without inherited conversation history when the
  first `send` or fresh `jump` activates the prepared workspace. The first turn
  supplies the work instruction; provenance still records the selected code
  base and any explicitly supplied source material.
- `fork` uses Codex's native `thread/fork` on first activation from either a selected workspace
  thread or an exact native `thread.id`, preserving its history while binding
  the new thread to the newly prepared worktree and effective configuration.
  The transition must explicitly tell the agent that `cwd`, branch, and base
  commit may differ. CoCo requests a metadata-only fork response; this omits
  duplicated turn data from the transport response without removing any
  inherited conversation context from the child.
- `handoff` starts a fresh thread from bounded, reviewable transfer material
  rather than copying the full conversation. The material may be authored by
  an agent, supplied as an existing Markdown document or CLI input, or refer
  to an already-associated external record such as a ticket. Generated
  material may include a plan, but generation is deliberately separate from
  attaching and consuming the handoff. Its exact artifact and reference model
  remains open.

The first context-transfer delivery implements `fork` only. Conversation
context is independent from code selection: `--context`/`-c` first resolves a
workspace in the destination repository and otherwise treats the reference as
an exact readable native Codex `thread.id`, which may originate elsewhere.
`workspace:` and `thread:` prefixes force the rare ambiguous case. The source
thread must be idle or unloaded. CoCo always creates a child; it never adopts
or moves the source thread. An explicit compact modifier runs
`thread/compact/start` on the new child, never on the source, and must finish
before an initial message can be sent. Compaction is recorded as fork
provenance rather than a fourth context mode and is never enabled
automatically.

`handoff` remains unimplemented until its relationship to plans, existing
documents, direct operator input, and external references is designed. A
successful native-fork implementation is not evidence that those artifact
semantics have been settled.

`thread/resume` is not a context mode: it reconnects the same thread. Before
`handoff` ships, CoCo still needs redaction and size limits, immutable
provenance for its artifacts, authoring and review UX, artifact/reference
semantics, and failure/retry behavior. Git remains the authority for code;
context transfer never implies copying uncommitted files. Local-state transfer
is an explicit independent request.

### Required invariants

- A workspace name is unique within the repository. A newly allocated branch
  must also be collision-free; an explicitly selected existing branch must not
  be checked out by another worktree.
- A workspace name is 1-63 bytes split into non-empty `/`-separated components.
  Every component uses lowercase ASCII letters, digits, or `-`, starts and
  ends alphanumerically, and is validated before it participates in a ref or
  filesystem path. Leading, trailing, or repeated `/` and dot components are
  invalid.
- A managed worktree path and Codex thread ID belong to at most one workspace.
- `base_sha`, repository, worktree mode, optional branch, worktree path, and
  context provenance do not change after the workspace passes provisioning. A
  replacement creates a new workspace.
- Every turn belongs to exactly one workspace and uses that workspace's thread and
  worktree.
- A workspace has at most one in-progress turn in v0.
- CoCo never guesses that a workspace is complete from a successful turn. Turn
  completion clears the active-turn correlation; a fresh native Codex
  `idle` observation makes the derived phase ready for another send. Explicit
  workspace completion is deferred until a lifecycle command is specified.
- External Git state is observed, never overwritten to make persisted state
  appear correct.

## Supported commands

```text
coco repo add [path]
coco repo (list | ls) [--json]
coco model (list | ls) [--json]
coco [<repository-path>] create [<name>]
  [--base <ref> | --base-workspace <workspace>]
  [-c|--context <workspace-or-thread>] [-C|--compact-context]
  [--branch <branch> | --checkout <branch> | --detached]
  [--carry-changes [--carry-untracked] | --dirty]
  [--profile <name>] [--model <model>] [--send <message>] [--jump]
coco [<repository-path>] (list | ls) [--json]
coco (list | ls) --all-repos [--json]       # `-a` is the short form
coco [<repository-path>] status [--resources] [--follow] [--json]
coco status --all-repos [--resources] [--follow] [--json] # `-a`, `-r`, and `-f` are short forms
coco [<repository-path>] status <workspace> [--resources] [--follow] [--json]
coco status <workspace> --global [--resources] [--follow] [--json] # `-g` is the short form
coco [<repository-path>] usage [<workspace>] [--follow] [--json]
coco usage --all-repos [--follow] [--json] # `-a` and `-f` are short forms
coco usage <workspace> --global [--follow] [--json] # `-g` is the short form
coco [<repository-path>] limits show [<workspace>] [--json]
coco limits show [<workspace>] --global [--json]
coco [<repository-path>] limits set [<workspace>]
  [--memory-high <size>] [--memory-max <size>] [--cpu-max <cores>]
  [--cpu-weight <1..10000>] [--tasks-max <count>] [--clear <field>]... [--json]
coco limits set [<workspace>] --global <changes> [--json]
coco [<repository-path>] limits reset [<workspace>] [--json]
coco limits reset [<workspace>] --global [--json]
coco [<repository-path>] send [<workspace>] [<message>] [--wait]
coco send [<workspace>] [<message>] --global [--wait]
coco [<repository-path>] jump [<workspace>]
coco jump [<workspace>] --global
coco decide <decision-id> [--choice <number>]
coco [<repository-path>] diff [<workspace>]
coco diff [<workspace>] --global
coco [<repository-path>] signal (list | ls) [<workspace>] [--follow] [--json]
coco signal (list | ls) --all-repos [--follow] [--json]
coco signal (list | ls) <workspace> --global [--follow] [--json]
coco hook (list | ls) [--json]
coco hook validate [--json]
coco hook reload [--json]
coco hook (history | deliveries) [--limit <number>] [--json]
coco mcp serve --repository <path> [--allow-send] [--signal-catalog <directory>] [--allow-emit <signal>]...
```

For repository-aware commands the omitted leading path is exactly equivalent
to `.`. An explicit path may point anywhere inside a registered repository;
the daemon resolves its canonical Git identity. `--all-repos` and `--global`
are each mutually exclusive with that path and with one another. Both are
invalid for `create`, which necessarily creates inside one repository.

`<workspace>` accepts a full workspace ID everywhere and resolves that ID
independently of the current directory. A workspace name resolves only within
the selected/current repository unless `--global` (short: `-g`) is present.
Global name resolution succeeds only for exactly one match. Multiple matches
return `WORKSPACE_REFERENCE_AMBIGUOUS` with matching workspace IDs, names, and
repository paths. A local miss never silently targets another repository, but
the error points out global matches when they exist. `--all-repos` is a
read-only collection scope for `list`, targetless `status`, targetless `usage`,
and targetless `signal list`; it never broadcasts a mutation. CoCo does not
encode a path and workspace name into a composite string.

`signal list`/`signal ls` accept an optional positional workspace, not a
separate `--workspace` flag. `--name` filters the signal type independently.
Their history view is not a current-state projection and has no `signal status`
equivalent. Retained history remains readable by UUID after workspace deletion.

`model list`/`model ls` and `decide` are intentionally not repository-scoped.
The model collection reads the daemon-owned App Server catalog and accepts
only its optional `--json` output flag. `decide` accepts the globally unique
opaque CoCo decision ID printed by `status` and an optional deterministic
approval-choice number; a leading repository path or either scope flag is an
error.

All `hook` commands reject a leading repository path or repository-scope flag.
`hook validate` checks the complete configured file offline without executing
commands. `hook reload` asks the running daemon to validate a new complete
snapshot and swap it atomically; an invalid replacement leaves the current
snapshot active. `hook list`/`hook ls` show the active reactions and guards
without exposing command arguments. `hook history`/`hook deliveries` return
the newest 1–100 post-event delivery summaries without event payloads; they do
not include synchronous guard checks and are not a manual replay API.

Human terminal commands share one interactive input contract. `create` may
prompt for a missing name and, when implicit `.` cannot select a registered
repository, offer the registered repositories. `send`, `jump`, and `diff` may
select an omitted workspace from the local scope or the daemon-wide `--global`
scope; `send` then prompts for an omitted message. Targetless `status` is
always a non-interactive collection view. The picker uses
arrow keys or `j`/`k` for immediate cursor movement, Enter to confirm, direct
one-key selection for options 1 through 9, and Escape, `q`, or Ctrl-C to cancel.
It is also the single option-selection implementation for `decide`. The
picker keeps only its title and choices on screen: successful selection and a
single available choice do not add redundant confirmation lines.
Only the selected row starts with `›` and uses cyan/bold styling; inactive rows
reserve the same marker column as blank space so labels remain aligned. The
marker alone identifies selection with color disabled, without changing labels.
The terminal's hardware cursor stays hidden during selection. The visible window
fits the terminal height, reserves space for its title, and scrolls correctly
at the bottom margin. Page Up/Down use that visible window's size.

CoCo's own binary confirmations are line input, not choice menus: display
`[y/N]`, accept case-insensitive `y`/`yes` or `n`/`no`, and require Enter.
Empty input chooses No; invalid input asks again and EOF never authorizes an
operation. `--yes` remains the explicit non-interactive bypass where supported.
Native `decide` options retain their original multi-choice semantics.

Human output is command-specific and concise. It uses the terminal's default
foreground for primary information, dim text for secondary paths and metadata,
cyan for selection and live status, green for success, red for failures, and
magenta only when identifying Codex. Styling is enabled only for the relevant
terminal stream, honors `NO_COLOR`, and is never added to JSON, redirected
output, raw `diff` patches, or exact responses printed by `send --wait`.
Ordinary success and collection output omit opaque repository, workspace,
thread, turn, and operation IDs. An ID remains visible when the next action
requires it, such as a pending `coco decide` command or an interrupted wait.

Collection commands never prompt. Neither do commands with non-terminal input
or diagnostic output or explicit `--no-input`. Those invocations fail before
ambiguity can turn into an implicit choice; fully explicit command forms
preserve their deterministic behavior.

All orchestration commands must use the daemon contract. The CLI must not open
SQLite or operate worktrees. `jump` first resolves the workspace through the daemon,
then launches the official Codex TUI against the daemon-owned App Server; it
does not duplicate thread or turn orchestration.

### `coco repo add [path]`

- Default `path` is the current directory.
- Resolve symlinks and use Git's top-level path as the canonical repository
  path.
- Require a local Git worktree and an accessible common Git directory.
- Register the repository idempotently and return its stable ID and root.
- Do not create a Git commit, branch, config entry, or worktree.
- Do not require a clean checkout merely to register it; `create` evaluates
  local state when deciding whether to warn, omit, or explicitly carry it.

### `coco repo list` / `coco repo ls`

- List every repository registered with the user-scoped daemon, including its
  stable ID, display name, and canonical root path.
- This is a daemon-wide inventory and therefore needs no repository scope.
- `--json` returns one versioned document with stable repository identities.

### `coco model list` / `coco model ls`

- Page through `model/list` on the daemon-owned App Server and return every
  visible catalog entry in its reported order.
- Keep the App Server's exact `model` selector, display name, description,
  default marker, supported reasoning efforts, input modalities, and
  personality support. Ignore added upstream fields safely.
- This is a daemon-wide capability query, not repository data. A leading
  repository path, `--all-repos`, and `--global` are invalid.
- `--json` returns one versioned document under the `models` key.

### `coco create`

Creation has four independent inputs:

The name remains mandatory at the coordinator boundary. The CLI may collect a
missing name from a human terminal before it constructs that request. When the
caller omitted the leading path and `.` is not a registered repository, the
same interactive layer may choose from `repository.list`; an explicitly
supplied path always fails directly instead of silently falling back.

1. **Code base.** `--base <revision>` resolves a commit and defaults to the
   invoking checkout's `HEAD`. `--base-workspace <workspace>` instead uses
   that same-repository workspace's committed `HEAD`.
2. **Conversation context.** No context option starts a fresh thread.
   `--context`/`-c` resolves a same-repository workspace first and otherwise
   calls native `thread/fork` from that exact thread ID. Prefixes may force
   either interpretation. A direct native reference means exact `thread.id`,
   not the root `thread.sessionId`. `--compact-context`/`-C` is valid only
   with a source and compacts only the child; `-Cc <reference>` combines both
   short options in value-safe order.
3. **Git binding.** The default allocates `coco/<workspace>`.
   `--branch <branch>` allocates another new branch, `--checkout <branch>`
   uses an existing local branch that Git reports as free, and `--detached`/
   `-D` allocates no branch. Existing-branch selection supplies its own base
   and therefore rejects a separate base option.
4. **Local state.** The default creates from the selected committed base even
   when the invoking checkout is dirty. The CLI warns and leaves its tracked
   and ordinary untracked changes there. `--carry-changes` instead preserves
   staged and unstaged tracked changes with separate binary patches.
   `--carry-untracked` also copies ordinary non-ignored untracked files and
   requires tracked carry. `--dirty`/`-d` is the CLI shorthand for both. It is
   independent from detached mode, so `-dD` combines dirty-state carry with
   detached HEAD.

Every creation also honors Codex's repository-root `.worktreeinclude`
convention for selected ignored local files. Only paths that Git confirms are
ignored are eligible; an ignored root `AGENTS.override.md` is included
automatically. The copy skips symlinks, refuses overwrites, and applies bounded
path, file-count, per-file, and aggregate-byte limits. Other ignored files are
never copied automatically. Snapshot contents remain in owner-process memory
only; persisted provenance contains bounded counts, sizes, source/base identity,
and a hash. The source checkout is never stashed, reset, staged, or otherwise
mutated. Carrying local state requires the selected base SHA to equal that
checkout's `HEAD`.

Optional `--profile <name>` loads the complete
`$CODEX_HOME/<name>.config.toml` document and applies it only to the new
thread; omitting it sends an empty overlay and keeps the App Server's base
configuration. Optional `--model <model>`/`-m` passes an explicit top-level
model override alongside that profile overlay. Codex remains authoritative
for resolving effective configuration; CoCo does not duplicate its precedence
rules.

By default creation accepts no instruction or open-ended metadata field.
`--send`/`-s` starts the first turn after preparation; `--jump`/`-j` then
opens the same thread in the Codex TUI. Creation executes as a recoverable
saga:

1. Resolve the registered destination repository and serialize creation under
   its repository lock.
2. Resolve the selected Git binding and immutable `base_sha`. Resolve a base
   workspace only within that repository.
3. Validate and snapshot the explicitly selected source-checkout state in
   memory; resolve context separately with a non-loading native read.
4. Validate workspace/path/ref invariants and reject name, branch-namespace,
   existing-checkout, or destination collisions before creating artifacts.
5. Persist a `provisioning` workspace with the typed worktree mode, optional
   branch, and versioned creation provenance.
6. Create the worktree at the exact base SHA, verify its binding, then apply
   the in-memory staged, unstaged, untracked, and ignored-file selections.
7. Persist the verified worktree binding as lifecycle `ready` with no native
   thread and return the derived phase `prepared`.
8. On the first `send`, materialize fresh or forked context in the destination
   worktree. For fresh context, bind the thread only in the transaction that
   records the direct first-turn acceptance response. For inherited context,
   retain exact parent/source provenance independently from Git-base
   provenance and compact only the child when requested.
9. On a first fresh `jump`, let the official TUI start its native thread and
   adopt only its exact durable candidate; an empty TUI exit leaves the
   workspace prepared.
10. `--send` and `--jump` run after successful preparation in that order;
    failure of a later action does not roll back an earlier successful action.

If deferred thread start or fork fails before binding, CoCo leaves the ready
Git workspace prepared so the operator can retry. A post-binding compaction
failure marks the workspace failed while preserving the child and worktree.
CoCo never hides failure by destructively cleaning up. Retrying an operation
ID must not create duplicate artifacts.

### `coco list` / `coco ls`

- The human view lists workspace name, runtime phase, and branch. The
  all-repository view additionally includes the repository path. Opaque IDs
  and the complete workspace projection remain available in JSON.
- Default to workspaces in the selected/current registered repository.
  `--all-repos` exposes the daemon-wide view and always includes repository
  identity in each human and JSON row.
- Normal lists omit closed workspaces. `--closed` selects closed workspaces
  instead and composes with repository or all-repository scope.
- Sort deterministically by most recent update, then workspace ID.
- For each bound ready workspace, read current Codex status with non-loading
  `thread/read`; do not resume a thread merely to list it. An unbound ready
  workspace projects `prepared`; a missing or invalid existing binding projects
  unavailable instead of falling back to stored status.
- `--json` emits one schema-version-10 JSON document and no decorative stdout
  text. Every row includes a compact repository identity.

### `coco status`

- With no workspace reference, return the same compact workspace collection as
  `list` in the selected/current repository. `--all-repos`/`-a` selects every
  registered repository. This form never opens a picker and supports `--json`.
- With an explicit reference, return exactly one workspace. The human view
  shows its current state, branch/worktree location, reported error, and any
  actionable decision. JSON returns the complete projection below. Resolve a
  name only in the selected/current repository by default or across all
  repositories with `--global`/`-g`. `--global` without a reference and
  `--all-repos` with a reference are invalid.
- The detailed projection includes immutable CoCo/Git binding, context
  mode, non-secret profile summary, Codex thread and active/latest turn IDs,
  runtime phase and wait reasons, Git facets, timestamps, last error, and a
  recent compatibility-event cursor. It also includes current `pending` or
  `submitted` decisions using opaque CoCo IDs and bounded presentation data;
  native App Server request IDs are never exposed. When per-workspace execution
  is enabled, it additionally includes the executor backend/state/scope, root
  PID, and only the resource measurements actually available on that host.
  A cgroup-v2 observation keeps charged memory distinct from fallback resident
  memory and may include its opaque unit, process/task counts, cumulative CPU
  use, and controller events. Collection status JSON attaches the same resource
  observation to each row. These samples are current, optional, and
  non-persistent.
- `--json` uses the same field meanings as the relevant daemon projection and
  includes a top-level schema version.
- One-shot status and every `--follow` poll perform non-loading native reads;
  stored status is never served as current when a read fails. An explicitly
  targeted follow and a targetless collection follow both watch until Ctrl-C.
  On a terminal they replace the prior live frame in place; with redirected or
  piped stdout they append only initial state and later changes without terminal
  control sequences. Neither form reads, persists, or prints conversation
  messages. Ctrl-C detaches only the display and does not cancel a turn.
- Human status omits executor implementation details and resource sampling by
  default. `--resources`/`-r` adds only memory, process count, and CPU when
  available; a collection uses dedicated `MEMORY`, `PROCS`, and `CPU` columns.
  Detailed human output labels fallback resident memory as RSS and cgroup
  memory as memory rather than conflating them. `--follow`/`-f` composes with
  it, including clustered `-fr`/`-afr`. JSON status always requests the
  complete optional observation without requiring `--resources`.
- `--follow` and `--json` are intentionally mutually exclusive in the current
  CLI; machine clients can poll `status --json`.

### `coco usage`

- With no workspace reference, return open workspaces in the selected/current
  repository. `--all-repos`/`-a` selects every registered repository and adds
  repository identity to each human row. This form is a collection, never a
  picker.
- With an explicit reference, return exactly one workspace, including a closed
  one when directly addressable. Use normal repository-local resolution,
  `--global`/`-g`, or a full workspace ID.
- Token evidence comes only from `thread/tokenUsage/updated` for the exact
  bound native thread. Retain the supplied cumulative and latest breakdowns,
  model context window, thread and turn IDs, observation time, source, and
  daemon generation. Never recompute the native total or merge different
  thread bindings.
- Persist one monotonic checkpoint per workspace. A checkpoint from the
  current daemon generation is fresh; after restart it remains visible as a
  stale last-seen value until a newer notification arrives. Absence and stale
  evidence remain distinct from zero and from complete lifetime attribution.
- Query native per-thread cost on demand through `account/usage/read`, cache
  the result briefly in memory, and invalidate it when newer token usage
  arrives. Preserve credits, optional USD, grouped detail, and observation
  time. A null or failed native result is explicit unavailable evidence and
  cannot fail token reporting.
- Human collection output contains only workspace, cumulative tokens, latest
  context-window percentage, and optional estimated cost. The targeted view
  adds input/cached and output/reasoning breakdowns. JSON exposes the complete
  typed projection with top-level schema version 10.
- One-shot and `--follow` reads are passive: they do not call `thread/resume`,
  create context, or start a workspace executor. A terminal replaces the prior
  frame; redirected output appends only changed frames. `--follow`/`-f` and
  `--json` are mutually exclusive.

### `coco limits`

- `show` returns the durable desired policy, its revision, the selected
  controller's capabilities and runtime state, and the policy snapshot
  actually applied to a live runtime. An omitted workspace uses the normal scoped
  picker; `--global`/`-g` uses the global picker or exact global resolution.
- `set` patches only fields named by the caller. `--memory-high` and
  `--memory-max` accept exact bytes or decimal/binary suffixes through TiB;
  `--cpu-max` accepts up to three decimal places in logical-core units;
  `--cpu-weight` accepts 1 through 10,000; and `--tasks-max` counts kernel
  tasks/threads. Repeatable `--clear <field>` removes individual values, while
  `reset` removes the complete policy.
- No limit is enabled by default. A non-empty policy is accepted only when the
  selected execution backend advertises every requested semantic capability.
  Activation checks the persisted policy again and fails closed rather than
  silently using the shared or process-tree fallback.
- A running systemd/cgroup-v2 runtime applies and verifies supported updates
  without restarting. Lowering the hard memory ceiling below current charged
  use is rejected. Removing an already-applied CPU maximum is recorded as
  desired but remains pending until the next runtime start because the current
  systemd live-reset path is not reliable; CoCo never stops the runtime
  implicitly for that update.
- Mutations are durable, revisioned, and rolled back when enforcement fails.
  `--json` exposes exact integer fields plus desired/applied snapshots and
  capability flags; human output uses concise sizes and clearly distinguishes
  applied, next-start, and pending-restart state.

### `coco send`

- Accept a non-empty text message and return after `turn/start` is accepted by
  default. In a human terminal, an omitted workspace uses the scoped picker and
  an omitted message uses a subsequent text prompt. An explicit workspace is
  resolved through the daemon before opening that prompt, so an unknown or
  ambiguous reference fails without collecting unused text. Non-terminal and
  `--no-input` callers must supply both.
- `--wait` remains attached to the exact client operation until its native turn
  finishes and prints only the last completed agent message for that turn.
  Completion status and at most 1 MiB of response text are retained for the
  latest 256 operations within an 8 MiB total response budget, only in the
  accepting daemon generation's memory.
  Ctrl-C stops waiting without interrupting the turn. A daemon/App Server loss
  reports the result as unavailable rather than loading an unbounded native
  history or confusing another turn's output with this one. Interrupted and
  failed turns return a non-zero CLI result after printing any captured text.
- Start the first or a later turn with the stored worktree as `cwd` and the
  stored profile/model request. For unbound fresh context, create the native
  thread immediately before the real first turn and bind both IDs only after
  the direct turn response proves acceptance. For inherited context, fork and
  optionally compact the child before dispatching the first turn.
- For an existing binding, read and validate the thread and resume it when
  Codex reports `notLoaded` or the current daemon generation does not yet own a
  subscription. Reload and verify named-profile provenance and preserve the
  explicit model override on that resume.
- Reject workspaces that are provisioning, already active, waiting, completed,
  failed, still unloaded after activation, in native system error, or
  unavailable after connection or daemon loss. v0 does not silently queue
  messages.
- Generate a unique client operation ID and expose `--operation-id` for
  replaying the exact same send after an interrupted CLI response. Persist
  `prepared` before dispatch and `dispatching` immediately before the App
  Server call. Only its direct response proves acceptance; an unconfirmed
  dispatch returns `OPERATION_UNCERTAIN` and is never sent again under that ID.
- After a successful completed turn and fresh native `idle` status, another
  `send` is permitted.

### `coco jump`

- Require the workspace's managed worktree. A human terminal may select an
  omitted workspace locally or with `--global`; deterministic callers must
  provide one.
- For a bound workspace, validate and when needed resume the exact native
  thread so the daemon connection is subscribed, then run `codex resume` in
  that worktree through the authenticated relay. Every ordinary TUI turn is
  assigned to the workspace executor.
- For an unbound fresh workspace, acquire one temporary activation lease and
  launch the TUI in native remote-start mode through a one-use local relay.
  Correlate the exact `thread/start` response, but bind it only after its first
  action creates a durable rollout. Empty exit leaves the workspace prepared.
- Renew the lease while that relay is alive so the TUI may remain open before
  its first action. A missing heartbeat expires after thirty seconds; normal
  exit, relay startup failure, or App Server disconnect releases the lease.
- Once adoption binds the durable thread, the lease records presence rather
  than exclusive activation. An idle thread accepts `send` with its TUI open,
  and multiple bound TUIs may coexist. Every live TUI still prevents close or
  delete; ending one attachment does not release the others.
- Pass the capability token through a child-process environment variable, not
  an argument or persisted workspace metadata.
- Turns started in the TUI must appear in the same native workspace projection
  as turns started with `coco send`; exiting the TUI does not delete the
  workspace.
- A normal `/quit` or `/exit` detaches the remote TUI without interrupting an
  active turn. Explicit interruption remains the separate cancel action.
- Codex's `!command` shortcut is host-local and is not a supported workspace
  shell path with the default remote executor. Review or compaction immediately
  after a newly loaded resume may also use Codex's local default until the next
  ordinary turn selects the workspace executor; CoCo must not hide this native
  0.154.0 limitation.

### `coco decide`

- Resolve one globally unique opaque decision ID without repository scope.
- Support native command-execution approval, file-change approval, and
  structured `requestUserInput` requests. Unknown server-request families
  remain observational and cannot be answered through this command.
- Print bounded, redacted request details and the exact native options in their
  supplied order, then use the shared cursor/direct-number picker. Questions
  accept a listed label or free text only when their native shape allows it.
  `--choice <number>` provides deterministic one-based submission for approval
  options; it does not flatten structured questions into a number.
- Collect secret answers through a cross-platform no-echo terminal prompt;
  never print or persist their values.
- Atomically claim the decision from `pending` to `submitted` before writing the
  response, and accept final `resolved` confirmation only from the matching
  App Server generation, thread, and native request ID. Never retry a submitted
  or orphaned response against a replacement process.
- Persist no raw user-input answer. The App Server response necessarily carries
  it in memory. Schema v11 retains old schema-v6 answer-free submission rows only as
  migration data; all current live correlation is generation-local.

### `coco diff`

- A human terminal may select an omitted workspace locally or with `--global`;
  deterministic callers must provide one.
- Compute from Git at request time against the workspace's immutable `base_sha`, not
  only from the latest Codex turn notification.
- Include committed, staged, and unstaged tracked changes. Report untracked
  paths explicitly; v0 need not serialize binary/untracked file contents into
  a patch.
- Never mutate the index or worktree.
- If the worktree is missing or no longer matches the recorded mode, optional
  branch, or repository binding, return a structured invariant error and
  preserve the record for diagnosis.

### `coco close`, `coco reopen`, and `coco delete`

- `close` accepts one open workspace by scoped name, global unique name, full
  ID, or the common interactive selector. It runs a non-mutating checked plan
  first and removes only the exact managed Git worktree by default. The CoCo
  record, optional branch, and native thread remain.
- Apply requests from the CLI carry the previewed workspace ID and resource
  plan, including `HEAD`. A changed target or plan is rejected before effects
  and must be reviewed again. Managed-path identity is checked before native
  effects and again before Git removal; symlink redirection is never accepted.
- A normal close requires ready provisioning state, a quiescent `idle` or
  `notLoaded` native thread when one exists, no current operation, pending
  decision, live TUI lease, loaded background terminal, worktree lock, binding
  mismatch, or detached commits not retained by another branch/tag/remote ref. Tracked,
  ordinary-untracked, and ignored files require explicit `--discard-changes`; `--yes` only bypasses
  the confirmation for that already-selected policy. A thread already archived
  outside CoCo requires `--archive-thread` so reopen retains an explicit
  unarchive obligation.
- Ordinary close also requires descendant agents using this worktree to be
  quiescent, with no live background terminal. Activity in a verified
  independent directory does not block it. Recheck runtime/descendant safety
  at removal and recovery-archive boundaries rather than trusting an earlier
  plan.
- `close --archive-thread` uses native `thread/archive` after descendant checks
  and before removing the worktree. `reopen` restores the exact close-time
  worktree binding and calls `thread/unarchive` only when CoCo archived it.
- `delete` accepts one open or closed workspace, including failed provisioning
  when its actual resources can be safely verified. The CLI defaults to removal
  of its managed worktree/runtime, record, native thread, and owned branch;
  `--keep-thread` and `--keep-branch` request retention. Existing/adopted
  branches are always retained, and detached workspaces have no branch effect.
  A planned branch name alone does not prove ownership after failed creation:
  require a verified worktree or persisted successful creation evidence.
- Both close and delete report local files. Deletion additionally reports
  commits losing branch/tag/remote-ref reachability. Interactive deletion offers
  one explicit confirmation naming the selected losses; scripts need
  `--discard-changes` and/or `--discard-unretained-commits`. `--yes` cannot expand either
  policy. Close protects detached commits rather than offering to discard
  them, because reopening must remain possible.
- Delete computes the combined plan before effects. For a present open
  worktree, it runs close and delete guards before persisting one deletion
  intent; it revalidates the plan after those programs return. One successful
  operation emits `workspace.deleted`, not a synthetic intermediate closed
  event. A blocker discovered in the combined preview changes nothing.
- The local RPC retains required explicit `deleteThread`/`deleteBranch`
  booleans; omission is invalid instead of acquiring the new CLI defaults.
  The plan reports actual effects, including retaining adopted branches.
- Native archive/delete is blocked when Codex reports spawned descendants.
  Native delete is additionally blocked for CoCo workspaces whose context
  provenance references the thread; an external native reference may still be
  rejected safely by Codex. A rejected native write returns the record to
  closed when an exact follow-up read proves the thread remains; an ambiguous
  result stays recoverable in `deleting` instead of claiming success or
  failure. If worktree removal succeeded before a later failure, return
  `WORKSPACE_DELETION_INCOMPLETE` with the workspace ID and a sanitized cause;
  do not imply that the operation rolled back all resources.
- A prepared context fork protects its saved source before a native child
  exists. A workspace source blocks record deletion until the dependant is
  materialized or deleted; a raw thread source only blocks native thread
  deletion. This applies across repositories and during deletion recovery.
- `--dry-run`/`-n` applies to close and delete. Close retains its `-t` archive
  shorthand. Delete's previous `-t`/`-b` removal flags are removed, not inverted
  into retention. Long keep/discard flags make that breaking alpha change
  explicit, while `-g`, `-n`, and `-y` remain.
  There is no `--all-repos` or other destructive collection form.

## Local MCP surface

Launch the v0 server with:

```text
coco mcp serve --repository <path> [--allow-send]
```

The command is a thin MCP-to-daemon adapter. It speaks MCP over its own
stdin/stdout, sends diagnostics only to stderr, and calls the same daemon
methods as other clients. It must not open SQLite, invoke Git, start Codex, or
duplicate validation and transition rules. This outward-facing MCP `stdio`
transport is separate from the daemon's authenticated loopback-WebSocket
connection to the Codex App Server.

The repository is fixed when the MCP process starts. Workspace names resolve
only within that repository; full workspace IDs are accepted only when they
belong to that same fixed scope. v0 exposes no MCP tool that registers
arbitrary paths or expands filesystem authority at runtime.

### Minimum tools

| Tool | Mutability | Input and result |
| --- | --- | --- |
| `workspaces.list` | read-only | Optional phase filters; returns the same workspace summaries and status/Git field meanings as `coco list --json`. |
| `workspaces.status` | read-only | Workspace name/ID; returns the same projection as `coco status --json`. |
| `workspaces.diff` | read-only | Workspace name/ID and optional output bound; returns base/head, tracked patch, untracked paths, and truncation metadata from the same use case as `coco diff`. |
| `workspaces.send` | mutating, opt-in | Workspace name/ID, non-empty text, and optional operation ID (generated when omitted); starts the same turn as `coco send` and returns workspace/thread/turn correlation. Advertised only with `--allow-send`. |
| `signals.types` | read-only | Selected immutable definitions and JSON Schemas with `emitAllowed`; without a selected catalog, retained repository definitions are read-only. |
| `signals.list` | read-only | Bounded independent pages with scope-bound resume cursors; does not consume records or wake models. |
| `signals.emit` | mutating, version-granted | Exact granted name/version, JSON payload, and required idempotency key. Native Codex thread metadata must resolve to a workspace in the configured repository. |

The default tool list is therefore useful but read-only. Enabling
`workspaces.send` is an explicit operator delegation; it does not enable
approvals, cleanup, repository registration, arbitrary Git commands, or
permission changes.

The adapter records a sanitized audit for calls reaching its daemon dispatcher:
tool action, repository path, optional workspace/operation ID, outcome, and
error code. Denied tools or missing metadata can fail before dispatch. Neither
message contents nor signal payloads are copied into audit records.
Native-first persistence retains this only if a concrete
security, diagnostics, or external consumer needs it; the MCP adapter itself
does not make an audit history product authority.

This v0 surface is not an A2A mailbox. Signal publication derives its sender
from the native thread binding, but directed workspace-to-workspace messaging,
correlation/response routing, `agents.ask`, `integration.request`, and
autonomous delegation policy remain later work.

## Runtime status contract

The public workspace projection contains CoCo's provisioning/binding
`lifecycle`, a native `threadRuntime` projection, a derived `phase`, zero or
more `waitReasons`, and a separate Git projection. `threadRuntime.status`
retains Codex's native status, `runtimeGeneration`, `observedAtMs`, and
`isFresh`. Schema v11 retains old schema-v6 snapshots only as migration data and can mark
them stale on process loss; thread start, resume, and notifications never
write a new snapshot. An unbound ready workspace projects `prepared` without a
native read. Bound ready-workspace reads obtain a fresh projection directly
from stable `thread/read` without loading the thread; until that succeeds they
report unavailable and never serve an old SQLite value as current. Physical
removal of the compatibility columns is a later schema-only checkpoint.

### Runtime phases

| Phase | Meaning |
| --- | --- |
| `provisioning` | CoCo has recorded intent and is creating/verifying external resources. |
| `starting` | A newly forked child is completing requested compaction. |
| `prepared` | The Git workspace is ready and native context has not been materialized yet. |
| `active` | A turn is in progress with no known wait flag. |
| `waiting_for_approval` | The App Server has an unresolved approval request. |
| `waiting_for_input` | The App Server has an unresolved user-input request. |
| `idle` | The thread is available and no turn is running. |
| `not_loaded` | Codex reports that this thread is not loaded in the current runtime. |
| `system_error` | Codex reports a native thread-level system error. |
| `unavailable` | CoCo has no current-generation native thread observation. |
| `failed` | CoCo could not complete workspace preparation or startup. |
| `completed` | Reserved for an explicit future workspace-completion operation; never inferred in v0. |
| `closing` | CoCo is converging a requested worktree close. |
| `closed` | The managed worktree is absent while its retained binding remains reopenable. |
| `reopening` | CoCo is restoring the exact retained worktree and optional archived thread. |
| `deleting` | CoCo is converging an explicitly selected permanent deletion plan. |

The `phase` field is not stored independently. CoCo derives preparation and
terminal phases from its lifecycle; for a ready workspace it derives runtime
phases from fresh native status plus any minimal in-flight operation
correlation. If
Codex reports both wait flags, expose both in `waitReasons` and render
`waiting_for_approval` as the summary phase. Resolving one flag reveals the
remaining reason rather than incorrectly returning to `active`. A server
request by itself never changes phase.

### Git facets

At minimum `status` and `list`/`ls` expose:

- current `headSha`, recorded `baseSha`, and whether they were observed;
- `dirty`, based on tracked, staged, and untracked status;
- `aheadBy` and `behindBy` relative to the immutable base;
- `baseRelation`: `at_base`, `descendant`, `diverged`, or `unknown`.

`ahead_of_base` and `dirty` may be rendered as badges. `merge_ready` must not be
claimed in v0: real readiness requires an integration target and policy that
the handoff does not define.

## Current normalized-event compatibility surface

Schema v11 can still decode the complete prerelease event vocabulary, including
rows created by older CoCo builds. The current runtime writes only this reduced
subset:

```text
workspace.created
worktree.created
agent.started
context.compacted
agent.failed
```

Every retained event carries a monotonic database cursor, event ID, optional
workspace and turn IDs, source, source timestamp when available, recording
timestamp, normalized kind, source method, and versioned payload. Sent user
messages, local/native turn start and completion, plan and diff notifications,
native status/error updates, supported decisions, and unsupported server
requests are no longer event rows. Unknown App Server notifications must be
logged safely without crashing the daemon or inventing a normalized meaning.

This vocabulary is not a copy of Codex history. Native reads supply current
status, while `send --wait` correlates completion and a bounded final response
generation-locally without writing conversation text to SQLite. Retain
CoCo-owned provisioning/context events that serve an implemented consumer.
Remove the now-unconsumed `event.list` compatibility method only through a
versioned daemon-protocol change. Schema v11's hook tables are a separate,
narrow transactional outbox for matching CoCo-owned events; they do not revive
full native event mirroring.

## Safety and lifecycle behavior

- The worker's effective `cwd` must equal its canonical worktree path on every
  thread start/resume and turn start.
- Worker writes must be constrained to the worktree and explicitly justified
  supporting roots. Network access is off by default in the draft profile.
- The daemon-owned App Server may bind only an authenticated IPv4-loopback
  WebSocket; CoCo must not expose it on a LAN or public interface.
- The CoCo MCP server uses local stdio, is read-only unless the operator starts
  it with send or per-signal publication capabilities, and never bypasses daemon authorization or
  validation.
- CoCo hook and guard commands are trusted operator programs, invoked directly
  without a shell. They receive a restricted environment and bounded JSON
  input, but remain same-user code rather than a security sandbox. A hook runs
  after its source fact and cannot change it; a guard runs before a supported
  destructive action and can only allow or deny that exact checked request.
- Supported correlated server requests become generation-bound decisions and
  are never auto-approved. The daemon registers them in memory, performs a
  lock-protected pending-to-submitted transition before the native response,
  and orphans them on connection loss. Other request families are warned and
  left unanswered unless a concrete consumer justifies a bounded record.
  Native request IDs and response values stay private to the daemon.
- Ordinary commits in a branch-backed workspace are supported worker behavior,
  not a separate CoCo transaction. CoCo does not make the whole shared Git
  common directory an ordinary writable workspace merely to enable them;
  sandbox crossings use Codex's native command-approval request and the
  selected user profile remains authoritative.
- After Git-changing activity, CoCo observes and validates the recorded
  repository/worktree/mode/optional-branch binding. It reports drift instead
  of resetting refs or repairing another workspace behind the operator's back.
- Git commands are invoked as argument arrays with validated paths/refs, never
  through interpolated shell strings.
- No lifecycle path uses `git reset --hard`, automatic stash, or automatic
  cleanup. Explicit close delegates removal to `git worktree remove` only
  after exact path/binding, lock, local-state, and detached-commit checks.
  Explicit branch deletion uses compare-and-delete `git update-ref -d` only
  for a branch CoCo created and while its tip equals the previewed `HEAD`.
  It checks other branch/tag/remote-ref reachability before removing commits
  unless the user explicitly authorized commit discard.
- Workspace bindings, provisioning failures, and idempotent operation evidence
  survive daemon or App Server restarts. The current schema fails unfinished workspace
  preparation and marks a turn-start dispatch without a proven response
  `uncertain`; it never retries that operation automatically. Live decision IDs
  deliberately disappear with the daemon generation because the corresponding
  native requests are no longer answerable. Startup does not load every bound
  thread: passive reads reconstruct current runtime state with validated
  `thread/read`, while later `send` and bound `jump` resume only the selected
  workspace thread. First activation creates or adopts fresh context, or
  addresses the exact inherited source through `thread/fork`. An individual failure remains explicit
  and does not stop another workspace or daemon startup. Recovery must not
  report guessed success or create a replacement thread.
- `closing`, `reopening`, and `deleting` are durable saga states. Startup
  reconciles them from verified Git presence/binding and exact native thread
  state. Recovery of an open-delete intent never repeats worktree discard:
  a still-present verified worktree returns to open for fresh confirmation;
  once absent, the recorded thread/branch effects can finish. Proven native
  delete rejection after worktree removal returns the record to `closed`; an
  ambiguous result remains `deleting`, and a missing thread is treated as an
  already-applied explicitly requested deletion during reconciliation.

## Non-goals

The following are intentionally outside v0:

- a CoCo-native TUI, web UI, tmux navigation, remote access, or multi-user
  operation;
- other coding-agent runtimes;
- automatic merging or destructive cleanup;
- automatic or implicit transfer of dirty source state;
- autonomous coordinator policy, workspace-to-workspace A2A messaging, or privileged MCP
  tools such as approval, cleanup, integration, and arbitrary command access;
- multiple simultaneous Codex App Server processes or process pools;
- aggregate CoCo resource pools, automatic host-pressure scheduling, native
  non-Linux enforcement, and a container execution backend;
- a user-managed worker MCP catalog, arbitrary per-thread MCP selection, or an
  Agentgateway integration; the future boundary is documented, but
  Agentgateway is not currently planned;
- a custom CoCo MCP proxy or gateway;
- cross-repository Git-base transfer and the `handoff` context mode;
- user-defined workspace annotations or external ticket/PR references;
- a monorepo/package split for hypothetical future clients.

## Acceptance criteria

### Creation and recovery

- Integration tests create temporary repositories and prove new-branch,
  existing-branch, and detached bindings agree across the stored mode, optional
  branch, base SHA, and worktree; after activation, the exact bound thread ID
  and native `cwd` agree with that Git binding.
- A dirty source with no carry selection produces a CLI warning, creates from
  the selected committed base, and leaves the source unchanged. Invalid base,
  unsafe carry, duplicate name/branch, already-checked-out existing branch,
  and existing destination all fail before an unintended second worktree or
  thread is created.
- A native-fork test selects the Git base independently from either a
  workspace or exact native-thread context source, binds the returned child and
  parent thread, and invokes optional compaction only on that child before it
  becomes ready.
- Local-state tests prove staged and unstaged binary patches preserve index
  placement, ordinary untracked files require explicit selection, ignored
  files require `.worktreeinclude`, the source remains unchanged, and path,
  symlink, overwrite, file-count, and byte bounds fail closed.
- Failures before the worktree is ready leave a diagnosable failed workspace.
  Deferred start/fork failures leave the Git workspace prepared; post-binding
  compaction failure preserves artifacts and marks it failed. No path deletes
  external artifacts automatically.
- Restarting the daemon preserves the stored binding without eagerly resuming
  every `ready` thread. Passive list/status calls return `prepared` for an
  unbound workspace, or a validated native `not_loaded`, current status, or
  explicit unavailable projection for a binding without loading it or serving
  an old SQLite snapshot as current.
- The first post-restart `send` or `jump` activates only its selected
  workspace. A bound thread is resumed and subscribed; prepared fresh context
  is created or adopted, while prepared inherited context uses native
  `thread/fork` from its exact source without coupling conversation to Git base.
  Activation rejects a missing, invalid, moved, or changed named profile and a
  mismatched returned thread ID or working directory. The default profile
  consistently reloads as an empty overlay; one failed activation does not
  prevent other bound workspaces or the daemon from remaining available.

### Workspace retirement

- A dry-run reports the exact worktree, branch/thread disposition, tracked
  change presence, ordinary-untracked and ignored counts, detached-commit
  risk, native descendant count, and every blocker without changing state.
- Normal close refuses active or unavailable native state, pending decisions,
  live TUI leases, loaded background terminals, Git locks, local files without
  explicit discard, mismatched bindings, and detached commits not retained
  by another branch/tag/remote ref.
  `--yes` alone does not relax any of those checks.
- Closing uses only verified `git worktree remove`; it never recursively
  removes a path and never deletes a branch implicitly. Normal lists hide the
  resulting closed record, while an explicit closed list and direct lookup
  retain it.
- Reopen recreates the exact stored path and new/existing/detached binding at
  the recorded close-time `HEAD`, rejects moved or already-checked-out branches,
  and unarchives only a thread CoCo archived for that close.
- Permanent delete accepts open and closed workspaces with one plan and one
  confirmation; thread and owned-branch retention are independent opt-ins.
  Failed preparation is cleaned only from verified resource evidence. Branch
  removal is limited to an unchanged CoCo-created branch; commit discard is
  independent from file discard. Native descendants and context dependants block a potentially
  cascading thread action. No bulk destructive scope exists.
- Failure-injection tests cover interruption before and after Git/native saga
  boundaries. Startup either completes the observed result or rolls back to a
  coherent open/closed record without inventing a replacement worktree,
  branch, or thread.

### Interaction and observation

- Fake App Server process tests exercise Git-only preparation, atomic first
  send, native fork/compaction, event correlation, one-use fresh-TUI relay,
  exact adoption, detach, and completion/failure. A separate opt-in real Codex
  compatibility test consumes no model turn: it checks 0.154.0, registers and
  probes workspace environments, proves an empty remote candidate remains
  unbound, materializes one exact local-shell candidate solely for the history
  contract, then verifies history and exact resume through fresh daemon and
  App Server processes. It also proves two active workspaces have distinct exec
  server PIDs and that close plus normal daemon shutdown stop their tracked
  executor roots. Generated-schema drift is reviewed diagnostically rather
  than rejected byte-for-byte.
- Status returns no invented measurements for an inactive runtime.
  Linux cgroup-v2 tests require a live opaque scope in the instance workspace
  pool, at least one process and task, nonzero charged memory, cumulative CPU,
  whole-scope shutdown, and instance-scoped stale cleanup. Process-tree
  fallback tests require a live root PID, at least one attributed process, and
  nonzero RSS. Interval CPU remains optional on the first sample. Collection
  scanning occurs only for `--resources`, JSON status, or the read-only MCP
  status projection, and no sample is persisted.
- Resource-policy tests require exact set/clear patch semantics, monotonic
  durable revisions, validation before mutation, capability failure before
  activation, rollback after an enforcement failure, and distinct desired
  versus applied snapshots. The live Linux test must prove launch-time policy,
  dynamic update, safe reset of memory/weight/task controls, rejection of a
  hard memory ceiling below current use, staged CPU-cap removal, and raw cgroup
  verification.
- Two sequential `send` operations use the same thread and different turn IDs;
  concurrent sends yield one accepted turn and one deterministic conflict.
- A crash/error before dispatch leaves a replayable prepared operation. A
  crash/error after the dispatch marker but before a proven response leaves an
  `uncertain` operation that the same ID never redispatches. An accepted replay
  returns the same native turn ID, and a different payload under any existing
  operation ID returns `IDEMPOTENCY_CONFLICT`.
- Explicit `status --follow` can attach during a turn, reflects native phase
  changes until Ctrl-C, prints no conversation text, and can detach without
  affecting the turn. Targetless follow observes a local or all-repository
  collection under the same lifetime rule. `send --wait` separately prints only
  the exact accepted turn's generation-local final response, including when
  completion notifications overtake the direct start response.
- `status` exposes an opaque ID for supported pending decisions; `coco decide`
  renders the offered choices through the common picker (or accepts explicit
  `--choice` for an approval), validates input, and sends exactly one response
  to the originating live App Server request.
- `list --json`, targetless `status --json`, and explicitly targeted
  `status --json` parse as JSON with stable version and status fields; JSON and
  `--no-input` never open a prompt. Captured human output is ANSI-free and
  omits opaque IDs that are not required for an immediate action.
- Bound `jump` resumes the exact stored thread in its worktree. Fresh `jump`
  leaves an empty candidate unbound, adopts only the exact materialized
  candidate, releases failed leases, and keeps daemon observation active after
  the TUI exits without sending `turn/interrupt`.
- `diff` reflects changes across all turns and does not change Git status.
- Repository-scope tests cover the implicit `.`, an explicit path, daemon-wide
  `--all-repos` collection, `--global` globally unique and ambiguous name
  lookup, global interactive selection, globally unique workspace IDs, and a
  local miss that suggests but never performs a cross-repository retry.
- Terminal interaction tests cover repository/workspace selection, immediate
  cursor movement, one-key choices 1 through 9, cancellation, missing-name and
  missing-message prompts, and deterministic non-terminal failure without
  consuming redirected input.
- Workspace creation accepts safe slash-separated names such as `feat/login` and
  rejects traversal, empty components, unsafe ref syntax, and ref-prefix
  collisions before creating external artifacts.

### MCP adapter

- An MCP protocol test launches `coco mcp serve` over stdio and proves its
  read-only tools return the same semantic projections as the corresponding
  daemon/CLI calls.
- The default server does not advertise `workspaces.send`. With `--allow-send`, one
  call with an operation ID starts exactly one turn; retrying that operation ID
  cannot create a second turn.
- Every successful and failed tool invocation leaves the current sanitized
  durable audit record. Killing the MCP adapter never stops `cocod` or a
  running workspace.
- No A2A or integration tool is advertised in v0.

### Hooks

- Daemon startup accepts an absent hook file as no configuration and rejects an
  invalid, oversized, symlinked, group/world-writable, or unsupported-version
  configuration before serving clients. Offline validation and atomic reload
  apply the same checks; failed reload preserves the active snapshot. The
  daemon does not watch the file, and command arguments are absent from CLI
  output.
- A new accepted signal and each successful final workspace create, close,
  reopen, or delete transition atomically commit any matching hook event and
  delivery rows. An idempotent signal retry creates no second delivery, and a
  failed hook cannot roll back the source fact.
- Command hooks run without a shell, with bounded input, time, attempts, and
  concurrency and with the inherited environment cleared except for `PATH`.
  One hook ID observes commit order and blocks its own newer deliveries during
  a retry, while different hook IDs may execute concurrently.
  Interrupted running deliveries become pending after restart; a missing or
  changed exact hook definition cancels its old queued delivery rather than
  executing different code under the same ID.
- `workspace.close` and `workspace.delete` guards run after CoCo has resolved
  and checked the exact retirement plan but before the stored saga or an
  external effect begins. Matching guards run once in stable ID order, stop at
  the first denial, never rewrite or retry a request, and use their required
  `onError` policy for execution failures. Dry-runs do not execute guards;
  recovery of an already started saga does not rerun them.
- Process tests prove offline validation, atomic reload preservation,
  signal-filtered and workspace lifecycle delivery, guard request placement,
  and terminal delivery history. An isolated model-free test against the
  selected Codex executable proves native `SessionStart` execution through App
  Server so CoCo does not duplicate the native lifecycle-hook layer.

### Safety

- Filesystem permission tests prove normal writes outside allowed roots are
  denied by the selected App Server/sandbox version.
- Pending approval requests are registered before presentation and resolutions
  are correlated to the original live thread and request. Runtime tests cover
  exact options, concurrent single-response submission, disconnect orphaning,
  restart loss, and secret redaction. Restart and disconnect never replay a
  dead request.
- An explicit opt-in real-Codex test proves that a Git administrative write in
  a linked workspace worktree follows Codex's native approval path and that an
  accepted ordinary commit advances only the workspace's bound branch. The test
  uses an exact command allowlist and does not broaden the common Git directory
  into an unconditional writable root.
- The daemon socket, SQLite file, App Server endpoint descriptor, and
  capability token are user-only. The shared App Server port is authenticated
  and bound to IPv4 loopback. Workspace exec servers also bind ephemeral
  loopback ports whose URLs are not published, but upstream local mode has no
  equivalent CoCo token; v0 therefore remains a single-local-user tool rather
  than a multi-user security boundary.

### Native-first migration and release proof

- A released Codex build, not upstream documentation or an unreleased merge,
  passes behavioral tests for every native contract CoCo adopts: exact thread
  read with `cwd` and status, resume, fork/compaction where used, request
  responses, and remote-TUI unsubscribe. Generated schemas assist review but
  are not a byte-for-byte gate.
- Native-projection tests cover active/waiting/idle/error/not-loaded status,
  read failure, restart, final-output selection, and binding mismatches before
  any table or existing record is removed. The initial direct read cutover is
  acceptable without a prolonged shadow-only checkpoint because the original
  0.147.0 proof and current 0.154.0 real-process gate prove non-loading reads,
  restart persistence, exact ID/`cwd`, and optional history hydration. Eager recovery
  status/failure writes stop with that obsolete startup path. The later live-
  event reduction also stops user-message, native status/plan/diff/error,
  server-request, and decision writes while existing legacy data remains a
  reversible bridge. Schema v6 additionally stops local-turn,
  `active_turn_id`, start/resume snapshot, and turn start/completion-event
  writes after migrating operation-bearing legacy turns into the minimal
  ledger.
- Existing command help, repository scope, stable error codes, prepared-create
  behavior, versioned JSON field meanings, idempotency, and `jump` detach
  behavior remain compatible through cutover.
- Each native mirror stops receiving writes before its column/table is removed.
  Migration fixtures cover every supported prerelease schema, and crash
  injection proves the minimal operation ledger still prevents duplicate
  worktrees, threads, and turns.
- End to end, CLI clients create and start workspaces in two repositories and
  exit; an MCP client inspects and continues one; another CLI answers a live
  request; `jump` attaches and detaches the exact thread/worktree; coordinator
  and App Server restart; repeated operation IDs produce no duplicate external
  artifacts.

A public alpha is a **go** only when that multi-client control-plane workflow
works against the selected released Codex build and a real second client uses
it. It is a **no-go** if practical use is only one operator running `create`
then `jump`, or if native Codex publishes an equivalent programmable
worktree/thread binding with multi-client control. In either no-go case reduce
CoCo to private glue rather than expanding it feature-for-feature.

## Open decisions and blockers

Decision-response closure and the common interactive selector are implemented.
The shipped flow preserves request-specific session and policy-amendment
choices whenever Codex supplies them; `--choice` is intentionally limited to
the one-dimensional approval form. `decide` still requires the opaque decision
ID because choosing among unrelated pending requests has not been designed.

The Git administrative-storage decision is closed: workspaces use ordinary native
worktrees and may commit on their own branches through Codex's existing
approval model. CoCo neither supplies a separate Git database nor adds a
custom commit proxy. A later app-side commit action may be useful UI
convenience, but it is not part of the isolation contract.

Codex 0.154.0 is now the selected and proven compatibility baseline;
maintaining a broader range is optional rather than an alpha blocker. The
remaining release gate is the two-repository, multi-client product proof.
Windows support, profile configuration UX, and a future explicit
workspace-completion command remain follow-ups rather than reasons to delay
the control-plane proof. Background supervision also remains post-alpha: do
not ship an auto-enabled service until graceful SIGTERM and App Server
child-failure/recovery semantics are implemented and tested across the
supported service managers.

Creation now separates four choices: Git base, Codex conversation context,
worktree binding, and explicit local-state carry. A context source may be a
same-repository CoCo workspace or an exact native Codex `thread.id`;
`thread.sessionId` is deliberately not accepted because it identifies a fork
tree rather than one exact conversation. The former coupled `--fork-from`
surface is hidden as a temporary compatibility alias that selects both base and
context from one workspace; public docs and help use only the independent
inputs.

Remaining creation follow-ups are deliberately narrower: design an atomic
detached-to-branch promotion before considering detached as the default,
decide whether cross-repository Git-base transfer has a safe use case, and
design handoff artifacts independently from native conversation forking.
