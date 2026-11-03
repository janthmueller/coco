---
type: Working Document
title: "feature/status-ordering: status, creation and Codex compatibility"
description: Implements status queries, repository-aware context, workspace-named branches and verified Codex 0.166.0 compatibility independently of Compose.
tags: [work, branch, cli, status, sorting, filters, context, git, codex, usage]
status: complete
branch: feature/status-ordering
updated: 2026-11-03
---

# feature/status-ordering — status queries and workspace creation

## Intended outcome

Explicit status sort keys control priority across repositories, with typed
numeric ordering, stable ties and missing values last. Optional state filters
compose with flat/tree/follow/JSON without changing execution or native state.
Context selection also spans registered repositories, with the current
repository first and code selection still repository-local.
Newly allocated default branches use the workspace name without an added prefix.
Keep Compose isolated. The user now authorizes finishing the confirmed
provider-dispatch fix, final verification, committing, merging onto main and
releasing through GitHub CI. Preserve unrelated changes and do not change the
user's live installation/services.

## Active work

### Finish provider rejection, integrate and release — 2026-11-03

- [x] Classify only proven pre-turn native rejection at the worker boundary;
  preserve unknown-dispatch protection for every ambiguous failure.
- [x] Restore the retained operation intent safely after a proven rejection,
  release its runtime guard, and cover retries/fingerprints and real CLI output.
