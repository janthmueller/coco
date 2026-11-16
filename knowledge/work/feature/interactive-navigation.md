---
type: Working Document
title: "feature/interactive-navigation: editable creation walkthrough"
description: In-memory back navigation, reusable terminal input and explicit creation review.
tags: [work, branch, cli, ux]
status: complete
branch: feature/interactive-navigation
updated: 2026-11-16
---

# Interactive creation navigation

## Intended outcome

Complete the approved first delivery of the navigation proposal recorded in
`../main.md`: users can back out of nested choices, revisit previous creation
questions, edit existing answers and explicitly approve the resulting draft.
The branch starts from released main 0.11.0; the existing uncommitted audit
record is preserved. Current Git/publication authority is recorded under
Authorized delivery; installed-service changes and real model calls remain
outside scope.

## Authorized delivery — 2026-11-16

- User explicitly authorizes the proposed commit → local main integration →
  push → exact-source CI → stable rehearsal → release sequence. Preserve all
  reviewed files, including prior main audit/history notes and new modules.
  Expected version is 0.12.0; the workflow rehearsal must confirm it.
- [x] Commit the complete reviewed slice and fast-forward local main safely.
- [x] Refresh origin, push normally and require successful Rust/Docs workflows
  for that exact main SHA, including package, Nix and native Linux/macOS smoke.
- [x] Recheck remote tip/disabled automatic-alpha state, dispatch stable with
  publish=false and verify version/metadata before publish=true.
- [x] Verify the actual release/tag, Cargo availability/provenance and native
  assets/checksums, update delivery receipts and synchronize local main.
- Do not change release settings, stamp versions manually, bypass guards,
  rewrite history, alter installed services or invoke real models. The last
  reviewed source passes 958 model-free tests, Clippy, formatting, expanded
  terminal probes and static docs; exact-source hosted gates remain mandatory.
- Source `6c518cdff4eca22581f6023b3514d42a24abb657` is fast-forward integrated
  on main and pushed normally. Hosted Rust `43918411568` and Documentation
  `43918411780` succeeded, including Nix/package, native platform smoke and
  Pages. Stable rehearsal `43920610702` confirmed 0.12.0 / v0.12.0 before the
  first authorized publishing attempt `43921063354`.
- Delivery is complete on main as stable v0.12.0. The first publishing attempt
  pushed only metadata before GitHub rejected its tag; exact-tip Rust recovery
  `43921694078`, stable rehearsal `43924039945` and publishing `43924502204`
  succeeded without changing settings or bypassing guards. Release tag and
  clean crate provenance resolve to `d0e5a04d4a724dac32516867dc6a2365ade7c539`.
  Both native archive checksums/contents, uploaded Cargo.lock and public registry
  metadata were independently verified. Full delivery receipts are in main's
  working document; no installed-runtime changes or real model calls.

## Delivery readiness check — 2026-11-16

- User asks whether commit/merge/push/release are ready, not to execute them.
  Read-only GitHub verification confirms remote main remains
  `020803419fa7316d210ab28b27790e8bb44204b4`, also the local main and current
  feature-branch base. No integration conflict or rebase is currently indicated;
  the complete slice, including new modules, still needs staging and commit.
- `COCO_RELEASE_ENABLED` is confirmed false and unchanged. Pushing main runs
  Rust verification and documentation delivery, not automatic crate publication.
  Local review/gates support committing and fast-forward merging the slice.
- Require exact-pushed-SHA hosted Rust/Docs success before release: fresh
  dependency policy, registry-package verification, Nix flake checks and native
  Linux/macOS smoke remain CI gates, not locally completed evidence. Then run
  stable `publish=false` rehearsal and verify the proposed version before an
  explicitly authorized `publish=true` dispatch. A feature commit is expected
  to propose 0.12.0; the actual rehearsal is authoritative.
- No staging, Git ref write, push, workflow dispatch, variable change or
  publication performed. Initial SSH remote read failed at sandbox SSH-config
  ownership checking; the successful GitHub API read establishes the actual tip.

