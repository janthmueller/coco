---
type: Working Document
title: "feature/navigation-recency: Navigation recency and public MCP cleanup"
description: Align native recency ordering and extend checked cleanup to Control MCP and AI.
tags: [work, branch, cli, navigation, mcp, retirement]
status: complete
branch: feature/navigation-recency
updated: 2026-11-11
---

# feature/navigation-recency — Recent directory and status ordering

## Intended outcome

Interactive `coco cd` and `coco path` offer the most recently active native
threads first, show the existing compact relative ages and support `--sort name`.
Entering a saved worktree remains possible when Codex metadata is unavailable.
Status additionally supports explicit recent-first multi-key ordering without
changing its default overview or adding a timestamp column or native request.

## Active work

- [x] Reuse workspace ordering and relative-time presentation without adding
  status columns to directory choices.
- [x] Add bounded, optional passive recency enrichment to path discovery;
  keep explicit targets and name sorting metadata-only.
- [x] Cover scope, fallback, timing, selection revalidation and shell output
  with isolated, model-free tests.
- [x] Update affected public instructions and canonical internal contracts.
- [x] Run sequential resource-limited verification and review the full diff.
- [x] Add the approved optional `recent` status sort key using existing native
  metadata, preserving multi-key ordering, unknown-last handling and defaults.
- [x] Verify direction, ties, invalid/missing dates, JSON/human output,
  filters and follow; update help and affected documentation.

## Decisions

- 2026-11-11 — Branch from local `main` at `b17e770` (0.9.0). Preserve the
  existing review note in `knowledge/work/main.md`. No commit, push, release,
  live model call or installed-service change is authorized for this slice.
- 2026-11-11 — Use native optional `recencyAt`, not CoCo mutation time or a
  new navigation history. Unknown dates sort after known dates, followed by
  natural name, repository path and exact workspace ID.
- 2026-11-11 — Optional recency must have a short total budget and cannot
  change worktree eligibility. Direct targets and `--sort name` retain the
  native-free path. Selection order and relative ages freeze before input.
- 2026-11-11 — Opt in through the private path-list DTO (`includeRecency`,
  default/omitted false; optional `recencyAtMs` on entries). A one-second
  budget and at most four passive reads keep navigation responsive without
  spawning processes, loading threads or persisting a second timestamp store.
  Partial dates stay attached to exact entry indices. A legacy daemon's
  `INVALID_PARAMS` permits one identical-scope metadata-only retry; other
  errors retain their normal meaning.

## Findings

- Before this slice, navigation used natural-name order and saved path metadata;
  other workspace pickers already share recency ordering and dim trailing ages.
- Existing `read_bound_thread` is passive and validates both thread ID and
  worktree path; generic workspace-list hydration is sequential and not bounded
  by a picker-level deadline, so directory discovery must not depend on it.

## Verification

- Focused first run: 22 tests passed; one new assertion expected
  `NOT_A_GIT_REPOSITORY` for a nonexistent directory, whose established error
  is `GIT_ERROR`. Corrected the test expectation without changing scope policy.
- `nix develop . --command cargo test --locked --all-targets -j 2 --
  --test-threads=2`: 822 library and 40 integration tests passed, no failures;
  native/manual/paid opt-ins remained ignored. Includes all four new
  coordinator metadata/deadline tests and the existing actual-shell, fake-RPC
  and real-daemon/fake-Codex process coverage. No real model or user state used.
- `nix develop . --command cargo clippy --locked --all-targets --all-features
  -j 2 -- -D warnings`: passed, including structure/size lints.
- `nix develop . --command pnpm --dir docs run build`: passed; verified 68
  static files, 20 pages, search, routing and the public-only boundary. The
  equivalent `nix run .#docs-build` first hit sandboxed Nix-daemon access;
  the existing cached development shell ran the build without escalation.
- `cargo machete` in the pinned development shell: passed, no unused
  dependencies. `cargo deny --offline check` passed all four policies against
  the locally cached advisory database; only existing duplicate-version
  warnings. No manifest or lockfile change and no fresh advisory fetch.
- Pinned `cargo fmt --all -- --check`, `git diff --check` and rebuilt
  `coco cd --help` passed. Final source/test/documentation diff reviewed; no
  lifecycle, scope, MCP or installation changes and no unresolved finding.
- `nix develop . --command pnpm --dir docs run check`: passed, zero errors,
  warnings or hints.

## Follow-up review — 2026-11-11

