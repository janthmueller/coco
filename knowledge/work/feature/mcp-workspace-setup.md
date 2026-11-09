---
type: Working Document
title: "feature/mcp-workspace-setup: recoverable public workspace setup"
description: Tracks public idempotent MCP creation, durable creation lookup and immutable worker MCP binding for generic external clients.
tags: [work, branch, mcp, creation, recovery, worker-config]
status: complete
branch: feature/mcp-workspace-setup
updated: 2026-11-09
---

# Public workspace setup and immutable worker MCP binding

## Intended outcome

Implement the generic CoCo capabilities requested by Palm's execution-setup
handoff, reusing creation, Git safety, profile provenance and restart behavior.
Palm remains an external client; no Palm code, workflow or scheduling changes.

## Active work

- [x] Inspect existing creation, persistence, MCP controls and native config
  behavior; verify the actually installed Codex version and model-free seams.
- [x] Define the smallest public create/lookup contract, durable request
  evidence and deletion-safe idempotency without mirroring conversations.
- [x] Add bounded private file/hash worker bindings separate from execution
  profiles; verify native merge, approval and start/resume behavior. Reject
  unsupported context/binding combinations rather than broadening authority.
- [x] Cover concurrency, lost replies, process cancellation, restart,
  tombstones, missing/changed sources, scope/permission and redaction failures.
- [x] Update public reference and canonical knowledge; provide a return
  handoff with actual API, local commit, matching builds and verified limits.
- [x] Fix the confirmed public/private claim race with transaction-level
  ownership checks, and keep private creation from loading a public recipe.
- [x] Retain minimal evidence for every known worker-bound native thread,
  including unconfirmed sends, across restart, operation cleanup and deletion.
- [x] Add permanent model-free regressions, rerun the affected gates and update
  the contract and Palm handoff.
- [x] Record the now-authorized local correction checkpoint in the Palm
  handoff, verify matching builds and provide the Palm adapter next steps.
  No push or publication is authorized.

## Decisions and scope

- Navigation was initially uncommitted and is preserved in full. The user
  explicitly approved a separate local checkpoint: `a47f368e347459b1407812fcd8e1762465ecbab1`
  (`feat: add shell-integrated workspace navigation`). This branch starts on
  that clean checkpoint; local main contains it too. No push or release.
- Source request is Palm's `knowledge/handoffs/coco-execution-setup.md` in its
  existing `init-palm` worktree. Read that handoff plus the linked setup
  proposal, integration contract and operator guide. Do not modify Palm.
- Public MCP creation needs its own explicit `--allow-create`; send permission
  and default read-only access must not grant creation. A receipt records
  accepted setup evidence, not tool readiness or successful model execution.
- Prefer the fresh-context, permanent-workspace MVP. Fork/rebind, registries,
  gateways, automatic Palm activation and automatic destructive compensation
  are outside scope unless native proof establishes a necessary safe extension.

## Findings

- Installed Codex reports `codex-cli 0.166.0`; version probing required no
  credentials or model call. Published documentation can be newer: exact
  native behavior must be established by isolated tests against that binary.
- Existing coordinator creation correlates a client operation ID and compares
  the creation request, but lookup is private and workspace deletion removes
  the original create-key row. Reuse the existing creation flow, extending
  only the durable evidence needed to keep retired keys non-reusable.
- A direct isolated 0.166.0 probe proved recursive native merging for both
  nested and dotted thread config overlays. An empty `env` object retained an
  old base value; `null`/string replacement failed parsing. Do not claim full
  slot replacement. The implementation rejects selected positions already in
  the execution profile or effective native `config/read` for that cwd, and
  repeats native checks on activation/resume. This is cooperative local
  configuration, not atomic protection against same-UID config writers.
- Public `workspaces.create` supports existing Git worktree selections and
  explicit local-change policy with fresh context and permanent lifetime.
  `workspaceCreations.get` is read-only; `--allow-create` independently grants
  creation. No new CLI product command, Palm adapter or workflow activation.
- Schema 19 adds a minimal non-expiring receipt/tombstone. Public setup claims
  an intent before Git effects; linking, successful preparation, native thread
  binding and deletion update evidence in their existing SQLite transactions.
  A cancelled/stranded intent is never stolen or replayed. Older/private create
  keys are retained with `unavailable` request evidence, not invented hashes.
- Bindings persist only canonical source path and exact-byte SHA-256. Private
  regular JSON files are bounded to 64 KiB and a strict five-field stdio
  recipe. Parser details/recipes are excluded from binding errors. Missing,
  changed or symlink-swapped sources fail closed; successful create replays
  remain historical receipts, not current connection health checks.
- Worker-bound native loading is configured by the coordinator. Forking a
  context known to carry one of these bindings is explicitly
  blocked, including native thread-ID references, rather than inheriting an
  old grant accidentally. Registry, rebind and remote file transport deferred.
- Review identified partial evidence loss: a missing creation receipt cannot
  authorize adoption/rebinding of an existing workspace. Added an independent
  binding-required workspace marker and durable retired-binding marker so
  loading fails closed and retained native conversations stay excluded as
  fork sources even after receipt loss/explicit cleanup. Lookup distinguishes
  missing receipt, missing target and foreign/unknown keys.
