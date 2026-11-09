---
type: Engineering Contract
title: Recoverable public workspace setup and immutable worker MCP sources
description: Defines creation receipts, deletion-safe idempotency and native configuration provenance.
tags: [mcp, creation, recovery, idempotency, worker-config]
status: stable
---

# Public workspace setup

## Ownership and scope

The supported external surface is Control MCP: `workspaces.create` and
`workspaceCreations.get`. Repository scope is fixed at startup. The separate
`--allow-create` grants creation and local executable configuration, not send,
interrupt or signal publication. The exact public contract is
[`reference/workspace-setup.mdx`](../../docs/src/content/docs/reference/workspace-setup.mdx).

Public setup reuses ordinary Git discovery, local-state snapshots, worktree
planning, provisioning, profile checks and retirement. It offers fresh context
and permanent lifetime. CLI context/temporary creation remains independent.
No Palm adapter, activation, scheduler, result validation, destructive
compensation, registry, gateway or remote file transport is introduced.

`domain::setup` holds non-secret references/receipts; `protocol::setup` owns
typed DTOs. `mcp::setup` fixes scope and forwards/audits safe action/key/outcome.
Private RPC `workspace.setup` and `workspaceCreation.get` are not another public
SDK. `coordinator::setup` owns policy, `store::workspace_creations` owns atomic
evidence, and `worker_mcp` is a bounded filesystem/config adapter analogous to
the profile loader. Native config reads stay in `daemon::worker::setup` behind
`WorkerRuntime::verify_worker_mcp_slots`. No crate expansion or dependency
direction reversal is needed.

## Claim, associate, confirm

Hash the serialized typed request, not the caller's JSON formatting. Include
worker path/hash, execution/code/local-change choices and operation ID; compare
repository identity separately. Externally the hash is opaque. Creation keys
are globally unique in their own ledger, independently of send keys. Foreign
repository lookup reveals no receipt, even with a known key.

1. Discover identity read-only. A retained matching receipt is replayed without
   recapturing Git/native context. An existing workspace with missing original
   receipt is unavailable evidence, never eligible for a new binding/adoption.
2. Validate external worker bytes outside SQLite writer locks.
3. Atomically claim `preparing`. Only the winner executes; racing callers see
   retained intent, and changed requests conflict. The claim also checks for
   older workspace-only keys within the same transaction.
4. Use normal creation. Read profile/source before the repository execution
   gate; recheck the worker source before inserting a provisioned workspace.
   Workspace insertion atomically checks the expected public request hash,
   repository and unassociated `preparing` claim before associating evidence.
   Private creation must own a new key; it cannot consume a public intent that
   appeared after its initial check. Replays also check claim ownership, and
   only the public owner passes its worker reference to the profile loader.
   These checks precede branch/worktree creation; planning may already have
   created empty managed parent directories.
5. The existing successful fresh worktree transition atomically records
   `configured`. Later native binding updates that receipt in its binding
   transaction, without conversation content.

Known errors record `rejected` before workspace association or `failed` after
it, with only a safe code. Rejection can follow implicit repo registration.
Restart reconciliation marks unfinished provisioned workspaces/receipts
failed. An unassociated intent stays `preparing`: it is never stolen or
restarted. This is durable at-most-one dispatch permission, not exactly-once
Git. Cancelled clients/lost replies do not trigger cleanup. A missing target
record is `unavailable`/`CREATION_TARGET_UNAVAILABLE`; a missing receipt with
an extant target is `unavailable`/`CREATION_RECEIPT_MISSING`.

## Persistence and retention

Schema 19 adds `workspace_creations`: key, repo identity, request hash, optional
unique workspace ID, one non-secret receipt JSON and a worker-binding marker.
The absence of a workspace foreign-key cascade is intentional. Delete records
`deleted` atomically before removing metadata; retired keys cannot recreate,
even when a branch/conversation is retained. There is no TTL/LRU/generation
binding. Source/hash and minimal checkout/profile/native-ID evidence survive.

Repository identity does not require prior registration: invalid worker files
can be rejected before repository mutations. Ordinary repo removal preserves
identity. Migration reserves existing private/CLI keys as `unavailable` with
no invented public request hash. New private creates reserve keys too; while
alive, they keep their ordinary request-based replay, but cannot resurrect
after deletion or take over public intent.

An independent `workspaces.worker_mcp_bound` marker prevents source loss from
being interpreted as an unbound workspace. Missing binding evidence blocks
native load, including already-loaded sends. The durable receipt marker also
keeps retained bound conversations excluded as context-fork sources after
cleanup, even if the original source reference was lost.