- Reviewed the complete tracked diff and both new test children, then traced
  selection through scoped RPC, candidate filtering, validated passive native
  reads, cancellation cleanup, shared ordering/presentation and shell stderr/
  stdout handling. No actionable correctness or security finding identified.
- Confirmed metadata-only direct targets/name mode, one same-scope legacy
  fallback, exact selected-ID revalidation and preservation of partial results.
  Four stalled reads may leave later dates unqueried at the shared deadline;
  this is the documented opportunistic behavior, not a claim of exhaustive
  recency discovery. No lifecycle or subscription mutation is introduced.
- Used the security-audit skill in guidance mode for boundary and execution
  safety, not a full audit. A proposed additional run of the existing compiled
  `navigation --test-threads=1` tests could not start its isolated OS sandbox:
  the default execution denied loopback netlink setup; escalated attempts
  failed namespace creation with `Resource temporarily unavailable`. Target
  tests never started, and no unsandboxed fallback or real-service probe ran.
- The proposed sandbox used no external network/host home, an empty environment
  with scratch-local HOME/TMPDIR/cache and only local Git/Bash, UTC/locale and
  Git noninteractive settings. Read-only executable/toolchain; 128 MiB scratch
  tmpfs; one CPU, 3 GiB address space, 25 CPU seconds, 35 wall seconds, 16 MiB
  file size and 128 descriptors; process limits of 128 then 512 still could
  not create namespaces. No test artifacts retained.
- `git diff --check` passed. The prior complete 822-library/40-integration
  result remains valid historical evidence, not a claimed fresh review run.
  Product source, tests and public docs were not changed during this review.

## Open questions and handoff

- Implementation and regular gates complete; no commit, push, release or
  installation change. Local work remains on this branch, based on main 0.9.0.
- Optional dates are opportunistic: reads left unfinished or unstarted at the
  shared deadline remain unknown and eligible. This is native conversation
  activity, not a persisted record of directory visits. No new follow-up scope.
- Additional review-time execution remains unverified because of environment
  namespace restrictions. If another fresh run is needed, repeat the focused
  compiled tests inside the same isolated controls where namespaces are
  available; do not weaken isolation or touch installed services.

## Remaining ordering opportunities — 2026-11-11

- User asks where else recent-first ordering would help. Inspection only;
  no product changes, test execution, commit or publication requested.
- All actual workspace selectors now use the shared policy: create code and
  context sources; send, jump, diff, limits, close, reopen, delete; cd/path.
  Repository selection is separate and still uses the registered repository
  list ordered by display name/path/ID. Models retain native catalog order;
  setup/action and approval options retain their semantic/native order.
- Highest-value proposal: an optional native-recency sort key in status's
  existing multi-key collection sorter (currently no such field), preserving
  stable default repository/name order and allowing state plus recency.
  Collection data already includes the optional native date.
- Secondary proposal: optional recent order for list/ls, which currently
  applies fixed repository/name order. Preserve stable defaults; do not
  replace code/context or conversation dates with management timestamps.
- Repository-picker recency is a later convenience, not a missing reusable
  timestamp today. It would need an explicit aggregation of workspace-native
  activity, unknown handling and bounded discovery, rather than using the
  repository record's mutation time. No new history store proposed.
- Signal streams and hook delivery history already have their own event-
  sequence ordering; do not reorder them by conversation activity. No new
  implementation scope has been accepted from this inventory.

## Approved status recency slice — 2026-11-11

- User approves `coco status --sort recent` and combinations such as
  `--sort state,recent`. Extend the existing collection sorter on this branch;
  leave the earlier navigation work intact. No list/repository-picker change,
  extra column, commit, push, release or live model call is in scope.
- `recent` means descending validated native `recencyAtMs`; `-recent` reverses
  it to oldest known activity first. Missing/negative/overflowing dates remain
  last in both directions. Preserve the existing repository/name/ID fallback
  ties rather than silently changing the collection ordering contract.
- Reuse the metadata already present in status's workspace response; no new
  RPC/native lookup, persistent date, resource/usage flag or backend change.
  Follow applies the same sort to every new snapshot. Preserve the tree and
  explicit-target exclusions already enforced for all status sort keys.
- Implementation uses only the existing CLI `SortField`, key parser and raw
  value reader. No backend or protocol changes for status; all other sort-key
  directions and default tie-breakers retain their previous behavior. Help,
  public usage instructions and canonical status rules updated.
