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

## Evidence and decision classification

### Observed facts

- At the specification baseline on 2026-09-05, the repository had no commit
  and no implementation files; it contained the handoff and documentation
  foundation only.
- The locally installed `codex-cli 0.147.0` can generate App Server protocol
  schemas.
- Schemas generated from that installation expose the `initialize`,
  `model/list`, `thread/start`, `thread/resume`, and `turn/start` client requests;
  thread/turn, plan, diff, item, token, error, and status notifications; and
  server-initiated approval and user-input requests.
- That observed protocol is version-specific. Generated schemas from the
  selected Codex executable, rather than this prose, are authoritative for
  field names and wire payloads.
- Schema v6 adds the minimal turn-start operation ledger and retains the old
  native-status columns, local turns, normalized events, completed messages,
  MCP audit events, and decisions as a reversible compatibility bridge. The
  retained shapes are not evidence of long-term CoCo ownership.
- Passive `list`/`ls`, `status`, and follow polling now validate the stored binding
  through stable, non-loading `thread/read` and derive current phase from the
  native response. Follow still obtains completed agent text from the stored
  compatibility event stream because the stable native history response is
  unbounded and the bounded APIs require an experimental capability. CoCo no
  longer stores sent prompt bodies, status/plan/diff/error notifications,
  unsupported server requests, live decisions, local turn rows,
  `active_turn_id`, start/resume status snapshots, or turn start/completion
  events. It retains only operation dispatch/idempotency facts, completed agent
  text, provisioning evidence, and MCP audit records until their separate
  removal gates pass.
- The pinned 0.147.0 schemas include native thread read/list, loaded-thread
  listing, optional history hydration, status, fork, and compaction. A
  model-free real-process test now proves the read/history subset and records
  that listing does not discover CoCo's prepared empty thread. Other operations
  from rolling documentation may become dependencies only after generated
  schemas and real compatibility tests select a released Codex build.
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
  IPv4-loopback WebSocket. This same endpoint lets `coco jump` attach the
  official Codex TUI without creating a second App Server process.
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
- Dirty source checkouts are rejected by default. An explicit local-state
  selection may copy tracked changes and ordinary non-ignored untracked files
  into the destination; CoCo never silently stashes, resets, stages, or mutates
  the source checkout.
- Worktrees live outside the registered repository by default.
- CoCo does not automatically perform destructive branch or worktree cleanup.
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
- `coco create` prepares a workspace by default. `--send <message>` starts its
  first turn, `--jump` opens its Codex TUI, and the two options compose in the
  fixed order create, send, jump. Failure of a later post-action does not roll
  back a successfully created workspace or accepted turn.
- `coco create` selects code, conversation context, Git binding, and optional
  local changes independently. `--base-workspace` selects committed code;
  `--context-workspace` or `--context-thread` selects native Codex history.
  A context source must be idle or unloaded. Optional `--compact` applies only
  to the child and completes before `--send` or `--jump` runs.
- `coco model list`/`coco model ls` exposes the visible catalog reported by the daemon-owned Codex
  App Server. `coco create --model/-m` is an explicit per-workspace model
  override. CoCo passes the named profile overlay in `config` and the explicit
  model in the App Server's separate `model` field; it does not reimplement
  Codex configuration precedence.
- Native worktrees intentionally share their repository's Git object and ref
  storage. CoCo does not proxy ordinary worker commits or allocate a separate
  Git database per workspace.
- Supported Codex approvals and structured user-input requests are projected
  as generation-bound decisions and answered only through an explicit
  `coco decide` operation; CoCo never auto-approves them. Their bounded prompt,
  private native correlation, choices, and state live only in the daemon
  generation that owns the App Server request. They are not written to the
  schema-v7 legacy decision table, and raw answers are never retained.
- v0 has no publicly reachable network listener. Its App Server endpoint is
  capability-token protected and bound only to `127.0.0.1`.
- Worktrees may use a new branch, an existing local branch, or detached HEAD.
  Detached worktrees remain registered Git worktrees, but CoCo has no promotion
  or cleanup command in this alpha.

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
  Schema v7 persists its name, source path, parsed-configuration hash, explicit
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

An operator can register a Git checkout, prepare a workspace and Codex thread
from an exact commit without starting work, send the first or a later turn,
inspect or follow its current state, enter the same thread with the official
Codex TUI, answer a supported approval or question, and inspect all workspace
changes relative to the fixed base commit.
A local MCP host can inspect the same workspace projections and, when the
operator explicitly enables the capability, send a turn through the same
daemon use case.

The smallest proof slice is successful when it demonstrates, end to end:

