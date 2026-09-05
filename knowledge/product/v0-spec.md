---
type: Product Specification
title: CoCo v0 technical product specification
description: Defines the first usable Codex Coordinator slice, its command contracts, domain semantics, lifecycle, and acceptance criteria.
tags: [product, v0, cli, mcp, codex, git, orchestration]
status: draft
---

# CoCo v0 technical product specification

## Purpose

CoCo coordinates Codex tasks and the Git working areas in which they run. Its
first release lets one operator create an isolated Codex work unit from a
known Git commit, send work to it, and observe both model activity and
repository changes without mixing orchestration policy into a presentation
client.

This document is normative where it says **must** or **must not**, subject to
the classification below. It describes intended v0 behavior; it does not claim
that the behavior is already implemented.

## Evidence and decision classification

### Observed facts

- At the specification baseline on 2026-09-05, the repository had no commit
  and no implementation files; it contained the handoff and documentation
  foundation only.
- The locally installed `codex-cli 0.147.0` can generate App Server protocol
  schemas.
- Schemas generated from that installation expose the `initialize`,
  `thread/start`, `thread/resume`, and `turn/start` client requests;
  thread/turn, plan, diff, item, token, error, and status notifications; and
  server-initiated approval and user-input requests.
- That observed protocol is version-specific. Generated schemas from the
  selected Codex executable, rather than this prose, are authoritative for
  field names and wire payloads.

### Confirmed product and architecture decisions

- The product is named **CoCo** (Codex Coordinator), with commands `coco` and
  `cocod`.
- It is implemented in Rust with Tokio.
- Codex integration uses the Codex App Server over local `stdio` in v0.
- Git isolation uses native Git worktrees and persistence uses SQLite.
- The CLI and a local CoCo MCP server are v0 client adapters. Later TUI and
  local web clients are equal peers and must use the same headless
  orchestrator behavior rather than reimplementing it.
- CoCo itself must be usable as an MCP server. Its initial external transport
  is local `stdio`; the MCP adapter must not contain orchestration policy.
- A task, Codex thread, worktree, branch, base commit, context provenance, and
  profile are distinct concepts even when one v0 operation creates them
  together.
- Dirty source checkouts are rejected on the normal creation path. CoCo does
  not silently copy, stash, reset, or snapshot uncommitted changes.
- Worktrees live outside the registered repository by default.
- CoCo does not automatically perform destructive branch or worktree cleanup.
- v0 has no publicly reachable network listener.
- v0 task worktrees are branch-backed. A detached task-worktree mode is a
  planned post-v0 capability, not current behavior.

### Draft assumptions

These choices make the contract implementable but were not explicitly fixed
by the handoff. They are reversible and require confirmation before a stable
release:

- v0 supports a single local operator on Linux and macOS. The local daemon
  protocol uses a Unix domain socket; a Windows named-pipe adapter can be added
  behind the same protocol later.
- `fresh` is the only context mode accepted by `coco new` in v0. `fork` and
  `handoff` remain reserved domain values and return a clear unsupported-mode
  error until their transfer contracts are implemented.
- v0 uses Codex's base configuration by default and accepts a named
  `[profiles.<name>]` overlay from `$CODEX_HOME/config.toml`. The applied,
  non-secret effective settings are snapshotted onto each task; profile CRUD is
  not part of v0. Future MCP capability profiles are a separate snapshot
  dimension rather than an unstructured extension of this execution profile.
- Task names are unique within a repository. Internal task IDs are globally
  unique and remain stable if a display name is introduced later.
- CLI and MCP adapter connect to one user-scoped `cocod`; the daemon owns one
  App Server child process at a time in v0.
- The MCP adapter is repository-scoped at launch and read-only by default. An
  explicit startup capability may add the narrowly scoped `agents.send` tool.

### Recommendations adopted by this draft

- Represent runtime phase and Git condition separately. Values such as
  `active` and `waiting_for_approval` describe execution, while `dirty` and
  `ahead_of_base` are independently calculated Git facets. A single combined
  enum would permit contradictory or lossy state.
- Store an immutable profile snapshot and context descriptor on each task so a
  later configuration edit cannot silently change the audit record.