- Full `nix develop . --command cargo test --locked --offline --all-targets
  -j 2 -- --test-threads=2`: passed 826 library and 43 integration tests,
  including four new sort-unit cases and three new real-CLI/fake-daemon cases.
  Follow reorders when native activity changes while both rows stay Ready;
  filtered/repeated-key/JSON/human forms use only the existing workspace list
  request. Native/manual/paid opt-ins stayed ignored; no real model called.
- An initial narrowly named unit-test filter selected zero tests and was not
  counted as verification. A subsequent Nix call needed daemon-socket access;
  the actual full run above used the approved pinned environment and Cargo's
  offline mode. No dependency installation or installed-service change.
- `cargo clippy --locked --offline --all-targets --all-features -j 2 --
  -D warnings` and `cargo fmt --all -- --check` in the pinned Nix development
  shell: passed. Rebuilt `coco status --help` lists recent/newest-first and
  describes reversing a key's normal order.
- Static docs build/export passed: 20 pages and 68 static files, search,
  routing and public-only boundary verified. `astro check` passed with zero
  errors/warnings/hints. Both ran through the same pinned development shell.
- `cargo machete` passed; `cargo deny --offline check` passed advisories,
  licenses, sources and bans using the cached local advisory database, with
  only existing duplicate-version warnings. Manifest and lockfile unchanged.
- Final source/docs/test diff reviewed and `git diff --check` passed. No new
  findings; the navigation slice remains preserved. No commit, push, release,
  installation change or real-service/model test performed.

## Combined slice re-review — 2026-11-11

- User retains `recent` / `-recent` after the naming discussion and requests
  another review. Inspected the whole open source/docs/test diff and both
  untracked navigation test children, with focused security-audit guidance for
  scope/passive-read boundaries. This is a code review, not a full audit.
- No new functional/security finding: only Recent reverses its default sort
  direction; unknown dates remain last per key, and higher-priority keys still
  win. Default collection and list ordering, global scope, ties, resource/usage
  opt-ins, tree/target restrictions and JSON shape remain unchanged. Follow
  sorts each new snapshot without adding metadata calls or output columns.
- Rechecked timestamp validation and identity mapping, partial native results,
  deadline cancellation, same-scope legacy fallback, exact selected-ID
  revalidation and shell output separation. The documented opportunistic
  one-second navigation budget is retained, not treated as exhaustive discovery.
- Minor existing documentation follow-up at public `reference/cli.mdx:397`:
  "For a leading minus, use --sort=..." should make clear that `=` is optional
  and the spaced form also works. `allow_hyphen_values` and the real-CLI
  recent/reverse fixture already cover the spaced form. This wording predates
  the slice and was acknowledged in the preceding syntax discussion; it is
  not a new runtime defect or security finding. Public docs left unchanged
  because this request authorizes review, not an automatic fix.
- `git diff --check` passed. No new test/build execution or namespace retry;
  the latest actual evidence remains 826 library + 43 integration tests and
  clean Clippy/fmt/docs checks recorded above. No source, tests, public docs,
  commit, push or release changed; only this continuation record is updated.

## Sort argument wording fix — 2026-11-11

- User authorizes fixing the remaining reference wording. Scope is the public
  argument example and this branch record only; preserve all existing work.
- [x] State that spaced and equals forms are equivalent, including a leading
  minus, without changing parser behavior or naming.
- [x] Validate the static docs build/export and source checks; no Rust test
  rerun, real model call, commit, push or deployment needed for this prose fix.
- Reference now explicitly states that `--sort -recent` and `--sort=-recent`
  are equivalent and `=` is optional. Parser/source behavior unchanged.
- `nix develop . --command pnpm --dir docs run build` passed: 20 pages and
  68 files, search/routing/public-only boundary verified. The corresponding
  `pnpm --dir docs run check` passed with zero errors, warnings or hints.
  `git diff --check` passed. The previously noted wording follow-up is closed.
- Post-fix review: checked the open production diff, timestamp/direction and
  scope/timeout behavior, parser/regression coverage and public examples again.
  The spaced and equals forms now agree with code/tests; no additional finding.
  Source-only review using focused security guidance; no product edits or new
  executions. Latest actual results remain 869 regular tests and the successful
  post-fix static build/source checks above. No commit, push or deployment.

## Control MCP capability review — 2026-11-11

- Requested scope: inspect the missing Palm/AI delete capability, compare
  public MCP with existing CLI/application actions and propose an extension.
  Review only; implementation is not authorized. Preserve the open navigation
  and status slice. Used focused security-audit guidance for permission,
  confirmation, repository ownership and destructive retry boundaries.
