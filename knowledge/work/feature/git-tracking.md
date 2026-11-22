---
type: Working Document
title: "feature/git-tracking: compact branch/upstream information"
description: Approved cross-surface Git tracking display and bounded native observation.
tags: [work, branch, cli, git, ux]
status: complete
branch: feature/git-tracking
updated: 2026-11-22
---

# Compact Git tracking display

## Intended outcome

Implement the main-branch review approved by the user: show compact native
upstream relationships while choosing code and using status/workspace pickers.
Preserve the existing main review notes. No commit, merge, push, publication,
installed-runtime change or real model invocation without separate approval.

## Active work

- [x] Add typed optional tracking evidence and bounded native Git observation,
  with batched repository reads and short-lived refresh caching.
- [x] Project it through existing application/protocol seams for creation,
  status/list/tree/follow and human workspace/path pickers. Preserve direct path
  output and graceful optional failure; no extra public CLI flag or registry.
- [x] Share compact semantic rendering, colors/plain text and responsive width;
  keep matching workspace/branch names deduplicated and recency usable.
- [x] Cover native fixtures, parser/error/cache/scope cases, creation races,
  protocol compatibility and render/terminal regressions without models.
- [x] Update affected public use instructions and canonical engineering/product
  knowledge; run sequential Rust, terminal and static docs gates; final review.
- [x] User authorized fixing both follow-up findings: first add reproducing
  regressions, then verify linked `commondir` identity and clipped-cell padding.
  Re-run bounded model-free checks; no commit, push or Codex-version work.

## Decisions and constraints

- Base-relative GitObservation aheadBy/behindBy keep their current meaning.
  Upstream evidence is separate, ephemeral and advisory, never a state authority,
  creation permission, cleanup guarantee or persisted conversation/workspace fact.
- Use configured upstream, including non-origin/local/differently named refs.
  Equal, ahead, behind, diverged, no upstream, missing target, unknown and detached
  remain distinct. Remote source refs are not local branches with no upstream.
- Read only local metadata; no fetch, branch/config changes, inferred upstream,
  automatic branch substitution or hidden network access. Unknown supplementary
  metadata must not break ordinary navigation/status or weaken base validation.
- Neutral types → protocol/application policy → Git adapter; CLI renders through
  typed RPC only. No crate/dependency/database changes or broad Git framework.
- Colors are semantic fragments applied after width/sanitization, not embedded
  escape sequences in data. Preserve NO_COLOR, pipes, JSON and pure path stdout.

## Findings

- Git source inspection currently discards native porcelain upstream/count
  headers; branch discovery omits upstream atoms. Collection/navigation rows
  have only saved branch identity. Targeted status observes validated Git/base
  facts, but its human renderer currently ignores them.
- Existing bounded asynchronous diagnostic capture can support optional metadata
  work without extending the ordinary mutating Git command runner's contract.
- Implemented optional metadata uses a repository-wide native for-each-ref and
  NUL-framed worktree inventory, after checking actual common-directory identity.
  It reads no working files/index and disables lazy fetch/replacement objects.
  The strict existing source inspection now skips its unused ahead/behind graph
  calculation; optional counts have one shared bounded owner rather than being
  computed twice. Mandatory base/ref/binding validation is unchanged.
- Five-second bounded memory cache includes failed attempts, with one in-flight
  read per repository/path key. Views budget 750 ms in total, two concurrent
  reads, 500 ms per repository and at most 64 repositories. Late/changed/missing
  evidence remains unknown; closed workspaces have no live tracking projection.
- Optional response fields extend existing typed reads without new request flags,
  RPC methods, worker calls, storage or dependencies. Creation's daemon adapter
  keeps mandatory inspection off the async executor and enriches only its typed
  result afterward. Shared semantic text preserves plain values until rendering.
- Fresh worktree metadata additionally checks canonical paths and small Git
  administrative links/backlinks. A missing/replaced directory cannot pass only
  because Git retains a stale registration. Native separate-git-dir repositories
  may have refs without a verifiable main-worktree entry; keep those branch refs
  usable while their unverified live worktree remains unknown.
- A new walkthrough regression initially used remaining-defaults before Code,
  skipping the step it intended to assert. Its script now explicitly visits Code
  and only then defaults later settings; the focused case and complete rerun pass.
