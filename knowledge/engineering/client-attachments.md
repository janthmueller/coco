---
type: Engineering Decision
title: Client attachments and tmux integration
description: Defines generic live client presence and built-in optional tmux discovery, with a deferred plugin UI.
tags: [architecture, clients, attachments, tmux, tui, status]
status: active
---

# Client attachments and tmux integration

## Status and intent

The status-first slice is implemented locally after `0.2.0`: ordinary CLI
location discovery and live client projection. A separately installable tmux
plugin, pane navigation, and other client adapters remain deferred. These
additions do not make tmux part of the coordinator's execution model.

The first user-facing slice is built-in CLI support: `coco jump` records its
optional tmux location and `coco status` can show where a workspace is open.
Neither location discovery nor the status view requires a tmux plugin. A
separately installable plugin is secondary, for status-bar and navigation
convenience. The CoCo core must model generic client attachments so the same
boundary can later represent other terminal, GUI, or remote clients without
changing workspace semantics.

## Runtime and attachment are different facts

A workspace executor and an attached user interface are independent:

- `cocod` owns the workspace, Codex thread, and per-workspace execution
  runtime. Agent and tool work does not run inside a tmux pane.
- A client attachment says that a foreground client, initially the native
  Codex TUI launched through `coco jump`, is viewing or controlling that
  workspace.
- A workspace can be `working` with no attached client, and it can be `ready`
  while one or more clients remain attached.
- Several clients may attach to the same already-bound workspace at once.
  Closing one must not remove or hide the others.

Status must therefore never derive agent activity from pane presence or imply
that a pane owns the workspace runtime.

## Core attachment contract

Extend the existing renewable `workspace.attach` lease with optional,
bounded presentation metadata. The conceptual shape is:

```text
attachment lease
  workspace id
  lease id
  client kind       e.g. native_tui
  integration kind  e.g. tmux
  integration scope opaque tmux-server identity
  client locator    e.g. pane id %12
  display label     e.g. dev:2.1
```

`WorkspaceAttachParams.client` optionally supplies `ClientMetadata` (`kind`
and optional `integration`). `ClientIntegration` has `kind`, opaque `scope`,
`locator`, and optional `label`. Each live `WorkspaceClient` has an independent
public `id` plus `metadata`; this ID is never the renewable lease capability.
Old attach requests remain valid and project the client kind `unknown`.
The following rules are part of the implementation:

- The lease ID remains the authority for renew and release. Integration
  metadata is descriptive and must never become authentication material.
- Attachments are generation-local and are not persisted in SQLite. Pane IDs,
  process IDs, and display labels are short-lived and can be reused.
- Registration occurs with the existing attach operation. Renewal preserves
  the metadata across sequential TUI reconnects, and explicit release or lease
  expiry removes it.
- Presentation has its own expiry, advanced only by registration and an
  explicit client heartbeat. Adoption may pin or extend authority to finish
  safely, but its start, completion, or reconciliation cannot extend presence
  or make an expired client visible again. A late heartbeat under still-valid
  authority can restore the same presence ID; release or generation clear
  keeps the client hidden even while its adoption remains pinned.
- The relation is workspace-to-many-attachments, not workspace-to-one-pane.
- Client metadata and integration are optional groups; a supplied group has
  required bounded identifiers and an optional readable label. Kinds are
  lowercase ASCII tokens (32 bytes); scope/locator allow 128 bytes and labels
  96 bytes. Empty text, control characters, and bidi embedding/isolate markers
  are rejected before native or Git work.
  Rendering still treats metadata as untrusted text. Locators are passed as
  argv data, never interpolated into a shell command.
- A tmux pane locator is unique only inside one tmux server. Navigation must
  use an opaque server identity together with the pane ID; a human label such
  as `dev:2.1` is display-only.

The ordinary attachment lease already prevents workspace retirement while its
client is live. Adding presentation metadata must not alter adoption,
reconnect, close, delete, or thread-ownership rules.

## tmux adapter

When `coco jump` runs inside tmux, it detects the current pane from
`TMUX_PANE` and resolve a bounded human label with tmux's structured format
output, such as session, window, and pane. Failure to inspect tmux must be
non-fatal: the TUI still attaches without integration metadata.

