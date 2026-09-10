---
type: Engineering Decision
title: Per-workspace Codex execution runtime
description: Defines the shared control plane, lazy workspace executors, environment routing, resource observations, lifecycle, and current containment limits.
tags: [architecture, codex, app-server, exec-server, workspaces, resources, isolation]
status: stable
---

# Per-workspace Codex execution runtime

## Decision summary

CoCo keeps one daemon-owned Codex App Server as its shared control plane and
starts one `codex exec-server` lazily for each workspace that needs execution.
The App Server continues to own threads, turns, model traffic, notifications,
and server requests. The workspace executor is the process and filesystem
boundary for normal agent tool execution.

This is an attribution boundary, not yet a hard containment boundary. CoCo can
observe the executor process tree on Linux, but it does not currently impose
CPU, memory, or process limits. Containers, Linux cgroup v2, Windows Job
Objects, host-pressure admission, and idle-runtime retirement remain separate
future contracts.

The default is `exec-server`. `COCO_WORKSPACE_EXECUTION=shared` disables the
per-workspace executors and retains the earlier shared-host execution behavior.
That fallback is intended for a Codex compatibility failure or diagnosis; it
also disables workspace resource observations.

## Authority and topology

```text
CLI / MCP / TUI relay
          |
          v
        cocod
          |
          +---- one Codex App Server (control plane)
                         |
                         +---- environment/add ---- workspace A exec-server
                         |
                         +---- environment/add ---- workspace B exec-server
```

The shared App Server remains outside every workspace executor tree. Its
memory, CPU, model networking, local control work, and any other shared
children cannot be attributed to one workspace and must never be included in
a per-workspace figure. Only processes descended from the selected executor
are attributed to that workspace.

The executor listens on an ephemeral loopback WebSocket port. Its URL remains
in daemon memory and is registered with the App Server under a stable opaque
environment ID derived from the CoCo workspace ID. The public workspace name
is not embedded in that identifier. The current upstream local exec-server
listener has no CoCo capability-token handshake; the unpublished ephemeral
endpoint and single-local-user deployment assumption reduce exposure but do
not make it a multi-user security boundary.

## Lifecycle

- `workspace.create` remains Git-only and starts no executor.
- The first `send`, inherited-context materialization, or `jump` starts the
  workspace executor on demand and registers it through `environment/add`.
- Repeated activation reuses a live executor only when its canonical worktree
  path still matches. An exited process or changed path is stopped and
  replaced under the same stable environment ID.
- Passive collection reads do not start executors. Detailed workspace status
  reports `inactive` after creation or daemon restart until an activating
  operation needs that workspace.
- Closing a workspace stops its tracked executor before Git removes the
  worktree. Reopening restores the worktree but remains lazy. Daemon shutdown
  stops every tracked executor before stopping the shared App Server.
- The runtime registry is intentionally generation-local. Workspace and
  thread bindings remain durable in SQLite/Codex; executor URLs, PIDs, samples,
  and CPU counters are never persisted.

The child uses Tokio's kill-on-drop behavior and the ordinary shutdown path is
covered by real-process tests. Upstream 0.154.0 only permits
`--exit-on-stdin-close` with its remote-registration mode, not with the local
`--listen` mode CoCo needs. A hard, uncatchable daemon death can therefore
orphan an executor. CoCo also does not yet own an OS process group or job that
can prove every detached descendant is gone. Those are explicit containment
gaps, not properties inferred from parent-process traversal.

## Environment routing

Codex 0.154.0 exposes environment selection only on `thread/start` and
`turn/start`:

- a fresh thread is started with exactly its workspace environment;
- every CoCo-started turn repeats the workspace environment selection, making
  it sticky for subsequent actions in that loaded thread; and
- the authenticated one-use `jump` relay replaces environment selection on
  downstream `thread/start` and `turn/start`, so the official TUI cannot route
  an ordinary turn to a different executor.

`thread/resume` and `thread/fork` do not accept an `environments` field in this
release. Codex also deliberately does not restore selected environments from
persisted rollout history when it loads or forks a thread. CoCo therefore
registers the destination executor before either request and selects it again
on the next ordinary `turn/start`. This is sufficient for normal `send` and
normal TUI prompts, but it leaves narrower pre-turn limitations:

- child compaction requested immediately after `thread/fork` executes before
  CoCo can select the child's executor;
- a review or compaction requested immediately after a fresh resume can use
  Codex's local default until an ordinary turn reselects the workspace
  environment; and
- Codex's interactive `!command` path calls host-local
  `thread/shellCommand`; upstream deliberately does not route that method to a
  remote-only selected environment, so it is unsupported as a reliable
  workspace shell shortcut.

These limitations must remain documented and covered by compatibility review
until upstream adds a setting/update or resume/fork environment field. CoCo
must not synthesize a hidden model turn merely to establish stickiness.

## Resource observation

Detailed `workspace.get` responses include an optional ephemeral
`runtimeResources` object. Human `coco status <workspace>` and its follow mode
render the same observation compactly. Collection views intentionally omit it
to avoid an unbounded host sweep.

The object distinguishes:

- backend (`exec_server`);
- state (`inactive`, `running`, or `exited`);
- evidence scope (`process_tree` on Linux, otherwise `root_process`);
- root PID; and
- optional process count, resident memory bytes, CPU percentage, and sample
  time.

Linux reads `/proc` on demand, constructs the executor's descendant tree,
sums `VmRSS`, and derives interval CPU use from process and host tick deltas.
The first successful observation has no CPU percentage because one earlier
sample is required. Processes can start, exit, or re-parent during a scan, so
the result is explicitly best effort. Other operating systems currently
report runtime state and root PID without claiming measurements they cannot
attribute.

Resource observations are not quota evidence, billing data, durable history,
or a complete account of Codex. In particular they exclude the shared App
Server and processes that escape the observed ancestry. A future hard-limit
backend must launch the executor inside the enforcing cgroup/Job Object or
container from process creation; moving an already running PID later is not a
sufficient child-inheritance contract.

## Compatibility evidence

The selected baseline is exactly `codex-cli 0.154.0`. The opt-in real-process
suite proves environment registration and readiness, fresh thread selection,
distinct executor PIDs for two simultaneously active workspaces, on-demand
resource reporting, close-time cleanup, daemon-shutdown cleanup, and the
existing thread/restart lifecycle. Fake process tests use
`COCO_WORKSPACE_EXECUTION=shared` because their intentionally narrow fake App
Server does not implement the experimental environment protocol.

Compatibility errors must preserve the workspace and return an actionable
fallback instead of marking an unconfirmed turn dispatch. A different Codex
version is unsupported until this same behavioral suite passes; a schema file
or presence of an `exec-server` subcommand alone is insufficient evidence.