- Final review found that a linked worktree's `.git` file could point directly
  to the shared main Git directory while leaving a stale linked registration.
  Only the first native worktree record may bind directly to that directory;
  linked records require their own matching administrative backlink. Isolated
  native tests cover the altered binding from both source and child paths and
  recovery after restoring it. Bare/unborn main records do not promote a linked
  entry. Git documents the main-first inventory order in its
  [worktree manual](https://git-scm.com/docs/git-worktree).
- Terminal smoke revealed a fixture-only ANSI assumption: tmux preserves bright
  yellow as indexed color 11 rather than always `33m`. The color assertion now
  accepts both encodings and still verifies NO_COLOR and terminal restoration.

## Verification

- Prior review: seven isolated native Git state probes passed; no model calls.
- Initial tracking gate passes 18 filtered cases (17 new and one existing),
  including actual native state/identity/worktree tests and cache/projection
  regressions. All-target check passes. Existing CLI suite passes 323 tests,
  with 9 opt-ins ignored, using isolated socket permissions. Its first sandboxed
  run hit denied Unix-socket binds (44 infrastructure failures), not source
  assertion defects; the same suite passed outside that socket restriction.
- Pre-follow-up all-target suite passed 944 library + 47 integration = 991 model-free
  tests, with 13 library/manual and 19 integration/native opt-ins ignored.
  This includes actual CoCo process/native-Git inspection, independent upstream
  versus base counts, scoped multi-repository rows, slow/failed/cache refresh,
  malformed/truncated/absent metadata, shallow histories, real administrative
  binding drift, source-review consent and a real CLI follow transition driven
  only by tracking evidence. No actual Codex process/model used.
- The seven focused coordinator tracking cases pass again after strengthening
  the refresh case to wait for real five-second cache expiration rather than
  only clearing the test cache. The 13 focused Git tracking/parser/native tests
  pass, including the final administrative-binding regressions.
- Exact Rust gates: `CARGO_INCREMENTAL=0 cargo test --offline --all-targets -j 2
  -- --quiet --test-threads=2`, `CARGO_INCREMENTAL=0 cargo clippy --offline
  --all-targets --all-features -j 2 -- -D warnings`, `cargo fmt --all -- --check`
  and `git diff --check` pass. Tests requiring local sockets ran with isolated
  socket permissions. Cargo jobs and test concurrency stayed bounded.
- `cargo machete` reports no unused direct dependencies. `cargo deny --offline
  --locked check` passes advisories, bans, licenses and sources against its
  existing local databases, with the unchanged duplicate-version warnings.
  Its initial sandboxed attempt could not lock the read-only cache; the offline
  rerun with that cache-lock permission passed. No dependency changes or
  vulnerability-database refresh were made.
- `python3 tests/terminal_smoke.py target/debug/deps/coco-b6c9c99902e1dbd7`
  passes all isolated tmux probes: picker positioning/resizing/restoration,
  guided creation/back/edit/defaults/error recovery, y/N input, Git badge color
  and NO_COLOR, recency and in-place follow refresh. The temporary tmux server
  is cleaned up; user sessions and installed CoCo services remain untouched.
- Public docs: direct local `astro check` has no errors/warnings; `astro build`
  completes the fully static export; `node scripts/verify-export.mjs` verifies
  70 files, 21 pages, search, routing and the public-only boundary. No deployment.
- Final diff/source review includes the new private modules and fixtures,
  optional/legacy DTO compatibility, plain/JSON/path output and unchanged
  mutation/base-validation boundaries. Nix packaging and macOS execution were
  not exercised locally; no real-model or latest-Codex compatibility claim.

## Next task after this slice

- Investigate current released Codex compatibility and the user-reported
  `coco ai requires the verified Codex 0.166.0 configuration contract` failure.
  The durable task is in `knowledge/product/v0-spec.md`. Do not bypass that guard
  or broaden this slice into a Codex upgrade before finishing Git tracking.

## Follow-up review, 2026-11-22

Review only; no product/test implementation changes or publication.

- **P2 — incomplete linked common-directory verification**
  (`src/git/tracking/binding.rs:51`): a linked administrative directory is
  accepted from its parent path and `gitdir` backlink without checking its own
  `commondir` file. An isolated native fixture points that file to a second
  repository with the same branch name and commit, but different upstream
  configuration. Native worktree inventory still reports the matching branch
  and commit; the actual production binding helper (included unmodified in a
  temporary Rust probe) accepts it. The snapshot can therefore show the first
  repository's tracking for a checkout whose actual common directory is foreign.
  Verify the resolved administrative `commondir` against the expected canonical
  common directory and cover same-commit foreign binding in native regressions.
  This finding concerns advisory tracking evidence, not a change to mandatory
  creation/mutation validation or an authorization bypass.
- **P2 — missing padding after wide-character truncation**
  (`src/cli/output/collections.rs:628`): segmented cells compute padding from
  the original text width, not the width actually rendered after truncation.
  Truncating before a double-width character can leave one unused column. A real
  `coco status -at` against an isolated fake RPC server, with one short workspace
  and `x` followed by 40 CJK characters, places STATE at column 66 for the header
  and short row but at 65 for the long row. Subsequent columns shift too; long
  Unicode activity labels can hit the same path. Pad the clipped semantic text
  to its assigned cell width and test column alignment, not just maximum width.
- Verification in this review: 30 focused tracking tests pass, one interactive
  opt-in skipped; `git diff --check` passes. An additional isolated native Git
  probe with a missing ancestor exits nonzero, so the existing capture boundary
  correctly rejects that graph instead of manufacturing equal counts. All
  probes are model-free and use temporary state; installed services, real
  repositories and user tmux sessions were not changed.

## Authorized review fixes, 2026-11-22

- First added both reproducing library regressions and ran them before changing
  production code. Both failed for the reviewed reasons: foreign common-dir
  metadata returned `InSync` rather than unknown, and a width-3 segmented CJK
  cell rendered only two columns. This establishes that the tests detect the
  original defects instead of only exercising the fixed path.
- Linked entries now additionally resolve their bounded `commondir` metadata
  relative to their administrative directory and compare its canonical target
  with the expected shared Git directory. Keep the existing backlink check,
  optional-read budgets and main-worktree handling. No extra subprocess or
  mandatory creation/mutation validation change is introduced.
- Segmented cells truncate semantic text before calculating the actual display
  width used for padding. Tests check exact cell width across 0–89 columns,
  colored/plain rendering and unpadded final-column behavior. A real CLI/fake-RPC
  regression additionally checks STATE, MODEL and BRANCH positions in flat and
  tree status with both long CJK workspace names and CJK activity labels.
- Native regression covers a foreign common directory at the same branch and
  commit with different upstream configuration, invalid/empty/oversized metadata,
  and restoration to valid relative and absolute common-directory targets.
- Both reproducing library cases pass after the fixes. Final all-target rerun
  passes 946 library + 48 integration = 994 model-free tests, with 13 library
  and 19 integration/native opt-ins ignored. The new real-CLI flat/tree column
  regression passes as part of the 16-case status-query target.
- Clippy (all targets/features, warnings denied), Rustfmt check and diff check
  pass again. The complete isolated tmux terminal smoke suite passes, including
  colors/NO_COLOR, picker/creation navigation and in-place follow restoration.
  Cargo gates remained sequential with two jobs and two test threads. No model
  calls, installed-runtime changes, dependency/API changes or publication.
- Canonical architecture now records the linked `commondir` check. Public
  wording and contracts are unchanged: unavailable tracking remains `?`; this
  restores the already documented behavior rather than adding another feature.

## Re-review after fixes, 2026-11-22

- No additional actionable findings from rechecking the complete tracking
  adapter/projection and presentation paths, with particular attention to
  canonical common-directory identity, relative/absolute metadata, bounded
  optional failure, clipped semantic width, repository scope and legacy reads.
- Re-ran 32 focused tracking cases (one interactive opt-in skipped), all 16
  status-query process cases and all 34 output cases; all pass. Rustfmt and
  diff checks pass. These targeted checks supplement the unchanged source's
  preceding full 994-test, Clippy and terminal proof; the full suite was not
  redundantly rerun for this read-only review.
- Confirmed follow compares rendered human frames, so observation-time-only
  refresh does not introduce duplicate unchanged tables. Direct path output
  still avoids the optional tracking probe.
- Only this working document changed during review. No model calls, installed
  runtime changes, Codex compatibility work, commit, merge or publication.

## Handoff

- User now authorizes the local checkpoint only. Include the completed Git
  tracking slice, its regression fixes/tests and documentation in one
  `feat(cli): show Git upstream tracking across workspace views` commit.
  No merge, push, release, installed-runtime change or new Codex work is authorized.
- Git tracking implementation, both review fixes and their passing gates are
  present locally on `feature/git-tracking`, based on `f3596fd`. Both reviewed
  P2 findings are resolved. User's prior main-branch review notes remain preserved.
  Product, tests and public/internal documentation belong to that local checkpoint;
  no merge, push, release or installed-runtime change is performed.
- Next concrete work is the separately queued current-Codex compatibility
  investigation. It was not started as part of these fixes, and the exact-version
  AI guard remains unchanged. Merge and publication still need separate approval.
- Tracking remains advisory and bounded: very large/slow or inconsistent Git
  metadata may show `?`, and remote freshness still requires the user's fetch.
