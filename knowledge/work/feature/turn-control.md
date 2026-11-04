---
type: Working Document
title: Exact operation-scoped turn inspection and interruption
description: Adds generic public turn-control contracts and durable technical receipts without Palm-specific behavior.
tags: [work, turns, mcp, idempotency, recovery, codex]
status: complete
branch: feature/turn-control
updated: 2026-11-04
---

# Exact turn control

## Local checkpoint and Palm handoff

The user authorizes committing the reviewed best-effort extension and supplying
a copyable message for Palm's agent. Include the delayed-completion correction,
permanent regressions and final contracts. The local checkpoint is titled
`feat(mcp): add explicit best-effort turn interruption`; its hash is reported
with the delivery. The handoff now identifies this checkpoint rather than an
uncommitted working tree. No source or test changes follow the final passing
review; only these delivery/status notes change before committing. Adapter,
scheduling and result publication remain Palm's responsibilities. No merge,
push, installation change, real model call or publication is authorized.

## Final delayed-completion follow-up review

The user requests another review after the durable-completion correction.
Inspect the complete uncommitted best-effort slice and especially accepted
identity lookup, start/binding races, newer guards, one-shot interruption,
scope/grants, restart behavior and the documented public contract. Use isolated
model-free checks; no product fixes, commit, push, publication or user runtime
changes are authorized by this review.

### Review conclusion

No new actionable findings. Reviewed the full current best-effort diff,
including untracked implementation/test children, against the public/Palm
contracts. The delayed-event fallback requires accepted exact native IDs,
preserves first receipts and leaves newer guards/pre-ack buffers untouched.
The owning unbound guard retains its earlier conflict handling. One-shot
immediate transactions, mode conflicts, lost replies, scope-before-lookup and
independent MCP mutation grants remain consistent with the documented contract.

The ordinary all-target suite passes again: 654 library tests, six process
scenarios and ten status-query tests. All-target/all-feature warning-denied
Clippy, formatting and `git diff --check` pass. Installed Codex is rechecked as
0.166.0; all three opt-in model-free native turn-control tests pass serially
in 5.52 seconds with isolated temporary state and authorized loopback access.
Seven manual library probes and paid/authenticated inference are intentionally
not run. Public source is unchanged in this review, so the prior site export
was not repeated; dependency checks from the fix phase remain unchanged.

The slice is ready for a local checkpoint under the explicitly accepted
best-effort contract, not a guarantee of native atomic targeting. Native
check/queue risk and the pre-dispatch claim/crash uncertainty remain explicit;
strict native interruption remains unsupported. Only this working document
was edited by the review; no product/test changes, commit, push, publication
or real user daemon/workspace changes.

## Delayed completion recovery fix

The user authorizes fixing the post-implementation review finding, without
commit, push or publication. Preserve the current best-effort contract and
one-shot dispatch semantics; fix only durable correlation of delayed terminal
notifications after runtime tracking is gone.

- [x] Persist exact accepted native thread/turn completions independently of
  generation-local guards, without guessing unconfirmed bindings.
- [x] Add permanent regressions for real database reopen, repeated/conflicting
  events, cache loss and preserving newer accepted or unconfirmed guards.
- [x] Update canonical recovery guarantees and the Palm handoff as needed;
  rerun isolated, model-free checks sequentially with bounded parallelism.

### Fix and verification

- The completion path reuses the existing unique native-result index and
  validates accepted turn-start kind plus saved thread ID before the existing
  checked receipt write. This adds no schema, history scan or native request.
  Missing/different runtime guards no longer prevent an exact receipt; known
  older events do not enter a newer unconfirmed start's buffer. The owning
  guard retains its original pre-ack handling, including conflict uncertainty.
- Three initial permanent regressions failed before the product fix and pass
  after it. Five final regressions cover all three terminal outcomes with
  acknowledged/lost interrupt replies, two actual database reopen cycles,
  repeated/conflicting completions, cache loss, newer unconfirmed/accepted
  guards, wrong-thread IDs, unconfirmed starts and the acceptance/binding
  conflict boundary. No second interrupt or invented answer-cache recovery.