- Final native probe supersedes the initial fresh-jump materialization idea:
  0.166.0 reports a path for empty threads but rejects resume with no rollout,
  even when loaded. Guard fresh worker-bound jump before any native start with
  `WORKER_MCP_FIRST_SEND_REQUIRED`; first send reuses the existing acceptance-
  before-binding path, then ordinary jump/resume works. This keeps the public
  Prepare/Create → Send contract small and avoids stranding an empty thread or
  introducing raw configuration transport into the TUI relay in this slice.

## Verification

- Navigation's earlier verified gates remain recorded in `knowledge/work/main.md`.
- `cargo check --offline -j 2`: passed.
- `cargo test --offline -j 2 --lib setup -- --test-threads=2`: final 16 passed.
  Includes scope, concurrent calls, conflicts, missing sources, profile/native
  collisions, immutable activation and actual database close/reopen/tombstones.
- Full runs found test-fixture migration assumptions, expected tool listings
  and a cleanup fixture that had not released its own simulated TUI attach.
  Corrected the fixtures. Final `cargo test --offline -j 2 --all-targets -q -- --test-threads=2`:
  752 library tests, 4 navigation process tests, 7 control-plane process tests
  and 11 status process tests passed. Live/model tests stayed ignored.
- `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --offline -j 2 --test real_codex_compat setup:: -- --ignored --nocapture --test-threads=1`:
  3 passed against installed 0.166.0, including native auto-vs-approve behavior,
  base-slot collision rejection, real worker submits, cross-grant denial,
  resume and daemon restart. Uses only scripted loopback responses.
- `cargo clippy --offline -j 2 --all-targets -- -D warnings` and
  `cargo fmt --all --check`: passed. `git diff --check`: passed.
- `pnpm --dir docs check`: zero errors/warnings/hints. A pnpm build invocation
  hit a read-only package-manager cache; invoked the already installed Astro
  entrypoint directly instead. Static `/coco` build and export verifier passed
  (19 rendered pages, 66 static files, Pagefind/routing/public boundary).
- `cargo build --offline -j 2 --bins`: passed. Matching Linux debug artifacts,
  exact implementation checkpoint `8cee678774ecb5ec74efa2462d2da3cb089e0c29`
  and SHA-256 values are in the [Palm return handoff](../../handoffs/palm-workspace-setup.md).
- Final staged credential-pattern check found no key/token material. Static
  export verifier explicitly includes the new integration reference page.

## Handoff and boundaries

### Follow-up review completed, 2026-11-09

- Reviewed the complete setup slice and uncommitted corrections: claim
  ownership/replay/insertion, repository isolation, migration and retention,
  immutable recipe reads, first-send acceptance, native loading and TUI resume,
  context-fork exclusion, public inputs/permissions and documented limits.
  **No additional actionable correctness finding confirmed.**
- Reran `cargo test --offline -j 2 --lib setup -- --test-threads=2`:
  **22 passed**; `store::tests::workspace_creations`: **7 passed**;
  `worker_mcp`: **4 passed**, with the same Cargo/test concurrency bounds.
- Temporarily tightened the installed-native setup scenario to require the
  fixture worker's exact `outside this run grant` refusal after a TUI-like
  remote resume, rather than accepting any `isError`. The model-free
  `setup::public_setup_keeps_two_worker_grants_separate_across_start_resume_and_restart`
  scenario passed against Codex 0.166.0. This confirms the worker was present,
  not just that an absent server refused a call. Only scripted loopback
  responses and isolated state were used; no real model or credentials.
- Removed that temporary assertion after the probe; the original test file
  has no diff. Product code and all preexisting edits remain unchanged by
  this review. `git diff --check` passed. The earlier full-suite/Clippy/docs
  results above remain the last full gates; they were not rerun in this turn.
- Remaining boundaries are unchanged: only verified 0.166.0 worker loading,
  fresh worker-bound first-send requirement, no bound-context fork/rebind,
  no reconstruction of lost native IDs, and no protection against direct
  same-UID native/config writers. No commit, push, publication or user-state
  mutation. Only this review record is retained from the review turn.

### Post-checkpoint review, 2026-11-09

At this checkpoint the user requested review only, so product code was left
unchanged. The previously green baseline missed the following two confirmed
cases. The subsequent authorized fixes and final gates are recorded below.

- **Confirmed P2: public/private create claim race.** Hold `context_dependencies`,
  poll an ordinary private create until it owns the repository gate, then poll
  public setup with the same operation ID but another name/model. Public setup
  saves its claim while waiting for the gate. When the private create continues,
  `observe_workspace` checks repo/target IDs but not claim ownership/request
  fingerprint; it consumes the public receipt and records `configured` for
  `private-target` instead of requested `first-workspace`. The public path's
  subsequent idempotency error cannot reject that already-configured receipt.
  Enforce the expected creation claim/request at atomic workspace association
  across both create entry points, not just the earlier prechecks.
