---
type: Engineering Architecture
title: CoCo v0 architecture
description: Defines the local daemon topology, native-first ownership boundary, persistence model, adapters, recovery, and staged migration for CoCo v0.
tags: [architecture, daemon, cli, mcp, sqlite, git, codex, protocol]
status: draft
---

# CoCo v0 architecture

## Classification

### Observed

- The repository now has an early Rust implementation for domain state, Git,
  SQLite, local RPC, the Codex App Server client, profiles, CLI parsing, the
  daemon coordinator, executable binaries, and CoCo's control MCP adapter.
- `codex-cli 0.153.4` is installed in the development environment and is the
  selected compatibility baseline. A model-free real-process test exercises
  the exact stable App Server methods and fields CoCo consumes.
- Schema v10 adds dedicated signal types, a cursor stream, and bounded signal
  records; it does not revive native-history mirroring. See
  [Agent-emitted signals](signals.md) for grants, attribution, and retention.
  It retains schema v6's minimal `turn_start` operation ledger and
  schema v7's typed worktree mode. Schema v8 adds the separate durable
  workspace-availability state plus close-time binding evidence, and schema v9
  adds the deletion intent needed to recover a partially completed permanent
  retirement. The database still keeps schema-v5 status columns, turn rows,
  normalized events, and decision rows as a reversible migration bridge.
  Production no longer writes thread-status
  snapshots, local turns, `active_turn_id`, decisions, sent prompts, turn
  start/completion events, or native status/plan/diff/error and unsupported-
  request observations. Passive workspace reads validate the bound thread
  through stable `thread/read` without loading it.
- The checked-in 0.147.0 schema snapshot remains an upgrade-review aid, but
  generated-schema equality is not a runtime compatibility gate. Additive
  upstream definitions do not affect CoCo's narrow owned projections.
- Released Codex 0.153.4 does not make an empty fresh thread resumable merely
  because `thread/start` returned it. CoCo therefore leaves a newly created
  Git workspace explicitly `prepared` and unbound. The real-process test proves
  that an empty remote TUI launch remains unbound, a native action creates a
  non-empty rollout, the exact resulting thread can then be adopted, and the
  same binding survives both daemon and App Server restart. No synthetic turn
  or unreleased `codex --worktree` behavior is used.
- The same real-process boundary proves exact native archive, unarchive, and
  delete behavior used by workspace retirement. Exact lookup uses
  `thread/read`: default `thread/list` source filtering omits App-Server-origin
  threads, while `thread/read` continues to find archived rollouts. CoCo
  classifies their native archive location from the returned rollout path and
  fails closed when Codex does not expose that evidence.

### Confirmed

- Rust, Tokio, SQLite, native Git worktrees, and one daemon-owned Codex App
  Server are fixed v0 choices. The daemon and interactive Codex TUI share its
  capability-token-protected IPv4-loopback WebSocket.
- `cocod` owns workspace/runtime coordination and safety policy. CLI, CoCo's
  local MCP server, and later TUI/web clients are equal presentation/control
  adapters only. Ticket scheduling and business workflow are not daemon policy.