- Full `cargo test --locked --offline --all-targets --jobs 2 --quiet --
  --test-threads=2` passes: 654 library tests, six process scenarios and ten
  status-query tests. Seven manual library probes and paid/authenticated
  inference remain deliberately unrun. The earlier focused coordinator run
  also passed all 41 then-present cases before adding the fifth regression.
- `cargo clippy --locked --offline --all-targets --all-features --jobs 2 --
  -D warnings`, formatting and `git diff --check` pass. Machete finds no unused
  dependencies; offline deny passes advisories/bans/licenses/sources with
  unchanged duplicate-version warnings. Its cached advisory lock required
  scoped sandbox access; no download or dependency change occurred.
- Installed Codex remains 0.166.0. All three opt-in model-free native
  turn-control checks pass serially in 4.35 seconds using temporary state and
  scoped loopback access. No real models, account credentials, user workspace
  or daemon changes. This does not prove native atomic interruption.
- Canonical recovery rules and Palm handoff describe delayed received evidence
  explicitly. Public inputs/outputs and guarantees are unchanged, so no public
  site source edit or repeat site build is needed for this correction.

The P2 review finding below is resolved and its not-ready conclusion is
superseded. The explicit best-effort slice is locally complete but remains
uncommitted; no commit, merge, push or publication was performed. Native
check/queue targeting risk remains the deliberately accepted limitation,
strict mode remains unsupported, and scheduling policy stays in Palm.

## Best-effort post-implementation review

The user requests another complete review of the uncommitted best-effort
extension. Inspect one-shot claims, reply/outcome separation, scope/grants,
restart/delayed events and native rejection behavior against public contracts.
The accepted native check/queue race is a stated limitation, not itself a new
finding. Use isolated model-free diagnostics and report actionable CoCo issues;
no product fixes, commit, push, publication or real-user runtime changes.

### Review result

One reproducible P2 remains in the shared completion path, rather than in the
native interrupt claim: `observe_native_turn_completed` returns early when the
generation-local active guard is absent. After actually closing/reopening the
database and constructing a new coordinator, an exact delayed terminal event
for a durably accepted thread/turn pair is discarded. If indexed native history
is unavailable, `turns.get` remains `unavailable` despite the received terminal
evidence. This does not assert that all native running turns survive a daemon
restart; it concerns how an event is handled if it arrives after recovery.

An isolated temporary regression probe reused the existing persistent fixture
and its real database reopen helper, disabled metadata recovery, requested one
best-effort interrupt and delivered the original turn's interrupted completion
after reopen. It failed with actual `Unavailable` versus expected `Finished`.
The probe was removed after diagnosis; no product source or permanent tests
were changed during this review.

Recommended correction: persist a narrow completion receipt by the accepted
durable mapping of both native IDs even without the old runtime guard. Never
substitute the current turn, clear a newer guard, or load legacy conversation
history. Add permanent recovery and newer-guard preservation coverage when
fixing this. The current slice should not be considered fully ready until this
gap is resolved; the earlier implementation-complete statement below predates
this review. No further actionable findings were established.

Verification after removing the probe: the complete offline all-target suite
passes 649 library tests, six process scenarios and ten status-query tests.
All-target/all-feature warning-denied Clippy, formatting and `git diff --check`
pass. Three opt-in isolated native turn-control tests against installed Codex
0.166.0 pass serially, without authentication or real model calls. These green
tests do not cover the failing recovery scenario above or prove native interrupt
atomicity. No commit, push, publication or user daemon/workspace mutation.

## Explicit best-effort native interruption

The user deliberately relaxes the earlier atomic-only requirement for an
explicit mode, without changing strict defaults. Baseline is local checkpoint
`eba9630`; branch and worktree were clean. The Palm handoff and linked turn
contract were read before implementation. No push, publication, installation
change, upstream patch, Palm code change or real model call is authorized.

- [x] Add `mode: strict | bestEffort`, default strict, on the existing interrupt
  tool and typed command, preserving independent `--allow-interrupt` admission.
- [x] Separate native best-effort from atomic exact-target worker capability;
  verify installed Codex 0.166.0 and send only saved nonempty native IDs.
- [x] Atomically claim one durable attempt before a native side effect; retain
  mode and uncertain/requested/rejected state. Same-mode retries only inspect;
  different modes cannot bypass a saved attempt. Crashes never cause redispatch.
