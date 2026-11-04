---
type: Engineering Contract
title: Operation-scoped turn inspection and interruption
description: Defines exact native correlation, minimal terminal receipts, bounded recovery and separately granted MCP interruption.
tags: [turns, mcp, idempotency, recovery, codex, integration]
status: stable
---

# Operation-scoped turn control

## Ownership and public surface

CoCo exposes generic execution controls, independently of any caller's task
domain. Palm's provider adapter, scheduling, artifact validation and task
completion remain in Palm. The public contract is
[`docs/src/content/docs/reference/turns.mdx`](../../docs/src/content/docs/reference/turns.mdx).
No new CLI command, MCP registry, workspace provisioning or response-text
transfer accompanies this slice.

- `workspaces.send`: existing caller-selected `operationId` and idempotency.
- `turns.get(workspace, operationId)`: default read-only control MCP tool.
- `turns.interrupt(workspace, operationId, mode?)`: separate `--allow-interrupt`
  grant, independent of send and signals. Omitted/explicit `strict` requires
  atomic targeting and remains unsupported on current Codex. Explicit
  `bestEffort` permits the ordinary native interrupt despite its known race.
- Local typed application methods: `TurnGetParams`/`TurnObservation` and
  `TurnInterruptParams`/`TurnInterruptResult`, routed through existing RPC.
  Internal method names are `turn.get` and `turn.interrupt`; they are not a new
  independently supported SDK or public network transport.

The repository path is fixed at MCP startup. Resolve that repository and the
workspace before looking up an operation; then require the same workspace ID
and `turn_start` kind. A foreign workspace UUID fails `WORKSPACE_NOT_FOUND`.
A foreign or absent operation key on an allowed workspace fails
`TURN_OPERATION_NOT_FOUND`, without a native read or interrupt. Audit the safe
action/key/result, not prompt or model content.

## Identity and durable minimum

The caller's globally unique `operationId` differs from CoCo's internal
operation primary key (`turnId` in the existing send response). The workspace
ID differs from native `codexThreadId`; each accepted native execution has its
own `codexTurnId`. Never infer a binding from nearby events, current workspace
activity, the latest turn, or a caller-supplied native target.

Database migration 16 adds four nullable columns to the existing `operations`
ledger: `native_thread_id`, `terminal_status`, `terminal_observed_at_ms`, and
`interrupt_requested_at_ms`. Dispatch commits the exact native thread ID
before its side effect. Only the direct `turn/start` response binds the native
turn ID. An accepted exact completion writes one of `completed`, `failed`, or
`interrupted`; repeated identical observations preserve the first time and
conflicting identities or outcomes fail instead of overwriting it.

Migration 17 adds nullable `interrupt_mode` and `interrupt_state` to that same
row. Public `interruptAttempt` exposes their typed pair; the existing timestamp
marks the first claim. Old v16 markers become strict/uncertain, never an
invented acknowledgement. Partial/malformed attempt metadata fails closed.

These receipts retain the caller-to-native correlation CoCo needs to answer
after losing its notification subscription or answer cache. They are not a
second conversation history, current thread-status store, or inferred task
result. Prompt, answer, item, error-text and usage histories are not added.
Existing v15 accepted records keep their turn ID but have no invented thread
snapshot; public lookup reports `legacyCorrelationMissing` and cannot
interrupt them. No lookup guesses that a current workspace binding is the
original binding of a legacy operation.

Receipts survive response-cache eviction, coordinator generations and actual
database close/reopen. Close/reopen retains operation rows; workspace deletion
cascades to them, even with `--keep-thread`. There is no tombstone or promise
that a missing key after deletion proves non-execution. There is no new
receipt retention horizon beyond that of the existing operation ledger.

A delayed terminal notification also records a receipt after runtime-guard or
answer-cache loss. Reuse the existing unique native-turn lookup, require an
accepted turn-start operation and its matching saved native thread ID, then
use the same checked receipt write. Do not derive the caller key from the
currently active turn. A newer guard, including an unconfirmed start, remains
untouched and does not buffer a known older operation's completion. The owning
guard retains its existing pre-confirmation buffering/binding path, including
conflict handling. Unknown IDs, uncertain starts and legacy missing thread
snapshots cannot acquire a receipt from event proximity. This is recovery of
received evidence, not a promise that a running turn or event stream survives
every coordinator restart. No new schema or native history read is needed.

## Projection and recovery

`dispatch` preserves the existing state machine: `prepared`, `dispatching`,
`accepted`, `uncertain`. It is independent of observed execution `state`:
`notStarted`, `starting`, `running`, `finished`, `unavailable`. Only `finished`
has an `outcome`. A persisted terminal receipt wins and requires no native
call. A prepared intent is not an outstanding dispatch; a dispatching start
is not yet an accepted turn. A crash or disconnect without a confirming start
response leaves `uncertain` / `acceptanceUnconfirmed`; proximity to a native
completion cannot repair that ambiguity and retry must not redispatch it.