- Confirmed in the router, startup arguments and AI launch configuration:
  Control MCP has no close, reopen or delete route/grant. `coco ai` intentionally
  uses that surface without shell fallback, so the same gap applies there.
  Existing create/send/interrupt grants cannot enable an absent route.
- The coordinator already owns checked retirement planning, dry runs, exact
  expected-plan validation, guards, runtime/TUI/context blockers, file/commit
  loss policies, branch ownership and interrupted-saga recovery. Reuse these
  methods rather than add a Palm-specific cleanup path or direct Git commands.
- Proposed first slice, pending approval: `workspaces.delete`,
  `workspaces.close` and `workspaces.reopen`, with independent startup grants;
  default MCP and AI read-only mode remain read-only. Match CLI resource-retention
  semantics and keep file discard separate from unretained-commit discard.
  AI Ask must present a server-generated checked preview through the existing
  native confirmation form, pin the selected UUID and revalidate the plan at
  apply. Auto approval skips the question, not blockers or loss-policy checks.
  Noninteractive clients need explicit grants, not mandatory user elicitation.
- Deletion retry needs more than another thin route: core recovery persists
  the in-progress workspace intent, but successful deletion removes that row
  and its send-operation receipts. A retained creation receipt records
  `deleted`, not the client deletion key and exact requested effects. Recommend
  a minimal durable deletion receipt keyed by caller operation ID and frozen
  repository/workspace identity, surviving target deletion. Replays must not
  resolve a reused name or treat absence as proof of successful deletion.
  Preserve explicit uncertainty after interrupted external effects; no
  exactly-once or automatic destructive compensation claim.
- Important integration limit: keeping the native thread does not keep CoCo
  turn-operation/outcome receipts. Palm must retain needed outcomes before
  deletion; close/reopen preserves that workspace evidence instead.
- Other gaps worth separate follow-up: responding to pending native decisions
  (status already exposes them), usage/cost and model discovery, optional client
  data, then explicitly granted resource-policy management if needed. Current
  private decision get/respond accepts only a decision ID; a public adapter
  must add an application-level repository/workspace ownership check before
  submission, not blindly forward a known foreign ID. Single-workspace MCP
  status already requests runtime resources; do not call that missing.
- Keep repository/hook/catalog administration and terminal/shell transfer out
  of this first slice. Worker MCP selection remains separate. Older generic
  architecture MCP inventory prose also lags current setup/turn tools; update
  that inventory with any approved extension, not public behavior from prose.
- Verification: source/DTO/router/test inspection only; no new test/build or
  target process, live MCP/model call, installed-service change, commit, push
  or release. Required implementation coverage: grant isolation, foreign repo,
  preview/UUID changes, dirty/ignored/unretained data, guards/busy/TUI blockers,
  lost replies, duplicate/conflicting keys, name reuse and real database reopen.
- Next action: discuss/approve the lifecycle contract before implementation;
  this record is a proposal, not a shipped feature or committed roadmap.

## Approved Control MCP lifecycle slice — 2026-11-11

- User approves implementation of the proposed lifecycle extension. Continue
  on the current branch and preserve all uncommitted navigation/status work.
  No commit, push, release, live model invocation or installed-service change.
- [x] Add separately granted public delete/close/reopen tools over existing
  checked coordinator retirement, with fixed repository scope and exact targets.
- [x] Retain minimal operation-keyed deletion evidence independently of the
  deleted workspace; integrate existing deletion recovery, never replay a
  destructive discard merely because the caller or daemon lost its response.
- [x] Add authoritative preview/pinned-plan native confirmations to AI Ask;
  preserve headless MCP, explicit Auto and read-only behavior.
- [x] Cover permission/scope, name reuse, loss policies, guards/races, lost
  replies, database reopen and confirmation rejection with isolated tests.
- [x] Update public reference, architecture, AI contract and Palm handoff;
  run sequential bounded checks and review the complete new diff.
- Deferred: native decision responses, usage/model discovery and resource-policy
  administration remain separate follow-ups, not implicit parts of this slice.

### Implementation decisions and findings

- Public tools are `workspaces.delete`, `workspaces.close`, `workspaces.reopen`
  and read-only `workspaceDeletions.get`. Six mutation grants are independent;
  AI Ask uses the existing native form, Auto retains policy checks and read-only
  omits all six grants. Delete requires a caller key; close/reopen remain ordinary
  state-sensitive transitions with status inspection after a lost reply.