- Make every mutating client request carry a client-generated operation ID.
  This lets the daemon reject or replay duplicate requests from CLI or MCP
  without creating a second branch, worktree, thread, or turn.
- Add a minimal approval response command before calling v0 generally usable;
  merely displaying an approval can otherwise leave a turn permanently
  blocked. The exact command spelling remains open below.

## v0 outcome

An operator can register a Git checkout, create a task from an exact commit,
observe the created worktree and Codex thread binding, send additional turns,
follow normalized events, and inspect all task changes relative to the fixed
base commit. A local MCP host can inspect the same task projections and, when
the operator explicitly enables the capability, send a turn through the same
daemon use case.

The smallest proof slice is successful when it demonstrates, end to end:

1. `cocod` starts and initializes a Codex App Server child.
2. CoCo creates a native worktree and branch from a fully resolved base SHA.
3. The App Server creates a non-ephemeral thread with that worktree as `cwd`.
4. SQLite atomically records the task-to-thread-to-worktree binding and the
   selected profile/context metadata.
5. CoCo starts a text turn in that thread.
6. At least thread-started, turn-started, agent-message, and turn-completed or
   failure events reach a CLI client and durable state can be shown afterward.

The MCP adapter is required for the complete v0 but does not need to be in this
first proof slice; it is added once the daemon methods it delegates to are
stable.

## Post-v0 worktree direction

CoCo should support a detached mode for lightweight or exploratory tasks. It
will create a full Git worktree at the resolved base SHA without allocating a
branch immediately. Filesystem and process isolation are identical to a
branch-backed task; only the Git ref lifecycle differs.

An explicit promotion or handoff operation will later create a branch while
preserving the task, Codex thread, worktree, base SHA, and audit history. This
mode must be represented independently from context modes such as `fresh` or
`fork`. Until its persistence and promotion contracts are implemented, `coco
new` continues to create `coco/<name>` branches.

## Domain vocabulary and invariants

### Repository

A registered, canonical Git worktree from which CoCo resolves refs and creates
task worktrees. Registration does not mean CoCo owns or may delete the
repository.

### Task

A stable CoCo work unit identified independently of the Codex thread. A task
owns exactly one repository binding, branch, base SHA, worktree path, context
descriptor, and profile snapshot. In v0 it acquires at most one Codex thread.

### Thread and turn

The Codex thread owns model conversation history. A turn is one execution in
that thread. CoCo stores correlation and lifecycle metadata, but the App Server
remains authoritative for conversation contents and Codex-native status.

### Code and context provenance

- `base_sha` is a full commit object ID and defines the immutable code origin.
- The branch and worktree define the task's current code state.
- `context_mode` and its descriptor define how model context was obtained.

These dimensions must not be inferred from one another. Forking context does
not imply copying uncommitted files, and sharing a base SHA does not imply
sharing conversation history.

### Required invariants

- A task name and generated `coco/<name>` branch are unique within the
  repository.
- A managed worktree path and Codex thread ID belong to at most one task.
- `base_sha`, repository, branch, worktree path, and context mode do not change
  after the task passes provisioning. A replacement creates a new task.
- Every turn belongs to exactly one task and uses that task's thread and
  worktree.
- A task has at most one in-progress turn in v0.
- CoCo never guesses that a task is complete from a successful turn. Turn
  completion returns the task to `idle`; explicit task completion is deferred
  until a lifecycle command is specified.
- External Git state is observed, never overwritten to make persisted state
  appear correct.

## Supported commands

```text
coco repo add [path]
coco new <name> --base <ref> --context fresh --goal <goal> [--profile <name>]
coco ls [--json]
coco show <task> [--json]
coco send <task> <message>
coco watch <task> [--json]
coco diff <task>
coco mcp serve --repository <path> [--allow-send]
```

`<task>` accepts a full task ID everywhere. A name is resolved within the
registered repository containing the CLI's current directory; ambiguity or a
missing repository context is an error rather than a guess.

All commands must use the daemon contract. The CLI must not open SQLite,
operate worktrees, or call the App Server directly.

### `coco repo add [path]`

- Default `path` is the current directory.
- Resolve symlinks and use Git's top-level path as the canonical repository
  path.
