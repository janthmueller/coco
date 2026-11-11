---
type: Integration Handoff
title: CoCo workspace cleanup for Palm
description: Describes the additive public cleanup tools, permissions, durable deletion outcomes and integration safety limits.
tags: [handoff, palm, mcp, cleanup, recovery]
status: publication-pending
updated: 2026-11-11
---

# Workspace cleanup: return handoff to Palm

## Availability

This additive CoCo slice was implemented on `feature/navigation-recency`,
alongside navigation/status work. Integration and guarded stable publication
were authorized on 2026-11-11; actual source commit, workflow and release
evidence are recorded in `knowledge/work/main.md`. Publication is pending
until those checks finish. No Palm implementation, operator-service restart
or real model invocation is part of delivery.

The locally tested package still reports 0.9.0; that number alone does not
establish that the binaries include these tools. Use matching programs built
from the complete reviewed source until a newer publication is confirmed.
An older released installation does not gain the API from this document.

State migration adds schema 21. Test with isolated paths and matching builds;
an older program cannot reopen migrated state. Do not silently replace Palm's
operator coordinator. Final verification is recorded in the branch work document.

Matching local Linux debug programs (SHA-256, from the tested working tree):

| Program | Absolute path | SHA-256 |
| --- | --- | --- |
| `coco` | `/home/jan/coco/target/debug/coco` | `6b88b4f2f5d36839c6b87c8d937685b6f240597fb1fba509c984582e2fb71073` |
| `cocod` | `/home/jan/coco/target/debug/cocod` | `880e0ab270eaa31e2b6e9e971e617a223f2b59548492a3bcda1c184e9a126016` |
| `coco-mcp` | `/home/jan/coco/target/debug/coco-mcp` | `7daa4c8cb10479a3e36f0232846bc65f2f7fedebc552c53619a97aef844a46cc` |

These were built by the full all-target gate below. Rebuilding can change hashes.
Do not substitute an older release that happens to report 0.9.0. The matching
source commit and confirmed release will be recorded with delivery evidence.

## Public API

Start one fixed-repository MCP process with only required grants, for example:

```text
coco-mcp --repository /absolute/path/to/project --allow-delete --allow-close --allow-reopen
```

These grants are independent of create, send, interrupt and signal emission.
`workspaceDeletions.get` is always read-only. Ordinary headless operation needs
no confirmation host; interactive clients may add `--confirm-changes`.
AI Ask uses native server-originated confirmations by default.

### Delete and preview

Tool `workspaces.delete` requires `workspace` (repository-local name or UUID)
and caller-selected `operationId`. Optional booleans default false:
`keepThread`, `keepBranch`, `discardChanges`, `discardUnretainedCommits`,
`dryRun`. Optional `expectedPlanSha256` pins a checked preview.
Prefer Palm's stored exact workspace UUID over a name for mutations, especially
state-sensitive close/reopen after a lost response.

```json
{
  "workspace": "run/build-widget",
  "operationId": "palm-cleanup:run-42",
  "keepThread": true,
  "dryRun": true
}
```

This returns `state: "preview"`, `plan` and `planSha256` without claiming the
key. Apply the same semantic request with `dryRun: false` and optionally the
returned digest. The default is removal of workspace/worktree, native thread
and verified CoCo-created branch; adopted branches remain. Keep options choose
retention. File/commit discard must be explicit and separate; guards, active
turns, uncertain starts, UI leases, background commands and context dependants
still block. No process-kill or close-as-cancellation fallback exists.

### Recover the outcome

Tool `workspaceDeletions.get` accepts only:

```json
{"operationId": "palm-cleanup:run-42"}
```

Response in `structuredContent`:

- `operationId`, `state`, `repositoryId`, `repositoryPath`;
- `requestSha256`, `plan`, `planSha256` (null for unknown);
- `errorCode` and observation timestamps `createdAtMs`, `updatedAtMs`.

The checked plan contains the exact `workspaceId`, name, managed path, HEAD,
branch/thread identities, thread disposition, selected removal effects, local
change/commit counts and any blockers. It contains no conversation or file
content. Treat hashes as opaque; retain the original request with its key.

States:

| State | Adapter handling |
| --- | --- |
| `preview` | Read-only plan; no claim or completion |
| `preparing` | Reserved dispatch; wait/inspect, never redispatch |
| `deleting` | Accepted cleanup not yet confirmed complete |
| `deleted` | Confirmed requested cleanup, independent of retained resources |
| `rejected` | Rejected before retirement acceptance; inspect safe error code |
| `uncertain` | Partial effects/completion may be unknown; operator/recovery inspection |
| `unknown` | No scoped evidence, not proof of non-execution |

Identical retries return the retained result/attempt without resolving a reused
name or dispatching again. The key is installation-wide in its own deletion
namespace. Another workspace reference or changed keep/discard policies under
the same key gives `IDEMPOTENCY_CONFLICT`. Explicit false and omitted boolean
defaults are equivalent. Dry-run/expected-plan preconditions do not change
intent; replay wins over a new precondition. A matching completion that overtakes
another caller's planning also wins over a missing-resource planning error.

Invalid input, scope/target failure, changed reviewed plan and transport errors
can occur before a receipt exists. Once claimed, action errors become receipt
states with a safe `errorCode`. **A successful tool call does not mean deleted.**
Check state. Stable errors include `WORKSPACE_NOT_FOUND`, `IDEMPOTENCY_CONFLICT`
and `WORKSPACE_RETIREMENT_BLOCKED`; rejection codes can include guard denial.
Foreign lookup returns unknown and never discloses another repository's receipt.

### Close/reopen

`workspaces.close` requires `workspace`. Optional `archiveThread`,
`discardChanges` and `dryRun` default false; optional `expectedPlanSha256`
requires a preview. Response: `workspace`, `plan`, `planSha256`, `applied`.
Close retains workspace identity, branch, conversation and send-operation
receipts. Detached commits require a retaining ref; there is no commit discard.

`workspaces.reopen` takes only `workspace` and returns `workspace`. It restores
the same checked identity/path/binding and starts no turn. These two tools do
not accept operation IDs or create deletion-style receipts. After a lost reply,
inspect `workspaces.status`. An already-applied transition returns
`INVALID_WORKSPACE_STATE` on repetition; do not interpret that alone as proof
that a particular earlier caller performed it.

## Palm safety and retention

- Record required `turns.get` outcomes before deletion. Delete removes CoCo's
  send-operation/outcome receipts **even with `keepThread`**. Keeping the native
  conversation does not keep that lookup boundary. Creation receipts instead
  retain their `deleted` tombstone; signal retention remains independent.
- Deletion results survive real database reopen and workspace removal. Normal
  startup recovery may complete the associated pinned native/branch cleanup
  after verified worktree absence. If the worktree remains, recovery preserves
  it and records uncertainty; it never repeats file discard automatically.
- A pre-saga crash consumes dispatch permission without inventing delivery.
  A later unrelated CLI deletion cannot be attributed to the old uncertain
  operation. Do not create a replacement cleanup key automatically.
- Plan digests are preconditions, not locks or dirty-file content snapshots.
  There is no exactly-once effect, rollback, automatic business compensation
  or guarantee after state loss/backup restoration.
- Palm owns when cleanup is desirable and which artifacts/outcomes to retain.
  This API does not add scheduling, result validation or worker-binding changes.

Canonical public reference:
[`reference/workspace-lifecycle.mdx`](../../docs/src/content/docs/reference/workspace-lifecycle.mdx).
Internal rationale: [Public workspace lifecycle](../engineering/workspace-lifecycle.md).

## Verification

- `nix develop . --command cargo test --locked --offline --all-targets -j 2 --
  --quiet --test-threads=2`: **891 passed** (845 library, 46 integration), no
  failures. Paid/manual/native opt-ins remained ignored.
- New focused cases cover checked previews, default/keep/discard policies,
  dirty/ignored files and commit loss, busy/TUI/guard rejection, concurrent
  retry and overtaking completion, reused names/foreign UUID aliases, actual
  database reopen, crash gaps, partial native failure, scope-path removal and
  recovery before/after worktree removal. No blind replay or false attribution.
- Actual MCP processes test independent grants and invocation rejection,
  authoritative native-form reviews, exact confirmation/decline/unavailable
  handling and pinned application. The full daemon/MCP/Git proof uses a fake
  native peer and covers close/reopen/delete, read-only lookup, replacement
  preservation and foreign repository rejection. No real model call.
- Clippy with warnings denied, fmt, dependency policy/usage checks and static
  docs build/check passed. The migration preserves genuine schema-20 state and
  does not invent earlier deletion results. No Palm implementation is included.
