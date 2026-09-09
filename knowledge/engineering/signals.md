---
type: Engineering Decision
title: Agent-emitted signals
description: Defines the bounded signal contract, native sender attribution, and independent replay without ticket workflow.
tags: [signals, mcp, persistence, orchestration]
status: stable
---

# Agent-emitted signals

## Boundary

A signal is an explicit agent claim with a JSON payload, not a Codex state,
an approval, a ticket transition, or a command. CoCo stores and exposes the
claim; an external consumer decides what it means. No emission wakes another
model, creates a turn, runs a hook, or authorizes a side effect.

## Contract

- Operators select a directory with `--signal-catalog` on the control MCP
  process. Direct `NAME@VERSION.json` files are ordinary JSON Schema 2020-12
  documents, not a custom schema wrapper. `description` is the optional standard
  annotation explaining use (otherwise the signal name is used). Boolean schemas
  are supported too. No catalog is discovered automatically in worker worktrees.
- The complete directory is validated at MCP startup and its immutable,
  repository-scoped definitions are persisted atomically. An identical parsed
  schema is a no-op; changing a definition requires a new positive integer
  version. Whitespace and object-key order do not create revisions. There is no
  CLI or MCP tool for imperative per-type registration.
- A configured MCP instance keeps the selected definitions until it exits.
  Restarting that instance loads changed files; it does not require restarting
  `cocod`. Deleting a file removes it from newly started instances, not history
  or existing instance snapshots. Files belong in an operator-controlled
  location, preferably outside agent-editable worktrees. This is not live
  revocation or a defense against a malicious same-user client.
- MCP exposes type discovery, bounded reads, and opt-in `signals.emit`.
  Repeated `--allow-emit NAME@VERSION` arguments grant exact name/version pairs
  in the fixed repository. A bare name grants only version 1, never all or latest
  versions. Registering another version does not expand an existing grant or
  let a worker select an ungranted weaker schema. Grants require a selected
  catalog containing the exact pair; discovery marks `emitAllowed` separately.
  Without a selected catalog the read-only discovery tool lists retained known
  versions, all non-emittable. There is no worker registration
  tool and no permission inferred from a payload or prompt.
- `signals.emit` takes name, version, payload, and a required idempotency key.
  Sender workspace/thread, record ID, sequence, and time are not tool arguments.
  Codex 0.153.4 already attaches `_meta.threadId` to both model MCP calls and
  native `mcpServer/tool/call`; CoCo resolves that ID to an existing bound
  workspace and verifies repository and availability. Missing/unbound/cross-repo
  origin fails closed. No exact native turn ID is inferred from current status.
- This is capability shaping for the existing trusted same-user local control
  plane, not protection from a malicious process running as that user. Native
  request metadata is not a signed attestation. A remote/multi-user deployment
  would need an authenticated boundary; do not advertise one here.
- There is no new automatic MCP injection or global Codex config mutation.
  The operator adds this opt-in CoCo MCP entry to the existing named profile.
  The native start/resume/fork config mechanisms carry it. A context fork has
  its own thread ID and resolves to its own CoCo workspace; inheriting a server
  scoped to a different repository does not grant cross-repo emission.

## Persistence and limits

Use dedicated type and signal tables in the existing SQLite store. Do not
resume mirroring native Codex history/status into the legacy event tables.
Accepted responses follow transaction commit. Idempotency is scoped to sender
workspace plus key; a repeated identical request returns the original record,
while different content conflicts. That guarantee lasts while its record is
retained, not forever after retention expires.

The MVP retains the newest 10,000 records globally, without an age expiry.
Payloads and schemas are at most 16 KiB each; JSON nesting is bounded. Each
repository can retain at most 128 type versions; one workspace may accept
at most 10 new signals per second. Duplicated accepted retries do not consume
that rate allowance. Schema references are not supported in this first inline
schema contract; neither file nor HTTP retrieval is enabled in the validator.
Format annotations do not implicitly enable format assertions. The selected
directory has at most 128 direct schema files and 1024 entries total; nested
directories and non-JSON files are ignored, while JSON-named non-regular files
(including symlinks) fail loading. Filename versions are canonical positive
integers without leading zeros. Files themselves are bounded before JSON parse.

Validation uses `jsonschema` 0.55.1 with default features disabled. Its
`fluent-uri` dependency uses `borrow-or-share` 0.2.4 under
[MIT No Attribution](https://spdx.org/licenses/MIT-0.html). The exact-version
license exception is recorded in `deny.toml`; no advisory or general license
check is disabled. Payload errors identify the instance path and schema rule
without echoing the offending value; the model can inspect discovery and retry.

Records preserve repository/workspace IDs, workspace name at emission, native
thread ID, type version, payload, and service-generated identity/time. Closing
a workspace rejects new emissions but preserves history; deleting the workspace
also preserves records until retention removes them. Reusing a name does not
reuse its UUID or inherited signal history. Payloads can contain sensitive data:
they are deliberately persisted locally and are not copied into audit logs.

## Reading and follow

`coco signal list [workspace]` and `signal ls` share the normal CLI scope rules:
omission reads the current/explicit repository, `-a` reads all repositories
without a workspace target, and `-g` resolves exactly one name globally.
Full workspace UUIDs select global history even outside a repository and after
workspace deletion. Local names never silently fall back to global lookup;
duplicates fail through the existing workspace resolver. `--name` remains the
independent signal-name filter. The initial draft `--workspace`/`-w` syntax is
removed rather than kept as an unreleased competing form.

Use MCP `signals.types` for definitions and CLI/MCP signal lists for
published occurrences. No `signal status` is added: a historical claim does not
prove present workspace state, and inferring supersession belongs to consumers.

Each reader has an independent opaque cursor tied to this database stream and
its repository/workspace/type filter. Pages contain at most 100 records, a next
cursor, and `hasMore`. The first read without a cursor starts at the oldest
retained match. Reusing a cursor with another filter/database is an error.
If retention passed a supplied cursor, report an explicit expired-cursor error
instead of silently skipping data. No consumer acknowledges or consumes another
consumer's records.

CLI follow drains pages and polls until interrupted. JSON follow emits one
page per line, including the resumable cursor; it does not stream Codex chat
messages. Reconnect/restart resumes with the saved cursor. Delivery is replayable,
not a promise of exactly-once external effects; consumers deduplicate by record ID.

## Evidence and next boundaries

The combined process scenario covers two repositories with identical names,
real CLI and MCP clients, opted-in continuation, decisions, exact remote attach,
restart, and operation retry. Its worker is deliberately fake. A separate
model-free test against installed `codex-cli 0.153.4` proves independent MCP
configuration under native start/fork/resume and calls the real CoCo MCP process.
It verifies native metadata overriding a supplied incorrect thread claim,
actual accepted emissions, scoped reads, original-record retry after restart,
named-profile restoration through CoCo, rejection of an unbound native fork,
and correct sender identity for a separate CoCo context fork.
Neither proves model behavior, automated consumers, or a public-release gate.

Source audit: matching local upstream tag `rust-v0.153.4`,
`core/src/mcp_tool_call.rs::with_mcp_tool_call_ids_meta` and
`app-server/src/request_processors/mcp_processor.rs::with_mcp_tool_call_thread_id_meta`.
The [official App Server documentation](https://learn.chatgpt.com/docs/app-server)
and [MCP configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
describe the native interfaces; the installed-version tests define our evidence.
