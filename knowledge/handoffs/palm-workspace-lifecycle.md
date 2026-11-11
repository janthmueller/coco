---
type: Integration Handoff
title: CoCo workspace cleanup for Palm
description: Describes the additive public cleanup tools, permissions, durable deletion outcomes and integration safety limits.
tags: [handoff, palm, mcp, cleanup, recovery]
status: released
updated: 2026-11-11
---

# Workspace cleanup: return handoff to Palm

## Availability

Available in **CoCo 0.10.0**, published on 2026-11-11:
[GitHub release](https://github.com/janthmueller/coco/releases/tag/v0.10.0).
The implementation commit is `a2b94cc5d895e646f314efc367155dadacc4ec8a`;
tag v0.10.0 points to the CI version stamp
`a3e68a8832a91f6b508e0e92610effad38bc6bcc`. Workflow and registry evidence
are recorded in `knowledge/work/main.md`. This is generic CoCo cleanup,
available to Palm and `coco ai`; no Palm implementation, operator-service
restart or real model invocation was part of delivery.

Use matching 0.10.0 `coco`, `cocod` and `coco-mcp` programs from the release,
Cargo package or tagged source. Earlier released versions do not expose these
tools. Pre-release local debug programs may still report 0.9.0; rebuild from
the tag rather than treating that old version number as capability evidence.

State migration adds schema 21. Test with isolated paths and matching builds;
an older program cannot reopen migrated state. Do not silently replace Palm's
operator coordinator. Source verification is recorded in the branch work document;
delivery verification is recorded in main's working document.

Verified release archives (SHA-256):

| Archive | SHA-256 |
| --- | --- |
| `coco-linux-ubuntu-22.04-x86_64-v0.10.0.tar.gz` | `15c019816d81dae409d489e91a52e6936fd819585305497eb973da71ecafa5da` |
| `coco-macos-15-arm64-v0.10.0.tar.gz` | `6e23522d5957fe7ba7e9063b420137853942e08d817642638a959665a25f9038` |

Each archive contains all three executables, README and license. Registry
publication and both native version/help smoke tests passed; downloaded
archives independently match the published checksums. The Cargo sparse index
also confirms non-yanked `codex-coordinator` 0.10.0.

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
