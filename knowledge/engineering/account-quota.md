---
type: Engineering Decision
title: Account-wide Codex quota projection
description: Defines the native App Server source, global ownership, cache behavior, and status projection for remaining Codex usage windows.
tags: [architecture, codex, app-server, accounts, quota, observability]
status: stable
---

# Account-wide Codex quota projection

## Decision summary

CoCo reads the active Codex account's remaining usage windows from the native
App Server method `account/rateLimits/read`. It does not parse Codex TUI output,
derive quota from workspace token totals, or persist account data in SQLite.

The public surface is the explicit `coco status --quota`/`-q` projection. It
composes with targeted and collection status, `--follow`, resources, usage,
tree output, and JSON. Repository and workspace scopes continue to select only
workspace rows; account quota appears once and is never attributed to a
workspace.

## Native contract

The selected Codex 0.154.0 App Server returns:

- `ordinaryUsageAllowed`, the authoritative optional permission for ordinary
  included usage;
- a backward-compatible `rateLimits` snapshot;
- optional `rateLimitsByLimitId` snapshots for every metered bucket;
- primary and secondary windows containing `usedPercent`, optional
  `windowDurationMins`, and optional `resetsAt`; and
- additional account, reset-credit, and promotion fields that are outside this
  first projection.

CoCo prefers the multi-bucket map and falls back to the legacy snapshot. It
retains every returned bucket in typed local-RPC and JSON output, ordering the
canonical `codex` bucket first. The compact human view selects that bucket, or
the first bucket with windows when it is absent.

Window names are derived from their duration with the same approximate
five-hour, daily, weekly, monthly, and annual boundaries used by Codex. The
primary and secondary positions are not stable semantic names. Human remaining
percentage is the clamped presentation value `100 - usedPercent`; JSON retains
the native used percentage, duration, and reset timestamp.

`ordinaryUsageAllowed: false` is rendered as blocked even when percentages or
reset timestamps appear recoverable. A missing value is unknown and must not be
inferred from the windows.

## Ownership and data flow

The daemon's shared control App Server performs the account read through the
existing `WorkerRuntime` port. The request explicitly omits Luna Reserve
support and excludes unused reset-credit detail. It does not resolve a
workspace, load a thread, or start a workspace exec server.

The coordinator maps the native result to `account.quota.get` and keeps one
generation-local snapshot for 15 seconds. It invalidates that cache when it
observes:

- `account/rateLimits/updated`;
- `account/updated`;
- an App Server disconnect.

The native rate-limit notification is sparse. CoCo deliberately invalidates
and refetches a full snapshot instead of treating omitted fields as deletion or
maintaining a partial merge implementation.

`thread/tokenUsage/updated` does not invalidate this cache. Codex emits the
dedicated account-rate-limit notification when a token-count event also carries
new account-limit evidence. Treating every thread-usage update as account
evidence would defeat the cache during active turns and cause unnecessary
backend reads.

Authentication rejection and an App Server without the method are normal
typed unavailable states. Other request or decode failures become a generic
read-failed state and do not make workspace status fail. The cache includes
unavailable results to avoid retrying a failing backend on every follow frame.

## Status and JSON contract

Human output appends one account line after the selected workspace view, for
example:

```text
Quota  5h 84% left · weekly 61% left
```

`--follow` includes quota in the same terminal frame. Redirected follow output
continues to append only when the complete requested view changes.

Schema-version-13 status JSON adds `accountQuota` only when `--quota` is
requested. The object is either:

- `available`, with `ordinaryUsageAllowed`, all typed buckets, and
  `observedAtMs`; or
- `unavailable`, with a bounded reason and `checkedAtMs`.

Account IDs, backend banners, promotion content, and credentials are never
included. The object is top-level for both targeted and collection status and
is not duplicated into workspace rows.

## Boundary with workspace usage

`status --usage` remains a workspace projection: cumulative tokens, context
occupancy, and an optional per-thread cost estimate. `status --quota` answers a
different question about the active account across every conversation. Neither
value is used to reconstruct the other.

## Deferred capabilities

This slice does not add quota history, alerts, budgets, enforcement guards,
signals, reset-credit redemption, Luna Reserve fallback, a standalone
`coco quota` command, or an MCP account-quota tool. Each would require a
separate retention, permission, or policy decision.

Primary upstream references are the official
[App Server documentation](https://developers.openai.com/codex/app-server),
the selected release's
[account protocol](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/account.rs),
and its
[rate-limit display shaping](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/tui/src/status/rate_limits.rs).
