---
type: Working Document
title: "review/new-version: Temporary cleanup and recent workspace selection"
description: Implementation and verification of temporary cleanup, shared recent-first pickers, remaining creation defaults and repository selection outside Git.
tags: [work, review, cli, workspaces, cleanup, recency]
status: complete
branch: review/new-version
updated: 2026-11-08
---

# review/new-version — Temporary cleanup and workspace selection

## Intended outcome

Investigate the latest release and reported temporary-workspace retention,
fix the verified failure path, and share recent-first workspace selection
across guided creation and all discussed target commands. Keep code sources
repository-local and conversation context available across registered repositories.
Extend guided creation so a user can keep chosen values and accept the remaining
defaults with Tab, then review and explicitly confirm the complete result.
Restore the registered-repository picker when interactive creation starts
outside Git, as demonstrated by the user's installed 0.6.0 failure.

## Approved plan and completion

1. [x] Reproduce the temporary failure before changing behavior, covering fresh
   start, unpersisted metadata and successful versus abnormal TUI completion.
2. [x] Handle only the verified initialization condition as pending persistence;
   preserve bounded adoption, exact identities and checked one-shot deletion.
3. [x] Share recent/name ordering, relative times and compact branch details
   across both create source pickers. Sort eligible context globally, with
   repository identity visible and no current-repository preference.
4. [x] Apply the same selection policy to send, jump, diff, limits show/set/reset,
   close, reopen and delete, retaining each scope, filter and confirmation.
   The closed-entry recency gap found in full review is resolved below.
5. [x] Add focused regressions, update canonical knowledge and public guides,
   and verify Rust behavior, native TUI exits and the static documentation export.

## Decisions

- 2026-11-08 — Initial investigation starts from clean `1d87f15` on
  `review/new-version`. GitHub confirms latest release v0.6.0 (2026-11-07,
  `ea0c70f`, temporary feature `1a71545`); recent-first jump shipped in v0.5.0
  through `80c5165`. Installed coco/cocod 0.6.0 and Codex 0.166.0 match.
- 2026-11-08 — User explicitly confirms cross-repository context reuse. Only
  create's code-source picker stays local. Context ordering spans every
  eligible registered source rather than grouping the current repository first.
- 2026-11-08 — User approves the documented plan and authorizes implementation.
  No commit, publication, installation or mutation of real user workspaces is
  requested. The earlier planning-only boundary is superseded for these changes.
- 2026-11-08 — After accepting the second full review, the user explicitly
  requests a local commit of the reviewed changes. This supersedes the earlier
  no-commit boundary; push, publication and installation remain unrequested.
- Keep native recency as the sole last-use evidence. Unknown timestamps remain
  absent and sort after known ones, with natural name, repository path and exact
  ID ties. Provide `--sort recent|name` consistently; collection status/list
  retain their own contracts. No schema, dependency or usage-history store.
- Keep presentation in private CLI modules. `cli/workspace_picker` replaces
  the jump-private implementation; `WorkspaceSelection` carries its CLI-only
  sort policy. This intentionally supersedes the previous jump-only scope.
  The existing typed protocol and crate dependency direction remain intact.
- Do not infer missing/idle/safe-to-delete from unrelated storage errors or
  add delayed destructive retries. Checked deletion and its one-shot policy
  continue to preserve changed files, commits, active work and dependencies.

## Findings and implemented changes

### Temporary completion

- The supplied daemon log reports native `thread/read` error `-32603`, failing
  session metadata read with an empty rollout. Keep actual user session IDs,
  paths and conversation data out of this record.
- Native probes confirm that missing rollout files are readable but a matching
  zero-byte file returns the exact reported error, including after unsubscribe.
  This establishes the native condition, not when it naturally occurred in the
  user's run. No real session content was read, repaired or deleted.
- `find_materialized_thread` formerly propagated that RPC error before its
  existing empty/missing-file check. Adoption then failed the relay, so even a
  successful child exit bypassed temporary finish and cleanup.
- Regressions reproduced this before the fix at both worker and process
  boundaries. The process error was `cocod could not adopt the TUI-created
  Codex thread` after a successful fake TUI exit.