## Full post-correction review — 2026-11-16

- User requests a full review after the carry-policy correction. Keep the
  tracked/untracked slice intact; only this work record may change. No product
  fixes, staging, commits, push, release, services or real model calls.
- [x] Reinspect the complete changed behavior, test fidelity, documentation
  and original mutation boundary; audit presets, nested Back, retry checkpoints,
  changing Git evidence and return-to-review semantics.
- [x] Rerun sequential model-free Rust gates and isolated terminal checks,
  including additional representative edit/default/Back combinations.
- [x] Report only actionable, reproducible findings (or none), with exact
  verification and remaining native/platform/package limits.

### Full review outcome and verification

- No additional actionable defect or merge blocker identified in the full
  tracked/untracked slice. Rechecked creation entry/normalization, exact scoped
  source IDs, draft/answer checkpoint ownership, pending-versus-accepted input,
  dependency transitions, applicable history, read-only recovery, terminal input
  and restoration, and public/internal documentation boundaries. The existing
  creation/send/jump mutation boundary still starts only after wizard acceptance;
  post-submit errors never reopen/resubmit the walkthrough.
- All-target model-free suite passes again: 912 library + 46 integration = 958
  tests; 12 library/manual and 19 integration/native opt-in cases remain ignored
  in the ordinary run. Warning-denied all-target/all-feature Clippy, formatting
  and `git diff --check` pass. Cargo gates run sequentially with incremental
  caching disabled, two build jobs and two test threads.
- The complete expanded terminal smoke passes on its temporary test servers,
  including all four previously reported defects, clean-source Enter behavior,
  Esc/Shift-Tab, safe paste, size/error cleanup and unchanged confirmation/follow
  coverage. Additional ad hoc model-free terminal traversals preserve independent
  code/context/profile across name/detached edits, and clear temporary conversation
  retention when switching cleanup to persistent while retaining correct Back.
  Both extra traversals cancel; no workspace or native turn is created.
- Fresh direct Astro check has zero errors/warnings/hints. Fully static build
  and export verification pass: 70 files, 21 pages, search, root routing and
  public-only boundary. No package install, external hosting action or public
  source edit is involved.
- No real Codex/model runs, installed runtime changes, Nix package build,
  macOS/native-platform build or fresh online dependency/security audit claimed.
  These are unchanged verification limits, not evidence of a found defect.
  Source/tests/public docs/Git refs stay unchanged during this review; only this
  work record is updated. All prior/new files remain unstaged and uncommitted;
  no push or publication.

## Authorized carry-policy correction — 2026-11-16

- User authorizes fixing the follow-up P2. Preserve the entire slice, stay on
  this branch, and do not stage, commit, push, release or use real models.
- [x] Add a failing regression for dirty → clean final review → edit Git
  changes → Enter retaining the reviewed Don't copy policy.
- [x] Synchronize only the current cached choice when the clean-checkout path
  automatically resets the copy policy. Preserve checkpoint rollback and
  explicit empty-category carry presets.
- [x] Cover Back/Retry across this transition and Enter preserving explicit
  tracked/both-category presets; recheck the real CLI with isolated fake RPC.
- [x] Run focused/full model-free tests, Clippy, formatting and terminal checks
  sequentially; update durable notes and handoff with the actual evidence.

- Initial red verification: the exact Enter-after-reset case and its read-only
  retry variant fail on the unfixed code by re-enabling carry. Back-checkpoint
  and explicit empty-preset controls already pass.
- Fix records Don't copy as the current UI choice only on the automatic clean
  path. Explicit question answers reuse that same narrow recording method;
  insert-only preset seeding and the accepted answer checkpoint are unchanged.
- Four permanent scripted regressions now pass, including both tracked/both
  explicit presets. The existing navigation suite passes all 38 ordinary cases;
  three manual probes are ignored in that run. Added a manual clean-source
  probe to the permanent terminal smoke suite for real Enter behavior.