- Require a local Git worktree and an accessible common Git directory.
- Register the repository idempotently and return its stable ID and root.
- Do not create a Git commit, branch, config entry, or worktree.
- Do not require a clean checkout merely to register it; cleanliness is a
  creation precondition and must be reported by `new`.

### `coco new`

`--base`, `--context`, and a non-empty `--goal` are required in v0. Optional
`--profile <name>` applies the matching Codex profile table to only this new
thread; omitting it keeps the App Server's base configuration. Creation must
execute as a recoverable saga:

1. Resolve the registered source checkout from CLI context.
2. Under a repository-scoped lock, reject a dirty source checkout.
3. Resolve `<ref>^{commit}` to a complete object ID and retain that SHA, never
   the moving ref, as `base_sha`.
4. Validate the task name and `coco/<name>` with Git; reject existing task,
   branch, or destination-path collisions.
5. Persist a `provisioning` task and `task.created` event before external side
   effects.
6. Create `coco/<name>` and its worktree at the exact base SHA.
7. Start a non-ephemeral Codex thread with canonical `cwd` equal to the task
   worktree and with the snapshotted worker profile.
8. Persist the returned thread ID before starting work.
9. Start a structured first turn containing the goal, repository/worktree
   identity, branch, base SHA, constraints, and requested verification.
10. Return after the App Server accepts the turn; execution continues in the
    daemon and is observed with `watch` or `show`.

If a post-worktree step fails, CoCo must mark the task `failed`, record the
stage and discovered artifacts, and leave the branch/worktree intact. It must
not hide the failure by destructively cleaning up. Retrying an operation ID
must not create duplicate artifacts.

### `coco ls`

- List task ID, name, repository, runtime phase, Git badges, branch, and last
  update time.
- Default to tasks in the current registered repository. A later `--all`
  option may expose the daemon-wide view.
- Sort deterministically by most recent update, then task ID.
- `--json` emits one versioned JSON document and no decorative stdout text.

### `coco show`

- Return the complete CoCo task projection: goal, immutable Git binding,
  context mode, non-secret profile summary, Codex thread and active/latest
  turn IDs, runtime phase and wait reasons, Git facets, timestamps, last error,
  and recent event cursor.
- Refresh cheap Git observations before responding; stale persisted Git state
  must be labelled if refresh fails.
- `--json` uses the same field meanings as the daemon protocol and includes a
  top-level schema version.

### `coco send`

- Accept a non-empty text message and return after `turn/start` is accepted,
  not after the turn completes.
- Start a new turn in the existing thread with the stored worktree as `cwd` and
  the stored sandbox/profile policy.
- Reject tasks that are provisioning, already active, waiting, completed, or
  unreconciled after daemon failure. v0 does not silently queue messages.
- Use a unique client message/operation ID so an uncertain CLI retry is
  idempotent.
- A successful completed turn returns the task to `idle`, permitting another
  `send`.

### `coco watch`

- Replay durable events after an optional internal cursor, then stream new
  events until interrupted by the operator.
- Human output may render agent text deltas, plans, approvals, tool summaries,
  diffs, completion, and errors. Ctrl-C detaches the watcher; it must not
  interrupt the task.
- `--json` emits newline-delimited, versioned event envelopes because the
  stream is unbounded.
- Multiple watchers may observe one task without changing its lifecycle.

### `coco diff`

- Compute from Git at request time against the task's immutable `base_sha`, not
  only from the latest Codex turn notification.
- Include committed, staged, and unstaged tracked changes. Report untracked
  paths explicitly; v0 need not serialize binary/untracked file contents into
  a patch.
- Never mutate the index or worktree.
- If the worktree is missing or no longer matches the recorded branch, return
  a structured invariant error and preserve the record for diagnosis.

## Local MCP surface

Launch the v0 server with:

```text
coco mcp serve --repository <path> [--allow-send]
```

The command is a thin MCP-to-daemon adapter. It speaks MCP over its own
stdin/stdout, sends diagnostics only to stderr, and calls the same daemon
methods as other clients. It must not open SQLite, invoke Git, start Codex, or
duplicate validation and transition rules. This outward-facing MCP transport
is separate from the daemon's inward-facing Codex App Server `stdio`
connection.

The repository is fixed when the MCP process starts. Task names resolve only
within that repository; full task IDs remain accepted. v0 exposes no MCP tool
that registers arbitrary paths or expands filesystem authority at runtime.