- Fresh adoption now treats only the exact native empty-rollout message as
  pending. Require code `-32603`, absent structured error data, identical repeated
  absolute paths and a filename matching the exact requested thread. Bound-thread
  lookup and all other errors retain their original behavior. Existing final
  adoption bounds and checked cleanup remain authoritative.
- Upgraded the temporary process fixture to perform actual fresh-start and
  unsubscribe protocol traffic with a native-style empty-rollout error. It
  covers deferred jump, create-jump, daemon restart, failed launch and abnormal
  child exit, in addition to the worker's delayed-valid and negative cases.
- Added an opt-in real Codex/tmux proof: untouched temporary workspaces clean
  up after `/quit`, Ctrl+D and normal Ctrl+C, with no model calls. A second
  Ctrl+C is sent only if native Codex asks for it. Blindly interrupting again
  after the UI has already closed can abort CoCo during cleanup; the user guide
  now says to wait for the removal/retention message.

### Workspace selection

- All workspace target pickers and both create source pickers use the same
  sort/render helper and existing passive native summaries. Rows and choices
  sort together, preserving exact IDs after keyboard/numeric selection.
- Code-source discovery remains repository-scoped. Context-source discovery
  lists eligible workspaces from all registered repositories and no longer
  performs the obsolete current-repository-priority lookup.
- Tests prove newer foreign context precedes older local sources, same-named
  sources remain distinguishable, name override works, code stays local, and
  all action RPCs receive the selected ID. Lifecycle filters, dry-run previews,
  explicit inputs and noninteractive handling retain their contracts.
- Durable contracts are promoted into product v0 spec, engineering architecture
  and Rust source architecture, with an entry in `knowledge/log.md`. Public
  changes are limited to workspace/context guides and the CLI reference.

## Verification

All product tests use isolated repositories, data directories and Codex homes.
No paid inference or real user workspace deletion was performed.

| Check | Result |
| --- | --- |
| Failing worker regression before fix | Exact empty-rollout RPC error reproduced |
| Failing process regression before fix | Successful TUI exit prevented cleanup through adoption failure |
| Three focused materialization cases | Pass, including delayed-valid metadata and unrelated/identity/path error preservation |
| Initial CLI subset, before the defaults feature | 220 pass; four existing manual probes ignored |
| `cargo test --locked --offline --all-targets -- --quiet` | 730 library, seven process and eleven status tests pass; eight manual library probes, paid inference and twelve native opt-ins excluded |
| Native compatibility at `42e8f93`, explicit opt-in, `--ignored --test-threads=1` | All twelve pass with Codex 0.166.0 in 116.42 s, including fresh/forked temporary exits, original-source preservation and archived closed metadata; not repeated for the creation-only defaults feature |
| Remaining creation defaults | Prior choices, presets, non-first default markers, required sources/text, conditional temporary defaults and final cancellation pass |
| Fork cleanup regression before fix | Normal exit retained the workspace after passive foreign metadata/history lookup |
| Real temporary TUI regressions | Fresh and create-time inherited context both clean up after Ctrl+D, `/quit` and Ctrl+C; original workspace, branch, commit, tracked changes, untracked file and native history preserved |
| Read-only exception safety | Unit and transport proofs retain cleanup for metadata/history reads; foreign resume still requires durable disarm and cannot forward if that acknowledgement fails |
| `cargo clippy --locked --offline --all-targets --all-features -- -D warnings` | Pass |
| `cargo fmt --all -- --check` | Pass |
| `cargo-machete` | No unused dependencies |
| `python3 tests/terminal_smoke.py <library-test-executable>` | Navigation, narrow resize, colors, Tab defaults through real creation, explicit final approval/cancellation, confirmations and terminal restoration pass |
| `nix flake check --no-build --offline --no-update-lock-file` | Pass for the local system |
| Astro `check` | Zero errors, warnings or hints |
| Astro static `build`, `DOCS_BASE_PATH=/coco` | Eighteen pages built |
| `DOCS_BASE_PATH=/coco node docs/scripts/verify-export.mjs` | Sixty-four files, eighteen pages, Pagefind, routing and public-only boundary verified |
| `git diff --check` | Pass; the full-review recency finding is resolved and verified below |