- [x] Run sequential quality/native/docs gates and record final results.
- [x] Commit the completed slice and integrate current origin/main without
  Compose. Continue the exact-SHA CI/release checks in [main's record](../main.md).

Implementation and integration notes:

- The worker port now distinguishes a proven pre-turn rejection from an
  uncertain runtime error. Only the exact structured native provider mismatch
  is classified this way; text matches, other RPC failures and transport loss
  are not proof. Coordinator imports no Codex transport types.
- A transaction restores only a dispatching rejected operation to its original
  prepared intent, retaining ID/fingerprint and clearing dispatch timestamps.
  Accepted/uncertain records cannot be reset. Release the exact generation-local
  turn guard only after this succeeds; persistence failure remains fail-closed.
  Preserve an already materialized fresh binding as before, without pretending
  the rejected turn was accepted. No schema migration or automatic retry added.
- Focused coordinator tests cover fresh and materialized bindings, explicit
  retry with the same or another operation ID, and conflicting payloads. Store
  transition tests and a real-CoCo/native-Codex provider repair/restart test
  accompany the adapter classification and redaction cases.
- Origin/main advanced only by the generated 0.3.0 release stamp (`6461d27`).
  The automatic-alpha variable remains false; use an explicit stable CI
  rehearsal/publication after the exact integrated main revision passes Rust.
  Do not alter release switches, credentials or Compose.
- The real CLI regression now proves repeated same/new-ID rejection stays
  actionable and non-blocking, different payloads still conflict, and a saved
  same-ID send succeeds after matching the base provider and restarting the
  isolated daemon. One initial probe hit the existing 10-second native listener
  startup deadline; its isolated repeat and full seven-case run pass without a
  product timeout change or a hidden retry. Separate startup/restart logs and
  phase context improve failure diagnostics.
- Final code verification so far: 598 ordinary tests pass (582 library, six
  process-smoke, ten status-query; 15 opt-in/manual skips); all seven native
  Codex 0.166.0 tests pass explicitly (41.47 s), including provider repair.
  The 19 release/synchronizer unit tests also pass. The storage race regression
  confirms an already-uncertain operation remains closed despite a later
  proven-rejection response.
- Final Clippy (all targets/features, warnings denied), Rustfmt, Machete,
  cached offline dependency policy and whitespace checks pass. Astro check has
  zero diagnostics; static export verifies 17 pages and 62 files. A local
  registry-package dry-run cannot run fully offline because Cargo needs a
  registry HTTP request; retain the exact-main hosted package/Nix/native-binary
  gates before any publishing dispatch rather than bypassing verification.
- Completion handoff: `8061922` commits the compatibility/usage/rejection slice
  after checkpoint `fd25be5`. Local main includes origin's 0.3.0 stamp without
  conflicts; ongoing publication verification is tracked on main. No Compose
  or user-service changes were made.

### Fresh review and verification — 2026-11-03

- User requests another review and test run of the uncommitted compatibility
  and usage slice. Review all changed and new files, verify the native error
  paths and USD provenance, and rerun the isolated gates. Do not implement
  product fixes, commit, push, publish or change live services during this
  review-only request.
- [x] Review production paths, new native fixtures and affected documentation.
- [x] Reproduce actionable findings with bounded isolated checks where needed.
- [x] Rerun sequential Rust/native/static-documentation gates and report findings
  and remaining verification limits.

Confirmed finding (P2, not fixed in this review):

- The new provider-specific public guidance in `src/daemon/handler.rs` is
  bypassed when a first `send` reaches `turn/start`. The coordinator converts
  every worker error there into `OperationUncertain`, including Codex's explicit
  `-32600` / `modelProviderMismatch` rejection. It discards the actionable
  cause, marks the workspace unavailable and blocks subsequent sends in the
  same runtime generation, despite Codex having rejected the request.
- Reproduced with the built real CoCo CLI/daemon and installed Codex 0.166.0,
  not just a synthetic error-mapper unit test. In an isolated temporary Git
  repository, set base provider `provider-current`, create a workspace with
  a profile selecting `provider-other`, then issue two distinct sends. Creation
  succeeds; both sends return `OPERATION_UNCERTAIN` referencing the first
  operation. Status reports `phase: unavailable`, even though native thread
  status is `idle`. The test daemon's log contains the exact provider rejection.
  Providers point only to unreachable loopback endpoints, so no model request
  or user authentication is involved. The temporary daemon exits cleanly (0).
- The existing negative native test verifies Codex's contract directly, and
  the handler unit test verifies direct error projection; neither exercises
  this intervening CoCo turn-dispatch path. Add a CLI/daemon regression before
  claiming the provider-error UX complete. Distinguish this explicit rejection
  from genuinely unknown transport outcomes without weakening duplicate-send
  protection or retrying an uncertain request.
- Diagnostic harness: `/tmp/coco-provider-review.TznKsj/repro.mjs`; disposable
  fixture/log: `/tmp/coco-provider-review-fixture.y4qnDj`. These contain only
  synthetic test data and are not repository artifacts.

Fresh verification:

- Full ordinary `cargo test --locked --offline --all-targets --jobs 2 --
  --test-threads=4 --quiet`: 592 pass, zero failures, 14 opt-in/manual skips
  (576 library, six process-smoke, ten status-query passes).
- Explicit `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked --offline
  --test real_codex_compat --jobs 2 -- --ignored --test-threads=1 --quiet`:
  all six native cases pass against `codex-cli 0.166.0` (41.50 s). These are
  separate from the additional diagnostic reproducing the finding above.
- Warning-denied all-target/all-feature Clippy, Rustfmt and `cargo machete`
  pass. Offline `cargo deny --locked --offline check` passes using the existing
  advisory cache; existing duplicate-version warnings remain. The sandbox's
  cache-lock restriction required approved execution, not a database update.
- Pinned local Astro entry point: zero check errors/warnings/hints; static
  `/coco` build and export verification pass for 17 pages / 62 files, Pagefind,
  routing and the public boundary. No dependency installation was needed.
- `git diff --check` passes. No manifests, lockfiles or Nix inputs changed.
  No paid external inference, fresh account-billing proof or macOS run is
  claimed. Ordinary opt-in/manual tests remain intentionally skipped; six
  native opt-in cases were run explicitly as stated above.
- Review is complete; the confirmed provider-dispatch issue remains open.
  This request changes only this working record in the repository. Existing
  product/doc changes remain uncommitted and unstaged; checkpoint `fd25be5`
  is unchanged. No commit, push, publication, install or user-service changes.

### Checkpoint and Codex 0.166.0 compatibility — 2026-11-03

- User requests committing the current state first, then implementing sensible
  additions from the upstream review. Checkpoint the verified status queries,
  cross-repository context picker and workspace-named branches before editing
  compatibility code. Existing verification for that snapshot is recorded below;
  the final whitespace check is clean.
- [x] Create the local checkpoint without touching main or Compose.
- [x] Select the installed 0.166.0 binary for isolated real-process contracts;
  investigate actual failures rather than weakening assertions or security.
- [x] Add focused regression coverage and bounded native reuse where justified.
- [x] Run sequential Rust/documentation gates, update proven compatibility
  guidance and leave the upgrade changes reviewable without pushing/publishing.
- Queue/steer and designed handoffs remain separate feature work unless a
  compatibility finding makes a narrow change necessary. Consider the optional
  native USD field only with explicit provenance and validation tests.
- Checkpoint: `fd25be5` (`feat(cli): improve status queries and workspace
  creation`); working tree clean immediately afterward, no push. Select
  0.166.0 for the opt-in test gate now; public compatibility claims stay at the
  proven old version until the real native contracts pass.
- Initial isolated tests hit sandbox-only loopback/user-bus restrictions;
  approved execution removed those restrictions. Three native contracts passed,
  while context resume and the session-hook proof exposed 0.166.0's real
  `modelProviderMismatch` rejection. Workspace overlays cannot change the
  process's current provider for resume/input. Align local test-provider base
  configuration with the profile and preserve it when adding project trust;
  do not disable the native check or add a provider pool.
- All five existing opt-in native tests then pass (40.53 s): busy-source
  capture/independent restart, legacy boundaries, full preparation/adoption/
  resume/TUI/retirement/resources, session hooks and scoped MCP signals.
- Next: a model-free negative native provider contract and actionable bounded
  mismatch guidance; preserve existing unknown-dispatch/idempotency semantics.
  Carry optional native USD evidence separately from estimated USD/credits,
  prefer reported USD for display without calling it an invoice, and validate
  missing/null/zero/invalid amounts, cache propagation and JSON provenance.
  Do not add queue/steer, network-on-status, a price table or handoff behavior.
- Native negative provider proof now materializes shell history before resume:
  an empty thread instead fails with `no rollout found`, a separate native
  contract, not provider evidence. The persisted case proves both resume and
  input reject with the structured provider type and no inference; focused
  run passes. Session-hook/provider cases have focused private test modules.
- Full ordinary suite passes with the addition (592 passed, 14 opt-in/manual
  skipped). One new CLI assertion initially expected targeted usage reads;
  unfiltered collections correctly use the existing bulk usage read. Correct
  the assertion to its exact two-query method sequence, not product behavior.
- Final schema review preserves the new USD field's signed `int64` contract
  rather than inventing a non-negative rule: null/zero/sign/both integer
  boundaries are covered, invalid type/range is rejected, and exact formatting
  handles `i64::MIN`. Re-run gates for this final type refinement.
- Native provider mismatch guidance remains under the existing `CODEX_ERROR`
  code, exposes no upstream data/identifiers, and tells users to preserve the
  saved profile while matching base configuration before a stopped-work
  restart. Unknown-dispatch state and retry behavior are unchanged.
- A bounded isolated native CLI probe confirms `--exit-on-stdin-close` still
  requires `--remote` and `--environment-id` in 0.166.0, so it cannot replace
  CoCo's local runtime cleanup. Read-only inspection of the exact earlier
  failed test's hashed systemd scope namespace finds no remaining scopes.

Final verification and handoff:

- Final exact-version native suite: `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test
  --locked --offline --test real_codex_compat --jobs 2 -- --ignored
  --test-threads=1 --quiet` passes all six cases against installed 0.166.0
  (56.02 s). The context case uses a loopback provider stub; no paid external
  inference or user's authentication, daemon, workspace or configuration used.
- Final ordinary `cargo test --locked --offline --all-targets --jobs 2 --
  --test-threads=4 --quiet` passes 592 tests, zero failures, 14 opt-in/manual
  skips: 576 library, six process-smoke and ten status-query tests pass.
- Warning-denied all-target/all-feature Clippy, Rustfmt, `cargo machete` and
  cached offline `cargo deny --locked --offline check` pass. Deny retains
  existing duplicate-version warnings; dependency manifests/lockfiles and Nix
  inputs are unchanged. No fresh advisory-database fetch is claimed.
- Astro check has zero errors/warnings/hints; final static `/coco` build/export
  verifies 17 pages, 62 files, navigation/search and the public-only boundary.
  The sandbox prevented pnpm from opening its user-level database, so run the
  same scripts through the pinned local `node_modules/astro/bin/astro.mjs`
  entry point and `scripts/verify-export.mjs`, without installing dependencies
  or changing user-level tooling.
- Public tested-version/profile/cost guidance and canonical architecture,
  runtime, hooks/MCP, usage, product and knowledge-log records are updated.
  Reported USD is conditional backend evidence, not proof every account
  supplies it. No broader Codex-version range is certified.
- Checkpoint `fd25be5` is the only new commit. Upgrade/usage changes remain
  unstaged and uncommitted for review on this branch. Main stays `e5559de`,
  Compose stays `10e48d8`; no push, publication, service restart or installation.

### Upstream Codex review — 2026-11-03

- User requests a read-only comparison of newer stable Codex releases with
  CoCo's tested 0.160.1 baseline: compatibility risks, native improvements
  already reusable, and optional follow-up. Preserve all uncommitted slices;
  do not upgrade/install, implement, commit, push, publish or mutate live services.
- Local `codex --version` reports 0.166.0. The official changelog lists 0.166.0
  on 2026-10-29; verify the intervening release details and actual API shapes
  before recommending changes or claiming compatibility.
- [x] Review official release notes and current App Server/CLI documentation.
- [x] Compare relevant APIs against CoCo adapters, assumptions and opt-in tests.
- [x] Separate required changes, automatic benefits and optional features;
  record evidence, verification limits and the next recommended slice.

Review findings and evidence:

- Official changelog: <https://learn.chatgpt.com/docs/changelog>. Reviewed
  0.161.0, 0.162.0/0.162.1, 0.163.0, 0.164.0, 0.165.0 and 0.166.0; the
  newest stable entry is 0.166.0, released 2026-10-29. Installed CLI agrees.
  Notes include native reconnect/runtime-lifecycle fixes and improved
  permission and reasoning-token restoration. These are reasons to validate
  the upgrade, not evidence that CoCo's relay can be removed.
- Current official contracts: <https://learn.chatgpt.com/docs/app-server>
  and <https://learn.chatgpt.com/docs/cli/reference>. Compared against actual
  0.166.0 `--help` output and an experimental JSON Schema bundle generated
  into a disposable `/tmp/coco-codex-review.*` directory. No schemas added to
  the repository. Baseline comparison reads the exact existing local
  `rust-v0.160.1` experimental precomputed exports, not the old upstream
  working checkout or unverified current-main sources. No clone was updated.
- No request method was removed in the compared bundle. Relevant existing
  request properties remain, with no new required fields on consumed start,
  resume, fork, read, turn-start, turn-pagination or environment-add requests.
  The four native thread-status variants, raw usage notification contract,
  model catalog projection and consumed approval/input request fields remain
  compatible in this static comparison. New optional fields/unconsumed history
  variants do not inherently require generated types or a new state model.
- Environment selection is still on `thread/start` and `turn/start`, not
  `thread/resume`/`thread/fork`. Keep current explicit routing and the recorded
  pre-turn compaction/review/host-shell limits. No native resource-limit
  replacement was established; per-workspace executors/cgroups remain relevant.
- Native queue APIs already appear in the 0.160.1 experimental bundle; do not
  describe them as newly introduced in 0.166.0. The current CLI exposes
  `codex queue --resume`, and intervening fixes improve interruption/permission
  handling. An optional CoCo queue/steer UX is promising but separate from an
  upgrade: preserve enqueue-versus-start acknowledgements, idempotency,
  interrupt behavior and exact environment routing. Queue start has no
  environment parameter, so do not promise safe automatic unloaded-thread
  activation through a CLI wrapper or silently change ordinary `send`.
- `GetAccountTokenUsageResponse.ThreadUsage` adds optional
  `nativeUsageUsdMicros`: native dollar usage independent of credit estimates,
  with null meaning unknown. CoCo currently reads only credits and optional
  estimated USD. This is a small possible later usage enhancement; retain
  separate provenance and prove backend availability before presenting it as
  supported billing evidence. Never silently substitute zero or a price table.
- New experimental `thread/fork.attachToSource` is explicitly incompatible
  with `deferGoalContinuation`, hides the fork from ordinary lists and blocks
  deletion of its source. It is not a drop-in replacement for CoCo workspace
  forks. New `thread/sample` requires an already loaded, non-active source and
  its exact latest terminal `expectedTurnId`; it returns text/token evidence.
  It may help a later designed handoff but does not replace busy-source
  completed-prefix capture. `thread/memory/read` can return null and must not
  be advertised as an on-demand full conversation handoff summary.
- Native `codex doctor --network-only` is a useful optional diagnostic path;
  CoCo still owns binding/worktree/daemon diagnostics. Do not add network
  checks to passive status. Models/effort remain server-discovered, including
  newer catalog entries; no hardcoded model list is required.
- The installed CLI has native `--worktree`; official CLI docs restrict that
  workflow to local execution. New managed-worktree tools do not establish a
  compatible replacement for CoCo's remote-TUI, independent Git/context,
  dirty-copy, lifecycle-guard and runtime-policy contracts. Do not add a second
  worktree owner or replace the existing Git adapter during compatibility work.

Recommended follow-up and verification boundary:

- First request a bounded 0.166.0 compatibility slice, not an architecture
  overhaul or feature bundle. The exact-version opt-in gate in
  `tests/support/codex_compat.rs` still selects 0.160.1. During an authorized
  upgrade, select the new binary, run the real isolated native contracts and
  update the public tested-version claim only after they pass.
- Cover create-time busy-source capture, independent child resume after
  restart, prepared/adopted large-history TUI/reconnect and multiple clients,
  authenticated executor registration/routing/cleanup, retirement safety,
  native MCP signal attribution/hooks, model/permission restoration and usage
  projection. New API shape checks alone cannot prove these behaviors.
- Verification here was version/help inspection, native schema generation,
  schema comparison and source/test review only. No live model request,
  real-process compatibility suite, user daemon/workspace access, install or
  service mutation. 0.166.0 is a candidate, not newly certified compatible.
- Only this working-document review record changed for this request. Existing
  uncommitted product slices, tested baseline, main and Compose remain intact;
  no staging, commit, push or publication. Present findings and await direction.

### Workspace-named default branches — 2026-11-03

- User approves the reviewed default change: newly created branches use the
  workspace name verbatim. Preserve explicit branch overrides, existing-branch
  adoption, detached mode, collision safety and all saved prefixed bindings.
  Keep the previous uncommitted slices intact. No commit, push or publication.
- [x] Update coordinator defaults and CLI walkthrough/review/help consistently.
- [x] Add regressions for direct names, collisions, old-binding replay and retirement.
- [x] Update public guidance and canonical contracts without renaming existing branches.
- [x] Run sequential quality gates and record final verification/handoff.

Implementation notes:

- Coordinator new-branch allocation and both CLI default displays now use the
  workspace name verbatim. No prefix parsing, new flag, schema migration,
  stored-binding rewrite or branch rename is added. Prefix-bearing explicit
  branches and representative historical fixtures remain valid.
- Three new focused coordinator tests pass against temporary native Git
  repositories: exact-name creation/replay; exact/parent/child/main ref
  collisions without fallback, adoption or side effects; reconstructed old
  prefix binding with an original default request through coordinator restart,
  idempotent replay, close/reopen and deletion. The last case leaves an
  independent branch named after that workspace untouched.
- CLI default-choice/review assertions now check the exact workspace name.
  Existing custom/existing/detached creation tests passed after the default
  change. Update process/real-Codex artifact assertions that deliberately
  inspect newly generated defaults; do not rewrite arbitrary legacy fixtures.
- Public creation guides/reference and branch example, canonical product/Git
  rules and knowledge log are updated.
- Final `cargo test --locked --offline --all-targets --jobs 2 --
  --test-threads=4 --quiet`: 587 passed, zero failed, 13 opt-in/manual tests
  skipped (572 library, six process smoke, nine status-query tests). The suite
  includes retirement of prefix-free defaults as well as explicit legacy names.
- Formatting, warning-denied all-target/all-feature Clippy, `cargo machete` and
  cached offline dependency-policy checks pass. Astro check has zero diagnostics;
  static build/export verifies 17 pages, routing, search and the public boundary.
- Handoff: implementation and verification complete; changes remain unstaged
  on this branch. No install, rename/migration, real model use, commit, push or
  publication. Main and Compose remain at `e5559de` and `10e48d8` respectively.

### Completed status ordering

- [x] Add typed comma-separated/repeated sort keys and state selectors.
- [x] Implement raw-value multi-key sorting and preserve list/context defaults.
- [x] Share one collection query pipeline across one-shot and follow.
- [x] Apply filters before optional reads/tree construction; keep following empty views.
- [x] Add modular parser, ordering, filtering and real CLI/RPC regressions.
- [x] Update public use and canonical contracts; run sequential quality gates.

### Cross-repository context selection — 2026-11-03

- User approves implementation: offer eligible context sources across registered
  repositories, prioritize the selected repository, then natural repository/name
  order, and show repository details. Fresh remains the default; code/worktree
  base selection stays repository-local. Continue on this branch without
  committing, pushing, publishing or merging Compose.
- [x] Trace stable cross-repository source resolution and selected-repository identity.
- [x] Implement repository-labelled, current-repository-first context discovery.
- [x] Preserve same-repository code selection and default/current context behavior.
- [x] Cover duplicate names, nested/worktree scopes and actual context capture.
- [x] Update user docs/canonical rules and run sequential verification gates.

Implementation findings:

- Reuse existing `repository.resolve` for the target repository's exact Git
  identity, rather than guessing by path prefixes. Private `cli/create/context`
  lists eligible sources globally and stably promotes the current repository
  ahead of normal natural-path/name order. An unregistered target has no
  priority group but can select context from other registered repositories.
- Picker details and chosen-source review include repository paths; both
  identical workspace names and identical repository display names remain
  distinguishable. The selected context request carries its exact workspace ID.
- Only the coordinator's context-specific resolver accepts IDs from other
  repositories. Names remain local and code/operational resolvers are unchanged.
  Existing fork capture, completed-turn boundary, native child binding and
  fresh/current-workspace defaults are reused without new schema or RPC methods.
- Focused verification: all 29 create-walkthrough tests pass, including four
  new cross-repository cases. Three new coordinator tests with actual temporary
  Git repositories/fake Codex cover exact-ID and prefixed-ID capture, duplicate
  names, immediate child materialization, independent target code and continued
  rejection of foreign names/code IDs.
- Warning-denied all-target/all-feature Clippy, formatting, unused-dependency
  and offline cached dependency-policy checks pass. Astro check has zero
  errors/warnings/hints; build/export verifies 17 static pages and routing,
  search and the public-only boundary.
- First full run: 569 library tests pass, but the existing process-smoke
  lifecycle case times out while interrupting a targeted `status --follow`.
  Its unchanged helper waits a fixed 750 ms before SIGINT. The exact isolated
  test passes on immediate rerun in 10.50 s. No unrelated signal/timeout fix
  is introduced. Final complete rerun with the same concurrency passes: 584
  tests, zero failed, 13 opt-in/manual tests skipped (569 library, six process
  smoke, nine status-query tests). The one-off follow interruption timeout
  remains a separate diagnostic follow-up if it recurs; its cause is not
  established by this context-selection change.
- Final handoff: context selection, canonical contracts and user docs are
  complete. All changes remain unstaged on this branch; no commit, push,
  installation, live model use or user-runtime mutation. Main and Compose
  references remain at `e5559de` and `10e48d8` respectively.

## Decisions

- 2026-11-02 — User approves the preceding sorting-first proposal, then state
  filters. Work on this dedicated branch from local main, with no Compose merge
  or release-version synchronization. Move its uncommitted review record here
  and keep the historical main working document unchanged.
- Sort keys require the corresponding explicit resources/usage flags. Cost and
  client-label sorting remain deferred because their semantics need separate
  decisions. Existing account-global quota is not a per-row sort key.
- The first filter slice preserves normal open-overview scope. Positive closed
  selection is rejected with guidance to `list --closed`, rather than widening
  lifecycle/usage semantics implicitly. State names accept user-visible aliases
  and the existing raw phase spellings; filters do not invent domain states.

## Verification and handoff

- Implementation is private to the CLI status view. Sort values are read once
  per row/key, usage joins by workspace identity without replacing live state,
  and unavailable values remain last in either direction. Filtered usage reads
  address only retained rows; a concurrently removed row has unavailable usage
  until the next refresh. Global quota stays independent of the row filter.
- Initial verification: offline all-target check passes; 14 focused status
  unit tests and eight real CLI/isolated RPC integration tests pass. These
  exercise cross-repository priority, numeric/missing values, distinct states,
  filters, usage joins, tree pruning, JSON and follow through an empty view.
  No actual daemon, Codex process or model turn is started by these new tests.
- Public command/reference examples, workspace guide, canonical product
  contract and module ownership notes are updated.
- Edge-case review adds cgroup-versus-RSS/zero memory selection, exact integer
  token ordering through `u64::MAX`, last-seen usage, missing/zero context windows
  and real CLI follow reordering across repositories. The follow test initially
  counted both workspace and branch text as row occurrences; it now parses the
  workspace column, with no product change needed. Poll-count assertions allow
  scheduling delay rather than requiring exactly two polls.
- Final `cargo test --locked --offline --all-targets --jobs 2 --
  --test-threads=4 --quiet`: 577 passed, zero failed, 13 opt-in/manual tests
  skipped (562 library, six process-smoke and nine status-query tests passed).
  New coverage totals 27 tests across ordering/metrics/filters/parser and real
  CLI query/follow paths. No live model or user-service checks are needed for
  this presentation-only change.
- `cargo fmt --all --check`, warning-denied all-target/all-feature Clippy,
  `cargo machete` and offline cached `cargo deny --locked --offline check` pass.
  Deny reports existing duplicate-version warnings; dependencies/lockfile are
  unchanged. The advisory check uses the existing cache, not a fresh fetch.
- Astro check reports zero errors/warnings/hints. The static `/coco` build and
  export check verify 17 pages, routing, search and the public-only boundary.
- Main remains at `e5559de` and Compose at `10e48d8`; neither branch reference
  changed. No staged files, runtime changes, commit, push or publication.
- No authorization for commit, push, release, installation or user-runtime changes.

## Handoff

- Status/context/naming changes are checkpointed in `fd25be5`. Subsequent Codex
  0.166.0 compatibility and reported-USD changes are complete and verified but
  remain unstaged/uncommitted on `feature/status-ordering`. Main and Compose
  are unchanged, and nothing was pushed or published. Current verification is
  recorded in the active-scope section above; earlier review notes are history.
- Use `status -af --sort state,name --exclude-state unloaded` for an
  attention-first live view across repositories, or `status -ar --sort
  state,-memory,name` for state followed by largest memory use.
- Explicit sort and tree remain mutually exclusive; filters work with both.
  Sorting cost and multi-client labels, and widening status into closed
  workspaces, remain deferred design choices rather than implemented promises.

## Default branch-name review — 2026-11-03

- User questions the `coco/` prefix and asks whether workspace names can map
  directly to branch names. Investigation/recommendation only; no default,
  branch, worktree, stored binding, public-doc or code change authorized yet.
- Production prefix policy is a default in coordinator creation plus CLI
  walkthrough/review presentation (and help), not an identity/safety mechanism.
  `--branch` already accepts arbitrary validated new-branch names, including
  prefix-free ones. Worktree paths use repository/workspace identity; lifecycle
  operations use the stored actual branch name.
- Delete ownership uses `NewBranch` mode and verified creation/worktree evidence,
  not the branch prefix. Adopted branches remain protected independently of
  their names. Git ref validation/collision checks reject both exact matches and
  parent/child names such as `feat` versus `feat/login` before creation.
- Recommendation: use the workspace name verbatim for newly allocated default
  branches, preserve explicit `--branch` and `--checkout`/detached semantics,
  and leave existing prefixed branches/bindings unchanged. The prefix's benefits
  are discoverability and reducing collisions with ordinary repository branches;
  neither is needed for isolation. Never silently adopt/rename on collision.
- Any approved implementation must update default displays/help/docs and tests
  for creation, collisions, close/reopen/delete and old bindings/replay/recovery;
  keep arbitrary prefixed names valid rather than globally replacing fixtures.
- Verification: existing walkthrough custom/existing/detached-worktree test
  passes, including a prefix-free custom new branch. Creation replay resolves
  the stored operation before deriving a new default. Only this review record
  changed; product defaults, user branches and the Git index are untouched.

## Published-release compatibility check — 2026-11-03

- User asks which Codex version CoCo 0.3.0 works with. The tag is not present
  in this local clone, so read its exact README through the GitHub contents API
  at `ref=v0.3.0`, without fetching or changing repository references.
- The published tagged README states Codex 0.160.1 as its tested version.
  Report that baseline rather than claiming all later versions work. No
  installation change, compatibility test run, commit or push performed.

## Feature recommendation — 2026-11-03

- User asks what would be useful next. Review/recommendation only; no new
  implementation, commit, push or publication authorized.
- Checked the canonical follow-up order and existing hooks boundary. Compose
  already has its isolated implementation branch, and automatic idle shutdown
  remains deliberately lower priority. Do not present either as newly absent
  core behavior, or duplicate native Codex session hooks.
- Recommend exploring workspace presets: reference existing Codex profiles
  and prefill CoCo-owned creation choices such as Git base, worktree mode and
  resource policy. Keep context selection explicit/fresh by default and retain
  ordinary flags/walkthrough. Suggested `--preset` syntax is hypothetical, not
  implemented or approved. A smaller alternative is named saved status views;
  designed handoff artifacts remain a larger separate candidate.
- No product files or canonical roadmap changed for this recommendation.

## Create context picker scope review — 2026-11-03

- User asks for a quick diagnosis of missing interactive context sources,
  especially workspaces from other repositories. Review only; no product fix,
  commit, push or runtime mutation requested.
- Confirmed `CreateWalkthrough::prompt_context` calls `list_workspaces`, whose
  shared repository-list helper always sets `RepositoryScope::Repository` for
  the selected creation path. Other registered repositories are not queried.
  The existing context-picker test explicitly asserts this repository scope.
- Additional selection constraints: only idle, not-loaded, active,
  waiting-for-approval and waiting-for-input phases are requested; the current
  worktree's owner is offered separately and removed from the other-workspace
  list. The empty-state text does not disclose the repository restriction.
- This behavior predates status ordering: its only create-walkthrough change
  updates the default sorter call, not source discovery or phase selection.
- Follow-up proposal, not implemented: keep code/worktree base selection scoped
  to its repository, while allowing context selection across registered
  repositories with repository labels to disambiguate names. Backend workspace
  reference rules must be checked before implementing cross-repository choice:
  workspace sources currently call `resolve_workspace_in_repository`; native
  thread-ID sources instead read the exact thread independently of repository.
- Verification: the single existing context-picker eligible-source/scope test
  passes offline against its isolated test RPC server. No product code changes;
  diagnosis is recorded here only. Status-ordering work remains complete.

## Prior review

### Status ordering and filtering review — 2026-11-02

- User reports that `status --sort state` appears repository-first and requests
  thorough review plus a proposed multi-key/per-column sorting and filtering UX.
  This is investigation/design only: no product changes, commit, push, runtime
  mutation or Compose-branch work is authorized.
- Plan: trace current sorting through flat/tree/follow/JSON and inspect existing
  tests; verify available typed field/phase/resource/usage evidence; recommend
  explicit sort priority, tie-breakers, direction, optional projections and
  unloaded filtering without changing execution or native state semantics.
- Confirmed current behavior:
  - `cli/status.rs::sort_workspace_collection` always compares repository
    display name, then repository path/ID, before the selected name/state key.
    Explicit state order therefore cannot put a waiting workspace from a later
    repository above a ready workspace from an earlier repository.
  - State sorting uses six broad attention buckets, not distinct per-state
    keys. Prepared/unloaded, ready/completed, and several other states can be
    interleaved by workspace name. Natural numeric names still work correctly.
  - The flat renderer preserves input order. Both one-shot JSON/human and
    collection follow use this same comparator; this is not a renderer-only
    or follow-only failure. Tree rendering requires contiguous repository/name
    groups and currently rejects every explicit sort.
  - The visible repository column contains the path while its primary sort
    key is the display name, which can disagree with visible lexical order.
  - Only one `name|state` key is accepted; no CLI state filter is exposed.
    Existing tests check natural comparison, phase ranks, parsing and rendering,
    but not the composed multi-repository state-priority expectation. The
    canonical product spec explicitly records the older repository-first rule;
    change that contract intentionally when implementation is authorized.
- Proposed UX, not implemented/accepted:
  - Keep normal collection defaults repository/name. Explicit ordered keys
    (`--sort state,name`, or repeated `--sort state --sort name`) always win
    left-to-right; append deterministic missing default tie-breakers only
    after all requested keys, with identity last. `--sort repo,state,name`
    explicitly requests the previous grouping behavior. Make repository sorting
    consistent with the displayed path, not an undisclosed name.
  - Prefix a key with `-` for descending (`--sort=-memory,state,name`). Retain
    attention-first state semantics, but use an explicit distinct state order
    within each urgency class. Never derive state from activity text or create
    new native/domain states. Preserve the current natural workspace-name
    comparison and keep unavailable values last in either direction.
  - Typed keys should start with state/name/repo/model/branch and resource
    memory/cpu/procs plus usage tokens/context. Use native current model/effort,
    actual displayed memory source, raw token totals and context ratio, never
    formatted/truncated/colored strings. Resource/usage keys should explicitly
    compose with `-r`/`-u`; do not silently sort missing unrequested data or add
    expensive hidden billing calls. Clients need an explicit count-versus-label
    definition; cost must not numerically mix USD and credits. Account quota is
    global and is not a meaningful workspace sort key. Avoid a generic query
    language, extra per-column flags or an unrequested recent/activity key.
  - `--exclude-state unloaded` is a useful next small addition; generic comma-
    separated/repeated state selectors scale better than one hide flag for each
    state. A positive `--state` selector can use the same matching rules. Use
    visible CLI labels (unloaded/working/needs-input) mapped onto existing phase
    evidence, retaining raw JSON phases. Hide only the view, never stop, close
    or unload anything. Prepared remains distinct from unloaded.
  - Keep explicit sort and tree mutually exclusive in the first slice rather
    than pretending a hierarchical grouping can also be globally state-first.
    Filters can run before constructing the tree and prune empty groups. Apply
    identical ordering/filter decisions to JSON and each follow refresh; an
    empty filtered view must keep following, not exit.
- Implementation cautions for a later authorized slice:
  - Resource keys must request and compare actual available running-runtime
    fields; missing/unsupported/nonfinite values are not zero. Usage must be
    joined by workspace ID before sorting; keep hydrated status/model fields
    from the status query rather than replacing them with the usage endpoint's
    stored workspace snapshot. No optional usage/cost read for simple name/state
    sorting. Quota remains one independent account-global result.
  - The daemon already accepts positive phase filters. However, passing any
    `phases` list disables its normal closed-workspace exclusion; do not build
    an exclusion as all-phases-minus-unloaded and accidentally include closed
    workspaces. Preserve default scope/lifecycle exclusions. Any future explicit
    closed-status selection needs a deliberate contract and usage support.
  - This comparator is also used by plain list and the create-context workspace
    selector. Preserve their existing defaults instead of accidentally making
    them globally name-sorted when explicit status `name` changes meaning.
  - Required regressions: multi-repo waiting-versus-ready precedence; each
    ordered/reversed key and ties; distinct states within priority classes;
    natural names and visible repository paths; null/unsupported metrics; usage
    joins and unit handling; follow transitions and empty filtered frames;
    tree pruning; JSON parity; unchanged list/context-picker defaults; invalid,
    duplicate and repeated sort/filter arguments. No new protocol or schema
    shape is inherently required for presentation-only order/filter controls.
- Verification: 24 existing focused tests pass offline (two status, 11 collection
  CLI parser, 11 collection rendering). A disposable `/tmp` Rust harness copied
  the actual pure comparator unchanged and used minimal matching field fixtures:
  four probes reproduce repository-first state sorting, prepared/unloaded ties,
  hidden display-name precedence and natural numeric workspace ordering. No
  daemon, model turn, user workspace, runtime or network operation was needed.
- Handoff: present sorting-first recommendation, then state filtering. Only this
  mandatory internal review record changed; no CLI implementation, public docs,
  Git index/history, branch, remote or Compose-feature content was changed.
  No commit, push or publication. Await implementation authorization.
