---
type: Engineering Contract
title: Public checked workspace retirement
description: Defines separately granted MCP close/reopen/delete, pinned confirmations and durable deletion receipts over existing retirement.
tags: [mcp, retirement, recovery, idempotency, approvals]
status: stable
---

# Public workspace lifecycle

## Boundary

Control MCP adds `workspaces.close`, `workspaces.reopen`, `workspaces.delete`
and read-only `workspaceDeletions.get`. Each changing tool needs an independent
`--allow-close`, `--allow-reopen` or `--allow-delete` grant. Existing grants do
not confer cleanup. Default MCP and AI read-only mode remain read-only.
The external contract is
[`reference/workspace-lifecycle.mdx`](../../docs/src/content/docs/reference/workspace-lifecycle.mdx).
These are generic CoCo controls, not a Palm scheduler or worker-MCP expansion.

The adapter fixes repository scope and dispatches typed application requests.
Existing coordinator retirement owns Git/native identity, runtime/TUI/descendant
and context blockers, guards, loss policies and recovery. The adapter never
invokes Git or reads SQLite. Keep/discard semantics match the CLI; Auto skips
confirmation, not loss policies or safety checks.

## Checked review

Delete dry-run returns an unclaimed `preview`. Close retains its existing
preview result. Both expose `planSha256`, a hash of the typed plan containing
the exact workspace UUID, resources, HEAD, effects and risk counts. It is a
precondition, not a credential, filesystem lock or content snapshot of every
dirty file. Independent Git/native clients still require final core checks.

AI Ask obtains an authoritative preview and generates an escaped server-owned
review through existing native form elicitation. After exact host consent,
delete applies with the digest; close applies with the full expected plan and
UUID. Reopen reads the closed target and restores only that exact UUID through
ordinary restoration checks. Missing capability, decline and cancellation
fail closed. Delete replays cannot dispatch and need no fresh confirmation.
A blocked preview may fail before claiming. Headless MCP remains noninteractive
unless explicitly started with `--confirm-changes`.

## Durable deletion minimum

Hash the normalized typed delete intent: caller `operationId`, original
workspace reference and keep/discard policies. Dry-run and the optional plan
precondition are not intent fields. Replay precedes target resolution and
native inspection. Read-only repository discovery also supports lookup after
repository removal; foreign lookup is unknown and foreign mutations conflict
without disclosing a receipt.

Known UUIDs take precedence over names; a foreign UUID cannot resolve to an
allowed workspace with that string as its name. A matching receipt also wins
over a planning error when another caller completes during a repository-lock
wait. Return the final receipt under the frozen authenticated repository ID,
without rediscovering a scope path that deletion may itself have removed.
Subsequent calls still require a valid scope path; configure long-lived MCP
servers against a persistent checkout.

Schema 21 adds `workspace_deletions`: key, repository identity, frozen workspace
UUID, receipt JSON and a private association bit. A partial unique index permits
one associated operation per workspace. There is no cascading workspace foreign
key, prompt, transcript, recipe, file content, TTL, LRU or generation boundary.

1. Compute the checked preview and validate any supplied plan precondition.
2. An immediate transaction claims `preparing`; only its winner may dispatch.
   Identical repeats return evidence and changed intents conflict. Claims are
   never stolen after cancellation or restart.
3. Reuse core deletion with exact UUID and expected plan, including guards and
   revalidation before the existing durable retirement intent.
4. In that intent's transaction, associate only the matching public claim and
   advance it to `deleting`. CLI/temporary deletion cannot adopt a public claim.
5. Commit `deleted` with workspace-record removal and the ordinary hook outbox
   event. Lost final replies cannot redirect replay to a reused workspace name.

Pre-saga rejection records `rejected` and a safe error code. Errors after
association record `uncertain`, not success. Canceling core deletion detaches
its receipt as uncertain in the same transaction; a later independent CLI
deletion cannot falsely complete it. Confirmed deletion wins over a raced error.

## Recovery and limits

Existing retirement recovery handles associated intents. If the verified
worktree remains, restore availability and mark uncertainty without repeating
file discard. If its path/registration is verified absent, the pinned native/
branch saga may finish and atomically confirm only its associated receipt.
Ownership, runtime, dependency and commit checks still apply. Startup marks
unassociated pre-saga claims uncertain; queries/repeats never redispatch them.

This is durable at-most-one initial dispatch permission, not exactly-once
external effects, atomic Git/native cleanup or rollback. `unknown` means scoped
missing evidence, not non-execution. Data loss/restoration can lose deduplication.
Native history and a missing workspace cannot manufacture a deletion receipt.

Close/reopen remain state-sensitive commands without another ledger. After a
lost reply, inspect scoped workspace status. An already-applied transition
returns its ordinary invalid-state error on repetition.

Delete still removes send-operation/outcome receipts even with `keepThread`.
Consumers must preserve needed run outcomes before cleanup. Creation receipts,
worker-thread exclusions, signals and hook evidence keep their independent
retention contracts. There is no new native protocol or history recovery.

## Code and verification

Neutral plans/receipts live in `domain/retirement`; protocol reexports existing
plan names unchanged and owns typed deletion commands. Coordinator's private
`retirement/public` composes replay and core retirement; Store owns atomic
association/evidence. MCP's private lifecycle/dispatch/confirmation children
own presentation. One executable permissions value replaces a growing list of
boolean entry-point arguments. No new dependency, crate or Rust SDK is added.

Isolated cases cover replay conflicts, name reuse, plan drift, scopes, blockers,
retention, real database reopen and recovery before/after worktree removal.
MCP process fixtures exercise grants and actual form confirmation against a
no-op backend. A daemon-process proof combines public cleanup, real Git and a
fake native peer. No real model or operator state is needed.