1. `cocod` starts and initializes a Codex App Server child.
2. CoCo creates a native worktree with the selected new-branch,
   existing-branch, or detached binding at a fully resolved base SHA.
3. The App Server creates a non-ephemeral thread with that worktree as `cwd`.
4. SQLite atomically records the workspace-to-thread-to-worktree binding,
   provisioning outcome, idempotent operation correlation, and only the
   profile/context provenance required for verified recovery.
5. Creation returns the prepared workspace in `idle` without starting a turn.
6. A separate `send` starts a text turn in that thread.
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
process, owns the worktree registration. CoCo deliberately has no automatic
cleanup or promotion operation yet. A later promotion design must atomically
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
binding. Schema v6 currently exposes a broader redacted profile snapshot. In v0
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

- `fresh` creates a new thread without inherited conversation history. The
  first turn supplies the work instruction; provenance still records the
  selected code base and any explicitly supplied source material.
- `fork` uses Codex's native `thread/fork` from either a selected workspace
  thread or an exact native `thread.id`, preserving its history while binding
  the new thread to the newly prepared worktree and effective configuration.
  The transition must explicitly tell the agent that `cwd`, branch, and base
  commit may differ.
- `handoff` starts a fresh thread from bounded, reviewable transfer material
  rather than copying the full conversation. The material may be authored by
  an agent, supplied as an existing Markdown document or CLI input, or refer
  to an already-associated external record such as a ticket. Generated
  material may include a plan, but generation is deliberately separate from
  attaching and consuming the handoff. Its exact artifact and reference model
  remains open.

The first context-transfer delivery implements `fork` only. Conversation
context is independent from code selection: `--context-workspace` resolves a
workspace in the destination repository, while `--context-thread` addresses
an exact readable native Codex `thread.id` and may originate elsewhere. The
source thread must be idle or unloaded. CoCo always creates a child; it never
adopts or moves the source thread. An explicit compact modifier runs
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
  [--context-workspace <workspace> | --context-thread <thread-id>] [--compact]
  [--branch <branch> | --checkout <branch> | --detached]
  [--carry-changes [--carry-untracked] | --dirty]
  [--profile <name>] [--model <model>] [--send <message>] [--jump]
coco [<repository-path>] (list | ls) [--json]
coco (list | ls) --all-repos [--json]       # `-a` is the short form
coco [<repository-path>] status [<workspace>] [--follow] [--json]
coco status [<workspace>] --global [--follow] [--json] # `-g` is the short form
coco [<repository-path>] send [<workspace>] [<message>]
coco send [<workspace>] [<message>] --global
coco [<repository-path>] jump [<workspace>]
coco jump [<workspace>] --global
coco decide <decision-id> [--choice <number>]
coco [<repository-path>] diff [<workspace>]
coco diff [<workspace>] --global
coco mcp serve --repository <path> [--allow-send]
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
the error points out global matches when they exist. `--all-repos` is reserved
for `list` and never broadcasts a mutation. CoCo does not encode a path and
workspace name into a composite string.

`model list`/`model ls` and `decide` are intentionally not repository-scoped.
The model collection reads the daemon-owned App Server catalog and accepts
only its optional `--json` output flag. `decide` accepts the globally unique
opaque CoCo decision ID printed by `status` and an optional deterministic
approval-choice number; a leading repository path or either scope flag is an
error.

Human terminal commands share one interactive input contract. `create` may
prompt for a missing name and, when implicit `.` cannot select a registered
repository, offer the registered repositories. `status`, `send`, `jump`, and
`diff` may select an omitted workspace from the local scope or the daemon-wide
`--global` scope; `send` then prompts for an omitted message. The picker uses
arrow keys or `j`/`k` for immediate cursor movement, Enter to confirm, direct
one-key selection for options 1 through 9, and Escape, `q`, or Ctrl-C to cancel.
It is also the single option-selection implementation for `decide`.

Collection commands never prompt. Neither do commands with non-terminal input
or diagnostic output, explicit `--no-input`, or a missing target required for
JSON output. Those invocations fail before ambiguity can turn into an implicit
choice; fully explicit command forms preserve their deterministic behavior.

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
- Do not require a clean checkout merely to register it; cleanliness is a
  creation precondition and must be reported by `create`.

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
   `--context-workspace <workspace>` or `--context-thread <thread-id>` calls
   native `thread/fork` from an idle or unloaded source. The direct form means
   exact `thread.id`, not the root `thread.sessionId`. `--compact` is valid
   only with one of these sources and compacts only the child.