- [x] Preserve exact completion receipts, scope checks and no-op terminal
  handling. Native acknowledgement, rejection and unknown reply must not
  invent an interrupted outcome or attribute another turn's interruption.
- [x] Add isolated coordinator/store/wire/MCP/native proofs for grants, modes,
  races, stale rejection, repeated/concurrent calls, lost replies and real DB
  reopen. A controlled race fixture demonstrates remaining risk, not atomicity.
- [x] Update public reference/help, canonical decisions and Palm handoff;
  verify serially with bounded test parallelism and record final limits.

### Design constraints

The existing operation's interrupt marker becomes an atomic one-shot claim,
not a retry permission. Persist uncertainty before dispatch; only a received
reply can record acknowledgement or a proven native rejection. Old v16 markers
can only be migrated as strict/uncertain, never as acknowledged. A later
different mode must not authorize a second side effect. Terminal outcome
always refers to the originally stored thread/turn pair. Palm's rule not to
start more work while cancellation is unresolved stays in Palm.

### Current evidence

- Domain/protocol modes are `strict` (including omission) and explicit
  `bestEffort`. MCP exposes enum/default/unknown-field validation without an
  MCP dependency in domain types. The existing grant controls both modes.
- Schema 17 adds only mode and reply state to the operation row. An immediate
  transaction grants one caller dispatch; concurrent/reopened connections,
  lost replies and restart cannot re-acquire it. Old v16 timestamps migrate
  as strict/uncertain, not acknowledged. Mode changes fail with
  `TURN_INTERRUPT_MODE_CONFLICT` except terminal no-ops.
- Native 0.166.0 best-effort support is separate from exact capability (still
  false). Only saved nonempty native IDs are sent; empty startup/current-turn
  cancellation is prohibited. Proven stale/no-active rejection is retained,
  without fallback, and invalidates only the exact old in-flight hint.
- Attempt state and turn outcome are independent. In the controlled race A
  finishes, B starts and is interrupted; A stays completed. Even a terminal
  original outcome can coexist with an uncertain interrupt reply. Palm's
  reuse/scheduling rule stays external and must inspect both dimensions.
- Fifty focused turn-control tests pass. Full offline all-target checks pass
  649 library tests, six process scenarios and ten status-query tests. The
  first broad run found only a stale retirement migration-version expectation
  (16 versus 17); updating that assertion gives the passing complete rerun.
- All ten installed-Codex 0.166.0 compatibility checks pass serially in 48.26
  seconds, including the actual MCP best-effort request and persisted attempt
  across cocod restart. No real model/authentication is used; the success does
  not prove native atomicity or eliminate the explicitly simulated race.
- All-target/all-feature warning-denied Clippy and formatting pass. Unused-
  dependency and offline advisory/license/source checks pass, with unchanged
  duplicate-version warnings. No dependency changes were required.
- Public reference/help, canonical architecture/product contracts and the Palm
  handoff are updated. `pnpm --dir docs check` reports zero errors/warnings/hints.
  The final static `/coco` build verifies 18 pages/64 files, local routes,
  Pagefind and the public-only boundary. Cache locks/build and isolated loopback
  fixtures required the normal scoped sandbox escalation, not a deployment.
- Final source/contract diff and `git diff --check` pass. Seven manual library
  probes and paid/authenticated inference remain deliberately unrun; all native
  tests above are isolated and model-free. No source/configuration in Palm or
  user installation, real daemon/workspace, upstream Codex or remote repository
  was changed. No temporary review probe is retained.

### Delivery and remaining boundary

The user's revised explicit best-effort slice is complete locally, uncommitted
on top of `eba9630` on `feature/turn-control`. The historical atomic-only
blockers below no longer block this explicitly relaxed contract: strict remains
unsupported by native Codex, while best-effort is a separate accepted-risk
capability. No new CLI command, mutation flag, scheduling layer, force-retry
mechanism or exactly-once guarantee was introduced. Future native atomicity is
still desirable, not a prerequisite silently assumed by this implementation.