### Tooling details

- Nix develop/run test wrappers attempted unavailable dependency builds. Cargo
  gates instead use the exact flake-selected cached Rust 1.98.1 and complete
  vendor directory `/nix/store/zfl65d8yh8pbz0d6lqr7k4inlfcivvpq-cargo-vendor-dir`,
  with locked/offline resolution and `CARGO_HOME=$PWD/target/cargo-home`.
  Source replacement is supplied through Cargo `--config` options. Clippy's
  options must follow its subcommand so they reach the underlying Cargo check.
- Docs packages match the unchanged manifest/lock: Astro 7.2.10, Starlight
  0.41.10, Astro check 0.9.10 and TypeScript 6.0.3. The local pnpm 11.25 starter
  and the subsequently resolved pinned pnpm 11.21 both waited on metadata after
  linking all 436 locked packages. Those task-owned processes were terminated;
  no dependency-policy exception was added. Validation then directly invoked
  the pinned local Astro CLI with cached Node 24.19.0 and the export verifier.
  Package files, workspace settings and both dependency lockfiles are unchanged.
- During implementation, new files used intent-to-add so Git diff and Git-flake
  capture included them without staging contents. After the completed reviews,
  the user requested a local commit of the full reviewed change.

## Open questions and handoff

- The reviewed cleanup/recency work is committed as `42e8f93`. The subsequent
  Tab-defaults feature is implemented, reviewed and verified in this checkout;
  it is not committed or installed. CLI/daemon services remain unchanged, and
  no outstanding implementation finding remains.
- The original exact create command and final CLI exit message remain unknown.
  Narrow read-only attempts to associate the logged thread with a live SQLite
  row failed with `unable to open database file`. Do not claim that this log
  proves the reason every retained temporary workspace was kept: the verified
  bug path is fixed and independently reproduced, while ordinary safety blockers
  remain intentional.
- Full Nix binary builds, package publication and a refreshed advisory check
  were not run; dependency inputs are unchanged. The required source, Rust,
  native compatibility, terminal and static documentation checks above passed.

## Follow-up: inherited-context temporary workspace

- 2026-11-08 — User clarifies that their retained temporary workspace reused
  existing context (fork). The verified fresh-adoption error does not establish
  the cause of that case; the earlier chat explanation was too broad.
- [x] Reproduce normal temporary cleanup with real inherited context and no
  external model calls; inspect the bound-resume relay and retirement reasons.
- [x] Distinguish create-time context inheritance from an additional `/fork`
  within an already attached TUI, which intentionally disarms cleanup.
- [x] Correct the diagnostic conclusion and fix only an independently verified
  defect, preserving the completed picker work and cleanup safeguards.

- Native create-time context inheritance is now reproduced: a normal Ctrl+D
  exits with code zero but reports another conversation and permanently disarms
  cleanup. Narrow temporary instrumentation identified a `thread/read` request
  for a different thread as the trigger; instrumentation is removed.
- The first correction exempted the known passive `thread/read` method. Later
  foreign resumes/actions and all starts/forks retained acknowledged durable
  disarm. Its first native fork test passed all three exit methods before the
  stronger source-preservation fixture exposed an additional passive lookup.
- A read-only list of the current repository found no remaining matching
  temporary fork. It did not inspect transcripts or modify user workspaces.
- A stronger native fixture now creates an original CoCo workspace, keeps its
  tracked modifications and an untracked file, and verifies its identity,
  branch, HEAD and source conversation history after temporary TUI exits.
  This exposed an additional passive `thread/turns/list` lookup triggering the
  same false disarm. The explicit read-only exceptions now cover metadata,
  turn pages and item pages; all foreign resumes/actions remain gated.
  Temporary method-only diagnostic instrumentation is removed.
- Final isolated native proofs pass for all six fresh/forked exit paths. The
  stronger fixture verifies the original after successful removal of all three
  temporary descendants, including its dirty tracked file, untracked content,
  branch/commit identity and unchanged source conversation/history availability.
  The full twelve-test native suite, 717 library tests, seven process tests,
  eleven status tests, all-target/all-feature Clippy and formatting pass.