3. **Git binding.** The default allocates `coco/<workspace>`.
   `--branch <branch>` allocates another new branch, `--checkout <branch>`
   uses an existing local branch that Git reports as free, and `--detached`/
   `-D` allocates no branch. Existing-branch selection supplies its own base
   and therefore rejects a separate base option.
4. **Local state.** The default rejects tracked or ordinary untracked changes
   in the invoking checkout. `--carry-changes` preserves staged and unstaged
   tracked changes with separate binary patches. `--carry-untracked` also
   copies ordinary non-ignored untracked files and requires tracked carry.
   `--dirty`/`-d` is the CLI shorthand for both. It is independent from
   detached mode, so `-dD` combines dirty-state carry with detached HEAD.

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
7. Start a non-ephemeral Codex thread for fresh context or fork the selected
   source thread. Canonical `cwd` is always the destination worktree;
   `config` contains the profile overlay and the separate `model` field is
   present only for an explicit override.
8. Set the returned thread name, verify and persist its ID and `cwd`, and for
   a fork retain exact parent/source provenance independently from Git-base
   provenance.
9. When requested, compact only the child and await its native terminal
   lifecycle before returning it ready.
10. Return without starting a user turn unless `--send` was supplied. If
    requested, start the turn and then run `jump`; failure of a later action
    does not roll back an earlier successful action.

If a post-worktree step fails, CoCo must mark the workspace `failed`, record the
stage and discovered artifacts, and leave the branch/worktree intact. It must
not hide the failure by destructively cleaning up. Retrying an operation ID
must not create duplicate artifacts.

### `coco list` / `coco ls`

- List workspace ID, name, repository, runtime phase, Git badges, branch, and
  last update time.
- Default to workspaces in the selected/current registered repository.
  `--all-repos` exposes the daemon-wide view and always includes repository
  identity in each human and JSON row.
- Sort deterministically by most recent update, then workspace ID.
- Read each ready workspace's current Codex status with non-loading
  `thread/read`; do not resume a thread merely to list it. A missing or invalid
  native binding projects unavailable instead of falling back to stored status.
- `--json` emits one schema-version-5 JSON document and no decorative stdout
  text. Every row includes a compact repository identity.

### `coco status`

- Return the complete projection below for exactly one workspace. Resolve an
  explicit name only in the selected/current repository by default or across
  all repositories with `--global`/`-g`.
- When a human terminal omits the reference, choose from the selected/current
  repository or, with `--global`, from every repository. A missing reference
  under `--json`, `--no-input`, or non-terminal execution is an error pointing
  to `coco list` for the collection view. `--all-repos` is invalid for status.
- Return the complete workspace projection: immutable CoCo/Git binding, context
  mode, non-secret profile summary, Codex thread and active/latest turn IDs,
  runtime phase and wait reasons, Git facets, timestamps, last error, and a
  recent compatibility-event cursor while follow polling still needs it. It also
  includes current `pending` or `submitted` decisions using opaque CoCo IDs
  and bounded presentation data; native App Server request IDs are never
  exposed.
- `--json` uses the same field meanings as the daemon protocol and includes a
  top-level schema version.
- One-shot status and every `--follow` poll perform a non-loading native
  `thread/read`; stored status is never served as current when that read fails.
  The current unary follow loop still uses `event.list` as its polling envelope,
  but native status determines when it stops. A decision-free terminal or ready
  phase must remain stable for a second poll so the matching completion event
  can arrive before its bounded completed-message text is printed. CoCo does
  not automatically load the complete native thread history. Ctrl-C detaches
  only the display and does not cancel the turn.
- `--follow` and `--json` are intentionally mutually exclusive in the current
  CLI; machine clients can poll `status --json`.

### `coco send`

- Accept a non-empty text message and return after `turn/start` is accepted,
  not after the turn completes. In a human terminal, an omitted workspace uses
  the scoped picker and an omitted message uses a subsequent text prompt.
  Non-terminal and `--no-input` callers must supply both.
- Start the first or a later turn in the existing thread with the stored
  worktree as `cwd` and the stored sandbox/profile policy.
- Before starting a turn, read and validate the binding and resume it when
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

- Require the workspace's managed worktree and existing Codex thread binding.
  A human terminal may select an omitted workspace locally or with `--global`;
  deterministic callers must provide one.
- Resolve the workspace through `cocod` and invoke its internal attach preflight,
  which validates and, when needed, resumes the native thread so the daemon
  connection is subscribed. Then run `codex resume` in that worktree against
  the daemon-owned authenticated loopback App Server.
- Pass the capability token through a child-process environment variable, not
  an argument or persisted workspace metadata.
- Turns started in the TUI must appear in the same native workspace projection
  as turns started with `coco send`; exiting the TUI does not delete the
  workspace.
