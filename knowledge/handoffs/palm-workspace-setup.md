---
type: Integration Handoff
title: CoCo workspace setup for the Palm adapter
description: Gives Palm the verified public creation, lookup and immutable worker configuration contract, matching local builds and recovery limits.
tags: [handoff, palm, mcp, creation, recovery]
status: ready
updated: 2026-11-09
---

# Workspace setup: return handoff to Palm

## Checkpoint and matching programs

Implemented in **CoCo only** and published in **0.8.0**:

- Matching released source: `v0.8.0`, commit
  `d51541e8e7037b2d3f4762d2e5f0ae088fe9c0b4`.
  It preserves remote main `3451d27` and all published 0.7.0 behavior/tests
  alongside the local navigation and reviewed workspace-setup additions.
  Use this source checkpoint or a later documentation-only descendant for the
  matching builds below. The setup API and its guarantees are unchanged by
  integration. No Palm files were changed.
- The checkpoint includes both reviewed corrections: atomic public/private
  claim ownership and durable exclusion of known unconfirmed worker threads.
  Use the complete implementation, not an earlier partial setup prototype.
- The source is on `main`. It includes patched rusqlite 0.41.1 /
  libsqlite3-sys 0.39.1 and rmcp 3.9.0; tool contracts and permissions are
  unchanged. [Regular GitHub release and binary archives](https://github.com/janthmueller/coco/releases/tag/v0.8.0)
  and [the non-yanked Cargo package](https://crates.io/crates/codex-coordinator/0.8.0)
  are independently verified. The earlier GitHub service interruption is resolved.
  No installation change, user service restart or real model call.

Matching **local debug builds**, Linux x86_64, Rust 1.98.1, all reporting 0.8.0:

| Program | Absolute path | SHA-256 |
| --- | --- | --- |
| `coco` | `/home/jan/coco/target/debug/coco` | `6156cb98cfccb66a9b6ea7c0687aca1dd1876a0205a10ecc876eeff78e3d73e5` |
| `cocod` | `/home/jan/coco/target/debug/cocod` | `6bbc14ccbc3a5fee22e3b12269088ac331bab2364126a93e0c092a6224766951` |
| `coco-mcp` | `/home/jan/coco/target/debug/coco-mcp` | `e32ce247137570f729baceadfe7f9add76aaf85a84ef8be050d0835c95bb3ba8` |

Build command used: `cargo build --locked --offline -j 2 --bins`. These local
debug builds use the tagged 0.8.0 source; they are not the downloaded optimized
release binaries. Use matching explicit paths or install the complete published
package, never mix versions of `coco`, `cocod` and `coco-mcp`. Rebuilding can
change artifact hashes. The builds were refreshed after integrating the
generated release metadata and all three hashes/version outputs were checked.

Use isolated CoCo state/socket paths and temporary repositories for adapter
tests. Both processes must use the same `COCO_*` paths; use an isolated
`CODEX_HOME` with a scripted loopback provider for any model-free send test.
Do not replace an operator's coordinator or reuse their live state silently.
This code migrates state to **schema 20**; old released binaries cannot read
that newer schema. No user database was migrated as part of this work.

## Public API and permissions

Run the matching coordinator, then one stdio MCP server per allowed repository:

```text
/home/jan/coco/target/debug/coco-mcp --repository /absolute/path/to/project --allow-create --allow-send --allow-interrupt
```

Grant only needed permissions. `--allow-create`, `--allow-send` and
`--allow-interrupt` are independent; default MCP is read-only. `coco mcp serve`
accepts the same options. Create permission also grants local executable worker
configuration and belongs only to trusted operators, not run-bound workers.

### Create

Tool: `workspaces.create`. Example arguments:

```json
{
  "operationId": "palm-setup:run-42",
  "name": "review/login",
  "worktree": {
    "kind": "newBranch",
    "base": {"kind": "revision", "revision": "HEAD"}
  },
  "changes": "reject",
  "profile": "default",
  "workerMcp": {
    "path": "/private/palm/setup/run-42/worker.json",
    "sha256": "<SHA-256-of-exact-worker.json-bytes>"
  }
}
```

Required: `operationId`, `name`, `worktree`, `changes`, `profile`.
Optional: `model`, `workerMcp`. The hash placeholder must be replaced with 64
lowercase hex digits. Context is fresh, lifetime permanent; fork/context,
temporary and repository override fields are rejected. Existing new-branch,
detached and existing-branch selections and local-change policies are reused.
The name is the frozen planned name supplied by Palm, not derived by CoCo from
a later run ID. Creation registers the repository if needed but starts no turn,
TUI or Palm activation. Fresh `codexThreadId` is normally null.

### Lookup

Tool: read-only `workspaceCreations.get`, arguments:

```json
{"operationId": "palm-setup:run-42"}
```

Both tools return a receipt in MCP `structuredContent`: `operationId`,
`requestSha256`, `state`, `repositoryId`, `repositoryPath`, `workspaceId`,
`workspaceName`, `baseSha`, `worktreePath`, `branchName`, `profile`, `workerMcp`,
`codexThreadId`, `errorCode`, `createdAtMs`, `updatedAtMs`. Nullable fields remain
explicitly null. Profile evidence includes name/source path/hash/model only,
never the expanded config. IDs for creation, send, workspace, native thread and
native turn are distinct. `requestSha256` is opaque; it is neither Palm's
manifest hash nor the worker-file hash.

Full inputs, responses, worktree variants, errors and examples:
[`workspace-setup.mdx`](../../docs/src/content/docs/reference/workspace-setup.mdx).

## Idempotency and recovery

- Persist Palm intent/key before dispatch. Identical calls return the original
  association/evidence without recapturing a moving branch. Changed inputs,
  including worker path/hash, fail `IDEMPOTENCY_CONFLICT`. Name collisions never
  adopt an unrelated workspace. Public/private create paths cannot take over
  one another's reserved key, even while an earlier caller waits at a gate.
  Claim ownership is checked in the workspace-insertion transaction, before
  branch/worktree creation; empty managed parent directories can already exist.
- `configured`: saved setup confirmed. This does not assert MCP initialization,
  model success or Palm activation. Later lookup can gain a confirmed thread ID.
- `preparing`: intent exists but completion is unconfirmed. Identical retry
  observes it and never re-executes. Poll or inspect; do not mint a new key.
- `rejected`: known rejection before association; `failed`: known preparation
  failure or interrupted linked preparation. Inspect resources before deciding
  replacement/cleanup. `errorCode` is safe; raw diagnostics are not returned.
- `deleted`: CoCo removed the workspace, possibly retaining resources according
  to its explicit delete policy. The retired key remains non-reusable.
- `unknown`: no scoped receipt. `unavailable`: missing public provenance or
  partial local evidence, including legacy/private creations. Neither is proof
  of no side effects. Do not recreate or destroy automatically.
- `CREATION_RECEIPT_MISSING` means an existing target has lost its original
  receipt; no adoption/rebind is allowed. `CREATION_TARGET_UNAVAILABLE` means
  a retained receipt has lost its target record. Missing worker binding evidence
  blocks loading with `WORKER_MCP_BINDING_UNAVAILABLE`.
- Receipts/tombstones have no expiry and survive close, restart and ordinary
  deletion. Local data loss/restoration can lose evidence. No exactly-once Git
  guarantee or native-history reconstruction of a lost creation key is claimed.
- Scope is checked on create, retry and lookup. A known foreign key reveals
  no foreign receipt and cannot create/reconfigure through another repository.
- Client cancellation/lost reply can leave a workspace or stranded intent.
  CoCo performs no destructive compensation or silent restart of that intent.

Malformed inputs/conflicts return an MCP error. Known setup failures usually
return a receipt with `rejected`/`failed` and codes such as `WORKSPACE_EXISTS`,
`DIRTY_SOURCE`, `PROFILE_NOT_FOUND`, `WORKER_MCP_BINDING_INVALID`. Missing create
permission removes the route; clients must handle the MCP rejection, not assume
a custom permission code. Keep the original creation and send keys separate.

## Worker configuration

The source file has this exact small shape, using Palm's own executable:

```json
{
  "mcp_servers": {
    "palm_worker": {
      "command": "/absolute/path/to/palm",
      "args": ["--db", "/private/palm/palm.db", "mcp", "serve", "--worker-run", "run-42"],
      "enabled_tools": ["run.submit"],
      "default_tools_approval_mode": "approve",
      "required": true
    }
  }
}
```

CoCo tests used a generic fixture worker, **not Palm**. Palm must still test
its own exact-run rejection and result submission against this API.

The recipe must be credential-free UTF-8 JSON, private regular symlink-free
file, canonical absolute local path, at most 64 KiB. On Unix use mode 0600 or
0400. Only the five shown fields and 1–16 stdio servers are accepted; no env,
HTTP/auth/unknown settings. Hash **exact bytes**, including newlines. CoCo
stores only source/hash and non-secret evidence, not the raw recipe. An
arbitrary argument can hide a secret, so callers must keep arguments clean.

Native 0.166.0 merges same-slot fields recursively, including old env values.
CoCo rejects profile/native base collisions and ambiguous normalized names,
then rechecks at native loading. Choose a slot not already defined in base or
execution profile. Each workspace gets its own runtime selection; two workspaces
can use the same slot name with distinct run grants. Other intentionally
configured profile/base tools remain available.

Source/hash are immutable and reloaded for activation/resume and already-loaded
sends. Missing/changed/symlink-swapped files fail closed, with no cached fallback,
hot reload or rebind. An identical regular file with the same bytes is the same
recipe; inode identity is checked across each read, not permanently pinned.
This assumes a shared local filesystem, not remote file transfer. It is local
capability shaping, not an OS boundary against same-UID clients/config editors.

Important native limits:

- Tested/accepted worker binding backend: **Codex 0.166.0**. Other native
  versions fail closed until verified; the ordinary CoCo version alone is not
  evidence of this capability.
- `auto` can pause mutating tool calls for approval. For unattended narrow
  run-bound submit, choose **`approve` explicitly**. CoCo does not rewrite
  native modes or auto-answer these prompts, and its decide surface does not
  project this MCP approval. The paired native tests prove the difference.
- A fresh worker-bound workspace must first receive a send before `jump`:
  `WORKER_MCP_FIRST_SEND_REQUIRED` prevents an unresumable empty thread from
  being bound. The native probe proved even a reported path does not mean a
  rollout exists. Post-send attach/resume is tested. Palm's Prepare → Send
  flow is supported; fresh-TUI recipe injection is outside this slice.
- Context forks from a known worker-bound native conversation are rejected,
  including raw thread IDs from unconfirmed first sends and retained deleted
  targets. Known native identities remain excluded across restart, operation
  rejection/cleanup and workspace deletion. This evidence does not invent a
  confirmed `codexThreadId` in a setup receipt or confirm turn acceptance.
  IDs never returned by Codex, or lost before upgrading, cannot be recovered
  without surviving local evidence. Public setup exposes no fork/rebind.
  Setup and execution/cleanup remain separate decisions.

## Verification and next Palm work

Passed:

- `nix shell --offline --inputs-from . nixpkgs#zsh nixpkgs#fish --command cargo test --locked --offline -j 2 --all-targets -q -- --test-threads=2`:
  788 library, 4 navigation, 7 control-plane and 11 status process tests pass
  on the merged tree (810 total); explicit opt-in/model tests remain ignored
  in this ordinary run. New tests include actual database reopen/migration, held-gate
  concurrent calls, cancelled clients, retired keys, lost receipts/targets,
  source drift/symlink substitution, scope, grants and safe error contents.
  The review added 10 regressions, including public/private claim collisions,
  missing/foreign/retired claims, late materialization after unconfirmed send,
  multiple attempted identities, rejection/reset, deletion-safe exclusion and
  genuine v19 identity backfill. Remote 0.7.0 regressions are retained unchanged.
- All 15 installed-0.166.0 compatibility fixtures pass in explicitly selected,
  sequential model-free subsets: two temporary-TUI cases; ten setup/context/
  provider/turn cases; three exact lifecycle/MCP/hook cases. Setup's three
  fixtures can be repeated using
  `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked --offline -j 2 --test real_codex_compat setup:: -- --ignored --nocapture --test-threads=1`.
  They use scripted loopback Responses,
  actual native worker submits, cross-grant denial, allowlist filtering,
  auto/approve behavior, native slot collision, post-send opening and coordinator
  restart/resume. No real model or user credential was used.
- Clippy all targets/all features with `-D warnings`, formatting, diff checks,
  cargo-machete (no unused dependencies), and cached/offline cargo-deny pass.
  Existing duplicate-version policy warnings remain; no fresh online advisory
  fetch was claimed for the earlier integration run. All 19 release-tooling
  unit tests also pass.
- After the authorized security update, all 810 ordinary tests, 15 selected
  model-free Codex 0.166.0 fixtures, real terminal probe and nine shell PTY
  scenarios pass again. Fresh online cargo-deny now passes all policy groups
  without advisory exceptions. Clippy, formatting, dependency-use checks and
  Git-flake evaluation pass; no test expectation or schema was weakened.
- Prior feature review found no new actionable issue: 22 setup, 7 Store and
  4 recipe checks passed before integration and remain covered by the full
  merged suite. One temporary stronger native assertion also
  confirmed that the original worker itself refused the foreign run after
  a TUI-like remote resume, rather than merely observing an absent-server
  error. That isolated scenario passed; the temporary assertion was removed.
  No setup product behavior changed during integration.
- The unchanged remote terminal probe passes picker resize/cancellation,
  creation Tab defaults with explicit final approval, y/n confirmations and
  live-status frame restoration. All nine Bash/Zsh/Fish navigation PTY cases
  pass against the matching CLI build.
- Astro check: zero diagnostics. Static `/coco` export: 19 pages, 66 files,
  Pagefind, routing and strict public-doc boundary verified. Cargo bin builds
  passed; targeted credential-pattern checks found no key/token material.
  Git flake evaluation passes with `nix flake check --no-build --offline
  --no-update-lock-file`; package file listing includes the worker binding,
  creation and shell modules. Hosted exact-source Rust run `42673369545`,
  attempt 2, now passes for `0b4d185`, including fresh policy, registry-package
  dry-run, full Nix checks and native Linux/macOS binary build/version/help/
  archive smokes. Full behavioral/native-Codex tests on macOS are not claimed.
  After GitHub recovery, exact-source Docs run `42683202414` passes build and
  deployment. Stable rehearsal `42683548614` validates 0.8.0 and publishing run
  `42684117998` passes registry and both tagged native archive jobs. Independent
  downloads match GitHub digests and both SHA-256 sidecars; the released lockfile
  matches the tag. Crates.io and the Cargo sparse index agree on the non-yanked
  version/checksum; the crate's VCS metadata identifies exact tag commit
  `d51541e`, its original manifest matches the tag, required modules are present
  and root knowledge/docs/tests/automation are excluded. No user installation,
  service restart, real inference or guard bypass was performed.

### Next implementation in Palm

Implement the adapter **in Palm**, following its AGENTS.md and existing
prepare/activate contract. Do not access CoCo's private RPC/database or change
CoCo while implementing the client.

1. Persist the frozen setup request and creation operation ID before dispatch.
   Create Palm's private run-bound worker source outside the CoCo worktree;
   save its exact-byte hash separately from the Palm manifest hash. Keep the
   existing execution profile separate; never rewrite an older profile to
   grant a new run.
2. Call public `workspaces.create` through a repository-fixed MCP process with
   `--allow-create`. Supply the already planned workspace name and explicit
   code/worktree, local-change policy and profile choices. Treat the returned
   receipt as setup evidence, not activation, readiness or a successful run.
3. Persist the `configured` association and its evidence before offering the
   existing manual `run activate` path. Do not silently activate or send from
   preparation. No Codex thread ID is required before the first send.
4. On cancellation/lost response, reconcile the original creation key through
   `workspaceCreations.get` or an identical create retry. Never mint a new key
   or auto-delete on `preparing`, `unknown`, `unavailable` or ambiguous failure.
   Surface the distinctions and retain the original request/key for inspection.
5. On authorized dispatch, reuse the existing send adapter with its own send
   operation ID. Track the exact turn via `turns.get`; explicitly choose
   `mode: "bestEffort"` only if interruption is desired and permitted. Observe
   the unchanged uncertainty/scheduling rules in
   [`palm-turn-control.md`](palm-turn-control.md).
6. Add isolated, model-free Palm tests for lost replies, restart, unchanged and
   conflicting retries, scope/grants, tombstones, source drift and a scripted
   submit through **Palm's actual exact-run worker**. The CoCo fixture does not
   replace proof of Palm's own run authorization or artifact/result validation.

Use matching explicit local program paths and isolated CoCo/Codex state for
these tests. No operator database migration, service replacement, real model
call, push or release is authorized by this handoff. Palm retains ownership of
run activation, exact-run grants, scheduling, artifacts, outcomes and publication.

Implementation decisions and durable boundaries:
[`workspace-setup.md`](../engineering/workspace-setup.md).