- Cleanup addresses the temporary workspace's exact ID and its own child
  conversation; no original ID is substituted as a retirement target. The
  defect prevented cleanup by consuming its one-shot permission. It did not
  initiate deletion of the original. Existing safety checks remain in force.
- The correction is local to this checkout. Previously disarmed cleanup is
  not silently rearmed, and installed binaries/services remain unchanged.

## Full diff review

- 2026-11-08 — User requests a full review of the current uncommitted change.
- [x] Review production changes and surrounding lifecycle/selection contracts.
- [x] Check regression coverage, public documentation and internal consistency.
- [x] Reproduce actionable findings where practical and record final results.
- **P2 — Closed candidates cannot sort by native recency.** The new
  `ReopenArgs.sort` and shared picker rely on `threadRuntime.recencyAtMs`.
  `hydrate_native_thread_runtime` returns immediately for closed workspaces,
  and Store rows do not persist recency. Consequently every actual reopen
  candidate has unknown recency, so both the default and explicit `--sort
  recent` always fall back to name order; relative times never appear. Closed
  entries in delete/limits also have this limitation. The new action-dispatch
  test supplies open/idle rows with fabricated recency even for reopen, so it
  does not catch this missing data flow.
- Reproduction: a temporary coordinator regression materialized a workspace,
  assigned its native thread a known recency, closed it without archiving, and
  listed `phases: [closed]`. The native timestamp remained available, but the
  listed timestamp was `None` instead of `Some(1800000000000)`; the focused
  test failed at that assertion. The probe was then removed, and
  `git diff --exit-code -- src/coordinator/tests/recency.rs` confirmed exact
  restoration. Product code is unchanged by this review.
- Follow-up: obtain the closed thread's recency through passive native reads
  without reopening or changing its closed phase, and add a regression using
  actual closed coordinator results rather than synthetic open picker rows.
- No additional confirmed findings in the remaining diff. In particular,
  read-only relay exceptions do not bypass foreign resume/action disarm;
  empty-rollout handling stays confined to fresh discovery and does not alter
  strict bound-thread retirement; selected IDs and retirement previews remain
  intact; public documentation contains no internal implementation material.
- Verification considered the immediately preceding unchanged-code gates:
  717 library, seven process, eleven status and twelve native compatibility
  tests plus Clippy, formatting, terminal, Flake and static-doc checks. They
  were not repeated; this review added the targeted failing probe above. A
  refreshed dependency advisory check remains outside those recorded gates.

## Fix: closed workspace recency

- 2026-11-08 — Resume the approved picker implementation to resolve the review
  finding. Reuse validated passive native thread reads for stable closed/ready
  workspaces, including archived threads; keep closed phase, archive ownership
  and absent worktree unchanged. Unreadable bindings leave recency unknown.
- [x] Add realistic closed-workspace regressions and correct the reopen mock.
- [x] Extend passive hydration without activating closed conversations or
  projecting them as open/unavailable on metadata failure.
- [x] Update canonical knowledge, run the relevant gates and review the fix.
- Both permanent regressions failed before the fix: native metadata was
  missing and no passive read occurred for a bound closed workspace. They now
  pass for archived/unarchived sources, absent timestamps, unbound workspaces,
  read/missing-thread errors and mismatched identity/working directory.
- The passive hydrator now accepts ready closed workspaces, validates the same
  exact binding, and attaches native metadata without projecting local active
  work over their closed phase. Error/unbound paths preserve that phase and
  omit unknown metadata. Activation/materialization guards remain open-only.
- Closed candidates are removed before native reads when the requested list
  does not include their phase, avoiding extra reads for context/open pickers.
  Tests assert no calls in that case and only `Read` for eligible bound sources;
  store timestamps, archive flags, thread identity and absent paths stay intact.
- The CLI action probe now supplies genuinely closed-shaped rows for reopen.
  The native archive/retirement proof also verifies available recency and the
  unchanged closed binding/archive state after passive listing.