The adapter parses `TMUX` from the right (socket paths may contain commas),
validates the absolute socket and numeric PID/pane identifiers, and uses
`tmux -N -S <socket> display-message -p -t <pane> <fixed-format>`. The query
has a 300 ms deadline, bounded output, literal argv, no stdin, and discarded
stderr. `-N` prevents starting a missing server. It verifies socket/PID/pane
against the launch environment before accepting the formatted location.
The format fields follow the [tmux manual](https://man.openbsd.org/tmux#FORMATS).
The opaque scope is a length-framed SHA-256 of socket path, PID, and server
start time. Exact same pane IDs in separate/restarted servers cannot collide.
Raw socket paths and PIDs are not included in the public client metadata.

Readable labels are launch-time snapshots: renaming sessions, renumbering
windows, or moving panes can make them stale until another jump. Renewal
preserves metadata without invoking tmux; `status` never starts a helper.
Exact pane/server identity remains separate from its display label. Refresh
or navigation belongs to a later adapter slice, not background discovery.

This location discovery belongs to the ordinary `jump` CLI and supplies the
direct status view. It is part of the first usable client-presence slice, not
deferred until plugin installation. The displayed location should identify
the session, window, and pane clearly, for example `dev:2.1`.

Only a client started through `coco jump`, or a future explicit registration
flow, may claim an exact workspace/thread attachment. A manually launched
`codex` process in a managed worktree can at most be associated heuristically
with the directory; CoCo must not present that guess as an exact attachment.

The plugin is an optional presentation and navigation adapter. Candidate
capabilities are:

- a tmux status-line segment such as `CoCo fix/login ● Working`;
- a popup listing workspaces that need attention;
- navigation to an already attached pane; and
- a key binding that starts `coco jump` for a selected workspace.

Installation through TPM and Nix/Home Manager can be supported independently.
Neither tmux nor the plugin becomes a dependency of `coco`, `cocod`, or the
workspace runtime.

## Status and polling

The normal compact `coco status` view is unchanged. Explicit
`coco status --clients` (`-c`) adds a `CLIENTS` column or targeted detail;
the short flag composes with other status switches, such as `-fartc`:

```text
WORKSPACE   STATE     CLIENTS
fix/login   Working   tmux dev:2.1
api/ref     Ready     tmux dev:3.0, work:1.2
research    Ready     -
```

A targeted view uses the same readable labels as the collection. Unavailable
locations show `TUI`, old callers show `Client`, absent clients show `—`, and
identical labels are counted (`TUI ×2`). Labels are sorted deterministically.
Schema-version-15 status JSON always includes a structured `clients` array,
even when empty; ordinary list/picker responses omit the projection. The
internal get/list opt-in is `includeClients`, default false for old callers.

This direct CLI view is the primary deliverable. It must support scoped and
cross-repository views, tree rendering, and live follow without changing the
underlying agent-state projection. A plugin is not needed to see tmux locations.

A tmux status bar refreshes frequently. The plugin must not run the full
workspace-status path on every redraw because that path may inspect native
Codex state and multiple repositories. Use pane-local tmux options for stable
identity and a bounded cached or dedicated read-only CoCo snapshot for live
state. That read path must have a short timeout, perform no thread load or
runtime start, and degrade to an empty or last-known segment when unavailable.
The daemon-owned live lease remains the source of truth; pane-local options
are only a rendering cache.

## Delivery slices

1. Implemented: optional generic metadata on attachment requests and the
   generation-local lease registry, with bounded text, separate presentation
   IDs, independent release/expiry, and adoption/reconciliation coverage.
2. Implemented: opt-in structured status projection. Tests prove that reading
   presence never loads threads or starts executors and preserves native state.
3. Implemented: bounded tmux discovery during ordinary `jump`, with readable
   session/window/pane labels in target/collection/tree/follow and structured
   JSON. Missing/broken/slow tmux falls back to generic TUI presence. Isolated
   native Codex 0.160.1 tests cover two real TUIs in separate tmux servers and
   independent closure; no plugin is required.
4. Optionally add the secondary tmux plugin with status-line, popup, and exact
   local-pane navigation, using the same client-presence contract.
5. Reuse the generic contract for another client only when a real consumer
   requires it.

This work follows the completed responsiveness/diagnostics and create-time
context-capture baseline. The status-first slice can ship independently of a
plugin; the release-channel decision remains separate.

## Acceptance boundaries

- A non-tmux `coco jump` behaves exactly as it does today.
- A TUI launched through `coco jump` in tmux has its location visible through
  the opt-in status view without installing or loading any plugin.
- One workspace can report zero, one, or several live client attachments.
- A relay reconnect retains the same attachment; normal exit, failed renewal,
  or expiry removes only that attachment.
- Attachments in separate tmux servers cannot collide even when their pane IDs
  match.
- A headless working workspace is not reported as attached, and an attached
  ready workspace is not reported as working merely because its TUI is open.
- A manually launched Codex TUI is not claimed as an exact CoCo attachment.
- Status-bar polling is bounded, passive, and cannot delay ordinary workspace
  control operations materially.
- No terminal identifier, label, or pane-local option is persisted as durable
  workspace state or trusted as authorization evidence.

## Non-goals

- running the agent or workspace executor inside tmux;
- requiring tmux for CoCo operation;
- treating a pane as the single owner of a workspace;
- process-tree guessing for arbitrary manually launched Codex clients;
- durable pane restoration after daemon or host restart; and
- remote or multi-user terminal navigation in the first adapter.
