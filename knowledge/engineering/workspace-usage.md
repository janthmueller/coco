---
type: Engineering Decision
title: Workspace token usage and billing estimates
description: Defines the native evidence, attribution boundaries, recovery requirements, and implemented surface for per-workspace token and cost reporting.
tags: [architecture, codex, app-server, workspaces, tokens, billing, observability]
status: stable
---

# Workspace token usage and billing estimates

## Decision summary

CoCo obtains token usage from the Codex App Server instead of counting
text, parsing Codex's private rollout files, proxying model traffic, or coupling
usage to Codex goals. The implemented read path consumes the stable
`thread/tokenUsage/updated` notification and retains a compact checkpoint for
the native thread bound to each workspace.

Raw token usage and monetary cost are separate evidence contracts:

- raw usage is native thread evidence and can be available whenever a
  completed provider response supplies usage;
- current-context occupancy is a latest-response view, not cumulative spend;
  and
- cost is an optional billing estimate. It must never be inferred as zero when
  the authenticated Codex route cannot provide it.

Codex 0.154.0 also accepts an optional thread ID on `account/usage/read` and
can return backend-estimated credits, optional USD, and grouped model usage.
CoCo queries that route on demand and keeps its short-lived result in memory;
it does not ship a price table. The response is nullable and eventually
consistent, so token reporting remains useful without it.

The approved read surface is `coco usage [workspace]`, with current-repository
collection scope by default, `--all-repos` for a global collection, `--global`
for one globally resolved name, one-shot JSON, and terminal-aware `--follow`.
Operational state remains in `status`; a future vocabulary review may consider
whether read-only surfaces should share a `show` namespace, but that does not
change the current command contract.

## Scope and vocabulary

A CoCo workspace currently binds one native Codex thread. Token and cost
evidence therefore belongs to that binding, not to the workspace exec-server
cgroup. The cgroup measures local execution resources; the App Server reports
model usage. Neither includes the other.

Use these terms precisely:

- **native thread total**: Codex's cumulative token counters for the bound
  thread;
- **workspace-attributable usage**: the supported delta generated after CoCo
  established a known baseline for that binding;
- **current context**: the latest native response's token count relative to
  the reported model context window; and
- **estimated cost**: credits or currency calculated by a provider/backend,
  with an observation time and an explicit unavailable state.

Do not label an inherited native total as workspace-attributable spend unless
the baseline is known. Do not label a context-window percentage as cumulative
token usage.

## Native evidence in Codex 0.154.0

| Source | Evidence | Strength | Limitation | CoCo role |
| --- | --- | --- | --- | --- |
| `thread/tokenUsage/updated` | Exact thread and turn IDs, cumulative `total`, latest `last`, and model context window | Direct stable App Server notification already flowing through CoCo's control plane | Push-only; replay is not a reliable read API on every resume/fork path | Primary raw-usage source |
| `account/usage/read` with `threadId` | Estimated credits, optional USD, and model/reasoning/speed/token groups | Backend owns plan and billing-route knowledge | Requires supported service authentication; may be null and can lag settlement | Optional native cost provider |
| Codex OpenTelemetry | Per-turn token metrics, response span attributes, and route-dependent cost telemetry | Useful for external dashboards and fleet observability | Requires an exporter/collector and creates a second asynchronous path; attribution and availability vary by metric/version | External integration, not CoCo state authority |
| `account/usage/read` without a thread | Account summary and daily token activity | Useful account overview | Cannot attribute usage to a CoCo workspace | Out of scope for workspace accounting |

The installed schema gives both `total` and `last` these counters:

- `inputTokens`;
- `cachedInputTokens`;
- `cacheWriteInputTokens`;
- `outputTokens`;
- `reasoningOutputTokens`; and
- `totalTokens`.

Use the supplied totals rather than recomputing them. Cached input is a
breakdown of input, and reasoning output is a breakdown of output; adding
either again would double-count. One user turn can issue multiple model
responses around tool calls, so `last` is not a complete per-turn bill.
`total` is the appropriate cumulative source; per-turn values should be
derived only from validated cumulative boundaries or a future explicit native
turn contract.

Context compaction can lower current-context occupancy while still consuming
tokens itself. A cumulative usage view must therefore not fall merely because
the latest context became smaller.

The per-thread account result, when available, includes:

- `estimatedUsageCreditsMicros`;
- optional `estimatedUsageUsdMicros`; and
- groups keyed by optional model, reasoning effort, and speed with estimated
  credits plus optional input, net-new input, cached input, output, and total
  tokens.

That result is an estimate rather than an invoice. Codex's own TUI refreshes
it asynchronously after a turn, which confirms that a just-completed turn may
not be settled when first queried.

## Attribution and recovery requirements

Live notifications alone are insufficient for a durable workspace view. CoCo
uses bounded `thread/resume` and `thread/fork` requests with `excludeTurns`,
and Codex 0.154.0 does not guarantee that those paths replay the latest token
snapshot. A daemon can also be offline while another native client uses the
thread. CoCo must not report a stale checkpoint as a complete fresh total.

The implementation persists only a compact native checkpoint, not conversation
content or a duplicate event ledger. It contains:

- workspace and exact bound native thread identity;
- the complete native `total` and `last` breakdowns plus context-window size;
- the native turn ID that supplied the checkpoint;
- observation time and the owning daemon runtime generation.

The read projection marks a checkpoint fresh only when it was observed in the
current daemon generation. After restart the saved checkpoint remains visible
as `last seen` until another native notification arrives. No checkpoint is an
explicit unavailable state. This generation marker proves observation recency;
it cannot prove that another App Server did not use the same thread while CoCo
was disconnected.