- Final verification passes: all-target model-free suite has 912 library + 46
  integration = 958 passing tests, with 12 library/manual and 19 integration /
  native opt-in cases ignored by the ordinary run. Used `CARGO_INCREMENTAL=0
  cargo test --offline --all-targets -j 2 -- --quiet --test-threads=2`, with scoped
  escalation only for temporary local socket fixtures. No real model calls.
- All-target/all-feature warning-denied Clippy, formatting and
  `git diff --check` pass. The full expanded terminal smoke passes, including
  the permanent dirty → clean → Edit → actual Enter probe. Its own temporary
  tmux servers are cleaned up, with cursor/wrapping/raw-mode checks preserved.
- Replayed the original review reproduction against freshly built
  `target/debug/coco` and a strictly read-only fake RPC server. Both review and
  editor show Don't copy, and actual Enter retains it. Cancelled before create;
  exactly one repository resolution and four Git inspections, no create/send /
  model requests. All data/socket/Codex paths are temporary and cleaned up.
- Durable product semantics record the automatic-policy/current-choice link.
  No public-doc, dependency-version or lockfile changes in this correction;
  no new docs/Nix/native build or dependency audit is claimed. All edits remain
  unstaged/uncommitted; no push, release or installed-runtime action.

## Follow-up review — 2026-11-16

- User requests another full review after the three corrections. Preserve
  product/tests/public docs and Git refs; only this work record may change.
  No fixes, staging, commits, remote writes or real model calls.
- [x] Check checkpoint/rollback consistency, history/applicability and dependent
  settings against the corrected source and all added regressions.
- [x] Reproduce any further actionable issue with isolated model-free probes;
  rerun focused checks and record exact evidence and residual limits.

### Follow-up finding (recorded at review; corrected above)

- **P2 — Automatic clean-checkout policy reset leaves stale editor defaults.**
  `git_source.rs::prompt_copy_policy(false)` resets the carry args and returns
  early for a clean checkout, without synchronizing the cached Git changes
  selection. Reproduce by choosing Copy tracked while one tracked file is
  changed, then returning clean Git evidence at final review. Review correctly
  says Don't copy. Open Edit settings → Git changes: it incorrectly highlights
  Copy tracked (0 files), because `seed_changes` uses insert-only `seed_choice`
  and the old `tracked` key still wins. Enter can therefore re-enable copying
  instead of retaining the policy shown in the accepted review.
- Reproduced with the actual freshly built `target/debug/coco`, its real
  terminal adapter and an isolated Python fake RPC server. The server allows
  only repository resolution/inspection; initial inspection reports one tracked
  change, the next two inspections report clean. Recorded calls are exactly
  repository.resolve and three repository.git.inspect requests. Cancelled at
  the mismatched editor; no workspace.create, send, native/model call or real
  repository operation. All socket/data/Codex paths are temporary; test tmux
  server and fake listener are cleaned up. No source/test changes were needed.
- Fix the automatic policy transition and UI memory together, without clearing
  deliberate explicit empty-category presets or resetting unrelated answers.
  Add a regression asserting both review value and the following editor's
  focused key, including Enter preserving Don't copy.

### Follow-up verification and outcome

- Rechecked the complete corrected state machine, checkpoint ownership,
  dependent-setting handling, terminal boundary, tests and affected public
  usage claims. The previous three findings do not recur in their permanent
  tests or expanded real-terminal sequences.
- All 89 Create and 34 shared-prompt tests pass (123 distinct focused cases;
  five ordinary opt-in probes ignored), including all 34 navigation cases.
  The complete expanded terminal smoke passes again, including its separately
  isolated size-error case. `git diff --check` passes.
- The full 954-test/Clippy/format evidence above is from the correction turn,
  not rerun wholesale in this review. No new docs/Nix/native build or fresh
  security audit claimed. Source, tests, public docs and Git refs are unchanged;
  only this record is updated. One new P2 remains for user-authorized correction.

## Authorized review corrections — 2026-11-16