- Final gates: 719 library, seven process and eleven status tests pass;
  twelve native compatibility tests pass in 116.42 s. Clippy for all targets
  and features, formatting, unused-dependency check, local Flake evaluation
  without builds, and diff whitespace checks pass. No public documentation or
  rendering changed in this follow-up; their preceding checks remain valid.
- The fix and its surrounding diff were reviewed. The P2 finding is resolved;
  no schema, dependency, installation or real-workspace mutation was needed.

## Second full diff review

- 2026-11-08 — User requests another full review after the closed-recency fix.
- [x] Re-read the full production diff and the new closed metadata paths.
- [x] Re-check regression coverage, lifecycle/identity safety and documentation.
- [x] Investigate concrete remaining concerns and record the review outcome.
- Outcome: no new confirmed findings across the full 34-file diff. The first
  review's P2 is resolved by the validated closed metadata path, actual closed
  coordinator regressions, corrected reopen rows and native archive proof.
- Rechecked closed-phase preservation on missing/invalid metadata, archived
  identity, open-only activation guards and pre-read exclusion of ineligible
  closed rows. Picker sorting keeps exact IDs aligned; code/context scope,
  lifecycle filters, noninteractive behavior and retirement previews remain
  intact. Cleanup still targets only its own workspace/thread/owned branch;
  passive-read exemptions retain the gate for foreign activation/actions.
- Reviewed all changed test fixtures and public/internal documentation against
  the implementation. Temporary probe hooks are absent. No product source or
  test was changed for this review, and no additional executable probe was
  needed after the code/contract checks.
- Verification: `git diff --check` passes again and staged contents remain
  empty. The latest unchanged-code gates remain 719 library, seven process,
  eleven status and twelve native tests, plus Clippy/formatting/dependency-use,
  Flake evaluation and the preceding static-doc/terminal checks. These were
  not repeated for this read-only review. Paid inference, full Nix builds and
  a refreshed advisory database remain outside the recorded validation scope.

## Delivery

- User authorizes one local commit covering the reviewed temporary cleanup
  fixes, shared recent-first workspace pickers, closed native metadata,
  regression proofs and public/internal documentation.
- The latest code gates remain unchanged. Final staging must contain the
  reviewed source/documentation paths only and pass `git diff --cached --check`.
  Commit identity and delivery are recorded by Git history.

## Feature: accept remaining creation defaults

- 2026-11-08 — Previous reviewed work is committed as `42e8f93`. User now
  requests completing a partially configured walkthrough with its remaining
  defaults and explicitly selects Tab as the shortcut.
- Preserve supplied flags and committed selections. Apply only marked semantic
  defaults; still collect required text/source choices with no such default.
  Final Create/Cancel review remains explicit and does not accept Tab.
- Keep the behavior scoped to creation setup. Ordinary workspace/decision
  selectors and destructive confirmations retain their interaction contracts.
- [x] Add an opt-in typed setup selection and visible Tab hint to the shared
  terminal adapter, with terminal restoration and ordinary selectors preserved.
- [x] Apply remaining defaults in a private creation interaction wrapper,
  avoiding unnecessary discovery and preserving conditional/preset defaults.
- [x] Cover shortcuts, required values, previous choices, presets, temporary
  actions and cancellation with behavior tests and real terminal probes.
- [x] Update public guidance and canonical knowledge, verify and review.
- Implemented a private creation wrapper around `Interaction`. The terminal
  returns a typed defaults action only for setup, with a bounded footer and
  reserved display row. Ordinary selectors, confirmations and final review use
  their existing key behavior. Skip state belongs to one walkthrough only.
- Defaults use the actual `is_default` marker, including a model default after
  the first row. Required source picks do not adopt the most recent/first row;
  they explain the requirement and retain explicit selection. Optional current
  context discovery is skipped when an early shortcut already selects Fresh.
- Terminal proof selects code/context, highlights Compact context, then presses
  Tab and verifies the marked Full default is applied. It resizes to 24x5 and
  back, verifies the remaining summary, proves Tab does not submit that review,
  exercises both Create and Cancel, and checks cursor/wrap/raw-mode restoration.
  Existing ordinary-picker Tab, cancellation and destructive-confirmation
  behavior remain covered by the same isolated tmux suite.