- Reuse checked coordinator retirement rather than duplicate Git/native logic.
  Plan SHA-256 is an optional precondition; AI computes it from authoritative
  preview data and pins the exact UUID/resources after host consent. Separate
  file and unretained-commit loss policies remain default false. Guard, native
  activity, TUI, context and resource-ownership blockers remain in force.
- Schema 21 adds only non-cascading deletion receipts with a private unique
  active-saga association. Claim before dispatch; associate with the existing
  core intent transaction; record success with metadata removal; detach as
  uncertain when core recovery preserves resources. No destructive retry,
  new history store, provider/protocol change or exactly-once claim.
- A matching completion can overtake another caller's preview/lock wait.
  Reread the retained receipt before propagating a planning error. Added a
  deterministic held-repository-lock regression for that interleave.
- Preserve known-UUID precedence: a foreign UUID cannot fall back to a local
  workspace named with that string. Public deletion returns its already-scoped
  receipt without rediscovering the original path after effects, because that
  path may itself have been the removed worktree. Later calls still require
  a valid scope path; public instructions recommend a persistent checkout.
- Neutral plans moved unchanged from protocol to private domain retirement;
  protocol retains reexports. Store, application, transport and presentation
  keep their staged dependency direction. The binary facade takes one explicit
  permissions value rather than an expanding boolean argument list.
- First new coordinator run: 12 passed. MCP host fixtures initially omitted
  required workspace fields; fixed only the synthetic data, then all 10 AI/MCP
  process tests passed. Full test run found a historical hard-coded schema-20
  assertion; updated it to supported schema and added a genuine v20 upgrade
  preservation test. No safety check or gate was weakened.
- Subsequent full model-free gate: 844 library + 46 integration tests passed
  (890 total), with paid/native/manual opt-ins ignored. Clippy passed after
  extracting coherent handler/fixture helpers instead of lint exceptions.
  Final scope/UUID hardening and strengthened regressions are being rechecked.

### Final verification and handoff

- Final `nix develop . --command cargo test --locked --offline --all-targets
  -j 2 -- --quiet --test-threads=2`: **845 library + 46 integration tests passed
  (891 total)**. Nine library/manual/native checks, the paid live scenario and
  18 opted-in native scenarios remained ignored. No real model/authentication
  or installed daemon was used. Includes the scope-path removal, UUID/name
  collision, ignored-file policy and overtaking-completion regressions.
- A strengthened foreign-scope process fixture initially stopped at
  `REPOSITORY_NOT_REGISTERED`; register its synthetic foreign checkout before
  checking `WORKSPACE_NOT_FOUND` for all three actions. Final full gate above
  passed with that corrected fixture, without changing production behavior.
- `cargo clippy --locked --offline --all-targets --all-features -j 2 --
  -D warnings` passed; pinned `cargo fmt --all -- --check` and `git diff --check`
  passed. `cargo machete` found no unused dependencies; `cargo deny --offline
  check` passed all four policies against the cached advisory database, with
  only existing duplicate-version warnings. No manifest/lockfile change.
- Pinned docs build passed: 21 pages / 70 static files, Pagefind, routing and
  public-only boundary verified. `astro check` returned zero errors, warnings
  and hints. No internal knowledge or handoff is part of the public export.
- Reviewed new production/DTO/storage/adapter files and focused test children,
  alongside the preserved navigation/status diff. Exact scope, immutable keys,
  checked loss policy, atomic saga association and explicit uncertainty remain
  the invariants. No unresolved implementation finding for this slice.
- Public reference: `docs/src/content/docs/reference/workspace-lifecycle.mdx`.
  Canonical decision: `knowledge/engineering/workspace-lifecycle.md`. Palm
  return handoff: `knowledge/handoffs/palm-workspace-lifecycle.md`, including
  exact API, local builds, tests and recovery limits. Package version remains
  0.9.0; the new API is not yet committed or released.
- All earlier uncommitted work remains on this branch. No staging, commit,
  push, release, operator configuration/service change or Palm edit. Next:
  user review/approval before any checkpoint or publication; deferred decision,
  usage/discovery and resource-policy tools remain outside this slice.

## Requested lifecycle re-review — 2026-11-11

- Review the complete new Control-MCP lifecycle slice, including untracked
  production/test files, durable receipt/recovery boundaries, grants and AI
  confirmations. Preserve navigation/status work and perform no automatic
  source fix, commit, push, release or operator-service/model call.