- User authorizes fixing the three review findings. Preserve the complete
  existing slice and main audit; remain on this branch. No staging, commit,
  push, release, service change or real model call.
- [x] Add permanent scripted regressions for the exact three reproduced
  transitions and demonstrate failure on the unfixed implementation.
- [x] Include selection/text memory in accepted-setting rollback. Preserve
  ordinary nested Back and pending input on retry/first-time walkthrough Back;
  those are not cancellation of an already accepted setting.
- [x] Clear edit return mode when the edit menu returns to ordinary review.
- [x] Reuse an applicable-step predicate for backward history, edit choices
  and Back availability; skip invalidated Code entries backward, not forward.
- [x] Extend terminal smoke with the actual failing keyboard sequences;
  rerun focused/full model-free Rust tests, formatting, Clippy and terminal
  checks sequentially, then update canonical knowledge and this handoff.

### Correction progress

- Added six initial permanent regressions before the fixes: four fail on the
  reviewed implementation (all three reported defects plus accepted-input
  rollback outside the review-edit path), while nested/first-visit input cases
  remain green. They pass after the initial fixes.
- Added explicit Back-availability coverage when every earlier step has become
  inapplicable, and retained pending Code focus on read-only discovery retry.
- The combined retry-then-abandon regression exposed a remaining rollback
  issue, failing with `abandoned` instead of accepted `source`. A private
  answer checkpoint now spans retries of the same reopened setting/review;
  pending UI input stays available on retry but cannot become the rollback
  baseline. The final correction passes that regression and the full suite.
- Extended the existing isolated terminal suite with the actual review
  sequences for Esc and Shift-Tab. Move the deliberately unusable-window probe
  to its own fresh temporary tmux server: the previous shared pane could match
  old cancellation output or retain a mismatched kernel size after shrinking
  (observed logical pane 80x20 versus `stty` 3x80). Fresh history and process/PTY
  isolation ensure the current error is awaited without contaminating later
  input probes. No production size workaround or user tmux server is involved.

### Correction verification and outcome

- All three review findings are fixed. Accepted UI selection/text rollback is
  part of the draft snapshot, anchored across read-only retries; pending input
  on first visits, nested Back and Retry still behaves as before. Leaving the
  edit menu resets edit return mode. Current applicability is shared by history,
  edit choices and Back availability, so branch adoption cannot reverse the
  direction of a Back action or advertise an unavailable previous question.
- Eight permanent new regressions pass, covering the three reported failures,
  reopened-main-step rollback, retry-then-abandon, first-visit/nested input
  retention and no-applicable-history Back availability. The prior discovery
  retry test also asserts that pending Code focus remains available.
- Final model-free all-target suite passes: 908 library + 46 integration = 954
  tests; 11 library/manual and 19 integration/native opt-in tests remain ignored
  by the ordinary run. Command: `CARGO_INCREMENTAL=0 cargo test --offline
  --all-targets -j 2 -- --quiet --test-threads=2`, with scoped escalation only
  for isolated local socket fixtures. No real model or installed Codex service.
- All-target/all-feature warning-denied Clippy, `cargo fmt --all -- --check`
  and `git diff --check` pass. Dependency versions/lockfile and public docs
  are unchanged by these corrections; no fresh dependency audit, docs or Nix
  build is claimed for this correction turn.
- The entire expanded `tests/terminal_smoke.py` passes against the freshly
  built `target/debug/deps/coco-b598e2530c5d52da`: all previous terminal checks,
  the abandoned-context/edit-mode sequences for Esc and Shift-Tab, applicable
  backward history after branch adoption, and the separately isolated layout
  failure. Temporary test servers are cleaned up; user sessions are untouched.
- Remain on `feature/interactive-navigation`; all existing and new changes
  are preserved, unstaged and uncommitted. No push, release, workflow dispatch,
  installed service/configuration change or real model call.

## Independent slice review — 2026-11-16