### Minimum tools

| Tool | Mutability | Input and result |
| --- | --- | --- |
| `tasks.list` | read-only | Optional phase filters; returns the same task summaries and status/Git field meanings as `coco ls --json`. |
| `agents.status` | read-only | Task name/ID; returns the same projection as `coco show --json`. “Agent” is presentation language for the task's Codex binding, not a second domain entity. |
| `changes.diff` | read-only | Task name/ID and optional output bound; returns base/head, tracked patch, untracked paths, and truncation metadata from the same use case as `coco diff`. |
| `agents.send` | mutating, opt-in | Task name/ID, non-empty text, and operation ID; starts the same turn as `coco send` and returns task/thread/turn correlation. Advertised only with `--allow-send`. |

The default tool list is therefore useful but read-only. Enabling
`agents.send` is an explicit operator delegation; it does not enable approvals,
cleanup, repository registration, arbitrary Git commands, or permission
changes.

Every MCP invocation is durably audited by the daemon with tool name, MCP
adapter instance/client label, task when applicable, operation ID, timestamps,
outcome, and sanitized error. Message contents are not copied into audit
events by default; correlation and byte length are recorded while Codex keeps
conversation history.

This v0 surface is not A2A messaging. The caller is an external MCP client and
the target is a CoCo task. Task-to-task identity, correlation/response routing,
`agents.ask`, `integration.request`, and autonomous delegation policy remain
later work.

## Runtime status contract

The public task projection contains a runtime `phase`, zero or more
`waitReasons`, and a separate `git` object.

### Runtime phases

| Phase | Meaning |
| --- | --- |
| `provisioning` | CoCo has recorded intent and is creating/verifying external resources. |
| `starting` | The worktree exists and CoCo is creating/resuming the thread or first turn. |
| `active` | A turn is in progress with no known wait flag. |
| `waiting_for_approval` | The App Server has an unresolved approval request. |
| `waiting_for_input` | The App Server has an unresolved user-input request. |
| `idle` | The thread is available and no turn is running. |
| `failed` | Provisioning or the latest execution ended in a non-recovered failure. |
| `interrupted` | The latest turn stopped without completing, including unreconciled daemon loss. |
| `completed` | Reserved for an explicit future task-completion operation; never inferred in v0. |

If Codex reports both wait flags, expose both in `waitReasons` and render
`waiting_for_approval` as the summary phase. Resolving one flag reveals the
remaining reason rather than incorrectly returning to `active`.

### Git facets

At minimum `show` and `ls` expose:

- current `headSha`, recorded `baseSha`, and whether they were observed;
- `dirty`, based on tracked, staged, and untracked status;
- `aheadBy` and `behindBy` relative to the immutable base;
- `baseRelation`: `at_base`, `descendant`, `diverged`, or `unknown`.

`ahead_of_base` and `dirty` may be rendered as badges. `merge_ready` must not be
claimed in v0: real readiness requires an integration target and policy that
the handoff does not define.

## Normalized events

The client-facing event vocabulary is independent of App Server method names.
The minimum durable set is:

```text
task.created
worktree.created
agent.started
message.received
turn.started
plan.updated
approval.requested
approval.resolved
diff.updated
agent.message.completed
turn.completed
agent.failed
task.completed        # reserved; not emitted by current v0 commands
control.call.started
control.call.completed
```

Every event carries a monotonic database cursor, event ID, optional task and
turn IDs, source, source timestamp when available, recording timestamp,
normalized kind, source method, and versioned payload. Task IDs are present
for task-scoped events but may be absent for an audited global call such as
`tasks.list`. High-volume text/reasoning/output deltas may be live-only;
completed messages and all state-changing events must be durable. Unknown App
Server notifications must be logged safely without crashing the daemon or
inventing a normalized meaning.

## Safety and lifecycle behavior

- The worker's effective `cwd` must equal its canonical worktree path on every
  thread start/resume and turn start.
- Worker writes must be constrained to the worktree and explicitly justified
  supporting roots. Network access is off by default in the draft profile.
- The daemon must not bind a TCP/WebSocket listener in v0.
- The CoCo MCP server uses local stdio, is read-only unless the operator starts
  it with the send capability, and never bypasses daemon authorization or
  validation.