- **Confirmed P2: unconfirmed first-send thread bypasses the fork exclusion.** A
  worker-bound fresh workspace with an ambiguous `turn/start` reply can retain
  the exact native thread ID only in `operations.native_thread_id`, while the
  workspace and creation receipt remain unbound. `thread_has_worker_mcp_binding`
  consults only those latter two records. The fixture confirmed `bound_guard=false`
  and a successful raw-thread context fork after `OPERATION_UNCERTAIN`. Include
  durable dispatching/uncertain native associations in the exclusion and preserve
  necessary evidence through cleanup. Actual native grant inheritance was not
  asserted by this review; the demonstrated bug is bypassing CoCo's documented
  known-worker fork prohibition.
- Reproduced using two temporary model-free coordinator tests:
  `cargo test --offline -j 2 --lib coordinator::tests::setup::review:: -- --nocapture --test-threads=1`.
  Both intended-invariant assertions failed, deterministically (2 failures,
  no model calls). The probes were removed from tracked test modules afterward;
  their source is retained locally at `/tmp/coco-mcp-workspace-setup-review.rs`
  for the follow-up fix. No application-code correction is authorized in this
  review turn. After removing the probes, the original focused command
  `cargo test --offline -j 2 --lib setup -q -- --test-threads=2` passed all
  16 existing tests; `git diff -- src` was empty. Only the review record changed.

### Authorized corrections, 2026-11-09

- The user then authorized both fixes. Creation now carries its public claim
  explicitly; private creation never reads a newly appeared public worker
  recipe. Store replay checks and the insertion transaction reject another
  owner, a missing/foreign/retired claim or a different request hash. A waiting
  private caller cannot consume a later public intent, including when core
  name/model choices happen to match. An insertion loser leaves the receipt,
  workspace rows, events, branches and Git worktrees unchanged; existing
  planning can create empty managed parent directories.
- Schema 20 adds two-column, non-cascading native worker identity evidence.
  Record known IDs after native start and in the dispatch transaction, before
  calling native turn/start. Keep all observed IDs after uncertain or rejected
  sends, operation cleanup and workspace deletion. They restrict forks but do
  not manufacture an accepted turn or confirmed workspace/receipt thread ID.
  Backfill surviving workspace, operation and deleted-receipt evidence; never
  invent an ID that was not returned or was lost before migration.
- Permanent focused children now cover private/public races with same/different
  requests and worker recipes, foreign repositories, atomic rollback, late
  native materialization, actual database reopen, cleanup, rejected attempts,
  empty known threads, ordinary unbound threads and v19 migration.
- Final `cargo test --offline -j 2 --lib setup -- --test-threads=2`: **22 passed**.
  Final `cargo test --offline -j 2 --all-targets -q -- --test-threads=2`:
  **762 library + 4 navigation + 7 control-plane + 11 status = 784 passed**;
  7 library, 13 native compatibility and 1 live-model test remained ignored.
  Includes all 7 Store creation/identity cases. An early new fixture reused a
  unique operation fingerprint; corrected the fixture before the green run.
- Final Clippy (`--all-targets --all-features -- -D warnings`), Rustfmt and
  whitespace checks passed. Cargo-machete found no unused dependency; no new
  dependencies were added. Targeted credential-pattern checks found no secrets.
- The three opted-in `real_codex_compat setup::` tests passed again against
  installed Codex 0.166.0 using scripted loopback Responses only. The sandbox
  initially denied local sockets; the authorized outside-sandbox repeat
  passed. No real model, credentials or user state was used.
- Installed Astro entrypoint `check`: zero errors/warnings/hints. Static
  `/coco` build and export verification: **19 pages, 66 files**, including
  Pagefind, routing and the public-only boundary. No publishing occurred.
- Rebuilt all three local debug binaries with `cargo build --offline -j 2 --bins`.
  The updated Palm handoff records corrected artifact hashes and explicitly
  distinguishes these uncommitted corrections from the baseline source commit.

- Implementation is committed locally on this branch, based on the separate
  navigation checkpoint. Return handoff and final work record are a subsequent
  documentation-only checkpoint; they do not change the verified binaries.
- Initial generic CoCo implementation and the two review corrections are
  committed locally as `8c2e3aebaa20e84b9c480726ad5d05d232fe0578` after the
  user's explicit request. Refreshed the three bin builds and rechecked their
  hashes; exact values and the concrete Palm adapter sequence are in the
  updated return handoff. Its checkpoint metadata follows as a documentation-
  only commit. Rustfmt and staged whitespace/credential-pattern checks passed.
  The existing full verification and subsequent focused review remain valid;
  no product change was made after those gates. No push or release.
  Palm's adapter, activation, real-run integration
  and result validation remain Palm work. Fresh worker-bound first-TUI support,
  safe bound-context forks/rebinding, remote source transport and additional
  native-version verification remain explicit unsupported extensions.

- No real model calls, user configuration edits, installation changes or
  actions against user workspaces are authorized. Use isolated files/repos/
  stores and scripted loopback providers/MCP servers. Run heavy gates
  sequentially. Do not push or publish.