- A normal `/quit` or `/exit` detaches the remote TUI without interrupting an
  active turn. Explicit interruption remains the separate cancel action.

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
  it in memory. Schema v6 retains old answer-free submission rows only as
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
| `workspaces.send` | mutating, opt-in | Workspace name/ID, non-empty text, and operation ID; starts the same turn as `coco send` and returns workspace/thread/turn correlation. Advertised only with `--allow-send`. |

The default tool list is therefore useful but read-only. Enabling
`workspaces.send` is an explicit operator delegation; it does not enable
approvals, cleanup, repository registration, arbitrary Git commands, or
permission changes.

The current alpha durably audits every MCP invocation with tool name, MCP
adapter instance/client label, workspace when applicable, operation ID,
timestamps, outcome, and sanitized error. Message contents are not copied into
audit events. Native-first persistence retains this only if a concrete
security, diagnostics, or external consumer needs it; the MCP adapter itself
does not make an audit history product authority.

This v0 surface is not A2A messaging. The caller is an external MCP client and
the target is a CoCo workspace. Workspace-to-workspace identity,
correlation/response routing, `agents.ask`, `integration.request`, and
autonomous delegation policy remain later work.

## Runtime status contract

The public workspace projection contains CoCo's provisioning/binding
`lifecycle`, a native `threadRuntime` projection, a derived `phase`, zero or
more `waitReasons`, and a separate Git projection. `threadRuntime.status`
retains Codex's native status, `runtimeGeneration`, `observedAtMs`, and
`isFresh`. Schema v6 retains old snapshots only as migration data and can mark
them stale on process loss; thread start, resume, and notifications never
write a new snapshot. Ready-workspace reads obtain a fresh projection directly
from stable `thread/read` without loading the thread; until that succeeds they
report unavailable and never serve an old SQLite value as current. Physical
removal of the compatibility columns is a later schema-only checkpoint.

### Runtime phases

| Phase | Meaning |
| --- | --- |
| `provisioning` | CoCo has recorded intent and is creating/verifying external resources. |
| `starting` | The worktree exists and CoCo is creating or resuming its thread. |
| `active` | A turn is in progress with no known wait flag. |
| `waiting_for_approval` | The App Server has an unresolved approval request. |
| `waiting_for_input` | The App Server has an unresolved user-input request. |
| `idle` | The thread is available and no turn is running. |
| `not_loaded` | Codex reports that this thread is not loaded in the current runtime. |
| `system_error` | Codex reports a native thread-level system error. |
| `unavailable` | CoCo has no current-generation native thread observation. |
| `failed` | CoCo could not complete workspace preparation or startup. |
| `completed` | Reserved for an explicit future workspace-completion operation; never inferred in v0. |

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

Schema v6 can still decode the complete prerelease event vocabulary, including
rows created by older CoCo builds. The current runtime writes only this reduced
subset:

```text
workspace.created
worktree.created
agent.started
context.compacted
agent.message.completed
agent.failed
```

Every retained event carries a monotonic database cursor, event ID, optional
workspace and turn IDs, source, source timestamp when available, recording
timestamp, normalized kind, source method, and versioned payload. Sent user
messages, local/native turn start and completion, plan and diff notifications,
native status/error updates, supported decisions, and unsupported server
requests are no longer event rows. Unknown App Server notifications must be
logged safely without crashing the daemon or inventing a normalized meaning.

This vocabulary is not a long-term copy of Codex history. Native reads supply
current status and history; only bounded completed-agent text remains because
the selected stable App Server contract has no bounded final-output query.
Retain CoCo-owned provisioning/context events that serve an implemented
consumer. Remove `event.list` and the remaining native event row only through
a versioned daemon-protocol change after polling and completed-output behavior
have equal replacements. A future hooks feature must define a narrow
transactional outbox rather than revive full native event mirroring.

## Safety and lifecycle behavior

- The worker's effective `cwd` must equal its canonical worktree path on every
  thread start/resume and turn start.
- Worker writes must be constrained to the worktree and explicitly justified
  supporting roots. Network access is off by default in the draft profile.
- The daemon-owned App Server may bind only an authenticated IPv4-loopback
  WebSocket; CoCo must not expose it on a LAN or public interface.
- The CoCo MCP server uses local stdio, is read-only unless the operator starts
  it with the send capability, and never bypasses daemon authorization or
  validation.
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
- No lifecycle path uses `git reset --hard`, automatic stash, forced branch
  deletion, or automatic worktree deletion.