- User requests a full review of the current tracked and untracked slice.
  Preserve product code; only this review record may change. No fixes, staging,
  commits, remote writes or real model calls.
- [x] Recheck wizard history, draft restoration, defaults, dependency changes,
  shared terminal ownership and public behavior against the actual tests.
- [x] Reproduce actionable edge cases with isolated fixtures/terminal probes,
  rerun relevant checks and report findings with narrow source references.
- [x] Record remaining limits and the next concrete action without treating
  the previous green suite as proof that every new transition is correct.

### Findings (recorded at review; resolved by the authorized corrections above)

1. **P2 — Abandoned context edit survives in UI memory.**
   `navigation/draft.rs::Snapshot` restores args/labels but does not restore
   `DefaultsInteraction` selections/texts; `seed_choice`/`seed_text` only insert
   absent entries. Confirm context `thread-old`, edit its thread ID to
   `thread-abandoned`, Back from Context size and abandon the entire setting.
   Review correctly retains `thread-old`, but reopening Context/Thread ID
   prefills `thread-abandoned`. A later Enter/Tab can reuse the abandoned
   value rather than the accepted draft. Restore the accepted UI memory when
   abandoning a whole setting, while retaining intentional child navigation.

2. **P2 — Edit mode remains active after abandoning the edit menu.**
   In `navigation.rs`'s Back handler, the editing branch updates `current` and
   `reopen`, but not `editing`. After backing out of an edited Context and
   backing out of Edit settings to Review, Esc on Review reopens Edit settings
   rather than the previously visited main question (Profile in the probe).
   The command can loop Review ↔ Edit settings. Clear the edit return mode
   when returning to normal Review and add a repeated-back regression.

3. **P2 — An inapplicable historical step makes Back move forward.**
   `history.pop()` selects an earlier Code step without checking whether it
   still applies. After editing Worktree to adopt an Existing branch, Code is
   deliberately skipped when `args.checkout` is set. Reopening that historical
   step therefore immediately advances through completed steps to Review.
   Reproduction: normal current-code walkthrough → review → edit/adopt a branch
   → Esc to Worktree → Esc to Git changes → Esc unexpectedly returns to Review.
   Walk backward to the previous applicable step instead of executing normal
   forward progression for a skipped historical step.

All three are reproduced with the existing compiled, fake-inspection manual
probe on two independent temporary tmux servers. Every probe cancels without
creating a workspace; cursor/wrapping restoration is explicitly verified. The
ad hoc checks exist only in the review invocation, not as new repository tests.
The fixes should add permanent regressions for these exact transitions.

Review verification: all 81 existing Create and 34 shared-prompt tests pass
(five opt-in cases ignored), including all 26 scripted navigation cases. The
complete existing terminal smoke suite passes again. `git diff --check` passes.
The full 946-test, Clippy and Docs evidence above is from implementation, not a
fresh full review run. Source, tests, public docs and Git refs are unchanged in
this review; only this record is updated. No real Codex/model calls or remote
actions. No additional actionable blocker identified in the inspected slice;
the three open defects mean it should not yet be treated as merge-ready.

## Implementation plan

- [x] Add opt-in typed Back/Cancel outcomes to shared picker and text adapters.
  Preserve ordinary pickers, y/N confirmations and direct/noninteractive calls.
- [x] Give creation an explicit step/history controller, bounded to this
  walkthrough, with transactional per-step draft restoration on Back.
- [x] Retain accepted selections by stable branch ref/workspace UUID/model ID;
  prefill text. Child Back returns to its parent without replacing a previously
  accepted setting. Esc at the root cancels; Shift-Tab at the root does nothing.
- [x] Reset automatic remaining-default advancement on Back. A subsequent Tab
  preserves valid accepted answers; unresolved mandatory sources still prompt.
- [x] Offer Create / Edit settings / Cancel at review. Editing presets is
  deliberate, returns directly to review and never broadens repository scope.
- [x] Apply dependency rules: derived branches follow name changes, explicit
  branches do not; code/context remain independent; branch adoption supplies
  code; fresh context clears compaction; persistent cleanup clears keep-thread.
  Preserve explicit model overrides across profile edits.