Schema 20 additionally retains `worker_mcp_threads`: native thread ID and its
originating workspace ID, without a cascading foreign key, recipe, turn
history or conversation content. Save known identities immediately after a
native start response and in the same transaction as turn dispatch. Binding
observation also records them. These conservative exclusions survive failed
or rejected sends, multiple attempted threads, operation cleanup and workspace
deletion; they do not declare a confirmed workspace binding or accepted turn.
The migration backfills IDs still present in workspaces, operations and
retained receipts. IDs lost before upgrading or never received from Codex
cannot be reconstructed from this evidence.

Local data loss/restoration can still lose evidence. `unknown` means no scoped
evidence, not proof of no effects. Native history cannot recover a lost client
creation key. Missing evidence never justifies automatic new-key creation,
grant replacement or destructive compensation.

## Worker source and merge decision

Bindings are separate from execution-profile provenance. The immutable
reference is a canonical absolute UTF-8 path and lowercase exact-byte SHA-256.
Only that reference/minimal evidence is persisted. Recipes, command/args and
environments never enter CoCo receipts, events, audits or profile snapshots;
native Codex persistence is Codex-owned. In-memory recipe types deliberately
have no Debug/Serialize implementation.

Accept only private regular symlink-free JSON, at most 64 KiB, bounded local
stdio entries and the five required fields `command`, `args`, `enabled_tools`,
`default_tools_approval_mode`, `required`. Unknown settings, arbitrary env and
HTTP configuration fail closed. Unix uses no-follow/nonblocking open, bounded
reads and pre/open/post metadata/identity checks. Conservative normalized-name
checks reject ambiguous tool namespaces. Errors must not quote parser/native
recipe details. Secrets hidden inside argument strings are not detectable by
schema; credential-free recipes remain the trusted operator's responsibility.

An isolated Codex 0.166.0 probe proved recursive same-slot merging for nested
and dotted overlays. Empty `env` retained old values; null/string replacement
failed parsing. Therefore CoCo rejects colliding/aliasing positions in both
the execution profile and effective native `config/read` for the operative cwd,
instead of pretending to replace a slot atomically. Unverified native versions
or response shapes fail closed. Initial verification precedes Git mutation;
activation rechecks the actual worktree cwd.

On start/resume, reload exact source/hash, check profile provenance, compose
the memory-only overlay and verify base positions. Already-loaded sends also
revalidate. Missing/changed/symlink-swapped sources never fall back/hot-reload.
Create replay is historical evidence, not live source or connection health.
Intentionally configured other profile/base tools remain available: this is
not an exclusive global MCP allowlist.

An extra native probe proved that an empty 0.166.0 thread cannot be resumed
even while loaded: `thread/read` advertises a path but resume rejects absent
rollout. Therefore worker-bound fresh jump returns `WORKER_MCP_FIRST_SEND_REQUIRED`
before native start. The existing first-send path applies the binding and
confirms the workspace's thread association only after acceptance; ordinary
jump/resume then works. Minimal exclusion evidence is recorded earlier and
does not make an empty or unconfirmed thread resumable.
No raw recipe transport or fresh-TUI protocol injection is added for this MVP.
CoCo refuses forks of known worker-bound contexts,
including raw native IDs from unconfirmed first sends and retained deleted
targets. An uncertain send never bypasses exclusion merely because its thread
was not yet bound to the workspace. Public fork/rebind is
not offered; future expansion requires proof of no inherited old grant.

These are cooperative local capability controls, not protection against
same-UID processes, direct native clients or concurrent config editors. Native
base verification and loading are not atomic against an external writer.

## Native evidence and approvals

Opted-in `tests/real_codex_compat/setup*` fixtures use installed Codex 0.166.0,
temporary repos/HOME/CODEX_HOME, a generic run-bound stdio server and scripted
Responses events from loopback. They exercise public create/send, actual native
model-directed tool dispatch without a real model, disjoint grants, filtered
tool advertising, native resume, coordinator/database restart and safe collision
rejection. Records, audits and logs are checked for recipe leakage.

Native approval is preserved: a mutating `auto` fixture pauses for approval;
explicit `approve` executes its allowlisted submit. CoCo does not auto-answer
MCP approval/elicitation or interpret `auto` as unconditional preapproval.
Native TUI interaction handles prompts not projected by CoCo's decide surface.
Changing the mode changes the file hash and requires new creation, not rebind.

These tests establish this native version only. Official settings references:
[MCP configuration](https://learn.chatgpt.com/docs/extend/mcp) and
[App Server](https://learn.chatgpt.com/docs/app-server). Published docs alone
do not establish exact merge/approval behavior for another installed release.
