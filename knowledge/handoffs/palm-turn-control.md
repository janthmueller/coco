---
type: Integration Handoff
title: CoCo turn-control handoff for Palm
description: Gives Palm's adapter owner the implemented public execution contract, recovery rules, integration responsibilities and verification evidence.
tags: [handoff, palm, mcp, turns, execution]
status: active
updated: 2026-11-05
---

# CoCo turn-control handoff for Palm

## Status and responsibility

Generic turn lookup, durable receipts, explicit best-effort interruption and
the delayed-completion recovery fix ship in
[CoCo 0.5.0](https://github.com/janthmueller/coco/releases/tag/v0.5.0).
The source is integrated into `main`; the `v0.5.0` tag is release commit
`daaa9dd`. Registry availability of `codex-coordinator 0.5.0` is verified.
Use matching release builds or build that source, rather than installed
`0.4.0` binaries, which predate this API. Check that `turns.interrupt`
advertises `mode: strict | bestEffort`.

**Capability boundary:** start/lookup and durable technical receipts work.
Default/explicit strict cancellation remains **unavailable with native Codex
0.166.0**. Explicit `mode: "bestEffort"` uses the ordinary native interrupt,
accepting that a concurrently started newer turn could be interrupted instead.
This is the user's deliberate relaxation of the earlier atomic-only requirement,
not a claim that Codex's race is fixed. Only the original turn's confirmed
`interrupted` outcome proves it was interrupted; an acknowledgement alone does
not. Palm must choose best-effort explicitly, never as a silent strict fallback.

CoCo remains independent of Palm. The provider adapter belongs in Palm and
uses public MCP tools. CoCo contains no Palm workflows, task scheduling,
artifact validation, automatic workspace provisioning or result transfer.
This handoff describes the usable generic execution capability and its limits.

The authoritative public contract is
[`docs/src/content/docs/reference/turns.mdx`](../../docs/src/content/docs/reference/turns.mdx).
The internal implementation rationale is
[`knowledge/engineering/turn-control.md`](../engineering/turn-control.md).

## Process setup

Use matching `cocod` and `coco-mcp` builds from release `0.5.0` and the selected
Codex `0.166.0`. To build the released source locally:

```sh
cargo build --locked --bins --jobs 2
```

The matching binaries are then under `target/debug/`. For adapter development,
point explicitly to those binaries; installed `0.4.0` binaries predate this
slice. Use the same configuration/state paths for both processes, and do
not silently replace an operator's existing coordinator. Start the matching
coordinator normally. For each repository Palm is allowed to control,
configure a standard-input/output MCP server:

```text
coco-mcp --repository /absolute/path/to/project --allow-send --allow-interrupt
```

`coco mcp serve` accepts the same arguments. Repository registration and the
workspace must already exist. A server is fixed to its repository; the tools
cannot override that path. IDs alone do not bypass this restriction.

Default tools include read-only `turns.get`. `--allow-send` exposes
`workspaces.send`; `--allow-interrupt` independently exposes `turns.interrupt`.
Granting one never grants the other. Without the interrupt flag the tool is
absent and direct calls are rejected by MCP.
The interrupt grant allows connected clients to explicitly choose best-effort;
it does not turn that mode into a targeting guarantee. No extra CLI command or
second mutation-grant flag was added. Without `--allow-interrupt`, both modes
are absent/rejected even when send is enabled.

## Public calls

### Start or replay

Tool: `workspaces.send`

```json
{
  "workspace": "fix/login",
  "message": "Fix the redirect and run the tests.",
  "operationId": "palm-run-unique-id"
}
```

Generate and persist the key before dispatch through Palm's existing outbox.
Keys are unique across this CoCo installation, not just per repository. Reuse
the exact same workspace and message on retry. An accepted identical replay
does not start another native turn. Changed content or target under an
existing key fails `IDEMPOTENCY_CONFLICT`. An ambiguous native start remains
`OPERATION_UNCERTAIN`; it is never silently retried.

The existing send response has `workspace`, internal CoCo `turnId`, and
`codexTurnId`. The internal `turnId` is not the client key. Continue to use the
original `operationId` for lookup/cancellation. Prefer the stable workspace ID
over its human-readable name for subsequent calls.

### Find this exact execution

Tool: `turns.get`

```json
{"workspace": "fix/login", "operationId": "palm-run-unique-id"}
```

Both fields are required; additional arguments are rejected. The successful
MCP result has `isError: false` and this object in `structuredContent`:

```json
{
  "workspaceId": "coco-workspace-id",
  "operationId": "palm-run-unique-id",
  "codexThreadId": "native-codex-thread-id",
  "codexTurnId": "native-codex-turn-id",
  "dispatch": "accepted",
  "state": "finished",
  "outcome": "completed",
  "outcomeObservedAtMs": 1793782800000,
  "interruptRequestedAtMs": null,
  "interruptAttempt": null,
  "unavailableReason": null
}
```

| Dimension | Values and adapter interpretation |
| --- | --- |
| `dispatch` | `prepared`, `dispatching`, `accepted`, `uncertain`; this is acceptance certainty, not execution success |
| `state` | `notStarted`, `starting`, `running`, `finished`, `unavailable` |
| `outcome` | Only for `finished`: `completed`, `failed`, `interrupted`; otherwise null |
| `unavailableReason` | `acceptanceUnconfirmed`, `legacyCorrelationMissing`, `completionNotObserved`, `nativeLookupFailed`, `nativeHistoryUnavailable`, `nativeLookupTimedOut`; otherwise null |
| `interruptAttempt` | null, or `{mode: "strict" | "bestEffort", state: "uncertain" | "requested" | "rejected"}`; a saved attempt is not a terminal execution result |

`prepared` / `notStarted` means a saved intent without outstanding dispatch.
`dispatching` / `starting` means the confirming native response has not yet
arrived. `accepted` is proof of the particular native ID, not proof of success.
Exact native in-progress evidence or current-generation in-flight tracking
gives `running`. Only `finished` supplies a known terminal outcome.

All optional fields appear as null when unavailable. Observation and attempt
timestamps are Unix milliseconds recorded by CoCo, not exact native finish
times. No assistant answer text is included and no model call is made.
Polling does not load the conversation or start the workspace's executor.
Missing outcomes are recovered only through a verified indexed native history
path. Current support is Codex 0.166.0 with `historyMode: paginated`; legacy,
missing or unknown modes and unverified builds are not scanned. Native lookup
has a two-second total response budget. Timeout is unknown, not failed.

### Explicit best-effort cancellation

Tool: `turns.interrupt`. Palm must explicitly send:

```json
{
  "workspace": "fix/login",
  "operationId": "palm-run-unique-id",
  "mode": "bestEffort"
}
```

Omitting `mode` is exactly `strict`. A first strict attempt on an unfinished
native Codex turn returns `TURN_INTERRUPT_UNSUPPORTED`, with no native frame
or attempt marker. Explicit best-effort support is separate from the worker's
atomic exact-target capability, which remains false. Other/unverified Codex
versions reject a first best-effort attempt before recording/sending it.

Successful tool processing returns `{ "turn": <observation>, "interruption":
<value> }` in `structuredContent`; it is not itself proof of a stop:

| `interruption` | Interpretation |
| --- | --- |
| `requested` | Native acknowledgement saved; no confirmed terminal outcome for the original turn yet |
| `uncertain` | A saved attempt may or may not have been sent; its reply is unknown |
| `rejected` | A saved native stale/no-active validation rejection, with no fallback |
| `confirmed` | The original saved thread/turn's outcome is `interrupted` |
| `alreadyFinished` | The original turn is `completed` or `failed`, not cancelled by inference |
| `notRequested` | No confirmed target and no retained attempt; nothing was sent |

Before dispatch, CoCo durably records `{mode, state: "uncertain"}` and the first
`interruptRequestedAtMs`. A received acknowledgement changes the attempt state
to `requested`; a proven native target rejection changes it to `rejected`.
Neither state writes an execution outcome. Raw native IDs supplied by a client
are rejected; only the operation's nonempty stored IDs can be dispatched.
Known terminal turns are no-ops in either mode, regardless of backend support.

The first native timeout returns `TURN_INTERRUPT_UNCERTAIN`; a proven rejection
returns `TURN_INTERRUPT_REJECTED`; unclassified native/transport errors remain
`CODEX_ERROR`, unless the original turn's actual terminal outcome raced the
reply. In every case the saved attempt remains inspectable. The native reply
budget is two seconds, followed by a separately bounded outcome inspection.

Identical repeats, including after a connection loss or real DB reopen, only
return the saved attempt/outcome. They never blindly issue another interrupt.
Concurrent calls cannot both claim dispatch permission. A different mode under
an existing nonterminal attempt returns `TURN_INTERRUPT_MODE_CONFLICT`, not a
new permission. There is no force-retry API in this slice. `turns.get` never
issues an interrupt and remains available without the mutation grant.

A crash after recording a claim but before writing the native frame leaves
honest uncertainty, potentially with no request sent at all. A crash after
native acknowledgement but before saving it also leaves uncertainty. This is
not exactly-once delivery. Do not replace the key or silently retry through a
different mode to bypass it.

Execution and request delivery are independent. A raced `completed`/`failed`
outcome remains real, even if another turn was interrupted. An `interrupted`
outcome is an observation about the original turn, not proof this particular
request caused it. Even `confirmed`/`alreadyFinished` can accompany an
`interruptAttempt.state: "uncertain"` after a lost reply. Neither an interrupt
nor its acknowledgement promises rollback or that all background programs exit.

## Recovery and failure handling

- Lost **client** start reply: look up the original operation key. A saved
  accepted result or terminal receipt can be recovered without another send.
- Lost **native** start confirmation: keep `uncertain` /
  `acceptanceUnconfirmed`, even if an uncorrelated completion was observed.
  Do not turn this into NotFound or redispatch under a fresh key.
- Coordinator restart or bounded answer-cache eviction: the exact accepted
  identity, already-observed terminal receipt and interrupt attempt persist.
  Recorded/possibly-sent attempts are not auto-dispatched again. Answer text
  does not gain a new retention guarantee.
- A terminal notification received after restart can still confirm the exact
  saved native thread/turn pair, without metadata recovery. It cannot confirm
  an unacknowledged start, replace a conflicting stored outcome, or affect a
  newer turn. This does not guarantee delivery of notifications across restart
  or survival of every running turn.
- Missed completion: verified indexed item-free native metadata may recover it.
  Exact native in-progress evidence can report `running` after restart.
  Missing/expired/older native evidence remains `unavailable`; absence, Ready
  or a transport disconnect is never interpreted as successful completion.
  Legacy/unknown history is `nativeHistoryUnavailable`, without a transcript
  replay; the two-second response deadline reports `nativeLookupTimedOut`.
- Recovered terminal metadata releases only the exact operation's execution
  guard, so the next send can proceed. Independent bounded output collection
  drains late answer notifications briefly; technical outcome does not depend
  on answer text and old notifications cannot clear a newer operation's guard.
- Multiple completion/output updates before the start confirmation are
  correlated independently by exact native turn ID, including after old answer
  cache eviction. If early evidence exceeded safe retention bounds or conflicted,
  acceptance remains known but execution may be `unavailable`, not assumed
  running. Exact native metadata can repair that evidence gap. Never repeat
  the instruction with a new key to bypass it.
- Old migrated operations without an original thread snapshot report
  `legacyCorrelationMissing`, cannot be interrupted, and must not borrow the
  workspace's current thread binding.
- Closing/reopening a workspace preserves its operation receipts. Deleting its
  CoCo record also removes attempts and their deduplication boundary, even if
  the native thread was retained. There is
  no eternal tombstone or guaranteed reconstruction after removal of saved data.

Application failures use `isError: true` and
`structuredContent.error: {"code": "…", "message": "…"}`. Handle the code;
do not parse prose. Transport/protocol failures are a separate MCP failure
path, including attempts to call a tool without its grant.

| Code | Interpretation |
| --- | --- |
| `TURN_OPERATION_NOT_FOUND` | No retained operation on the allowed workspace matches this key, including foreign keys |
| `WORKSPACE_NOT_FOUND` | Workspace missing or outside the fixed repository scope |
| `INVALID_PARAMS` | Invalid input; operation keys must contain 1–256 bytes |
| `IDEMPOTENCY_CONFLICT` | Send replay changed its message or workspace |
| `OPERATION_UNCERTAIN` | Send may have started; never blindly replace its key |
| `TURN_INTERRUPT_UNSUPPORTED` | No first attempt was sent; requested mode unsupported (strict remains unavailable on native Codex) |
| `TURN_INTERRUPT_MODE_CONFLICT` | A different saved mode already consumed the single attempt; inspect it instead |
| `TURN_INTERRUPT_REJECTED` | Native target rejection saved; inspect the original outcome; no retry or fallback |
| `TURN_INTERRUPT_UNCERTAIN` | Reply deadline expired; the saved attempt remains uncertain and will not be auto-sent again |
| `CODEX_ERROR` | Native request/reply failure; inspect the retained uncertain attempt and exact original outcome |
| `DAEMON_UNAVAILABLE` | Restore connection and inspect the same key |
| `INTERNAL` | Local persistence/processing failed; do not infer execution or cancellation success |

## Palm implementation guidance

Map Palm's provider run handle to a durable tuple of repository identity,
workspace ID and the caller operation ID. Preserve native thread/turn IDs as
separate returned diagnostics, not substitute provider lookup keys.

Implement `start` with the existing durable outbox and `workspaces.send`;
implement `get`/`lookup` with `turns.get`. A `cancel` adapter explicitly selects
`mode: "bestEffort"` only under Palm's own accepted-risk policy. Strict callers
must continue to see unsupported cancellation, not a hidden fallback.

Palm must not schedule a new turn in that native thread while interruption is
unresolved. Inspect both the original outcome and saved attempt; an original
terminal outcome alone does not resolve unknown delivery of an interrupt.
If uncertainty cannot be reconciled, keep that thread from automatic reuse
pending an operator decision, or use a different execution thread. This
scheduling/lease policy belongs in Palm, not CoCo. Concurrent native TUI input
can still create the known race; a CoCo/Palm lock is not atomic native targeting.

Keep unavailable/unknown handles separate from confirmed failed/interrupted
runs. Do not finalize cancellation from Ready, request acknowledgement, a
successful MCP response containing `uncertain`, or another turn's interruption.
Do not substitute workspace close, thread deletion or process termination for
this API. Native operator action may independently produce an interrupted
receipt visible by the same operation key.

Palm's current `submit` path finalizes a Run and Work directly. A real adapter
must join an output/artifact proposal with a **confirmed successful technical
turn outcome** before finalizing domain success. `completed` alone does not
validate an artifact; an artifact proposal alone does not prove that execution
finished successfully. Define that join in Palm, not in CoCo. A cancellation
racing success also needs Palm's own domain policy.

## Verification evidence

Isolated tests use no real model calls or real user workspaces. Coverage
includes frozen send replay/conflicts, an unread real MCP start response,
overtaking completion, sequential turns, late/repeated/racing interrupts,
restart and actual database reopen, cache loss/eviction, missing or malformed
native evidence, migration, repository isolation and independent grants.

The opt-in native contract uses actual installed Codex `0.166.0` with a
temporary home, no account authentication and a deliberately unreachable
provider. Shell-only turns check rejection of an already-stale ID, not native
atomicity during a race. The public-path proof verifies strict rejection
without a marker, explicit best-effort interruption and old-operation replay
while a newer turn stays active. Operator-style cleanup of the control turn
provides a second receipt; restart preserves both receipts and the best-effort
attempt. Native success does not prove the known race gone.
A separate large synthetic legacy fixture proves non-loading history-mode
inspection; adapter tests ensure no legacy turn-history request is issued.
Other native compatibility checks remain separate from paid/authenticated
live inference.

The full regression suite passes 654 library tests plus six process scenarios
and ten status-query tests. This includes nine permanent regressions for
pre-confirmation evidence, both event orders, actual cache eviction, bounds and
metadata repair. New cases also cover one-shot claims through real DB reopen,
concurrent retries, stale rejection, mode conflicts, unknown replies and a
controlled check/queue race without wrong outcome attribution. Five follow-up
regressions cover delayed terminal events after real DB reopen, all outcomes,
retained interrupt attempts, repeated/conflicting evidence, exact ID pairs,
unconfirmed starts and preserving newer guards before/after their start reply.
All ten model-free installed-Codex checks passed before that correction; all
three turn-control checks were rerun successfully after it, including the public
strict/best-effort path. Formatting, warning-denied all-target Clippy and
dependency checks pass after the correction. The prior final static docs export
remains applicable; public inputs/outputs and site sources did not change.
Seven manual library probes and the paid/live authenticated inference proof
are deliberately not run.

Final gate results are recorded in the branch working document:
[`feature/turn-control`](../work/feature/turn-control.md).