- Final verification: 730 library, seven process and eleven status tests pass;
  eight manual library probes are excluded from that command. All-target,
  all-feature Clippy, formatting, unused-dependency check, extended terminal
  smoke, local Flake evaluation and diff checks pass. Astro check reports zero
  errors/warnings/hints; static build and export verification pass for eighteen
  pages and sixty-four files. No dependency, schema or protocol DTO changed.
- Reviewed the complete feature diff with no outstanding finding. Runtime
  cleanup/native adapter behavior is unchanged from `42e8f93`; its previous
  twelve-test native compatibility proof was not repeated. No inference,
  installation, push or new commit was performed for this feature.

## Full review: remaining creation defaults

- 2026-11-08 — User requests a full review of the uncommitted Tab feature.
- [x] Review every production change and its effect on required values,
  preset/conditional defaults, later safety prompts and terminal behavior.
- [x] Review tests and public/internal documentation against actual behavior.
- [x] Reproduce any actionable concerns and record the final review outcome.
- Outcome: no confirmed findings across all fourteen changed files. The typed
  setup action and its state remain local to creation; normal selectors,
  no-input checks, required values and final review retain explicit interaction.
- Rechecked every creation step and nested source/model choice against its
  semantic default. Flags and prior values remain intact, conditional temporary
  actions use their existing defaults, and no required source is guessed from
  the first/recent row. The wrapper is dropped before creation/turn/jump work;
  cancellation returns before a create request is submitted.
- Reviewed hint layout, resize bounds, key modifiers/repeats and terminal
  restoration against the state/controller and isolated PTY proof. The footer
  stays inside the frame, ordinary Tab remains inactive, and the final summary
  requires its own key decision. Public documentation describes this scope and
  its required-value exception without implementation details.
- No product/test changes or new executable probes were needed during review.
  `git diff --check` passes again, with no staged contents. The preceding
  unchanged-code verification remains 730 library, seven process, eleven status
  tests, extended terminal smoke, Clippy/formatting, dependency-use, local Flake
  evaluation and static documentation check/build/export; these were not rerun.
  The feature remains uncommitted and uninstalled.

## Fix: creation outside Git

- 2026-11-08 — User proposes selecting a registered repository when running
  `coco create` outside Git and supplies the actual 0.6.0 `GIT_ERROR` from
  `discover-root`. The earlier chat claim that this already works was based
  only on the intended CLI selection branch and was incorrect.
- Git's failed `rev-parse --show-toplevel` exits 128 before the empty-output
  check can return `NotAWorktree`. The generic command error becomes
  `GIT_ERROR`, while the CLI requires `NOT_A_GIT_REPOSITORY` for its fallback.
- [x] Reproduce with native Git and verify the resulting repository RPC code.
- [x] Translate only recognized non-worktree root-discovery failures into the
  existing typed error; preserve configuration, corrupt metadata and I/O errors.
- [x] Prove interactive selection through a real Git/daemon path, including
  empty inventory, explicit scope and noninteractive boundaries.
- [x] Update canonical knowledge/public usage guidance as needed and verify.
- Scope remains local source and isolated test fixtures. Do not install,
  restart the user's daemon or mutate registered user repositories.
- Before the fix, native non-Git and bare-repository tests fail with the
  original exit-128 diagnostics; the coordinator test reports `GIT_ERROR`
  instead of `NOT_A_GIT_REPOSITORY`. These pass after the narrow translation.
- Added five automated regressions covering native discovery, canonical error
  paths, coordinator classification, broken metadata/configuration, missing
  paths, known filesystem-boundary diagnostics and unrelated/truncated errors.
- Real PTY proof (`python3 target/repository_picker_check.py`, ignored local
  probe) uses built coco/cocod, native Codex 0.166.0, a localhost-only dummy
  provider and temporary HOME/data/socket/worktree directories. No model call
  or access to the user's daemon/repositories is involved.
- The proof runs plain `coco create` outside Git, chooses the second of two
  registered repositories, accepts setup defaults and explicitly creates.
  JSON and filesystem checks confirm the exact selected repository and worktree.
  A second walkthrough cancels without creating another workspace. Empty
  inventory explains registration; explicit invalid scope and no-input do not
  fall back; malformed Git metadata still fails; valid unregistered Git still
  enrolls automatically. The temporary daemon exits cleanly and terminal flags
  are restored.