The updated [`Palm handoff`](../../handoffs/palm-turn-control.md) describes
actual inputs, responses, errors, durable attempt states, mode conflicts,
recovery/deletion bounds and Palm's own unresolved-cancellation scheduling rule.
Use matching builds of the current working tree, not only baseline commit
`eba9630` or an installed/published `0.4.0`. No commit, merge, push or publication
was performed for this extension.

## Final correction and local Palm handoff

The user authorizes committing the slice and supplying Palm's agent with its
handoff. The second review's P2 was still open at this request; it is now fixed
and covered by permanent regressions before creating the local checkpoint. No push,
release, service switch or Palm source change is authorized.

- [x] Replace single pre-confirmation candidates with bounded observations
  keyed by native turn ID; preserve exact completion and output independently
  of event order or completed-output cache eviction.
- [x] Make overflow/conflicting early evidence explicitly unavailable rather
  than trusting an in-flight guard as proof of continued execution.
- [x] Add regression tests for event orders, cache eviction, bounds and exact
  metadata recovery; run bounded sequential verification.
- [x] Update canonical knowledge/public recovery guarantees and the Palm
  handoff. Commit only reviewed slice files; document native cancellation as
  unsupported, not a completed original capability.

### Correction evidence

- A focused private buffer holds at most 32 distinct native IDs (256 bytes
  each) and 1 MiB aggregate answer text until binding. Existing admitted IDs
  are never evicted for another early turn; exact repeated terminal facts are
  stable and contradictory outcomes stay unknown. UTF-8-safe truncation shares
  the existing answer limiter.
- The confirmed start selects only its own completion/output. Lost early
  evidence is separate from uncertain acceptance: the operation remains
  accepted with both native IDs, but the runtime guard does not justify a
  running state or indefinitely pending output. New sends remain fail-closed.
  Exact native in-progress evidence restores tracking; terminal metadata
  releases only the corresponding guard.
- Nine permanent new tests cover both event orders, actual 256-entry cache
  eviction, metadata reconciliation between durable acceptance and runtime
  binding, aggregate text/identity bounds, conflicts, explicit unknown state,
  blocked unsafe re-dispatch and exact native recovery. Focused pending tests
  pass; full offline all-target checks now pass 630 library tests, six process
  scenarios and ten status-query tests. Seven manual library probes and
  paid/authenticated inference are deliberately skipped.
- All ten installed-Codex 0.166.0 compatibility checks pass serially in 48.61
  seconds, without real authentication or model inference. Final warning-denied
  all-target/all-feature Clippy and formatting pass.
- `cargo machete` reports no unused dependencies. Offline `cargo deny` passes
  advisories, bans, licenses and sources using the existing cached advisory DB;
  unchanged duplicate-version warnings remain. No dependencies were added.
- Docs check reports zero errors/warnings/hints. The final static `/coco` export
  verifies all 18 pages/64 files, routing, search and public-only boundary.
  Only normal cache-lock/build and isolated loopback test access required
  sandbox escalation. No deployment or installation was performed.
- Final source/test/contract review and `git diff --check` pass. The checkpoint
  includes only the reviewed generic turn-control implementation, regression
  fixtures, related documentation and the preserved initial assessment notes.

This record accompanies the local `feat(mcp): add durable operation-scoped turn
tracking` checkpoint on `feature/turn-control`. The latest P2 is resolved.
The handoff for Palm's independent adapter is
[`knowledge/handoffs/palm-turn-control.md`](../../handoffs/palm-turn-control.md).
Start/lookup and durable observed outcomes are available; **strict native
cancellation remains unsupported**. This does not complete the original fully
cancellable-provider requirement. The working record remains active for that
explicit upstream dependency, without authorizing upstream work or unsafe
fallbacks. No push, merge, publication, real-user service change or Palm source
change is included.

## Second post-fix review

The user requests another complete review after hardening. Check the full
current slice and interactions with existing send/output paths, cancellation
capability rejection, scoped persistence, restart/eviction and public guarantees.
Use isolated model-free regression checks and report actionable findings; no
product fixes, commit, push or publication are authorized by this review.

### Finding