For an accepted operation without a receipt, native recovery is enabled only
for the verified indexed lookup contract (currently exact Codex `0.166.0`).
Preflight `thread/read` with `includeTurns: false`, require the snapshotted
thread ID and an explicit `historyMode: paginated`. Missing, legacy or unknown
modes and unverified versions return `HistoryUnavailable` without turn-history
requests. In 0.166.0, item-free legacy pagination still replays the entire
rollout before trimming output, so `notLoaded` alone is not sufficient.

For indexed history, use `thread/turns/list` with `itemsView: notLoaded`,
descending order, 50 entries per page and at most four pages. The complete
worker inspection, including preflight, has a two-second application deadline.
No resume, inference or transcript scan is used. Exact terminal metadata
backfills the receipt; exact native in-progress evidence or a matching current
generation's in-flight guard reports `running`. No evidence is not absence.
Without in-flight tracking, unindexed recovery is `nativeHistoryUnavailable`;
no indexed match is `completionNotObserved`; native failure is
`nativeLookupFailed`; timeout is `nativeLookupTimedOut`. None invents failure.

Before the direct start response binds its turn ID, retain observations by
native turn ID rather than one last-event slot. The generation-local buffer
keeps at most 32 IDs of at most 256 bytes and 1 MiB of aggregate answer text;
it does not evict an admitted candidate for another turn. Only the exact ID
from the start response selects completion and output. Identical terminal
repeats are safe; conflicting terminal observations remain unknown. This
also handles delayed events whose completed-output cache entries have expired.

If a safety bound discarded evidence for the eventual accepted ID, its guard
is no longer proof of execution still being in progress. Public lookup remains
`unavailable` with its ordinary evidence-gap reason, private answer polling
returns unavailable, and workspace projection is unavailable. The dispatch
still remains `accepted`, with exact native IDs; do not redispatch it or erase
the concurrency guard. Exact native in-progress evidence can restore tracking;
an exact terminal receipt releases the guard normally. Reconciliation between
durable acceptance and runtime binding preserves the selected early output.

After asynchronous metadata reads, resolve the same frozen workspace UUID
within the original repository and reread the operation, allowing a raced
terminal receipt to win even when the read failed. Malformed or unknown
native terminal status is never defaulted to `failed`.

Receipt projection also reconciles only the exact matching internal operation,
workspace and native thread/turn guard, even when another query saved the
receipt first. It never clears a newer turn. The execution guard is transferred
atomically into bounded late-output collection; the lock order is active guard
then output cache, with no asynchronous wait while locked.

The answer cache remains at most 256 terminal entries, 1 MiB per answer and
8 MiB total answer bytes for one generation. A metadata-repaired completion
allows up to two seconds for queued answer delivery, independent of the next
send. Matching native completion ends that drain immediately; missing delivery
cannot leave private `turn.result` pending forever. Old late output is routed
by both native IDs, including while a new operation awaits its start reply.
The public outcome API neither returns text nor extends its retention.

## Interruption modes and accepted residual risk

Decision updated on 2026-11-04 by explicit user instruction: atomic targeting
remains the strict contract, but no longer blocks a deliberately selected
best-effort mode. `mode` defaults to `strict`; `bestEffort` must be explicit.
Both use the existing grant, stored operation binding and ordinary native
`turn/interrupt`; neither accepts caller-selected native targets or fallback
current-turn/startup cancellation. Empty IDs are not valid targets.

The port keeps `supports_exact_turn_interrupt()` false for native Codex.
0.166.0 checks the ID under a thread-state lock, releases it, awaits bookkeeping
and submits untargeted `Op::Interrupt`; the core handler receives no expected
turn ID. A newer TUI/native turn can be interrupted instead. CoCo admission
serialization cannot eliminate that native race. An independent best-effort
capability verifies the ordinary request/rejection contract for exact 0.166.0,
not atomicity. Other/unverified builds fail closed. Upstream patches and local
installation changes are outside this slice.

After scoped lookup, terminal operations are no-ops (`confirmed` for interrupted,
`alreadyFinished` for completed/failed). No confirmed binding and no prior
attempt returns `notRequested`. An unsupported first attempt returns
`TURN_INTERRUPT_UNSUPPORTED` without a native frame or attempt marker. Neither
strict rejection nor normal `turns.get` inspection initiates best-effort work.

## Durable one-shot attempt and recovery