- Final Rust verification: 735 library, seven process and eleven status
  tests pass; eight manual library probes, twelve native compatibility opt-ins
  and one live-inference proof are excluded. All-target/all-feature Clippy
  and formatting pass. Astro check reports zero errors/warnings/hints; static
  build and export verification pass for eighteen pages and sixty-four files.
  Diff checks pass, with no staged contents. No module/public API, dependency,
  schema or protocol DTO changed in this fix.
- Handoff: the Tab feature and this discovery fix are implemented, verified,
  uncommitted and uninstalled. The last commit remains `42e8f93`. The user's
  installed 0.6.0 daemon still has its previous behavior; the fix must also be
  present in cocod, where Git discovery runs. No outstanding implementation
  finding remains in the current scope. Do not infer authorization to install,
  restart services, commit again or push from this completed source fix.

## Review: remaining defaults and repository discovery

- 2026-11-08 — User requests another review after the outside-Git fix.
- Scope: the complete uncommitted diff since `42e8f93`, including the Tab
  walkthrough, native Git error translation, tests and documentation.
- [x] Trace production changes and callers, including required values, preset
  defaults, final confirmation and all consumers of repository discovery.
- [x] Review regression coverage and reproduce actionable concerns in isolation.
- [x] Check public/internal contracts and record findings plus verification.
- Review does not authorize installation, daemon restart, publication or a new
  commit. Preserve current product changes while assessing them.
- Outcome: no confirmed findings across all eighteen changed files. Reviewed
  the complete production/test/documentation diff and the surrounding creation,
  terminal, Git command, coordinator and RPC paths. Required choices retain
  explicit input; presets and committed decisions survive Tab; final review and
  post-walkthrough operations use the original interaction contract.
- Git translation is confined to root discovery and recognized exit-128
  diagnostics. Checked its effects on registration, removal, scoped/ID lookup,
  workspace provisioning, worktree verification and audit resolution. Errors
  still propagate through those paths; the interactive fallback remains gated
  by implicit scope and terminal input, and create still enrolls valid Git.
- New isolated native probe creates a temporary directory on the separate
  `/dev/shm` filesystem and invokes `git rev-parse --show-toplevel` with C locale
  and filesystem discovery disabled. Actual exit 128 and both diagnostic lines
  match the new boundary classifier, supplementing the synthetic regression.
- Reviewed the five Git/coordinator regressions, Tab semantic/required-value
  coverage, scripted adapter and real terminal checks against the contracts.
  Existing native Git and real coco/cocod picker proof cover the original
  reported failure; no additional actionable concern remained to reproduce.
- Verification: the new native filesystem-boundary probe and `git diff --check`
  pass; staged contents remain empty. Product and test code were unchanged by
  this review. The previous 735 library, seven process and eleven status test
  passes, all-target/all-feature Clippy, formatting, PTY proof and static docs
  check/build/export remain the latest full verification and were not repeated.
- Handoff remains: Tab and repository-discovery changes are uncommitted and
  uninstalled; no outstanding review finding or required implementation work
  remains in the current scope.
- Commit-readiness check after the review: the expected eighteen paths remain
  in the working diff, `git diff --check` passes and staged contents are empty.
  The reviewed product code is unchanged; the recorded tests and Clippy remain
  the latest verification. The changes are ready for a local commit; none was
  created by this readiness check.

## Delivery: creation defaults and repository selection

- 2026-11-08 — After the full review and readiness check, the user explicitly
  authorizes the local commit. This supersedes the earlier no-new-commit
  boundary for the reviewed Tab feature and outside-Git discovery fix.
- Include the eighteen reviewed source, test and documentation paths only.
  Product code is unchanged since the recorded successful checks and review.
- Commit message: `feat(cli): add create defaults shortcut and fix repo selection`.
  Git history records the resulting commit identity. Verify the staged diff
  before committing and the clean worktree afterward.
- Installation, daemon restart, push and publication remain unrequested.