- Task management belongs to a separate product. It may call CoCo directly,
  or a separate integration may compose the two; neither deployment topology
  is required yet. The integrating component owns ticket/workspace mappings
  and business reactions to agent reports. CoCo must remain usable without
  that system, and a workspace/native-thread state must not become a second
  ticket status. See the [product boundary](../product/v0-spec.md#boundary-with-task-management-systems).
  Signals are a domain-neutral capability; directed peer communication remains
  future work. Neither authorizes an embedded task scheduler or ticket workflow.
- Alpha distribution installs `cocod` without enabling it as a service; its
  lifecycle is an explicit foreground command. A future user service is
  opt-in, cross-platform work rather than a Linux-only packaging side effect.
- CoCo's v0 MCP server uses local `stdio` and delegates every tool to `cocod`;
  it does not own Git, SQLite, or Codex orchestration.
- CoCo is a headless binding and control layer, not an alternative Codex
  runtime. Git owns files, refs, commits, and current worktree condition. The
  App Server owns threads, turns, conversation contents, native status,
  available models, effective Codex configuration, and server-request
  semantics. CoCo durably owns only repository/worktree/thread bindings,
  multi-step provisioning and failure evidence, idempotent control operations,
  and CoCo-specific policy or context provenance that neither upstream system
  represents.
- Native values may be cached in memory for projection, but a persisted copy
  must not become an independent source of truth. Decision projection and the
  active-send output correlation and concurrency guard are generation-local.
  The operation ledger retains only dispatch/idempotency facts that Codex
  cannot reconstruct; no Codex conversation text is persisted by CoCo.
- The native-first overhaul preserves the current command names, repository
  scoping, versioned JSON meanings, stable error codes, prepared-workspace
  behavior, `jump` detach semantics, and idempotency until a separately
  documented deprecation says otherwise.
- No public network API, destructive automatic cleanup, implicit dirty-checkout
  capture, multi-agent-runtime abstraction, or early monorepo is part of v0.
  Explicit checked workspace close and closed-only deletion are user-requested
  lifecycle operations, not cleanup automation.

### Assumed for this draft

- One OS user runs one daemon and one App Server child. The current build runs
  on Linux/macOS; Windows support requires the named-pipe backend described
  below.
- Client adapters use one request/event protocol over OS-local IPC: a Unix
  domain socket on Linux/macOS and, once implemented, a Windows named pipe.
  The MCP adapter separately speaks MCP over stdio to its host.
- The default Codex configuration is represented by an empty per-thread
  overlay. A named execution profile is the complete
  `$CODEX_HOME/<name>.config.toml` document. Schema v6 retains its provenance
  and redacted effective settings per workspace; the native-first target keeps
  only fields proven necessary for verified recovery. `fresh` and
  same-repository `fork` context are executable in v0.
- Model discovery delegates to the daemon-owned App Server's paginated
  `model/list`. An explicit workspace model remains a separate `thread/start`,
  `thread/fork`, or `thread/resume` field beside the profile `config` object so
  Codex, not CoCo, resolves their precedence.

### Recommended

- Model core behavior as use cases over narrow ports. Keep process, SQLite,
  Git, and Codex wire details in adapters; this is enough extension structure
  for later clients without introducing package or runtime abstraction early.
- Persist CoCo-owned binding, provisioning, and operation changes atomically.
  Treat Git and Codex calls as saga steps because they cannot share that
  transaction. Do not require a durable normalized event for every native
  state transition; add a transactional outbox later only for a concrete hook
  or replay consumer.
- Use App Server schemas generated by the selected `codex` executable, validate
  the Rust adapter against them, and pin or compatibility-check that executable
  for releases.

## Topology and authority

```text
 human / scripts --> coco CLI -------------------------+
 Web/TUI adapters (later) -----------------------------+-- local RPC --> cocod
 MCP host <-- stdio --> coco mcp adapter --------------+                  |
                                                                          |
                              +------------------+-------------------------+
                              |                  |                         |
                              v                  v                         v
                         SQLite store       Git adapter              Codex adapter
                                                                         |
                                      authenticated ws://127.0.0.1:<port>
                                              +--------------------------+
                                              |                          |
                                              v                          v
                                      codex app-server          official Codex TUI
                                                                  (`coco jump`)
```

Only `cocod` may mutate managed state, create worktrees, or own the App Server
process. CLI, Web, and MCP do not bypass daemon use cases. `coco jump` is the
narrow exception at the presentation edge: it resolves the workspace through local
RPC, then attaches the official Codex TUI directly to the existing App Server
and thread. SQLite is the durable source only for CoCo-owned bindings,
provisioning evidence, idempotent operations, and irreducible provenance. Git
is authoritative for files, refs, commits, and worktree condition; the App
Server is authoritative for Codex conversation and runtime objects. The
schema-v9 bridge still carries legacy columns and tables described below so
existing databases remain readable for one migration revision. Current
production writes are limited to CoCo-owned provisioning/failure evidence,
the operation ledger, and MCP audit rather than a second authoritative
history. Completed turn output exists only in a bounded generation-local
cache for `send --wait`.

CoCo's MCP `stdio` surface and its authenticated Codex WebSocket serve
different roles. No payload is blindly proxied between those protocols.

Worker-facing MCP selection is a third, separate concern. CoCo will own its
future catalog and immutable per-thread bindings while initially projecting
them into native Codex MCP configuration. Agentgateway is documented only as
an optional future data-plane adapter and is not currently planned. See
[MCP catalog and worker runtime boundary](mcp-runtime.md).

## Rust source boundary

The native-first overhaul keeps the existing single-package, focused-module
layout and staged dependency direction defined in
[Rust source architecture and code health](rust-architecture.md). It is a
change in semantic ownership, not a reason to reshuffle files, add a Cargo
workspace, expose implementation modules, or introduce one trait per adapter.
The typed daemon protocol, coordinator boundary, and `WorkerRuntime` port are
the seams for this migration. Move code only when one of those existing
modules acquires or loses a coherent responsibility, and never combine a
physical split with an authority change in the same commit.

## Core ports and use cases

Use the existing narrow worker seam and explicit owned state rather than
global singletons. The target responsibilities are:

```text
Store
  repositories and workspace bindings
  provisioning stages and retained failure evidence
  idempotent operation intent/result correlation
  only irreducible profile/context provenance

Git adapter
  repository identity and current worktree truth
  base/ref validation, worktree creation, binding checks, diff
  checked worktree removal, exact restoration, guarded owned-branch deletion

WorkerRuntime / Codex adapter
  start, resume, read, locate, and list native threads
  start turns, project live native status, answer exact server requests
  model discovery, subscription, archive/unarchive/delete, descendant and
  background-terminal inspection, and process lifecycle

Clock / IdGenerator
  injectable sources for deterministic tests and idempotency
```

The Store exposes the operation ledger and the still-needed event/audit paths.
Legacy turn, status-snapshot, and decision helpers remain only as migration or
test inputs; production decision routing is generation-local. None of those
legacy shapes justifies a second authoritative history.

Coordinator use cases are `RegisterRepository`, `ListRepositories`,
`CreateWorkspace`, `ListWorkspaces`, `StatusWorkspace`, `SendTurn`,
`FollowWorkspace`, `DiffWorkspace`, `CloseWorkspace`, `ReopenWorkspace`,
`DeleteWorkspace`, `GetDecision`, `RespondDecision`, and `AuditControlCall`.
MCP tools call their shipped subset through daemon RPC rather than importing
the use cases directly. Retirement remains CLI/RPC-only in v0.

## Local filesystem layout

Use XDG locations on Unix, with standard fallbacks:

```text
${XDG_DATA_HOME:-~/.local/share}/coco/worktrees/<repository-id>/<workspace-name>/
${XDG_DATA_HOME:-~/.local/share}/coco/coco.db
${XDG_DATA_HOME:-~/.local/share}/coco/cocod.lock
${XDG_RUNTIME_DIR}/coco/cocod.sock
${XDG_RUNTIME_DIR}/coco/codex-app-server.json
${XDG_RUNTIME_DIR}/coco/codex-app-server.token
```

Create state/data directories as `0700`; create the daemon lock, database,
socket, App Server endpoint descriptor, and capability token with user-only
access. The daemon acquires the lock before opening SQLite or starting Codex so
a second process cannot replace shared runtime files. If
`XDG_RUNTIME_DIR` is unavailable, a fallback
must be explicitly owned, mode-checked, short enough for Unix socket path
limits, and must not accept other users. Do not put the socket in a broadly
writable directory without an owned `0700` parent.

Canonicalize registered repository and generated worktree paths with
filesystem real paths after creation. Never derive a filesystem path from an
unvalidated workspace name; the opaque repository ID and strictly validated workspace
name form the directory components.

## Client-to-daemon protocol

### Transport

CLI and MCP adapter currently connect to a user-scoped Unix domain socket. The
transport boundary must gain a Windows named-pipe implementation without
changing request methods or coordinator behavior. The daemon accepts a
newline-delimited envelope protocol; transport framing is independent of both
MCP and the Codex App Server protocol.

Request:

```json
{
  "id": "8d67...",
  "method": "workspace.create",
  "params": {
    "repositoryPath": "/projects/app",
    "name": "feat/login",
    "context": {"kind": "fresh"},
    "worktree": {
      "kind": "newBranch",
      "base": {"kind": "revision", "revision": "HEAD"}
    },
    "changes": "reject",
    "profile": "default",
    "model": "gpt-5.6-sol",
    "operationId": "d414..."
  }
}
```

Unary response:

```json
{
  "id": "8d67...",
  "result": {}
}
```

Failure response:

```json
{
  "id": "8d67...",
  "error": {
    "code": "DIRTY_SOURCE",
    "message": "repository checkout is dirty: /projects/app"
  }
}
```

The current protocol has unary requests only. State following polls
`workspace.get` for one workspace or `workspace.list` for a collection; each
call derives current workspace state from a non-loading native `thread/read`.
Current thread state never falls back to the database, and status never reads
conversation history. `send --wait` polls `turn.result` for its exact client
operation ID. The coordinator correlates agent-message and completion
notifications in memory, bounds each response to 1 MiB, and retains only the
latest 256 completed results within an 8 MiB total response budget for the
current daemon generation. This avoids both unbounded native history reads and
persisted copies of Codex output.

Methods for the handed-off commands are:

| Daemon method | CLI | MCP tool |
| --- | --- | --- |
| `repository.register` | `coco repo add` | not exposed |
| `repository.resolve` | interactive client scope resolution | not exposed |
| `repository.list` | `coco repo list` / `repo ls` | not exposed |
| `model.list` | `coco model list` / `model ls` | not exposed |
| `workspace.create` | `coco create` | not exposed in v0 |
| `workspace.close` | `coco close` | not exposed in v0 |
| `workspace.reopen` | `coco reopen` | not exposed in v0 |
| `workspace.delete` | `coco delete` | not exposed in v0 |
| `workspace.list` | `coco list` / `coco ls`, targetless `status`, and interactive selectors | `workspaces.list` |
| `workspace.get` | explicitly targeted `coco status` | `workspaces.status` |
| `workspace.attach` | lease one bound resume or one exclusive fresh `coco jump` | not exposed in v0 |
| `workspace.attach.renew` | renew this TUI client's presence or pending-adoption lease | not exposed in v0 |
| `workspace.attach.adopt` | correlate and adopt the exact materialized fresh-TUI thread | not exposed in v0 |
| `workspace.attach.release` | release only this TUI client's lease | not exposed in v0 |
| `turn.start` | `coco send` | `workspaces.send` when explicitly enabled |
| `turn.result` | `coco send --wait` polling | not exposed in v0 |
| `event.list` | compatibility method with no current CLI consumer | not exposed in v0 |
| `workspace.diff` | `coco diff` | `workspaces.diff` |
| `decision.get` | request lookup for `coco decide <decision-id>` | not exposed in v0 |
| `decision.respond` | `coco decide <decision-id>` | not exposed in v0 |

Repository-aware CLI commands carry one repository path, an explicit
daemon-wide collection scope, or an explicit global-reference scope. An omitted
path means `.`, and a supplied path may point inside a registered worktree;
the daemon canonicalizes it through Git and resolves the stored common-directory
identity. The client never combines a repository path and workspace name into
one opaque selector.

Full workspace IDs resolve globally. Human workspace names resolve within the
selected repository, or across all repositories only when the client
explicitly sends the `--global`/`-g` scope. `--all-repos`/`-a` is instead a
collection scope for `list` and targetless `status`; it never broadcasts a
mutation. The daemon owns
global matching and returns bounded candidate IDs,
names, and repository paths for an ambiguous name; the CLI must not perform a
read-then-guess lookup itself. A local miss may return the same bounded
suggestions but must not silently retarget `send`, `jump`, or another command.
There is no daemon-global mutable "selected repository"; a future GUI may keep
a selection as client view state.

The CLI owns presentation, not repository or workspace resolution semantics.
Its interactive adapter is enabled only when stdin and stderr are terminals
and `--no-input` is absent. Missing-target selectors obtain deterministic
repository/workspace lists from the daemon, render prompts on stderr, and send
the selected stable workspace ID back through the normal unary method. Fully
explicit invocations and JSON output bypass the adapter. `repository.resolve`
exists so the CLI can discover whether implicit `.` already denotes a
registered Git identity without opening the store or reproducing Git
canonicalization. It is not exposed through the control MCP server.

`cli/prompt.rs` separates typed yes/no confirmation from option selection.
Confirmations use canonical line input with a safe No default; native decision
choices are not collapsed into a Boolean. The picker owns a temporary inline
frame through `cli/prompt/terminal.rs`. Real CRLF line feeds reserve and scroll
rows at the bottom margin; relative cursor motion only rewinds the prior frame.
Render at most `min(9, height - 2)` choices, budget text by Unicode display
columns, and keep line wrapping off during the frame. Each update is buffered
before writing. A guard clears the frame and restores wrapping, cursor
visibility, and input mode on selection, cancellation, and errors. This remains
inline, without an alternate-screen takeover or any daemon protocol change.

The `tests/terminal_smoke.py` opt-in probe runs the real shared interaction
adapter inside its own disposable tmux server and shell. It covers a full
terminal, navigation, resizing, cancellation, cursor restoration, and line
confirmation. The ordinary Rust suite separately covers state, layout,
Unicode width, output sequences, and confirmation authorization. The probe
uses no real workspace, daemon, Codex process, or model.

`operationId` is required inside mutating method parameters. Creation stores
it on the workspace; turn start stores it in the v6 operation ledger.
Repeating an accepted ID with identical parameters returns the prior result.
Repeating an unconfirmed ID returns `OPERATION_UNCERTAIN` without dispatch;
reusing any ID with different parameters returns `IDEMPOTENCY_CONFLICT`. The
CLI generates IDs, prints one in send-error context, and accepts
`--operation-id` only for deliberately replaying that exact send.
Authorization derives from the secured local transport and explicitly enabled
adapter capabilities; the current envelope carries no client identity claim.

### CoCo MCP adapter

`coco mcp serve --repository <path>` starts a local MCP server over stdio and
connects to `cocod` as another client. It fixes repository scope at startup,
reserves stdout for MCP frames, logs only to stderr, and exits with a clear MCP
error if the daemon is unavailable. It never opens SQLite, invokes Git, spawns
the Codex App Server, or implements fallback orchestration.

The default server advertises five read-only tools:

```text
workspaces.list
workspaces.status
workspaces.diff
signals.types
signals.list
```

Starting it with `--allow-send` additionally advertises `workspaces.send`. That
tool requires workspace and message, accepts an optional caller operation ID,
and delegates to `turn.start` with a generated ID when omitted.
The flag is an explicit operator capability grant; v0 exposes no approval,
cleanup, repository-registration, arbitrary-command, A2A ask, or integration
tool.

Repeated `--allow-emit NAME@VERSION` grants opt into `signals.emit` for exact
selected versions (a bare name means version 1). `--signal-catalog` selects an
operator-owned directory of plain `NAME@VERSION.json` schemas at MCP startup.
The daemon validates and atomically stores immutable snapshots; workers have no
registration tool. Native Codex `_meta.threadId` resolves the emitting workspace;
no model argument chooses the sender. Discovery distinguishes selected or
historically retained definitions from this instance's exact emission grants.
This is a small extension of control MCP, not the deferred worker-MCP catalog.
The instance-filtered router is used by the actual `ServerHandler`, including
discovery and invocation; constructing a fresh default router would discard
these opt-ins. See the [signal contract](signals.md).

Tool input and structured result schemas are owned by the MCP adapter but reuse
the semantic fields of daemon DTOs. Convert daemon errors into bounded MCP tool
errors without changing their stable code. Cancellation stops waiting for the
tool result where safe; it does not silently interrupt an already accepted
turn.

The current adapter records sanitized `AuditRecordParams` for requests routed
through its daemon dispatcher: tool action, fixed repository path, optional
workspace/operation ID, outcome, and an error code when applicable. Neither
request message text nor signal payload is copied into this audit record.
Denied routes or missing adapter metadata can be rejected before dispatch.
During the native-first migration this store is retained only while a concrete security, diagnostics,
or external consumer requires it; the existence of MCP alone does not require
durable auditing of every read.

This adapter enables an operator-controlled MCP host to coordinate CoCo and
lets bound Codex workspaces publish explicitly granted claims. It is not an
A2A mailbox: workspaces cannot directly address other threads, await correlated
responses, or gain delegated merge authority through signals.

### Error taxonomy

Errors use stable machine codes while messages remain human-oriented:

```text
METHOD_NOT_FOUND
INVALID_PARAMS
REPOSITORY_NOT_REGISTERED
WORKSPACE_EXISTS
WORKSPACE_NOT_FOUND
WORKSPACE_REFERENCE_AMBIGUOUS
WORKSPACE_BUSY
WORKSPACE_RETIREMENT_BLOCKED
ATTACH_LEASE_INVALID
CONTEXT_REFERENCE_UNRESOLVED
IDEMPOTENCY_CONFLICT
OPERATION_UNCERTAIN
INVALID_WORKSPACE_STATE
INCOMPLETE_WORKSPACE
PROFILE_CHANGED
DIRTY_SOURCE
INVALID_WORKSPACE_NAME
WORKSPACE_COLLISION
NOT_A_GIT_REPOSITORY
GIT_ERROR
NOT_FOUND
PROFILE_NOT_FOUND
INVALID_PROFILE
CODEX_ERROR
INTERNAL
```

Do not expose raw SQL errors, authentication material, complete environments,
or unredacted child-process stderr through structured details.

## Current SQLite model and target boundary

SQLite uses foreign keys, WAL mode, an explicit busy timeout, and ordered
migrations. `cocod` is the sole application writer. Store timestamps as UTC
Unix milliseconds and JSON as validated text. IDs are opaque UUIDs; no user
meaning is encoded in them. Creation idempotency remains on the workspace;
schema v6 introduced turn-start dispatch facts in the dedicated `operations`
table, schema v7 added the typed worktree binding, schema v8 added reversible
availability, and schema v9 records in-progress deletion intent. Listing a
legacy native projection below does not assign CoCo long-term ownership of it.

### `repositories`

| Column | Constraint and meaning |
| --- | --- |
| `id` | primary key |
| `root_path` | unique canonical registered worktree root |
| `git_common_dir` | canonical common Git directory observed at registration |
| `display_name` | basename used for rendering only |
| `is_linked_worktree` | whether registration occurred through a linked worktree |
| `created_at_ms`, `updated_at_ms` | required timestamps |

### `workspaces`

| Column | Constraint and meaning |
| --- | --- |
| `id` | primary key |
| `create_operation_id` | unique client-generated creation idempotency key |
| `repository_id` | required foreign key |
| `name` | required; unique with `repository_id` |
| `legacy_goal` | compatibility-only copy from the prerelease v1 schema; omitted from the domain/API and never written for new workspaces |
| `context_mode` | implemented `fresh` or `fork`, with `handoff` reserved |
| `context_json` | versioned independent base/context/worktree/local-state provenance, not conversation history or copied file contents |
| `profile_json` | immutable execution-profile snapshot: name, optional named-file path and parsed-configuration hash, separate model override, and non-secret effective settings; never the complete overlay |
| `lifecycle` | CoCo-owned `provisioning`, `starting`, `ready`, `completed`, or `failed` |
| `availability` | independent CoCo-owned `open`, `closing`, `closed`, `reopening`, or `deleting` resource state |
| `thread_status_json` | legacy nullable native `ThreadStatus`; no production writer remains |
| `thread_status_generation` | legacy App Server generation; no production writer remains |
| `thread_status_observed_at_ms` | legacy observation time; no production writer remains |
| `thread_status_is_fresh` | legacy freshness bit; startup/disconnect may only stale old rows |
| `worktree_mode` | required `new_branch`, `existing_branch`, or `detached` binding |
| `branch_name` | branch for branch-backed modes; null for detached |
| `base_sha` | complete immutable commit object ID |
| `worktree_path` | globally unique canonical path after creation |
| `codex_thread_id` | globally unique nullable binding during provisioning |
| `parent_thread_id` | source thread for a native fork; null for fresh context |
| `active_turn_id` | legacy local-turn correlation; migration clears it and production never writes it |
| `last_error_code`, `last_error_message` | nullable sanitized terminal detail |
| `created_at_ms`, `updated_at_ms`, `completed_at_ms` | lifecycle timestamps |
| `thread_archived` | whether the current close operation requested and established native archival; cleared atomically when reopening reaches `open` |
| `closed_head_sha` | exact worktree `HEAD` captured before close and required for exact restoration and optional owned-branch deletion |
| `closed_at_ms` | nullable timestamp for the stable `closed` state |
| `delete_thread_requested`, `delete_branch_requested` | internal saga intent retained only while availability is `deleting`; not public workspace metadata |

Base and path are nullable only while Git provisioning has not reached their
stage. The thread remains nullable after lifecycle `ready`: that represents a
fully prepared Git workspace whose native context has not yet been activated.
A `ready` workspace requires a complete, mode-consistent Git binding; detached
mode deliberately has no branch. `phase` and `waitReasons` are computed on
read and deliberately have no workspace table columns. Availability is
orthogonal to provisioning lifecycle: a `ready` workspace may be open, closed,
or in one of the durable retirement transitions. Stable `open` rows have no
close-time evidence, while stable `closed` rows retain the exact path, mode,
branch, base, close-time `HEAD`, and optional thread binding needed to reopen.

Repository display names are not selectors because unrelated paths may share a
basename. CLI repository scope is a canonicalizable path. The stable repository
ID is returned for identity and correlation but is not currently a CLI path
selector.

### Context transfer

The existing `fresh`, `fork`, and `handoff` values describe context provenance,
not different ways to copy code or attach arbitrary workspace metadata.
`resume` is deliberately outside this enum because it reconnects the same
Codex thread rather than creating a workspace with derived context.

- `fresh` creates a new thread only when the first `send` or fresh `jump`
  activates the prepared workspace.
- `fork` calls native `thread/fork` on first activation with either a selected workspace's exact
  thread or a directly supplied native `thread.id`, plus the new canonical
  `cwd` and configuration. CoCo records exact parent/source provenance.
  Every CoCo-started turn on a fork also supplies an application
  `additionalContext` entry containing the destination worktree, optional
  branch, base SHA, and source relation so inherited history cannot make the
  agent assume it still operates in the source checkout. `coco jump`
  separately starts the official TUI with that worktree as `-C`.
- `handoff` should create a fresh thread from bounded, human-reviewable
  transfer material without requiring one authoring mechanism. A source may
  be an agent-authored artifact, existing Markdown, direct operator input, or
  a snapshotted or linked external reference. A plan can be part of the
  material without becoming mandatory handoff structure. Producing, reviewing,
  storing, attaching, and consuming the material are separate operations.

The implemented context slice is independent from Git-base selection.
`--context`/`-c` resolves a bound workspace in the destination repository
first, then falls back to an exact readable native thread ID; `workspace:` and
`thread:` prefixes force an interpretation. The daemon retains distinct typed
resolution paths after this public input is normalized. Both accept only
native `idle` or `notLoaded` and always fork a child into the new canonical
`cwd`; neither adopts, moves, or mutates the source. `--base-workspace`
separately resolves committed code in the destination repository.

A caller may explicitly request compaction; on first activation CoCo calls
`thread/compact/start` only on the returned child thread and waits for its
terminal compaction lifecycle before accepting an initial turn or opening the
child TUI. Compaction is a modifier stored in the fork descriptor, not a new
`ContextMode`, and must not be selected heuristically. A failed compact step
preserves the worktree and bound child for diagnosis and marks the CoCo
lifecycle failed.

The future handoff design must define redaction and byte limits, immutable
hashes and provenance, regeneration/idempotency behavior, reference freshness,
and what happens when material is unavailable or cannot be generated. One
possible generated flow is a read-only ephemeral `thread/fork` at a completed
source turn, followed by a dedicated authoring prompt whose final response CoCo
saves outside the source worktree. A thread fork alone is not filesystem
isolation: if it retains the same `cwd`, file writes would still affect the
source worktree. Generating an actual file therefore requires either a separate
throwaway Git worktree or CoCo-owned persistence of returned text.

Native `turn/start.additionalContext` is a possible delivery field for a
snapshotted artifact, not yet a selected storage or wire contract. A destination
that receives only a handoff starts a fresh Codex thread; using native
`thread/fork` for that destination would also inherit the full conversation and
therefore has different semantics. No mode implicitly transfers dirty files;
Git state and model context remain independent dimensions.

### `operations`

| Column | Constraint and meaning |
| --- | --- |
| `id` | opaque local correlation ID |
| `operation_id` | unique client-generated idempotency key |
| `workspace_id` | required foreign key |
| `kind` | currently only `turn_start` |
| `request_fingerprint` | unique one-way hash of operation/workspace/message; prompt text is never retained |
| `native_result_id` | unique Codex turn ID, present only after proven acceptance |
| `state` | `prepared`, `dispatching`, `accepted`, or `uncertain` |
| `created_at_ms`, `dispatch_started_at_ms`, `result_recorded_at_ms` | state-machine timestamps |

`prepared` commits before any App Server side effect. `dispatching` commits
immediately before `turn/start`. Only the direct request response can move the
operation to `accepted`: the pinned and current App Server `turn/started`
notification contains a native turn ID but does not echo
`clientUserMessageId`, so thread proximity is not exact correlation. A lost
response or restart moves `dispatching` to `uncertain`; that operation ID is
never dispatched again automatically. The current-generation in-memory guard
prevents a transient native `idle` read from authorizing a second turn while
the request is unresolved.

### Legacy `turns`

Schema v6 retains the schema-v5 `turns` table and its foreign-key targets for
one reversible migration revision. Migration copies only rows with a client
operation ID into `operations`, treats a native turn ID as prior acceptance,
marks unconfirmed rows uncertain, interrupts old in-progress rows, and clears
legacy `active_turn_id`. Production creates or updates no turn row; Codex owns
turn lifecycle and history.

### `events`

| Column | Constraint and meaning |
| --- | --- |
| `sequence` | integer primary key/autoincrement; replay cursor |
| `id` | globally unique event ID |
| `workspace_id`, `turn_id` | optional workspace/local-turn foreign keys; null when an event cannot be correlated |
| `kind` | normalized, versioned CoCo event kind |
| `source` | `coco`, `git`, or `codex` |
| `source_method` | nullable original App Server method/internal action |
| `occurred_at_ms` | source time when available |
| `recorded_at_ms` | daemon receipt/commit time |
| `payload_json` | versioned normalized payload |

The current runtime persists CoCo provisioning/failure/context events. It no
longer writes user prompts, completed agent messages, local/native turn start
or completion, plan, diff, native status/error, decision, or unsupported-
server-request events. The table and cursor order remain a compatibility
surface for older data and `event.list`; they are not a second canonical
thread history and have no current CLI consumer.

### `audit_events`

The current control-MCP path uses a separate append-only audit table with a
monotonic sequence, opaque ID, source, action, optional workspace and operation
IDs, outcome, sanitized JSON details, and occurrence time. Raw prompt text is
not copied into this table. Retention in the target schema requires a concrete
security, diagnostics, or external consumer.

### Live decisions and the legacy `decisions` table

Current decisions live in a mutex-protected registry owned by one Coordinator
and App Server generation. Each runtime entry contains the public bounded
presentation, workspace and optional local-turn correlation, exact native
thread/request correlation, and exact offered native response values. The
public opaque ID is the only selector accepted by `coco decide`.

Submission validates the answer and changes `pending` to `submitted` under the
registry lock before awaiting the native write, so concurrent clients cannot
both respond. `serverRequest/resolved` supplies final confirmation. A failed
native write or App Server disconnect orphans the entry. A daemon restart drops
the registry entirely because the old callback is no longer answerable; CoCo
never replays it against a new generation. Raw user-input answers are neither
stored in the registry after response construction nor written to disk.

Schema v6 still contains this legacy bridge table so existing prerelease
databases remain readable and a later migration can remove it deliberately:

| Column | Constraint and meaning |
| --- | --- |
| `id` | stable CoCo decision ID shown to clients |
| `workspace_id`, `turn_id` | workspace and nullable local-turn correlation |
| `codex_thread_id`, `codex_turn_id` | exact native correlation |
| `runtime_generation` | identifies the owning App Server process generation |
| `native_request_id_json` | exact typed App Server request ID serialization; never projected publicly |
| `method`, `kind` | native source method and supported command approval, file-change approval, or user input kind |
| `state` | `pending`, `submitted`, `resolved`, or `orphaned` |
| `prompt_json` | bounded presentation model containing no environment or credential fields |
| `native_options_json` | exact private native approval values in display order |
| `response_summary_json` | answer-free submission metadata; raw user-input answers are never persisted |
| `received_at_ms`, `submitted_at_ms`, `resolved_at_ms` | lifecycle timestamps |

Production code no longer inserts, reads, transitions, or orphans these rows.
Their old bounded contents remain migration input only. Runtime tests now prove
the replacement's exact-option forwarding, concurrent single-response
transition, disconnect orphaning, restart loss, and secret-redaction behavior.

### Deferred workspace annotations and external references

Do not expose a generic `goal` or `metadata` field merely to hold unrelated
values. A later workspace-annotation design may cover human notes and structured
references such as tickets, pull requests, or URLs, but it first needs explicit
contracts for:

- typed versus user-defined keys and validation of links or identifiers;
- mutation, history, audit, and conflict behavior;
- privacy, redaction, search, and presentation across CLI and future clients;
- copy/inheritance behavior for fork, handoff, and detached-worktree promotion;
- whether a value is ever projected into Codex context (default: never).

Until that design exists, workspace creation accepts only operational fields. A
prerelease v1 `goal` column is migrated to a hidden `legacy_goal` compatibility
column so existing local data is not destroyed, but it is not part of the workspace
model or a foundation for the future schema.

### Schema additions to avoid initially

Do not add profile CRUD, A2A messages, workspace dependencies, merge requests,
Codex App Server pools, UI sessions, or a raw copy of all Codex history in the
native-first migration. Do not preserve normalized events merely to support
MCP call auditing. Add a narrow transactional outbox only when an implemented
hook or replay consumer establishes its delivery and retention contract.

### Native-first persistence target and removal gates

The target store contains repository identity, the stable CoCo workspace alias
and ID, Git worktree ownership/base/branch, the Codex thread binding,
provisioning and retained failure evidence, idempotent operation
intent/result correlation, and only profile/context provenance required to
reconstruct a verified binding. Native fields are removed only when the
selected released Codex build supplies an equivalent read after restart.

| Current persisted state | Removal gate |
| --- | --- |
| `thread_status_*` | stop-write achieved: `thread/read` covers current projections and no production path creates a snapshot; retain columns one revision before physical removal |
| turn status/history and `active_turn_id` | stop-write achieved: the v6 operation ledger prevents automatic duplicate dispatch and native reads supply current state; retain the old table/column one revision before physical removal |
| completed messages and normalized Codex events | current-status reads no longer consume them; completed text additionally needs a stable bounded native replacement or an explicitly reduced follow contract, and no implemented replay consumer may remain |
| generation-bound decision rows | achieved for production writes: the in-memory registry passes exact-option, concurrent single-response, disconnect, restart-loss, and secret-redaction tests; the old table remains migration-only |
| redacted effective profile fields | native resume preserves effective configuration without them; otherwise retain only the minimal requested overlay provenance proven necessary by compatibility tests |
| fork parent fields already returned by Codex | native fork provenance remains queryable while CoCo separately retains the code-base/source-workspace relation it alone owns |
| control-read audit events | an explicit security or diagnostics consumer does not require them |
| `legacy_goal` | all supported prerelease database fixtures migrate without exposing or losing user-owned data |

Migrations stop writing a removable field before dropping it. Old columns may
remain readable for one schema revision, and physical deletion is a separate
change after migration fixtures pass. A removed native projection must not be
replaced by another differently named mirror.

## Transaction and transition rules

Schema v9 uses short `BEGIN IMMEDIATE` transactions for each CoCo-owned state
transition:

1. Persist and validate the owned intent or lifecycle precondition.
2. Apply the operation or binding change.
3. Insert a durable event only when the changed fact has a current consumer.
4. Commit before or after the external saga step as its state machine requires.
5. Return the committed owned state plus a fresh native projection.

The transaction covers the CoCo-owned binding, provisioning state, and
idempotent operation record; `turn_start` transitions do not insert a
normalized native event. If a later hook requires durable delivery, its
narrowly scoped outbox row commits in that same transaction and is
acknowledged independently.

### Workspace lifecycle and thread runtime

Schema v3 introduced state ownership separation, schema v4 renamed the
aggregate, schema v5 added the now-retired durable decision shape, schema v6
added the operation ledger, schema v7 records the typed worktree mode, schema
v8 adds availability and close evidence, and schema v9 makes permanent-delete
intent recoverable. The current alpha writes the following provisioning
lifecycle:

| From | Trigger | To | Durable event/effect |
| --- | --- | --- | --- |
| absent | accepted `workspace.create` | `provisioning` | `workspace.created` |
| `provisioning` | worktree verified | `ready` | `worktree.created`; native thread remains unbound |
| `ready` without a thread | first accepted fresh turn or verified TUI materialization | `ready` | bind exact thread and emit `agent.started` |
| `ready` without a thread | native context fork requiring compaction | `starting` | bind exact child and start compaction |
| `starting` | child compaction accepted | `ready` | `context.compacted` |
| `provisioning` or `starting` | unrecoverable saga error | `failed` | `agent.failed` with retained artifacts |

`completed` remains reserved for a future explicit workspace operation. A
completed, failed, or interrupted Codex turn does not mutate the workspace
lifecycle or any local turn row. A later send is permitted when Codex reports
the thread `idle` and no current-generation operation guard remains.

The native-first target retains this lifecycle only as CoCo provisioning
truth. `ready` means the repository and worktree are usable; the optional
thread binding says separately whether native conversation context has been
materialized. It does not mean Codex is currently idle or able to accept a
turn. Native turn completion, interruption, and availability never become
stored workspace lifecycle transitions. Local turn rows and persisted
`active_turn_id` are now migration-only; schema v9 retains schema v6's send
idempotency in `operations` and keeps the short concurrency guard in memory.

Availability is a separate state machine for the managed worktree and retained
workspace identity:

| From | Trigger | To | Durable effect and convergence |
| --- | --- | --- | --- |
| `open` | accepted checked close | `closing` | store close-time `HEAD` and whether native archival was requested before any external side effect |
| `closing` | verified worktree absent and requested archive established | `closed` | retain workspace, Git binding, branch, and optional native thread |
| `closed` | accepted reopen | `reopening` | recreate the exact managed path and binding at the close-time `HEAD` |
| `reopening` | binding verified and CoCo-archived thread unarchived | `open` | atomically clear close-only evidence and archive marker |
| `closed` | accepted permanent delete | `deleting` | retain independent thread/branch deletion intent until its saga completes |
| `deleting` | requested native/branch effects complete | absent | delete the CoCo record and its compatibility children |

Close and reopen failures are compensated to their prior stable state when the
observed filesystem and native thread permit it. Startup examines every
`closing`, `reopening`, and `deleting` row after the App Server is initialized,
then converges from the actual Git path/registration and native thread state.
It never invents a replacement binding. A proven native or branch deletion
rejection returns a `deleting` row to `closed`; an ambiguous native result or a
crash between proven side effects remains retryable from the stored intent.

The thread-runtime truth is the exact `ThreadStatus` returned by stable
`thread/read`: `notLoaded`, `idle`, `systemError`, or `active` with the complete
active-flag set. Exact retirement lookup also uses `thread/read`, because
`thread/list` applies source filters that omit App-Server-origin roots. The
returned rollout path distinguishes active `sessions` from native
`archived_sessions`; a missing path is insufficient evidence and blocks the
operation. CoCo never opens or mutates the rollout itself. Start/resume
responses are projected in memory only;
`thread/status/changed` is drained but not persisted or treated as a parallel
authority. Passive `list`/`ls`, `status`, and `event.list` perform a fresh,
non-loading read, validate the returned thread ID and canonical `cwd`, and
derive the public projection in memory. Failure to obtain or validate native
truth projects `unavailable`, never the last SQLite value as current.

Legacy snapshots retain their old App Server generation and receipt time only
as migration data. Startup or connection loss can mark such rows stale, but
passive reads never serve them as current and no production path creates a new
snapshot. Physical column removal remains a later schema-only checkpoint.

Server requests are a separate fact. Supported decision requests are registered
only in the generation-local registry; unsupported requests are warned and
left unanswered. Neither path changes runtime state based on the method name.
Only native status reads determine wait flags.

The public `phase` and `waitReasons` fields are computed on each projection:

1. non-open availability projects directly to `closing`, `closed`,
   `reopening`, or `deleting`, with no native wait reasons;
2. a non-`ready` CoCo lifecycle projects directly to `provisioning`,
   `starting`, `completed`, or `failed`;
3. a `ready` workspace without a native thread projects to `prepared`;
4. a missing or stale current native observation for a bound thread projects
   to `unavailable`;
5. native `notLoaded` and `systemError` project without reinterpretation;
6. native `active` projects approval wait before user-input wait, then plain
   `active`, while retaining both known waits and every native flag;
7. an accepted correlated in-flight operation keeps the summary `active`
   during the short interval in which a fresh native read still reports
   `idle`; an unconfirmed dispatch projects `unavailable` until its direct
   response is known or the generation is lost;
8. fresh native `idle` with no in-flight operation projects to `idle`.

Git facets never affect this projection. The database migration retains old
v2 native-like phases as stale snapshots; it does not pretend that a status
observed by an earlier App Server generation is current.

## Git adapter

Invoke a known `git` executable directly with argument arrays and a controlled
working directory. Capture exit status, bounded stdout/stderr, and command
category, not shell-expanded commands.

### Registration and validation

- Obtain the top level and common Git directory from Git, then canonicalize.
- Record whether the supplied checkout is itself a linked worktree.
- Source cleanliness for the default `create` path uses porcelain status
  including ordinary untracked files. Explicit local-state carry is evaluated
  only against the checkout through which creation was invoked; dirty state in
  unrelated linked worktrees does not participate.
- Resolve the base as a commit object with the equivalent of
  `rev-parse --verify --end-of-options <ref>^{commit}` and persist its complete
  output.
- Accept slash-separated workspace names such as `feat/login`, but validate every
  component before using it in a filesystem path: 1-63 total bytes, non-empty
  lowercase ASCII alphanumeric/`-` components, and alphanumeric component
  boundaries. Reject traversal, leading/trailing/repeated separators, and dot
  components.
- Validate every newly allocated branch (the default `coco/<name>` or an
  explicit `--branch`) using Git's ref-format validation and separately check
  exact and ref-prefix collisions. An existing branch must resolve to a commit
  and must not be checked out by another worktree. Securely create and
  canonicalize any intermediate worktree directories below the repository's
  allocated CoCo root.

### Worktree creation

Under a repository-scoped mutex, create one of three typed bindings with native
`git worktree add`: a new branch at the resolved SHA, an existing free local
branch at its resolved SHA, or `--detach` at the resolved SHA. After success
verify:

- the canonical path is the allocated destination;
- `HEAD` equals `base_sha`;
- symbolic `HEAD` equals the recorded branch for branch-backed modes and is
  absent for detached mode; and
- the worktree appears in Git's porcelain worktree listing.

Persist the mode and optional branch with the binding. On later failure, report
the path and any branch but do not automatically remove either. Detached is a
Git-binding choice, not a Codex context mode, and it creates a normal
registered worktree rather than an ephemeral directory. A later atomic
promotion operation must create and verify a branch before changing persisted
binding metadata; it is not part of the current alpha.

### Explicit local-state carry

The no-carry default rejects tracked and ordinary untracked changes. The typed
carry request may add two independent source-checkout categories:

- staged and unstaged tracked changes, captured as separate full-index binary
  patches so destination index placement is preserved; and
- ordinary non-ignored untracked files, valid only together with tracked carry.

`--dirty` is CLI normalization for both categories, not a persisted mode.
Local-state carry is legal with new-branch, existing-branch, or detached
bindings, but only when the resolved `base_sha` equals the invoking checkout's
`HEAD`. The source is read-only throughout: CoCo never stashes, resets,
stages, moves, or deletes its files.

A repository-root `.worktreeinclude` is a separate Codex-compatible convention
for ignored local setup files and applies to every managed worktree. Git first
confirms every match is ignored; an ignored root `AGENTS.override.md` is
included automatically. Ordinary untracked and ignored selections remain
disjoint. Both copy paths reject traversal and destination overwrites, skip
source symlinks and non-files, create private directories/files, and share
strict count, per-file, aggregate-byte, and command-output limits. File
contents exist only in the in-memory provisioning snapshot. Durable provenance
contains source/base identity, counts, sizes, and a category-separated hash,
never file contents.

### Inspection and diff

At query time collect:

- canonical path, worktree mode, optional branch, and `HEAD` for binding
  verification;
- porcelain status for staged, unstaged, and untracked state;
- left/right commit counts and ancestry relative to `base_sha`;
- a binary-capable tracked patch from `base_sha` through index/worktree; and
  an explicit untracked path list.

Bound captured output and return a structured truncation marker for extreme
diffs rather than exhausting daemon memory. `coco diff` is read-only and never
stages untracked files to make them appear in a patch.

### Checked retirement and restoration

Under the same repository mutex used for creation and activation, close first
verifies the stored mode, branch, canonical managed path, Git registration,
and current `HEAD`. Its plan separately reports tracked changes, ordinary
untracked files, ignored files, a worktree lock, and detached commits relative
to the immutable base. A lock, binding drift, or a detached `HEAD` beyond the
base always blocks. Any local file state blocks unless the caller explicitly
requests `--discard-changes`.

The stored path must equal the destination derived from the canonical managed
root, repository ID, and workspace name. Existing descendants of that root
must be real directories, not symlinks. Binding verification refuses a supplied
path whose canonical form differs, so a replaced path cannot redirect removal
to a different worktree in the same repository. Recovery and reopen apply the
same managed-path check before any native or Git effect.

CLI confirmation pins the previewed workspace ID and returns its typed plan
as `expectedPlan` on the apply request. The coordinator recomputes and compares
the resource identities, close-time/current `HEAD`, selected effects, and
displayed risk counts before applying. Changed names or plans require a new
review; this is not a lock against independent Git or native clients, so final
effect-time checks still apply.

After CoCo persists `closing` and completes the relevant native-thread step,
the Git adapter repeats binding, `HEAD`, lock, and full local-state validation
immediately before removal. It invokes only `git worktree remove -- <path>`;
the `--force` argument appears only for an explicit discard. It never performs
a recursive filesystem deletion, never removes the branch as a close side
effect, and verifies that both the path and Git's worktree record are absent
after success.

Reopen derives the destination again from the managed worktree root,
repository ID, and validated workspace name. It refuses an occupied path,
moved or separately checked-out branch, changed mode, or close-time `HEAD`
mismatch, then uses `git worktree add` and verifies the restored binding. A
detached workspace is restored detached at its close-time commit. Optional
permanent branch deletion is available only for `new_branch`, after the
worktree is closed, and only while the ref still equals the recorded
close-time `HEAD`; an existing branch or detached binding can never acquire
that authority.

## Codex App Server adapter

### Protocol compatibility boundary

Generated App Server schemas are an upgrade-review aid, not a production input
or a byte-for-byte compatibility gate. To inspect a candidate Codex release,
run:

```bash
codex app-server generate-json-schema --experimental --out ./schema/codex-app-server
```

Review only the methods and stable fields CoCo consumes. The Rust adapter uses
narrow hand-owned projections rather than generated protocol types, so
additive upstream definitions must not block the real-process test. Concrete
request/response behavior is authoritative; removed methods, rejected params,
or missing/changed consumed fields fail the compatibility test.

The selected released build is Codex 0.153.4. Its real process behavior proves
native non-subscribing thread reads, history hydration, exact resume after an
App Server restart, and the remote attachment path CoCo consumes. A
documentation page, an upstream merge, or byte-for-byte equality of an
additive generated schema is not release evidence. A broader compatibility
range remains optional; every claimed version needs the same behavioral gate.

The committed bundle includes experimental fields so CoCo can audit the
remote-attachment surface it depends on. The daemon client advertises
`experimentalApi` because inherited-context preparation requires
`thread/fork.deferGoalContinuation`; this preserves CoCo's create-without-start
contract instead of letting Codex continue the inherited goal immediately.
Experimental fields remain opt-in by necessity rather than convenience. In
particular, omit `thread/start.runtimeWorkspaceRoots` when it would merely
repeat `cwd`; Codex already defaults the runtime root to that directory, and do
not adopt experimental pagination merely to mirror native history.

The verified 0.153.4 runtime uses this minimal sequence:

1. Generate a high-entropy capability token in a user-only runtime file.
2. Reserve an IPv4-loopback port and spawn `codex app-server --listen
   ws://127.0.0.1:<port> --ws-auth capability-token --ws-token-file <path>`,
   with stdout closed and stderr captured as bounded diagnostics.
3. Connect with `Authorization: Bearer <token>`, send `initialize`, await its
   response, and send the `initialized` notification.
4. Publish the selected loopback URL in a separate user-only descriptor only
   after initialization succeeds. Never persist the token in SQLite or workspace
   metadata.
5. `workspace.create` persists the selected profile/model provenance and
   verified Git binding, but deliberately creates no empty native thread.
   A `ready` workspace without a thread projects publicly as `prepared`.
6. On the first fresh `send`, resolve `default` to an empty `config` object or
   parse the complete `$CODEX_HOME/<name>.config.toml` overlay, call persistent
   `thread/start`, and immediately dispatch the real `turn/start`. Persist the
   turn intent before that durable action and install a generation-local guard.
   Only the direct native turn response allows one SQLite transaction to bind
   the exact thread and accept the operation. A rejected or unconfirmed
   dispatch never triggers an automatic retry; if Codex already materialized
   the rollout, preserve the exact validated binding for diagnosis and later
   resume.
7. On the first activation of inherited context, revalidate the recorded source
   thread and call native `thread/fork` with the independently selected
   destination `cwd`; optional compaction completes before the first message.
8. On later `send`, read and validate the bound thread, resume it only when the
   current daemon connection lacks a subscription, and issue `turn/start` with
   the same canonical `cwd` and a client message ID.
9. Never synthesize a model from profile contents. Supply the explicit model
   in the native `model` field, retain the profile overlay in `config`, and let
   Codex report the effective model. Use paginated `model/list` for discovery.
10. Correlate responses, notifications, and server-initiated requests by the
    consumed protocol fields; map only understood semantics into CoCo state.

For an already bound workspace, `coco jump` reads the user-only endpoint and
token, then launches `codex resume <thread-id> --remote <url>
--remote-auth-token-env <name> -C <worktree>`. The token is supplied only in
the child environment. A renewable per-client presence lease prevents
retirement while this TUI remains open, without excluding another bound TUI
or `send` on an idle thread.

For an unbound fresh workspace, `workspace.attach` acquires one expiring
generation-local lease and returns no invented thread ID. The CLI starts a
one-use authenticated loopback WebSocket relay to the daemon's App Server and
launches the official TUI with `codex --remote <relay> -C <worktree>`, plus the
stored profile and model options. The relay correlates the exact
`thread/start` request/response pair. It does not bind an empty candidate. Once
that candidate receives `turn/start`, `thread/shellCommand`, or `review/start`,
the daemon verifies through exact `thread/read` that Codex reports a non-empty
regular rollout file, the expected `cwd`, no fork parent, and the same thread
ID. It then names and binds that thread and resumes it on the daemon connection
so background observation survives TUI exit. Candidate substitution,
concurrent first activation, and stale leases are rejected. Relay startup
failure and App Server disconnect release or clear the lease. While the relay
is alive it renews the lease every ten seconds; if the CLI or relay dies, the
missing heartbeat lets the daemon expire it after thirty seconds.
Binding converts the exclusive adoption lease into ordinary TUI presence;
the relay continues its heartbeat until exit.

The normal remote-TUI exit path sends `thread/unsubscribe` and closes only its
client WebSocket; it does not send `turn/interrupt`. Because the daemon has
subscribed after binding, `/quit` and `/exit` detach from an accepted active
turn while explicit Codex interruption remains cancellation. Process coverage
proves empty exit, exact adoption, close-without-interrupt, later completion,
and profile/model/cwd propagation.

Daemon startup no longer resumes every stored thread. Passive reads call
`thread/read` with the stored thread ID and reject a returned ID or canonical
`cwd` that conflicts with the CoCo binding; they do not apply configuration or
subscribe the new daemon connection. A missing native thread or mismatched
binding projects unavailable and never creates a replacement.

An operation that requires a live connection subscription activates a bound
thread on demand. Later `send` and `workspace.attach` for bound `jump` first
read and validate the workspace binding, then call
`thread/resume` when Codex reports `notLoaded` or the current daemon generation
has not yet subscribed to that thread. Resume reparses the named profile and
verifies its name, source path, and parsed-configuration hash, supplies the
stored canonical worktree and explicit model override, and accepts only a
matching returned thread ID and `cwd`. `default` remains the empty overlay.
The first activation of inherited context calls `thread/fork` directly for a
validated idle or unloaded source; the returned child establishes its own
subscription. Successful start, fork, adoption, or resume records that
child/workspace subscription only in the current daemon generation; a globally
loaded status alone does not prove this connection receives notifications or
server requests. Fork and resume set `excludeTurns: true`: Codex still copies
or loads the complete conversation, but returns only the metadata CoCo needs
instead of serializing the full turn array across the shared WebSocket. This
keeps long histories native-owned and prevents otherwise valid activation from
failing at the transport frame limit.

Workspace retirement reuses native lifecycle methods rather than editing Codex
storage. A close preflight allows only `idle` or `notLoaded`, rejects an active
or system-error status, generation-local active/uncertain turns, pending
server requests, both fresh and resume TUI attach leases, and live background
terminals. Normal close also inspects descendants: active/error state or
background terminals in the same worktree block removal, while an unloaded
descendant or one verified in an independent directory does not. A descendant
that cannot be inspected fails closed. `thread/backgroundTerminals/list` is queried only for a loaded
thread because Codex rejects that call for an unloaded one; `notLoaded` already
means no live in-process terminal exists. CoCo unsubscribes its own connection
before removing the worktree. It does not wait for native idle eviction and
does not imply interruption. An already archived native thread blocks an
ordinary close because reopen would otherwise lack authority to unarchive it;
explicit `close --archive-thread` adopts that restoration obligation.

Optional close-time archival calls `thread/archive`, and reopen calls
`thread/unarchive` only when CoCo recorded that archival intent. Exact
`thread/read` validates the stored ID and `cwd` before either operation.
Archive state is inferred from the returned rollout path using the same
`archived_sessions` location that Codex uses internally; pathless results fail
closed. Before archive or delete, paginated `thread/list` queries both active
and archived descendants because those native operations are recursive. CoCo
also blocks native deletion when another stored workspace records the target
as its context parent, including an unmaterialized fork's saved native-thread
source. Record-only deletion protects pending workspace-ID sources too, but
does not block a raw-thread source whose native thread is retained. Runtime
and descendant guards run again before worktree removal and before recovery
archives a thread; recovery cannot acquire broader authority than a normal
request. Unknown external dependencies remain protected by
Codex's own `thread/delete` rejection. A successful native delete atomically
clears the binding before later branch/record steps; a rejection followed by
an exact read that still finds the thread returns the workspace to stable
`closed`. If the write result and follow-up read are both unavailable, the row
remains `deleting` for safe startup reconciliation rather than claiming either
outcome.

### Current notification mapping and target

| App Server input consumed from the selected release | CoCo handling |
| --- | --- |
| `thread/status/changed` | drain without persistence; `thread/read` remains current-state authority |
| `turn/started` | drain without local binding; it does not echo `clientUserMessageId`, so it cannot prove a CoCo operation |
| `turn/plan/updated` | no persistence; Codex owns the plan |
| `turn/diff/updated` | no persistence; Git remains authoritative for `coco diff` |
| `item/agentMessage/delta` | no persistence |
| completed agent `item/completed` | retain only the last bounded response on the matching generation-local turn operation; no durable event |
| `turn/completed` | finish the matching generation-local result and clear its guard after the direct response has bound the native turn ID; no durable event |
| non-retrying `error` | no event persistence; native status remains separate |
| command/file-change approval or `requestUserInput` | register bounded presentation and private native correlation in memory; never infer status from its method |
| `serverRequest/resolved` | transition the matching runtime decision to `resolved` in memory |
| any other correlated server request | warn and leave unanswered; no persistence and no inferred state |

App Server notification emission can race a request response. CoCo records the
operation as `dispatching` and installs an in-memory thread guard before the
request. Only the correlated response proves acceptance and supplies the
native turn ID. If agent output or `turn/completed` overtakes that response,
CoCo holds its native ID and bounded output generation-locally and publishes
the result only when the response later confirms the same ID. A notification
for the same thread never upgrades an
uncertain operation by proximity. Work started in `jump` remains visible
through native thread status rather than a fabricated local turn.

The adapter continues to drain notifications continuously. Only CoCo
provisioning/context events still enter the normalized event bridge. Completed
output, compaction synchronization, file-change previews, subscriptions, and
pending-request routing remain generation-local. Unknown notifications remain
safely ignored or logged and never acquire invented domain meaning.

### Sandbox and approvals

The draft default is `workspace-write`, user-reviewed approvals, no network,
and canonical workspace worktree as the only ordinary writable workspace. For the
observed protocol, apply the restrictive turn-level sandbox policy on every
`turn/start`, not only a broad mode at `thread/start`, and verify returned
effective settings when available.

Native worktrees deliberately share objects and refs while retaining their own
`HEAD`, index, and checked-out files. Ordinary worker commits on the workspace's
bound branch are supported; CoCo does not introduce a second Git database or a
commit proxy. Do not add the entire common Git directory as an unconditional
writable workspace. Let Codex's native command-approval flow mediate sandbox
crossings, with the user's selected execution profile remaining authoritative.
An already attached native TUI can present requests from its own event stream.
Starting `coco jump` after a request is already pending is not a replay
mechanism: the pinned TUI only resolves request IDs delivered to that client.
Daemon-originated turns therefore need the live request retained and answered
through the general decision surface. The generation-scoped runtime registry
does exactly that; process loss makes both its entry and the native callback
unanswerable.

The pinned live proof forces the native `untrusted` policy solely to make the
approval boundary deterministic. Codex 0.147.0 then emits
`item/commandExecution/requestApproval` with thread, turn, item, cwd, ordered
`availableDecisions`, parsed `commandActions`, and a proposed argv policy
amendment. CoCo's client can answer the original request ID with
`{"decision":"accept"}`; the server emits `serverRequest/resolved`, completes
the command and turn, and the test verifies exactly one commit on the bound
workspace branch while the source branch stays fixed. The allowlist validates
the shell argv, display form, and parsed inner action before responding.

This does not mean every user profile must prompt for every commit. With
`on-request`, the model may stop after a sandbox denial instead of requesting
escalation; with a more permissive profile, no prompt may be needed. Preserve
the selected profile and forward the exact native choices rather than inventing
a CoCo approval policy.

After an approved Git-changing operation, observe the worktree again and
verify its common directory, checked-out branch, and registered binding. A
drift is diagnosable state, never grounds for an automatic reset. The opt-in
pinned-Codex test now proves this linked-worktree approval/commit path without
granting broad permanent Git write access.

When the App Server sends a request, register it before notifying watchers. A
response uses the exact original process generation and request ID. The old
schema-v5 implementation registered it transactionally in SQLite; production
now exposes it only after an in-memory insert. On daemon or App Server loss,
expose it as orphaned and reconcile the native thread rather than fabricating
a denial response to a dead callback.

## Event routing

The current executable slice still exposes cursor-based `event.list` for
protocol compatibility, but no CLI command consumes it. An explicitly targeted
`coco status --follow` polls `workspace.get`; collection follow polls
`workspace.list`. Both use stable, non-loading `thread/read`, so the returned
phase is native truth rather than replayed event state. They print state and
decision hints only, and both continue until the observing client receives
Ctrl-C. A direct terminal owns one live output region and replaces its previous
frame; redirected or piped stdout stays ANSI-free and appends only changed
states so it remains a useful log. Reaching ready, waiting, unavailable, or an
error state never ends observation implicitly.

`send --wait` owns turn output separately. It polls `turn.result` with the same
client operation ID used for `turn.start`; agent-message and terminal
notifications populate an in-memory bounded result only after the direct start
response confirms the native turn ID. No path requests complete native history:
stable `thread/read(includeTurns: true)` is unbounded, and the pinned bounded
turn/item methods require an experimental client capability. Client disconnect
stops presentation or waiting and never interrupts a turn.

The native-first target does not promise replay of every App Server event. A
later live wakeup transport may replace polling, but it must retain native
status validation and avoid notification/hydration races. If a concrete consumer
requires durable CoCo-owned provisioning or hook events, an in-memory/live
publisher plus narrow transactional outbox must perform a race-free handoff:

1. Register a subscription and capture the current committed high-water mark.
2. Replay matching durable events from requested cursor through that mark.
3. Drain events committed after the mark.
4. Continue live until disconnect.

Each future subscriber has a bounded queue. A slow client is disconnected with
the last applicable CoCo outbox cursor, when one exists; it must not apply
backpressure to the App Server reader, whose transport must be drained
continuously to avoid deadlock.

Transient native deltas may be dropped for slow or reconnecting clients.
Conversation messages, plans, native status, and diff notifications are read
again from their owners where supported rather than made replayable by CoCo.

## Creation sequence and compensation

Current schema-v9 creation sequence:

```text
CLI             cocod              SQLite             Git          App Server
 | workspace.create    |                   |                 |                |
 |-------------->| validate/lock     |                 |                |
 |               | resolve base/context + snapshot selected local state      |
 |               | create intent --->| workspace+event      |                |
 |               |---------------- worktree add/apply ----->|                |
 |               | bind worktree --->| ready; thread null |                |
 |<--------------| prepared result     |                 |                |
```

The diagram's arrows are schematic. Creation normalizes the code base, native
context source, worktree mode, and local-state request while holding the
repository lock, but performs no native thread mutation. A later first
activation takes the same lock. Fresh `send` starts a candidate thread,
persists the turn intent before `turn/start`, then atomically binds the exact
thread and accepted turn response. Inherited context materializes through
`thread/fork`; fresh `jump` uses the leased correlation path described above.
The native-first sequence commits only CoCo-owned provisioning/operation
evidence and verified bindings; inserting a normalized event at every arrow is
not a target requirement.

Compensation is stateful, not destructive:

- before a worktree exists, mark the workspace failed with stage/error;
- after worktree creation, preserve the ready Git binding and leave the
  workspace prepared if deferred thread start or fork fails;
- after an ambiguous first-turn dispatch, mark the operation uncertain and
  preserve the exact thread binding only when Codex already exposes a
  validated durable rollout;
- after a compaction failure, preserve the bound child and worktree while
  marking the workspace failed; and
- never blindly issue a second thread or turn start after an uncertain write.

## Startup, shutdown, and recovery

Current daemon startup order after the native read cutover:

1. Acquire a user-scoped singleton lock.
2. Secure and open SQLite; run migrations, reconcile unfinished preparation
   without changing a bound workspace's lifecycle, and mark every unconfirmed
   `dispatching` operation `uncertain`. Legacy status, turn, and decision rows
   are migration data and are not loaded into the new generation.
3. Start/initialize a new authenticated loopback App Server generation,
   publish its private endpoint/token runtime files, and begin draining all
   events.
4. Reconcile durable `closing`, `reopening`, and `deleting` rows from actual
   Git registration/path and exact native-thread state. Recovery is
   repository-serialized and leaves an ambiguous transition visible for a
   later retry rather than guessing.
5. Bind the local CLI socket and report ready. Startup does not enumerate or
   resume all ordinary `ready`/`open` workspaces.

`workspace.list`, `workspace.get`, and `event.list` read and validate each
selected binding without loading its thread; after restart an unloaded thread
therefore truthfully projects `not_loaded`. `send` and `workspace.attach`
activate only their selected workspace with the verified
profile/worktree/model inputs described above. Native context creation reads
and forks its exact source independently. One activation failure does not
prevent daemon startup or passive inspection of another workspace. Recovery
never translates absence of evidence into completion, starts a replacement
thread, retries an uncertain operation, or rewrites mirrored history to
manufacture current state. The repository/worktree/thread binding survives;
only live callbacks and an in-flight turn owned by the terminated App Server
generation are lost.

On shutdown, stop accepting mutations, close watcher streams with their last
applicable cursor, interrupt or reconcile in-flight App Server requests according to its
supported protocol, checkpoint/close SQLite, and terminate only the child
process owned by this daemon. It does not delete worktrees or branches.

The alpha exposes only this foreground lifecycle and currently treats terminal
interrupt as its orderly stop trigger. Packaging must not install or enable a
systemd, launchd, or Windows service yet. Before an opt-in cross-platform user
service is offered, the daemon must handle SIGTERM through the same graceful
shutdown path and define what happens when its App Server child exits while the
local RPC server is still running: fail fast or restart a generation, stale
native observations, reconcile in-flight work truthfully, recover eligible
threads, and preserve isolated failure reporting. These paths require process
tests before a service manager may restart `cocod` automatically.

## Concurrency

- A repository-scoped in-process mutex serializes dirty/ref checks, worktree
  creation, activation, and close/reopen/delete sagas. Database uniqueness and
  durable transitional availability remain the cross-restart backstop.
- After acquiring the repository lock, context creation and deletion share a
  short-lived dependency mutex. Creation holds it from source validation
  through persisting the prepared reference, not during Git provisioning.
  Deletion, including restart recovery, holds it through dependency checking
  and effects. This prevents cross-repository raw-thread context capture from
  racing deletion before its dependency becomes durable.
- TUI leases are keyed by lease ID. Fresh adoption is exclusive only until
  the durable thread is bound; bound clients may attach independently and
  release only their own presence. All live leases prevent retirement, but
  only pending adoption blocks send. An idle bound thread remains sendable
  while one or more TUIs are open.
- The repository lock serializes send admission in the current daemon; a
  generation-local operation guard prevents a second turn while the native
  status may still read `idle`. The durable ledger is the restart/idempotency
  backstop and never claims current turn status.
- The App Server adapter multiplexes RPC requests with unique wire IDs and has
  one continuous WebSocket reader; callers never read the transport directly.
- Schema v6 still uses SQLite event sequence as its compatibility replay order. The
  native-first target requires durable order only for CoCo-owned operations or
  a future narrow outbox; it never reorders App Server history using local
  timestamps.

## Verification strategy

- Unit-test binding/provisioning transitions, name/path validation, native
  projection, and idempotency without child processes.
- Test SQLite migrations and binding/operation/retirement atomicity against
  temporary on-disk databases, including legacy v1/v5 fixtures migrating
  through current schema v9 and restart recovery from every transitional
  availability.
- Test Git behavior with temporary repositories and native worktrees,
  including new/existing/detached bindings, staged and unstaged patches,
  ordinary untracked and `.worktreeinclude` files, ref collisions, divergence,
  missing paths, bounds, symlinks, overwrites, and injected post-creation
  failure. Retirement coverage additionally includes tracked, untracked, and
  ignored late dirtiness, locked worktrees, detached-commit refusal, exact
  restoration, moved or externally checked-out branches, and owned-branch
  deletion at the recorded close-time `HEAD` only.
- Run protocol contract tests against a fake App Server executable that
  supports native thread read/history projections and fresh-TUI adoption, deliberately reorders
  response/notification delivery, requests approval, writes stderr,
  disconnects, deletes a bound thread, and emits an unknown method.
- Keep an opt-in smoke test against the installed authenticated Codex executable
  for an exact pinned version and concrete wire behavior: initialize, paginated
  model discovery, Git-only preparation, empty-thread non-materialization,
  exact adoption after a model-free shell action, native history reads,
  archive/unarchive/delete, close/reopen persistence, and resume through fresh
  daemon and App Server processes. The fake process test owns deterministic
  model-turn, event-flow, relay, detach, lifecycle-blocker, and interrupted-
  saga coverage.
  Run it explicitly with
  `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked --test real_codex_compat -- --ignored --test-threads=1`;
  `COCO_REAL_CODEX_BINARY` may select a non-default executable. Generated
  schemas may be reviewed separately during an upgrade, but additive schema
  drift alone is not a compatibility failure.
- Test CLI snapshots for human rendering, command help, stable error codes, and
  parse/shape tests for versioned JSON and NDJSON; the source of a projection
  may change without changing its user-visible meaning.
- Launch the MCP adapter under a protocol test harness, verify advertised tools
  in default and send-enabled modes, compare structured projections with daemon
  responses, and exercise cancellation/idempotent send. Assert the current
  sanitized durable MCP audit contract independently from native events.

## Original implementation sequence

These slices describe the historical path through the schema-v5 alpha;
completed entries remain context rather than the current schema-v9
native-first target. Each step remains independently runnable and testable:

1. **Toolchain and process skeleton:** one Rust crate, generated Codex schema
   command, `cocod` foreground lifecycle, CLI connection/health request,
   secure XDG paths, and test runner.
2. **Durable core:** initial SQLite migration, operations/repositories/workspaces/
   turns/events, transition functions, idempotency, and daemon unary protocol.
3. **Git registration:** implement `coco repo add`, repository resolution, dirty and
   base checks, temporary-repository integration tests.
4. **First vertical proof:** implement `coco create` through worktree creation,
   App Server initialization, thread preparation, idle binding persistence,
   status display, and failure injection.
5. **Observation:** implement `list`/`ls`, one-shot/JSON `status`, durable event
   polling with `status --follow`, and recovery reconciliation.
6. **Continued interaction:** implement idempotent first/later `send`, shared
   App Server `jump`, externally started turn projection, workspace concurrency,
   sequential-turn tests, and later App Server resume after restart.
7. **Git inspection:** implement full status facets and `diff`, including
   untracked reporting and bounded output.
8. **MCP adapter:** implement local stdio serving, repository-scoped read-only
   `workspaces.list`, `workspaces.status`, and `workspaces.diff`, opt-in idempotent
   `workspaces.send`, error mapping, cancellation, and control-call auditing.
9. **Interaction checkpoint:** correct native thread-state ownership and verify
   close-without-cancel plus reattachment through `jump`, then stop and review
   findings and all remaining priorities with the user.
10. **Workspace vocabulary migration:** rename the prerelease CoCo-owned `task`
    aggregate across domain types, storage through a lossless migration, daemon
    protocol, events, CLI/MCP schemas, tests, and documentation. Replace `new`
    with `create` without conflating a workspace with its Git worktree, Codex
    thread, or a future external ticket reference. Completed in schema v4.
11. **Create convenience pipeline:** add composable `--send <message>` and
    `--jump` post-actions with the fixed order create, send, jump. Preserve a
    successfully created workspace or accepted turn when a later action fails,
    and cover every mode in process tests.
12. **Native Git approval proof:** exercise an ordinary linked-worktree commit
    through the pinned Codex approval protocol, verify only the workspace
    branch advances, and retain the shared native Git model without a custom
    commit service.
13. **Repository-scope ergonomics:** add `repo list`/`repo ls`, optional
    leading-path scope, `--all-repos`/`-a` collection overviews,
    `--global`/`-g` single-name lookup, global workspace-ID lookup,
    deterministic ambiguity errors, and safe slash-separated workspace names
    without changing MCP's fixed repository capability. The initial overloaded
    `-a` lookup spelling was corrected before release. Completed without a
    daemon wire-schema change.
14. **Decision closure:** persist generation-bound native command/file-change
    approvals and structured user-input requests, project them through status,
    and answer an opaque ID through the numbered `coco decide` flow. Completed
    in SQLite schema v5 and CLI JSON schema v5; the control MCP remains unable
    to answer decisions.
15. **Interactive CLI completion:** add one terminal-gated picker shared by
    omitted repository/workspace targets and native decision options; prompt
    separately for a missing create name or send message; add deterministic
    `--no-input` and approval `--choice`; keep collections and JSON
    non-interactive. Completed without moving selection policy into the daemon.
16. **Remaining release hardening:** supported-version policy, filesystem
    permission tests, help/public docs, packaging, and clean-install test.

Do not split packages or build TUI/web scaffolding during these slices. The
daemon protocol and core ports are already the seam those clients need; MCP is
implemented as a thin adapter in the same package.

## Native-first overhaul sequence

The overhaul supersedes further feature expansion. Every phase remains
runnable, preserves the command/RPC behavior named above, and has a separate
exit gate. Follow the dependency direction and single-package decision in
[Rust source architecture and code health](rust-architecture.md).

### Phase A - freeze and select the native contract

- Freeze hooks, handoff, worker MCP registry, Agentgateway, native Windows,
  cleanup automation, TUI/web, and new orchestration concepts.
- Inventory command help, versioned JSON, stable errors, daemon methods, and
  schema-v1-through-v9 migrations as compatibility fixtures.
- Select one released Codex build, regenerate its experimental schema, and run
  real tests for native thread read/list/status/history, resume, fork,
  compaction, request responses, and remote-TUI unsubscribe.

Exit: the released build supplies every native read needed to replace a local
mirror. Otherwise stop and retain the current implementation without claiming
that the target has shipped.

### Phase B - separate projections and validate the read cutover

- Represent CoCo binding/provisioning, Git observation, native thread state,
  behind the existing coordinator and `WorkerRuntime` boundary. Live pending
  requests are generation-local and never part of that snapshot.
- Keep native `list`/`ls`, `status`, follow, and activation projections behind the
  schema-v5-compatible public DTOs until a deliberate protocol revision.
- Cover missing/deleted native threads, Git drift, duplicate names across
  repositories, App Server disconnect, notification/query races, and restart.

Exit: current behavior tests pass against the native projection with no
unexplained semantic difference and the old schema remains available as a
reversible bridge.

### Phase C - cut reads over to owners

- Make App Server queries/live notifications authoritative for thread, turn,
  status, history, model, and request facts.
- Make Git queries authoritative for current worktree/ref/diff facts.
- Keep SQLite authoritative only for the CoCo binding, provisioning evidence,
  idempotent operation correlation, and irreducible provenance.
- Preserve existing phase names and JSON fields by deriving them from the new
  projections; an unavailable owner produces an explicit unavailable result,
  never a seemingly fresh stored value.

Exit: no current-state read requires stored native snapshots, turn history, or
normalized events, and `status --follow` remains detach-only.

The implemented Phase B/C read slice used a direct, reversible cutover instead
of holding public output on a prolonged shadow-only branch. That exception was
accepted because the pinned published 0.147.0 real-process test proves
non-loading `thread/read`, ID/`cwd` validation, restart persistence, and
optional turn hydration, while focused fake-runtime tests cover projection
mismatches, failures, and final-output selection. No table or existing record
was deleted. Status/failure writes tied to obsolete eager recovery were removed
with that startup path; the subsequent live-event reduction stopped prompt,
decision, local-turn, active-turn, and native status/plan/diff/error/request
writes. Legacy structures and CoCo-owned provisioning events remain, while the
later state/output split removed the final completed-output write in favor of
bounded generation-local correlation for `send --wait`. Reverting the read
projection therefore remains local. This evidence does not waive Phase D's
physical cleanup and product-proof gates.

### Phase D - stop writes, migrate, then remove

- Implemented in schema v6 and retained by schema v9: the minimal `turn_start`
  ledger commits intent and
  dispatch boundaries, accepts only a direct native result, and makes an
  unconfirmed dispatch permanently non-retrying under the same operation ID.
- Implemented stop-write: production no longer creates local turns,
  `active_turn_id`, thread snapshots, turn start/completion events, or durable
  decisions. Existing schema-v5 rows remain readable as migration data.
- Pending separately: physically remove legacy tables/columns only after this
  checkpoint and its migration fixtures have survived the product proof.
  Never combine that cleanup with another authority change or replace one
  native mirror with another.

Stop-write exit: historical database fixtures migrate through v9, dispatch
crash injection does not issue a second `turn/start`, and restart rebuilds
user-visible state from the stored binding plus Git/App Server. Physical
cleanup remains gated on Phase E rather than blocking it.

### Phase E - product proof and release decision

2026-09-09 checkpoint: the combined isolated process scenario now covers this
sequence with two repositories and actual CLI/MCP processes, including decisions,
remote-client exit and retry after restart. Its Codex worker is fake. Separate
model-free 0.153.4 tests exercise native thread lifecycle and real MCP isolation,
profile restoration, fork attribution, and durable signal replay. These close
the automated core/integration gap for the signal slice; they do not replace a
supervised model-backed real-user acceptance run or authorize public release.

Use existing surfaces rather than adding a speculative feature: create and
start workspaces in two repositories through the CLI; let the initiating
clients exit; inspect and continue one through the MCP adapter; answer a live
request from another CLI; attach and detach the exact thread/worktree through
`jump`; restart the coordinator/App Server; and repeat operation IDs. Both
bindings must recover, active work must not be cancelled by client exit, and
no duplicate external artifact may appear.

Go for a public alpha only when this multi-client control-plane workflow works
against the selected released Codex build and the MCP/client handoff has a real
user. If actual use reduces to one person invoking `create` then immediately
`jump`, or native Codex exposes a documented programmable worktree/thread
binding with equivalent multi-client control, stop the release and reduce CoCo
to private glue instead of competing feature-for-feature.

## Open architecture decisions

The following still need confirmation; decision-response closure no longer
blocks a safe complete v0:

1. Which released Codex CLI build becomes the proven minimum for native reads;
   a broader compatibility range is optional follow-up evidence.
2. When the selected Windows named-pipe local-IPC backend and Windows CI become
   release requirements; the cross-platform transport shape itself is settled.
3. How an explicit atomic promotion operation anchors detached work on a new
   branch and updates the stored binding without racing Git or losing commits.
   Creation already separates base, exact workspace/native-thread context,
   worktree binding, and bounded local-state carry; handoff artifacts remain a
   distinct later design.