- Workspace bindings, provisioning failures, and idempotent operation evidence
  survive daemon or App Server restarts. Schema v7 fails unfinished workspace
  preparation and marks a turn-start dispatch without a proven response
  `uncertain`; it never retries that operation automatically. Live decision IDs
  deliberately disappear with the daemon generation because the corresponding
  native requests are no longer answerable. Startup does not load every bound
  thread: passive reads reconstruct current runtime state with validated
  `thread/read`, while `send` and `jump` resume only the selected workspace
  thread when activation is required. Native context creation addresses its
  exact source through `thread/fork`. An individual failure remains explicit
  and does not stop another workspace or daemon startup. Recovery must not
  report guessed success or create a replacement thread.

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
  branch, base SHA, worktree, thread ID, and thread `cwd`.
- Dirty source, invalid base, duplicate name/branch, already checked-out
  existing branch, and existing destination all fail before an unintended
  second worktree or thread is created.
- A native-fork test selects the Git base independently from either a
  workspace or exact native-thread context source, binds the returned child and
  parent thread, and invokes optional compaction only on that child before it
  becomes ready.
- Local-state tests prove staged and unstaged binary patches preserve index
  placement, ordinary untracked files require explicit selection, ignored
  files require `.worktreeinclude`, the source remains unchanged, and path,
  symlink, overwrite, file-count, and byte bounds fail closed.
- Injected failures after each saga stage leave a diagnosable `failed` workspace and
  never delete the external artifacts automatically.
- Restarting the daemon preserves the stored binding without eagerly resuming
  every `ready` thread. Passive list/status calls return a validated native
  `not_loaded`, current status, or explicit unavailable projection without
  loading the thread or serving an old SQLite snapshot as current.
- The first post-restart `send` or `jump` attach resumes only its selected
  thread and subscribes the new daemon connection. Native `thread/fork` reads
  and uses its exact source without coupling that conversation to the Git base.
  Activation rejects a missing, invalid, moved, or changed named profile and a
  mismatched returned thread ID or working directory. The default profile
  consistently reloads as an empty overlay; one failed activation does not
  prevent other bound workspaces or the daemon from remaining available.

### Interaction and observation

- A fake App Server contract test exercises initialization, thread preparation,
  explicit turn start, event correlation, and completion/failure. A separate
  opt-in real Codex compatibility test performs no model turn: it checks the
  pinned executable and generated schemas, prepares a persistent thread, then
  resumes the same thread through a fresh App Server process.
- Two sequential `send` operations use the same thread and different turn IDs;
  concurrent sends yield one accepted turn and one deterministic conflict.
- A crash/error before dispatch leaves a replayable prepared operation. A
  crash/error after the dispatch marker but before a proven response leaves an
  `uncertain` operation that the same ID never redispatches. An accepted replay
  returns the same native turn ID, and a different payload under any existing
  operation ID returns `IDEMPOTENCY_CONFLICT`.
- `status --follow` can attach during a turn, reflects native phase changes,
  waits for a stable terminal poll, prints the completed agent text retained in
  the compatibility event stream, and can detach without affecting the turn.
  The current event cursor is not the source of current status; it remains the
  temporary source of final text until a stable bounded native history API is
  available or the presentation contract is deliberately reduced.
- `status` exposes an opaque ID for supported pending decisions; `coco decide`
  renders the offered choices through the common picker (or accepts explicit
  `--choice` for an approval), validates input, and sends exactly one response
  to the originating live App Server request.
- `list --json` and explicitly targeted `status --json` parse as JSON with
  stable version and status fields; JSON and `--no-input` never open a prompt.
- `jump` resumes the stored thread in the stored worktree through the shared
  authenticated App Server and keeps daemon event projection active.
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
  capability token are user-only. The sole network port is authenticated and
  bound to IPv4 loopback.

### Native-first migration and release proof

- A released Codex build, not upstream documentation or an unreleased merge,
  passes generated-schema and real-process tests for every native read CoCo
  adopts: thread read/list with `cwd` and status, resume, fork/compaction where
  used, exact request responses, and remote-TUI unsubscribe.
- Native-projection tests cover active/waiting/idle/error/not-loaded status,
  read failure, restart, final-output selection, and binding mismatches before
  any table or existing record is removed. The initial direct read cutover is
  acceptable without a prolonged shadow-only checkpoint because a pinned
  published 0.147.0 real-process test proves non-loading reads, restart
  persistence, exact ID/`cwd`, and optional history hydration. Eager recovery
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

Selecting and proving one released minimum Codex version is now a blocker for
the native-first public-alpha decision; maintaining a broad compatibility
range is not. Windows support, profile configuration UX, and a future explicit
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
