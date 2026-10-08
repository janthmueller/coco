---
type: Engineering Decision
title: Client attachments and tmux integration
description: Defines the proposed generic client-presence model and an optional tmux adapter without coupling workspace execution to terminal panes.
tags: [architecture, clients, attachments, tmux, tui, status]
status: proposed
---

# Client attachments and tmux integration

## Status and intent

This is an accepted post-v0 direction, not implemented behavior and not a
requirement for the first stable `0.1.0` release. It records how CoCo may show
where a workspace is currently open and support tmux navigation without making
tmux part of the coordinator's execution model.

The first adapter may be a separately installable tmux plugin. The CoCo core
must model generic client attachments so the same boundary can later represent
other terminal, GUI, or remote clients without changing workspace semantics.

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

The exact wire names remain an implementation decision. The following rules
are part of the design:

- The lease ID remains the authority for renew and release. Integration
  metadata is descriptive and must never become authentication material.
- Attachments are generation-local and are not persisted in SQLite. Pane IDs,
  process IDs, and display labels are short-lived and can be reused.
- Registration occurs with the existing attach operation. Renewal preserves
  the metadata across sequential TUI reconnects, and explicit release or lease
  expiry removes it.
- The relation is workspace-to-many-attachments, not workspace-to-one-pane.
- Every metadata field is optional, length-bounded, control-character
  sanitized, and rendered as untrusted text. Locators are passed as argv data,
  never interpolated into a shell command.
- A tmux pane locator is unique only inside one tmux server. Navigation must
  use an opaque server identity together with the pane ID; a human label such
  as `dev:2.1` is display-only.

The ordinary attachment lease already prevents workspace retirement while its
client is live. Adding presentation metadata must not alter adoption,
reconnect, close, delete, or thread-ownership rules.

## tmux adapter

When `coco jump` runs inside tmux, it can detect the current pane from
`TMUX_PANE` and resolve a bounded human label with tmux's structured format
output, such as session, window, and pane. Failure to inspect tmux must be
non-fatal: the TUI still attaches without integration metadata.

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

The normal compact `coco status` view should not gain a permanently empty or
wide pane column. An explicit generic projection, provisionally
`coco status --clients`, may add a `CLIENTS` field:

```text
WORKSPACE   STATE     CLIENTS
fix/login   Working   tmux dev:2.1
api/ref     Ready     tmux dev:3.0, work:1.2
research    Ready     -
```

A targeted view may include both the readable label and exact local locator.
Machine output must expose a structured attachment collection rather than the
formatted cell. Final CLI naming belongs to the implementation review; it
must remain generic instead of introducing tmux-specific workspace status.

A tmux status bar refreshes frequently. The plugin must not run the full
workspace-status path on every redraw because that path may inspect native
Codex state and multiple repositories. Use pane-local tmux options for stable
identity and a bounded cached or dedicated read-only CoCo snapshot for live
state. That read path must have a short timeout, perform no thread load or
runtime start, and degrade to an empty or last-known segment when unavailable.
The daemon-owned live lease remains the source of truth; pane-local options
are only a rendering cache.

## Delivery slices

1. Add optional generic client metadata to attachment requests and the
   generation-local lease registry. Cover bounds, sanitization, reconnect,
   expiry, and multiple clients without adding tmux UI.
2. Add an opt-in structured attachment projection to status and prove that it
   neither loads threads nor starts executors.
3. Add the tmux adapter and plugin with status-line, popup, and exact local-pane
   navigation. Keep no-tmux behavior unchanged.
4. Reuse the generic contract for another client only when a real consumer
   requires it.

This work follows the pre-`0.1.0` responsiveness correction and bounded
diagnostics work. It is a suitable `0.1.x` integration rather than a stable
release blocker.

## Acceptance boundaries

- A non-tmux `coco jump` behaves exactly as it does today.
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