- [x] Refresh Git/file evidence and full-SHA expectation on every review;
  refresh a changed copy policy explicitly, never widen it silently. A failed
  discovery is an error, not a cancellation or a successful default.
- [x] Verify scripted state transitions, dependency changes, changed/disappeared
  entity catalogs, no mutation on cancellation and unchanged post-submit rules.
- [x] Verify actual terminal Esc/Shift-Tab/Ctrl-C, text editing/paste/Unicode,
  defaults/back, review editing, resizing and restoration. Run sequential
  formatting, focused/full tests, warning-denied Clippy and documentation gates.
- [x] Update canonical product/CLI architecture notes and focused public usage
  instructions only after the corresponding behavior is implemented/tested.

## Dialog and interaction contract

- Picker hints are dim and short: `Esc back · Tab defaults`, or `Esc cancel`
  at the root. Nested titles identify their parent (`Code > Branch`). Keep
  numeric selection, arrows and j/k; q cancels pickers, not text input.
- Text fields retain ordinary Backspace/Left/Right/Home/End behavior. Enter
  accepts prefilled text; Esc/Shift-Tab navigate, Ctrl-C cancels. Paste cannot
  submit a message or execute terminal control characters. Drafts remain only
  in process memory and are not logged or written to a history file.
- Editing review uses short setting names matching the review itself. A changed
  incompatible setting must be reselected with a concrete explanation, rather
  than silently dropping another explicitly requested action.
- Returning through Back/Edit never invokes create/send or adopts an old
  approval. Once the command submits creation, errors are handled by the
  existing operation/recovery path, not by replaying the wizard.

## Risks and limits

- Existing methods skip populated args, so restarting the old linear run is
  insufficient. The controller must deliberately reopen only the selected step
  while preserving the rest of the draft and separately recording visited steps.
- Catalog positions are unstable, and Git/files can change while a dialog is
  open. Remember identity, refresh catalogs and revalidate at review/apply.
- Terminal input must not reintroduce bottom-margin duplicate lines, leak raw
  mode, split UTF-8 or interpret pasted escapes as keystrokes. Reuse the existing
  inline-frame owner; test small/scrolling terminals and error exits.
- This delivery completes create, not every interactive command. Repository /
  workspace navigation and send can adopt the shared primitives subsequently;
  destructive y/N consent and secret structured-decision forms need their own
  scoped design and tests. No backend/schema/MCP or general workflow engine.

## Verification

- Baseline audit: 26 prompt unit tests passed; two manual PTY probes ignored.
- First focused run: 303 CLI tests pass (seven separately opted-in probes).
  New scripted coverage includes back/default/edit transitions, independent
  legacy context, stable identity and failed-preview/repository-replacement
  handling. Subsequent changes still require re-verification.
- Terminal smoke exposed a redundant, truncated Context breadcrumb in a narrow
  window; shorten it to `Context > Size`. Ordinary picker checks passed before
  that stop. Full terminal/navigation rerun pending.
- Compiler verification hit an exhausted filesystem. Read-only checks found
  85 GiB in this repository's regenerable `target/debug/incremental` cache.
  Removed only that exact cache directory; source, Git, builds and user data
  remain intact, restoring about 84 GiB free. Subsequent Cargo checks disable
  incremental caching for this task. No product failure was inferred from it.
- Final sequential Rust gates pass: `cargo fmt --all -- --check`, all-target /
  all-feature warning-denied Clippy, and `CARGO_INCREMENTAL=0 cargo test --offline
  --all-targets -j 2 -- --quiet --test-threads=2`. The full suite passes 900
  library and 46 integration tests; 11 library/manual and 19 integration/native
  opt-in cases are intentionally ignored in the ordinary run. Added 31 passing
  unit regressions compared with released main. Socket fixtures needed scoped
  sandbox escalation; an initial restricted run failed at bind with EPERM,
  not a product regression. No real Codex/model call.