**P2 — a second pre-ack completion can erase the accepted turn's outcome.**
`observe_native_turn_completed` keeps only one `completed_before_response`
candidate while the direct start reply has not bound its native ID. A later
completion from another turn on the same thread replaces that candidate.
`bind_runtime_turn_result` then discards the mismatching last candidate, having
lost the earlier exact completion. No receipt is saved and the original runtime
guard remains. This can occur with another native/TUI writer or delayed old
events; the response cache only protects events for still-retained completions.
For legacy/unverified history, which correctly skips native recovery, public
lookup can report `running` indefinitely and the guard rejects the next send.

A temporary isolated coordinator probe paused `turn/start`, delivered
`turn-1/completed`, then `external-next-turn/failed`, and released the direct
reply for `turn-1`. The assertion expecting `finished` failed with `running`.
This proves a bookkeeping failure without any inference, user workspace or
unsafe native mutation. The temporary probe was removed after verification;
the pre-existing product and test sources are restored.

Recommended next fix: retain bounded pre-confirmation evidence keyed by native
turn ID (including output candidates), select only the exact accepted turn
when its reply arrives, and test both event orders plus cache eviction. If a
safety bound is exceeded, report uncertainty instead of treating a known-lost
completion as ongoing execution. No product fix was implemented in this review.

### Verification and conclusion

- Full offline all-target regression: 621 library tests, six process scenarios
  and ten status-query tests pass (seven manual library probes and paid/live
  inference intentionally skipped). The additional probe above failed as
  expected and is not represented by the current passing regression suite.
- Installed `codex-cli 0.166.0` rechecked. All three model-free native
  turn-control checks pass serially. The sandbox initially denied the public
  loopback fixture with EPERM; the authorized isolated rerun passes in 4.44s.
- Formatting and `git diff --check` pass. Scoped read/mutation routing,
  migration/receipts, output-cache reconciliation, deadlines, safe native
  capability rejection and public/Palm contracts reviewed. No further new
  actionable findings established. Unchanged public docs were inspected, not
  rebuilt again in this review; their final fix-phase export remains valid.
- Resolve the new pre-ack evidence-loss case before calling the lookup slice
  ready for consumer handoff. Native atomic cancellation remains explicitly
  unavailable and is a separate known limitation, not a new finding.
- Only this working document is updated by the review; no product changes,
  commit, push, installation change or publication.

## Review fixes completed

The user authorizes fixing all four review findings, without commit, push or
publication. Preserve generic CoCo contracts and Palm ownership of its adapter.

- [x] Replace the unsafe interruption version allowlist with an explicit
  worker capability. Native Codex 0.166.0 fails closed without sending an
  interrupt or recording a native attempt; terminal no-ops still work.
- [x] Preflight non-loading native thread metadata and allow turn pagination
  only for a verified indexed history mode. Legacy/missing/unverified modes
  return an explicit unavailable reason without replaying their transcript.
- [x] Reconcile only the exact proven terminal runtime guard. Keep bounded
  late answer collection independent so an immediate next send works without
  losing late output or leaving `send --wait` pending indefinitely.
- [x] Bound native lookup and supported-interrupt waits; preserve intent and
  uncertainty after timeout and do not let an optional lookup delay mutation.
- [x] Promote regressions into permanent focused tests, update public/canonical
  contracts and Palm handoff, then rerun bounded sequential checks and review.

### Fix decisions and current evidence

- Native worker capability defaults off for exact interruption, including
  0.166.0. No new native interrupt frame or attempt marker is possible on that
  backend. Terminal no-ops and missing-binding handling remain scoped/read-only
  observations. Granting the MCP route never grants a missing runtime capability.
- Exact-tag 0.166.0 exposes `thread.historyMode`. Recovery preflights summary
  identity and `paginated`; missing/legacy/unknown mode or unverified lookup
  version returns `nativeHistoryUnavailable` without turn pagination. This
  restriction is local to turn-outcome recovery; context-fork behavior is unchanged.
- Application-owned two-second budgets cover complete native lookup and any
  genuinely supported interrupt. Interruption does not wait for preflight;
  timeout leaves intent and returns `TURN_INTERRUPT_UNCERTAIN`, while read timeout
  reports `nativeLookupTimedOut`. A raced saved receipt still wins.
- Exact receipt reconciliation transfers the matching runtime guard into the
  bounded output cache with lock order active -> output and no asynchronous
  wait. The next send is unblocked; late old output/completion is routed by
  both native IDs, including before a newer start reply. Missing late output
  drains for at most two seconds rather than leaving CLI waiting indefinitely.