- Operator approvals remain visible, identified by a stable CoCo request ID,
  linked to task/thread/turn, and durably record their request and resolution.
- Git commands are invoked as argument arrays with validated paths/refs, never
  through interpolated shell strings.
- No lifecycle path uses `git reset --hard`, automatic stash, forced branch
  deletion, or automatic worktree deletion.
- Task and event records survive daemon or App Server restarts. Recovery may
  mark in-flight work interrupted; it must not report guessed success.

## Non-goals

The following are intentionally outside v0:

- TUI, web UI, tmux navigation, remote access, or multi-user operation;
- other coding-agent runtimes;
- automatic merging or destructive cleanup;
- automatic snapshots of dirty source checkouts;
- autonomous coordinator policy, task-to-task A2A messaging, or privileged MCP
  tools such as approval, cleanup, integration, and arbitrary command access;
- multiple simultaneous Codex App Server processes or process pools;
- a user-managed worker MCP catalog, arbitrary per-thread MCP selection, or an
  Agentgateway integration; the future boundary is documented, but
  Agentgateway is not currently planned;
- a custom CoCo MCP proxy or gateway;
- fully implemented `fork` and `handoff` context modes;
- a monorepo/package split for hypothetical future clients.

## Acceptance criteria

### Creation and recovery

- An integration test creates a temporary Git repository, commits a fixture,
  runs registration and creation, and proves the stored base, branch,
  worktree, thread ID, and thread `cwd` agree.
- Dirty source, invalid base, duplicate name/branch, and existing destination
  all fail before an unintended second worktree or thread is created.
- Injected failures after each saga stage leave a diagnosable `failed` task and
  never delete the external artifacts automatically.
- Restarting the daemon preserves list/show output and either resumes an idle
  thread or truthfully marks an in-flight turn interrupted.

### Interaction and observation

- A fake App Server contract test and an opt-in real Codex smoke test both
  exercise initialization, thread start, turn start, event correlation, and
  completion/failure.
- Two sequential `send` operations use the same thread and different turn IDs;
  concurrent sends yield one accepted turn and one deterministic conflict.
- A watcher can attach before or during a turn, receives ordered events, and
  can detach without affecting the turn.
- `ls --json` and `show --json` parse as JSON with stable version and status
  fields; `watch --json` parses one envelope per line.
- `diff` reflects changes across all turns and does not change Git status.

### MCP adapter

- An MCP protocol test launches `coco mcp serve` over stdio and proves its
  read-only tools return the same semantic projections as the corresponding
  daemon/CLI calls.
- The default server does not advertise `agents.send`. With `--allow-send`, one
  call with an operation ID starts exactly one turn; retrying that operation ID
  cannot create a second turn.
- Every successful and failed tool invocation leaves a sanitized durable audit
  pair, and killing the MCP adapter does not stop `cocod` or a running task.
- No A2A or integration tool is advertised in v0.

### Safety

- Filesystem permission tests prove normal writes outside allowed roots are
  denied by the selected App Server/sandbox version.
- Pending approval requests are persisted before presentation and resolutions
  are correlated to the original App Server request.
- The daemon socket and SQLite file are user-only, and no network port is
  opened.

## Open decisions and blockers

No open decision blocks the storage/Git scaffold or a non-destructive App
Server proof turn. Two decisions block calling the complete v0 safe and
generally usable:

1. **Approval response UX:** the confirmed auditability requirement needs a
   response path, but the handed-off command list has no approval command.
   Recommendation: add
   `coco approve <request-id> --decision accept|decline|cancel` in v0 and defer
   session-wide/amendment decisions until their display contract is designed.
2. **Git administrative write scope:** a linked worktree stores objects and
   refs in the repository's shared Git directory. Decide whether v0 workers
   may commit. If they may, integration tests must establish the minimum safe
   App Server writable roots and document that worktree filesystem isolation
   is not complete Git-ref isolation. If they may only edit files, keep the
   shared Git directory read-only and make commits an explicit later
   coordinator operation.

Release follow-ups that are not blockers for the proof slice are the exact
supported Codex CLI version range, Windows support, profile configuration UX,
and the future explicit task-completion command.