- Use security-audit guidance only for this focused boundary review, not the
  full repository-audit workflow. Inspect source/contracts/tests first; do not
  run target-controlled code without the skill's complete OS sandbox controls.
- Last actual implementation evidence is the recorded 891 passing tests,
  Clippy/fmt/dependency gates and static docs build/check above. A review must
  distinguish that prior evidence from any fresh verification performed here.
- Completed source review of new domain/DTO/receipt/migration/application/MCP
  modules, modified saga/store hooks, startup/shutdown ordering, permissions and
  AI configuration, authoritative confirmation formatting, focused coordinator
  and process fixtures, public reference and handoff consistency. No new
  functional or security finding identified in the lifecycle slice.
- Rechecked at-most-one claim, association with core intent, atomic completion,
  cancellation detachment, pre-saga restart uncertainty, late failure/success
  precedence and non-attribution of later CLI cleanup. Replay precedes target
  resolution; foreign UUIDs cannot become same-named local targets; confirmed
  plans pin identities/effects and preserve separate file/commit loss choices.
- Accepted limits remain explicit: only deletion has an operation-keyed receipt;
  close/reopen require scoped status inspection after a lost reply. A plan
  digest is not a lock/content snapshot, no exactly-once effect is promised,
  and future calls need a valid persistent repository scope path.
- Fresh verification was `git diff --check` and source/test/contract inspection
  only. No additional target program, test, build, native or model call; do not
  relabel the prior 891-pass implementation run as a fresh review execution.
  No product or public-doc changes, staging, commit, push or release; only this
  continuation record changed. Next action remains user-approved checkpoint
  or follow-up, not an automatic mutation.

## Complete open-diff re-review — 2026-11-11

- User asks to review everything again: cover the complete navigation/recency,
  status-sort and public MCP/AI cleanup diff, including untracked source/test
  files and documentation. Review only; preserve all edits and do not fix,
  stage, commit, push, release or touch operator services/model access.
- Use focused security-audit guidance for trust/destructive boundaries, not a
  full repository security audit. Separate fresh source inspection from the
  previously recorded 891-pass implementation run and clean hygiene/docs gates.
- Completed source, contract and regression-fixture review across both slices,
  including untracked modules. No new actionable correctness or security
  finding identified. Recent-first path selection preserves native-free direct
  lookup/name mode, bounded optional enrichment, same-scope legacy fallback,
  unknown-last times and final UUID revalidation. Status applies user sort keys
  before deterministic fallbacks and updates ordering in follow snapshots.
- Rechecked cleanup grants in both executable entry points and AI modes,
  authoritative confirmation/plan pinning, repository isolation, immutable
  deletion intent, concurrent claims, atomic saga association/completion,
  cancellation detachment, migration and restart uncertainty. Public reference,
  CLI guidance, canonical decisions and Palm handoff agree on the implemented
  contract and unreleased availability. No new permission or retry guarantee
  is inferred from a successful transport reply.
- Fresh verification: read-only source/test/documentation inspection and
  `git diff --check` passed. No fresh target-controlled test/build execution;
  the prior 891 passing tests and static/code-health gates remain the latest
  execution evidence. Native/model/live behavior was not newly exercised.
- Only this working record changed during review. All product edits remain
  uncommitted and preserved; no staging, push, release, Palm edit or operator
  service/configuration change. A checkpoint/publication still requires the
  user's separate direction.

## Authorized integration and publication — 2026-11-11

- User authorizes committing the reviewed combined slice, merging into main,
  pushing and publishing through existing guarded CI. Preserve all reviewed
  navigation/status and generic MCP/AI lifecycle work. No Palm changes, real
  model calls or operator-service/configuration changes are needed.
- Refresh remote main before integration, checkpoint the complete reviewed
  source and prefer fast-forward integration without rewriting history.
  Recheck local format/tests, then require exact-source hosted Rust and docs
  success, stable non-publishing rehearsal and verified stable publication.
- Expected feature version is 0.10.0. Do not edit versions or publish locally;
  keep the automatic-alpha switch disabled. Record actual commits, workflows
  and verified artifacts in main's delivery record after completion.
- Refreshed origin/main equals local main and the branch base (`b17e770`);
  verified `COCO_RELEASE_ENABLED=false`. Fresh pre-checkpoint format and
  all-target offline tests passed: 845 library + 46 integration = 891, with
  explicit native/paid/manual opt-ins still ignored and no model call.