- All 34 focused turn-control tests pass, including new permanent regressions.
  All-target Clippy passes. The full ordinary suite passes 621 library tests,
  six process scenarios and ten status-query tests; seven manual library probes
  and paid/live inference remain intentionally unrun.
- Three native turn-control checks pass on actual installed 0.166.0: static
  stale-ID behavior (not atomicity), public unsupported-cancellation plus durable
  receipts/restart, and summary-mode inspection of a large synthetic legacy
  fixture. The native public fixture cleans its exclusively controlled turns
  through raw native input only as test setup/cleanup, never as a product fallback.
- Public reference, canonical contract and Palm handoff now explicitly state
  that strict native cancellation is unavailable. No upstream code or local
  installation is patched. The original fully cancellable provider requirement
  remains blocked on a safe native targeting primitive; this is not a claim
  that all original acceptance criteria are implemented.

### Final fix verification and handoff

- `cargo fmt --all --check`: pass.
- Warning-denied all-target Clippy: pass.
- Full offline all-target suite with two test threads: 621 library tests,
  six process scenarios and ten status-query tests pass. Seven manual library
  probes and paid/authenticated live inference remain intentionally unrun.
- All ten installed-Codex 0.166.0 model-free compatibility checks pass serially
  in 45.43 seconds, including all three turn-control contracts. The raw native
  cancellation in exclusive test cleanup is not a product capability.
- `cargo machete`: no unused dependencies. `cargo deny --locked --offline
  check`: advisories, bans, licenses and sources pass using the cached advisory
  database; existing duplicate-version warnings remain. No dependencies changed.
  The advisory cache lock required sandbox escalation, without any download.
- Docs type/content check: zero errors, warnings or hints. The final static
  `/coco` export passes all route/search/public-boundary checks for 18 pages and
  64 files. The package-manager cache required build sandbox escalation only.
- `git diff --check`: pass; final focused source, tests and documentation
  reviewed. No temporary review probes, real user service/workspace changes or
  Palm source changes remain from this task.
- Updated [`Palm handoff`](../../handoffs/palm-turn-control.md) describes the
  public DTOs, exact identities, recovery bounds, errors and native cancellation
  limitation. Palm's adapter still belongs in Palm. Start/lookup can be consumed;
  cancellation must be represented as unsupported, not successful.

The four review findings are addressed with safe failure for unsupported native
interruption. Feature changes remain uncommitted on `feature/turn-control`.
No commit, push, installation switch, upstream patch or publication performed.

## Follow-up full review

The user requests a complete independent review of the implemented feature.
Inspect the complete diff, identity and receipt invariants, interruption and
completion races, recovery, repository/grant boundaries and public contracts.
Run isolated diagnostics as needed; report actionable findings without product
fixes, commits, pushes or publication.

### Findings

The previous ready-for-handoff assessment is superseded by this review. The
Palm handoff and public guarantee text need correction as part of the fixes.

1. **P1 — native interruption is not an atomic exact-target primitive.** The
   exact `rust-v0.166.0` source, not merely the older local clone, checks the
   active ID under a thread-state lock, releases it, awaits request bookkeeping,
   then enqueues untargeted `Op::Interrupt`. Core's handler receives only the
   session. A completion and new input can interleave; TUI/native clients
   bypass CoCo's send guard. The current proof rejects an already stale ID,
   not this check/enqueue race. Do not assert or advertise stronger atomicity
   based only on the version allowlist. Keep this mutation fail-closed until an
   actual exact-target native guarantee or equally safe design is established.
   Evidence: upstream `turn_processor.rs` blob
   `f310c4ac65a9ff2ce3a6ff50efee326223b35c77`, lines 1419–1459;
   core `session/handlers.rs` blob
   `128bc7d5bf772695ae43632f9c806f6f0f799c79`, `interrupt`.
   This is a source-proven synchronization gap, not a claimed observed native
   wrong-turn kill in the binary.