- The complete `tests/terminal_smoke.py` passes against freshly built
  `target/debug/deps/coco-b598e2530c5d52da` on its own temporary tmux server:
  existing bottom-margin picker, defaults, confirmation and follow coverage;
  new Esc/Shift-Tab, root behavior, remembered selections/text, Unicode editing,
  harmless bracketed paste, ignored paste at review, direct edit/review,
  repeated traversal, Ctrl-C, discovery recovery and too-small layout cleanup.
  It explicitly executes both newly ignored manual probes. The test driver
  uses tmux's BSpace/DC key names and permits tmux's harmless visible control
  quoting; raw payload sanitization also has a separate pure unit regression.
- Prefilled message content (including whitespace/newlines) is preserved
  exactly while controls are rendered as visible markers. New multiline paste
  is normalized to a single line and cannot submit a field or picker action.
- `cargo machete` passes. Locked/all-feature `cargo deny --offline check`
  passes advisories, bans, licenses and sources using the existing cached
  database; existing duplicate-version warnings remain, with no lockfile or
  dependency-version change. This is not a fresh online advisory audit.
- Direct Astro check passes with zero diagnostics; Astro's fully static build
  and `node scripts/verify-export.mjs` pass (70 files, 21 pages, search, root
  routing and public-only boundary). Direct commands are used because the
  installed pnpm wrapper could not open its own cache database in the sandbox.
  No package install or pnpm cache/configuration change was needed.
- `git diff --check` passes. No staging, commit, push, workflow dispatch,
  installed service restart, Nix package build or new platform/native proof.

## Decisions and final review

- Keep the controller specific to create. Existing question methods use a
  typed Back/Cancel bridge; no string matching, blanket error swallowing,
  mutation replay or generic workflow framework. Recovery only wraps the
  read-only preparation phase.
- Treat CLI flags as initial presets, not permanent UI locks. Reopening a
  question is deliberate; Back restores its prior args and accepted metadata.
  The review is rebuilt rather than retaining consent after edits.
- Keep ambiguous context references verbatim as Selected source during edit;
  never coerce a workspace name into a forced native thread reference.
- Carry policy is keyed by categories, not row number. An explicitly edited
  empty-category preset remains selectable, and a changed base requires an
  explicit correction rather than silently discarding copy intent.
- Recheck original repository identity when refreshing a reopened source,
  preventing the refresh itself from replacing the repository anchor. Preserve
  the existing exact reviewed base expectation and backend race protections.
- All terminal frame ownership still restores raw mode, cursor, wrapping and
  bracketed paste on ordinary exits and failures. Real terminal tests caught
  a long breadcrumb and test-driver key/quoting assumptions before handoff.

## Handoff

The approved first delivery is committed as
`6c518cdff4eca22581f6023b3514d42a24abb657`, fast-forward integrated on main
and published as stable v0.12.0, based on released main 0.11.0. All source,
including the existing main audit record and new modules, is preserved.
The initial three P2 defects and the follow-up carry-policy/UI-memory defect
are corrected, with twelve added permanent regression tests, expanded terminal
coverage and final 958-test/Clippy/format verification recorded above. The
original carry-policy reproduction also passes against the real CLI using
read-only fake RPC. The subsequent full review found no additional actionable
issue and repeated the full Rust/terminal/static-doc checks successfully.
The authorized commit/integration/push, exact-source hosted checks, stable
rehearsal and publication are complete as recorded above and in main's work
document. The initial partial publication was recovered through the normal
exact-tip checks. Installed-runtime changes and real model calls remain excluded.

Follow-up scope is unchanged: apply the opt-in primitives to repository /
workspace navigation and send separately; close/delete need renewed pinned
consent and structured decisions need per-question/secret rules. The editor is
single-line with Unicode scalar editing, not a multiline/grapheme editor.
Initial discovery/preset errors still fail before the wizard, and an unusably
small terminal exits safely rather than forcing a broken recovery dialog.