Before dispatch, an immediate SQLite transaction claims the only attempt for
the retained operation, stores its immutable mode, first timestamp and
`uncertain` state, and returns dispatch permission to exactly one caller.
Concurrent callers and independently reopened connections cannot both win.
This is not a recoverable command outbox: a crash after the commit but before
the write leaves honest uncertainty, possibly without any native dispatch.
There is no exactly-once delivery guarantee or force-retry API.

Same-mode repeats inspect the existing attempt/outcome without another native
interrupt, even after restart or backend change. A different mode under an
existing nonterminal attempt fails `TURN_INTERRUPT_MODE_CONFLICT`; it cannot
grant another side effect. Confirmed terminal turns are no-ops in either mode.
Workspace/data deletion removes both the receipt and deduplication boundary.

A received native acknowledgement records `requested`; a proven pre-queue
stale/no-active validation rejection records `rejected`, with no fallback.
Only exact 0.166.0 error forms establish that rejection; unclassified RPC,
transport and malformed-reply failures stay uncertain. Rejection invalidates
only that operation's local in-flight hint, not a newer guard or the original
turn's unknown outcome. An exact native outcome can still repair its receipt.

The native response budget is two seconds, followed by bounded outcome
inspection. First timeout/native failure returns the existing structured error
(`TURN_INTERRUPT_UNCERTAIN`, `TURN_INTERRUPT_REJECTED`, or `CODEX_ERROR`) unless
the original operation's terminal outcome has raced it. The claim remains
durable. A later repeat can return `uncertain` or `rejected` directly from the
saved attempt. Successful reply persistence is required to report `requested`;
a persistence failure cannot silently promote an uncertain claim.

Execution outcome always refers to the stored native thread/turn pair.
`interrupted` confirms that original turn was interrupted, not the causal
effect of this request. Completed/failed outcomes remain unchanged, and an
interrupted newer turn is never attributed to the original operation.
Attempt delivery can remain `uncertain` even after an original terminal receipt
is known; the top-level `alreadyFinished`/`confirmed` is not an acknowledgement
of a lost native reply. Palm owns the rule not to start more work in that
thread while interruption is unresolved; no Palm scheduler is added to CoCo.

No workspace close, thread deletion or process termination substitutes for
this request. There is no promise of side-effect rollback or termination of
all spawned background processes.

## Evidence and verification

The installed `codex-cli 0.166.0` is selected by the existing exact-version
compatibility gate. Its generated `TurnInterruptParams` includes both IDs;
its turn pagination supports `itemsView: notLoaded`. The official
[App Server guide](https://learn.chatgpt.com/docs/app-server) distinguishes
interrupt acknowledgement from terminal `turn/completed`; wire shape alone
does not prove atomic target safety. Exact-tag source review established the
check/queue gap despite the earlier static stale-ID test passing.

`tests/real_codex_compat/turn_control.rs` uses a temporary Codex home, no real
authentication and an unreachable test-only model provider. Shell-only native
turns check rejection of an already-stale target, successful exclusive-fixture
interruption and item-free native terminal lookup. They do not prove the
internal validation/enqueue race safe. The fixture can read
persisted terminal records after a native restart without loading the thread.
The proof observes completed and interrupted records; failure projection also
has isolated adapter/coordinator fixtures. No inference is used.

Focused private test modules cover retry/fingerprint conflicts, lost start
responses, completion before start confirmation, sequential IDs, repeated/late
and racing interruption, uncertain starts, receipt reopen/migration,
generation/cache loss, exact metadata backfill, malformed data, and repository
or grant rejection. Real-process MCP fixtures verify lost-response recovery,
independent grants, scope enforcement and receipts after a coordinator restart.
Future callers consume these public tools instead of reading CoCo's database.

The public native proof exercises real `cocod` and `coco-mcp`: start, default
strict rejection without a marker, explicit best-effort native interruption,
old-operation replay while a newer turn remains active, operator-style cleanup
of the control turn, and restart with unchanged receipts/attempts. This success
does not prove the known native race gone. A third proof reads a large synthetic legacy thread's non-loading
summary and verifies its mode before adapter tests prove pagination is skipped.
Loopback fixtures use no real model/authentication. Focused regressions cover
stalled lookups/interruption, unknown history modes, same-generation repair,
late output during a new start, bounded output drain and exact guard retention.
Pre-confirmation regressions cover both event orders, real response-cache
eviction, aggregate UTF-8-safe output bounds, overflow/conflict uncertainty,
metadata repair and the acceptance/binding interleave.
Best-effort cases cover real DB close/reopen, concurrent claims, lost replies,
pre-write crash gaps, mode conflicts and proven native rejection. A controlled
check/queue fixture deliberately interrupts B after A completes; A retains
its true completed outcome. It demonstrates attribution correctness and the
accepted residual risk, not native atomicity.