2. **P1 — legacy native lookup rereads full conversation history.**
   `itemsView: notLoaded` trims the returned page but does not avoid rollout
   replay for legacy threads. Exact 0.166.0 `thread_processor.rs` blob
   `1ef252dcbf47f9346a7539c39a7ca9a3807a290f`, lines 3370–3378, explicitly
   replays the full rollout for each page. A model-free native probe with a
   4,195,071-byte synthetic transcript measured 4,244,380 bytes read on a
   repeated, warmed metadata-only query; the result had empty item arrays.
   Four pages can mean four scans per poll. Do not use this fallback blindly;
   detect a genuinely indexed metadata path or report unavailable without
   reading legacy history. Correct the public no-history guarantee.
3. **P2 — a recovered terminal receipt does not release the execution guard.**
   Same-generation probe: accept a send, suppress its completion notification,
   expose native idle plus exact completed metadata, call `get_turn`, then send
   new work. Lookup returns `finished/completed` but the next send fails
   `InvalidWorkspaceState { expected: idle, actual: Active }`. The native
   backfill branch only writes the receipt; the runtime guard and CLI pending
   correlation remain. Existing repair tests use a new coordinator with no
   guard and miss this. Reconcile the exact guard without losing independent
   bounded answer collection or ever clearing another operation's guard.
4. **P2 — turn inspection and interruption have no native-response deadline.**
   New pagination bounds page count but every read awaits `CodexClient::request`
   whose receiver has no timeout. `interrupt_turn` first awaits `get_turn`, so
   an open but unresponsive native connection prevents even sending the exact
   interrupt. Client disconnection does not cancel the daemon RPC handler.
   Add explicit total/read deadlines and preserve unknown dispatch/interrupt
   intent on timeout rather than inventing a terminal result.

Two temporary diagnostic probes reproduced findings 2 and 3 and were removed
after recording their failures. No product implementation or shipped test
content was changed during this review. The existing implementation test
results remain useful but do not establish the missing guarantees above.

### Review verification and next action

- After removing both probes, all 609 ordinary library tests pass again (seven
  manual/opt-in probes remain intentionally ignored).
- Both existing native turn-control tests pass on installed `codex-cli 0.166.0`
  again in 3.15 seconds. They do not cover the internal native check/enqueue
  race or legacy read-cost constraint reproduced/identified above.
- `cargo fmt --all --check` and `git diff --check` pass. Public docs were
  inspected against behavior, not rebuilt again in this review because their
  content was unchanged. No real model calls or user workspace/daemon changes.
- Review is complete; implementation is not yet ready for the Palm handoff.
  Resolve native target safety first, then history-mode-safe recovery, exact
  runtime-guard reconciliation and response deadlines. Product fixes require
  the user's next instruction; no commit, push or release was performed.

## Initial implementation record (superseded where corrected above)

The following entries preserve the original implementation chronology. The
review and completed fixes above supersede their original native-safety and
handoff-readiness conclusions.

### Scope

The user authorizes generic CoCo turn inspection and precise interruption,
followed by a handoff for Palm's independently implemented adapter. Reuse the
existing start ledger/application boundary and native Codex. No scheduling,
Palm domain/output logic, workspace provisioning, new CLI commands, push or
publication. The docs checkpoint is local commit `fc5f7d6`; existing main
assessment notes are preserved.

## Plan

- [x] Verify installed Codex and prove stale-turn interruption safety with an
  isolated model-free native contract before exposing a mutation.
- [x] Design repository/workspace/operation-scoped status with explicit unknown,
  dispatch certainty and real terminal outcomes; separate all identities.
- [x] Reuse the operation ledger and persist only minimal technical receipts,
  not model responses or conversation history. Define restart/deletion bounds.
- [x] Add public read and separately opted-in interrupt MCP operations through
  typed application methods. Never interrupt an implicitly current turn.
- [x] Cover start replay/lost replies, overtaking completion, sequential turns,
  late/repeated/racing interruption, restart/eviction and repository/grant safety.
- [x] Update public contracts, canonical knowledge and a concrete Palm handoff;
  run sequential bounded Rust/native/docs checks and review the final diff.

## Boundaries and evidence

- Current read-only assessment selected Codex 0.166.0; recheck it for this slice.
- Native `turn/interrupt` schema takes threadId and turnId, but shape alone
  does not prove safety against a stale target. Native paginated turn metadata
  can omit items; it must not load full history or invent a completion outcome.