The checkpoint is a cache of native evidence, not a new billing authority. A
later notification for the same binding may reconcile missed live increments
through its cumulative total. A changed thread binding starts a new
generation; totals from two native threads must never be merged implicitly.

These lifecycle cases require real App Server tests before a value may be
called workspace-attributable:

- a fresh workspace before and after its first response;
- multiple model responses in one tool-using turn;
- explicit and automatic compaction;
- daemon restart followed by bounded resume;
- context fork from a CoCo workspace and from an arbitrary native thread;
- a turn driven through the `jump` TUI; and
- activity performed through another App Server while CoCo is not subscribed.

For a fresh thread, zero is a valid baseline. For a fork or imported context,
CoCo must determine whether the child native total begins at zero, inherits a
restored total, or first becomes observable only after a response. Until that
behavior is proven against the selected release, expose native totals with
their provenance and do not manufacture a workspace delta.

## Cost-provider options

### 1. Native per-thread estimate — preferred

Call `account/usage/read` with the workspace's native thread ID on demand.
Preserve credits and USD separately, retain the grouped breakdown, attach an
observation time, and represent `threadUsage: null` as unavailable. Do not
wake a workspace executor for this account-level App Server request.

This is the preferred cost source because the backend can account for the
effective model, reasoning effort, speed, plan, credits, and billing route.
CoCo cannot reliably reconstruct all of those from a token total.

### 2. Versioned local price catalog — deferred fallback

API-key and custom-provider users may eventually want a local estimate when
no native per-thread billing route exists. That requires a provider-specific,
versioned price catalog with effective dates and explicit handling for cached
and cache-write input, output/reasoning, service tier, model rerouting,
long-context pricing, tool fees, and currency. Unknown inputs must produce an
unknown estimate, not a best-looking number.

This is maintenance-heavy and can diverge from an invoice. It should be added
only after a concrete unsupported-provider use case, never as the first cost
implementation.

### 3. External telemetry or billing reconciliation — integration only

Codex OpenTelemetry, provider admin usage APIs, gateway accounting, and
invoices can support fleet dashboards or financial reconciliation. They may
be more authoritative at account level, but they are asynchronous external
systems and do not necessarily carry a CoCo workspace identity. CoCo may later
export its workspace/thread mapping for such integrations; it should not run
an OTLP collector or intercept model traffic merely to populate local status.

## Rejected primary sources

- **Local tokenization of visible messages** misses hidden instructions, tool
  payloads, provider transformations, caching, reasoning, retries, and
  compaction.
- **Codex rollout JSONL or internal SQLite** couples CoCo to private storage
  formats even though the App Server already exposes a stable notification.
- **Internal raw-response notifications** are explicitly internal-only and
  would make CoCo depend on response-level implementation details.
- **Codex goal `tokensUsed`** is goal-scoped and optional; workspace accounting
  must not require or inherit goal semantics.
- **An account-wide token summary** cannot assign activity to a workspace.
- **A model-traffic proxy** would duplicate native App Server ownership and
  introduce credentials, transport, retry, and provider-compatibility risk.

## Implemented contract

1. **Raw usage foundation**: the centralized notification path decodes the
   stable wire payload into focused domain types, resolves the exact native
   thread binding, rejects regressing cumulative totals, and stores one
   schema-versioned checkpoint per workspace. Workspace deletion cascades it.
2. **Read surface**: typed `workspace.usage.get` and
   `workspace.usage.list` local-RPC methods expose the latest checkpoint. JSON
   retains every consumed native field plus provenance; human output limits
   itself to cumulative totals, useful breakdowns, context occupancy, and the
   optional estimate. Collection reads omit closed workspaces, while an exact
   target remains addressable.
3. **Optional native estimate**: the daemon adapter calls
   `account/usage/read` through the `WorkerRuntime` port. Results—including an
   unavailable result—are cached for 15 seconds and invalidated by a new token
   notification. The estimate and its observation time are returned separately
   from tokens and are never persisted. Failure cannot fail raw-token reads.
4. **Passive follow**: `--follow` polls the read projection without resuming a
   thread or starting its workspace executor. A terminal replaces its previous
   frame; redirected output appends only changed frames. JSON is one-shot.

No per-turn delta, usage history, budget, alert, repository total, local price
catalog, or MCP usage tool is part of this slice. Those require separate
ownership and retention decisions. The same applies to any future consolidation
of `list`, `status`, and `usage` under a shared read command.

## Compatibility evidence

This assessment was performed against the installed and selected
`codex-cli 0.154.0` on 2026-09-11:

- generated stable schemas contain `thread/tokenUsage/updated`, its complete
  breakdown, and the optional per-thread `account/usage/read` request/result;
- released source confirms CoCo's bounded resume/fork paths cannot be treated
  as a universal usage replay API;
- released source emits per-turn token OpenTelemetry metrics, but that remains
  an optional exporter path rather than a local query contract; and
- a read-only request for an existing CoCo thread returned
  `threadUsage: null` through the currently authenticated route, proving that
  monetary estimation must be optional on this machine.

The public App Server prose currently documents the active-thread token
notification and account-wide usage request, while the per-thread extension
is present in the generated stable schema and official Codex implementation.
The fake-process contract now covers notification ingestion, CLI scopes and
follow output, optional native cost, passive reads, persistence, and stale
projection after daemon restart. Keep the generated-schema fixture and
real-process compatibility gate; do not infer support solely from the prose
page. Attribution claims beyond native thread totals still require the real
model-consuming lifecycle cases above.

Primary upstream references are the official
[App Server documentation](https://developers.openai.com/codex/app-server),
the official
[per-thread usage implementation](https://github.com/openai/codex/commit/f1a1fce26af057ca568a876ff5ce6f49d72013f6),
and schemas generated from the selected executable with
`codex app-server generate-json-schema --out <temporary-directory>`.