- Existing `turn.result` is private and keyed only by operation ID. It is not
  directly suitable for the repository-scoped public contract. The existing
  response cache is generation-bound and evicts terminal metadata with text.
- Preserve the staged Rust dependency direction. New behavior lives in focused
  private modules/tests, not new crates or an expanded library SDK.

## Implementation and evidence

- Installed `codex-cli 0.166.0` passes an isolated real-app-server contract:
  shell-only turns prove that interrupting an old completed turn does not stop
  a newer one, exact interruption reports `interrupted`, and item-free native
  turn metadata remains available after restarting the server. No model calls.
- The native adapter enables interruption only for the verified exact version;
  other/unknown versions fail closed before an interrupt frame is sent.
- `turns.get` and separately granted `turns.interrupt` use fixed-repository,
  workspace and caller-operation scope. The existing send ledger now retains
  exact native thread/turn identity, terminal outcome and observation time.
  Interruption intent is not proof of an acknowledged or completed cancellation.
- Database migration 16 retains legacy acceptance without guessing missing
  thread identity. Receipts survive coordinator generations and response-cache
  eviction; deleting a workspace deletes its operation records too.
- The first 19 focused unit tests pass, including early completion, lost replies,
  sequential turns, interruption/completion races, actual database reopen,
  metadata backfill, explicit uncertainty, repository isolation and version
  fail-closed behavior. Initial all-target compilation and the native proof pass.
- Follow-up review keeps exact native `inProgress` distinct from missing
  metadata so a running accepted turn can be observed after generation loss.
  Unknown/missing terminal statuses do not invent failure. New tests prove
  uncertain native acceptance survives a lost generation and uncorrelated
  completion, and close/deletion obey their documented receipt-retention bounds.
- The real-process MCP test deliberately leaves a start reply unread. A second
  client recovers its terminal result by the original operation key, and a
  replay does not start another turn. Repository isolation, default denied
  mutation and independent interruption grant pass through actual subprocesses.
- Both new installed-Codex contracts pass, including the public end-to-end path
  through real `cocod`, `coco-mcp`, native exact interruption, a newer operation,
  and restart with unchanged durable receipts. Its paused loopback provider
  never produces inference or uses real account credentials.
- Public MCP guide/reference/command help now describe the exact tool contract;
  canonical engineering knowledge and a separate Palm adapter handoff are added.
  Public CLI JSON schema stays 16 (no existing fields changed); storage migration
  16 is a separate version. The static Pages export verifies 18 pages/64 files
  including `/coco` links, search and the public-only boundary.

## Verification and handoff

- `cargo fmt --all --check`: pass.
- `cargo clippy --locked --offline --all-targets --jobs 2 -- -D warnings`: pass.
- `cargo test --locked --offline --all-targets --jobs 2 --quiet -- --test-threads=2`:
  609 library tests, six process scenarios and ten status-query tests pass.
  Seven manual/opt-in library probes and the authenticated paid-live proof
  remain intentionally unrun; native tests are opted in separately.
- `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked --offline --test real_codex_compat --jobs 2 --quiet -- --ignored --test-threads=1`:
  all nine installed-Codex 0.166.0 checks pass serially in 44.62 seconds,
  including both new model-free turn-control contracts.
- `pnpm --dir docs check`: zero errors/warnings/hints.
- `DOCS_BASE_PATH=/coco pnpm --dir docs run build`: static 18-page/64-file
  export, Pagefind, all local route checks and public-only boundary pass. The
  package-manager metadata cache required the usual build sandbox escalation;
  no deployment was performed.
- `git diff --check`: pass; final source/documentation diff reviewed. No new
  dependencies, real user daemon/workspace changes, or Palm source changes.

The implementation is ready for review. The handoff is
[`knowledge/handoffs/palm-turn-control.md`](../../handoffs/palm-turn-control.md)
and documents public inputs/responses/errors, recovery bounds, matching-branch
build requirements and Palm's independent adapter/output join. Feature changes
remain uncommitted on `feature/turn-control`; the earlier docs checkpoint stays
`fc5f7d6`. No push, publication or local service/install switch was performed.
