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

On Linux with cgroup v2 and a usable user-systemd manager, this is now an
OS-owned process containment and accounting boundary. A workspace may opt into
a durable memory, CPU, and task policy; no limit is enabled by default. Hosts
without that capability retain the best-effort process-tree observation
backend and reject non-empty limit policies rather than silently running them
unrestricted. Windows Job Objects, containers, host-pressure admission, an
aggregate limit pool, and idle-runtime retirement remain separate future
contracts.

The default is `exec-server`. `COCO_WORKSPACE_EXECUTION=shared` disables the
per-workspace executors and retains the earlier shared-host execution behavior.
That fallback is intended for a Codex compatibility failure or diagnosis; it
also disables workspace resource observations.

## Authority and topology

```text
CLI / MCP / TUI relay ----> cocod ----> Codex App Server (control plane)
                                         |              |
                              environment/add   environment/add
                                         |              |
systemd user manager                    |              |
  `- CoCo instance parent slice         |              |
       `- workspace pool slice          |              |
            |- workspace A scope <------+              |
            `- workspace B scope <---------------------+
```

The daemon and shared App Server remain outside the instance workspace pool.
Its
memory, CPU, model networking, local control work, and any other shared
children cannot be attributed to one workspace and must never be included in
a per-workspace figure. Only processes descended from the selected executor
or charged to its selected cgroup are attributed to that workspace.

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
  stops every tracked executor scope before stopping the shared App Server.
- The runtime registry is intentionally generation-local. Workspace and
  thread bindings remain durable in SQLite/Codex; executor URLs, PIDs, samples,
  and CPU counters are never persisted.
- Desired workspace limits and their monotonic revision are durable. They are
  loaded before any executor can start, while the applied policy snapshot
  remains generation-local evidence from the live runtime.

The process-tree fallback uses Tokio's kill-on-drop behavior. The cgroup-v2
backend instead stops the complete systemd scope on ordinary shutdown. A hard,
uncatchable daemon death can leave a scope running because upstream 0.154.0
does not permit `--exit-on-stdin-close` with the local listener CoCo needs. The
next daemon generation removes only stale scopes in its own opaque instance
namespace before starting its App Server. Until that restart, the scope may
continue running. The process-tree fallback still cannot prove every detached
descendant is gone.

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

Detailed `workspace.get` and `workspace.list` responses include an optional
ephemeral `runtimeResources` object when the caller requests it. Human
`coco status <workspace> --resources`, collection status with `--resources`,
and their follow modes render the same observation compactly. The default
collection view omits it to avoid an unrequested host sweep.

The object distinguishes:

- backend (`exec_server`);
- state (`inactive`, `running`, or `exited`);
- evidence scope (`cgroup_v2`, `process_tree`, or `root_process`);
- root PID; and
- optional process/task counts, resident or cgroup-charged memory, interval and
  cumulative CPU use, controller events, opaque cgroup unit, and sample time.

The preferred Linux backend reads `memory.current`, `cpu.stat`,
`cgroup.procs`, `pids.current`, and controller events from the workspace's
verified cgroup. The fallback reads `/proc` on demand, constructs the
executor's descendant tree, sums `VmRSS`, and derives interval CPU use from
process and host tick deltas. The first successful observation has no CPU
percentage because one earlier sample is required. Processes can start, exit,
or re-parent during a fallback scan, so that result is explicitly best effort.
Other operating systems currently report runtime state and root PID without
claiming measurements they cannot attribute.

Resource observations are not themselves configured-policy state, billing
data, durable history, or a complete account of Codex. `workspace.limits.get`
is the authoritative desired-policy and applied-policy snapshot view. Observations
exclude the daemon and shared App Server. The cgroup backend contains its
executor and descendants from launch; the fallback can miss processes that
escape observed ancestry.

## Linux cgroup-v2 containment

This implemented contract deliberately keeps resource containment below the
existing workspace-executor boundary: thread ownership and App Server routing
do not change.

### Launch boundary and ownership

On a Linux host with a usable cgroup-v2 user manager, CoCo launches each
workspace executor through a transient `systemd-run --user --scope` unit. The
scope must be created before `codex exec-server` starts so every child inherits
the boundary. Moving a live executor into a cgroup after spawn is rejected as
racy because children created before the move could escape.

Direct cgroup-filesystem ownership is not the default. systemd is the cgroup
tree's single writer on these hosts; CoCo should use its transient-unit and
runtime-property interfaces unless cocod is explicitly given a delegated
subtree in a later deployment mode. A container is also unnecessary for this
resource-only slice and would add filesystem, image, networking, and Git
mounting contracts that are independent of CPU and memory accounting.

The scope includes the exec server and all tool descendants from process
creation. Its accounting is therefore the cost of the complete workspace
execution boundary. The shared Codex App Server remains outside every
workspace scope and cannot be assigned truthfully to one workspace.

Each CoCo data directory gets a stable opaque instance namespace derived from
its canonical path. Each workspace scope combines that namespace with the
workspace's stable ID-derived hash; neither the public workspace name nor a
repository path enters a systemd unit name. Workspace scopes live in an
instance-specific workspace slice below an instance parent slice even though
the parent and pool do not currently have aggregate limits. This preserves an
aggregate worker boundary for a later resource pool without changing every
scope's ownership model. It does not yet cap the daemon or App Server; that
would require launching the control plane inside a sibling scope under the
same instance parent.

### Lifecycle and crash recovery

`workspace.create` remains lazy and creates no scope. The first operation that
needs execution starts the transient scope, captures the exec-server endpoint
through the existing bounded stdout contract, resolves and verifies the
scope's `ControlGroup`, and only then registers the environment with the App
Server. The cgroup path and sampling counters are generation-local runtime
state; they are not workspace records.

Stopping or closing a runtime must stop the exact systemd scope and wait for
its supervisor to exit. Killing only the `systemd-run` child is insufficient:
systemd owns the scope and descendants may remain alive after their original
parent exits. Every partial startup path must therefore own a cleanup guard
that stops the deterministic unit before returning an error.

A hard cocod failure can leave a valid scope running, while its ephemeral
exec-server URL is lost. Adoption is not safe without that URL. After taking
the existing per-data-directory daemon lock, startup must enumerate and stop
only scopes in that instance's opaque namespace before starting the shared App
Server. A later activation also refuses to replace a same-name scope until its
old unit is demonstrably inactive. CoCo must never sweep another data
directory's namespace or a general `coco-*` pattern.

### Accounting contract

The kernel cgroup counters replace ancestry scanning when the systemd scope is
active:

- `memory.current` is the total memory charged to the workspace cgroup. It is
  the value relevant to cgroup enforcement and must not be called RSS or PSS;
- `cpu.stat` `usage_usec`, sampled against monotonic elapsed time, yields CPU
  percentage where one fully used core is 100 percent;
- `cgroup.procs` supplies the process count while `pids.current` supplies the
  task/thread count used by `TasksMax`; and
- `memory.events`, `pids.events`, and CPU throttling counters provide evidence
  that a configured boundary was reached.

The runtime response adds `cgroup_v2` as an evidence scope. It keeps
fallback summed RSS and cgroup-charged memory in distinct JSON fields rather
than pretending that they are the same measurement. Human resource output can
label the common column `MEMORY`, but the JSON contract must identify whether
that value came from summed process RSS or `memory.current`. Diagnostic unit,
limit, and event details remain opt-in resource data rather than default status
noise. All samples remain ephemeral; durable historical telemetry is a
separate retention decision.

### Capability and fallback policy

The backend has three internal selection outcomes through
`COCO_WORKSPACE_CONTAINMENT`:

- `auto`, the default, uses a systemd user scope only after confirming Linux,
  a unified cgroup-v2 hierarchy, the required controllers, and a reachable
  user manager;
- `systemd`, an explicit diagnostic/operational override, requires that
  backend and fails with an actionable error when it is unavailable; and
- `process-tree`, an explicit compatibility fallback, preserves today's
  best-effort observation without claiming containment.

An `auto` capability miss may select the process-tree backend before an
executor starts. Once systemd launch has begun, unexpected setup, identity, or
cleanup failures are errors rather than permission to silently weaken the
same activation. Non-Linux systems retain their present limited observation
until a native boundary such as Windows Job Objects is designed. The existing
`COCO_WORKSPACE_EXECUTION=shared` escape hatch remains separate and has no
per-workspace containment.

No hard limit is enabled by default. Once a workspace requests a limit,
failure to establish the enforcing backend must reject activation instead of
running it unrestricted.

The policy vocabulary and capability response are backend-neutral. Linux
currently advertises `systemd_cgroup_v2`; an unavailable or process-tree
backend advertises no enforceable fields. A future Windows Job Object backend
may implement only fields with equivalent semantics, and a macOS backend must
not claim aggregate enforcement it cannot prove. An OCI backend remains an
optional strict-isolation mode rather than the transparent host-native
default. This capability boundary is the intended extension point; the durable
policy contains no systemd property names.

### Workspace resource policy

The implemented version-one workspace policy is durable, revisioned, and
independent of Codex profiles. It exposes:

- `memoryHighBytes`, the primary memory pressure and reclaim boundary;
- `memoryMaxBytes`, a last-resort hard memory ceiling;
- `cpuMaxMillicores`, total CPU bandwidth where 1,000 means one fully used
  logical core;
- `cpuWeight`, relative CPU share under contention from 1 through 10,000; and
- `tasksMax`, the total kernel task/thread ceiling.

The CLI exposes the same semantics through `coco limits show`, `set`, and
`reset`. Human byte units are parsed at the CLI boundary; the daemon protocol
and store retain exact integers. A partial set request uses typed set/clear
operations so an omitted field and an explicit removal cannot be confused.
`MemoryLow` is deferred until aggregate-pool and admission semantics exist;
giving a workspace protection without defining the constrained parent would
be misleading.

The coordinator validates the complete resulting policy, resolves backend
capabilities, and serializes mutations with the repository lifecycle lock. It
then writes a compare-and-set revision and asks the runtime boundary to apply
that exact snapshot. A failed application restores durable intent with a new
monotonic revision and restores the runtime when possible. If recovery cannot
be completed, CoCo stops the runtime on a best-effort basis and reports a
dedicated incomplete-update error. Activation independently reloads and checks
the durable policy, so a daemon restart or explicit shared/process-tree
fallback cannot silently weaken a requested guarantee.

On a live Linux scope, supported changes use
`systemctl --user set-property --runtime` and are verified against the kernel
cgroup files before their revision is reported as applied. Lowering
`MemoryMax` below `memory.current` is rejected instead of risking an implicit
OOM kill. systemd currently acknowledges removal of a live `CPUQuota` without
reliably changing `cpu.max`, matching
[systemd issue 14917](https://github.com/systemd/systemd/issues/14917); CoCo
therefore stages that particular transition for the next runtime start. It
never kills a running workspace merely to make a settings update immediate.
The desired and applied snapshots remain distinct, and human output reports
both when a restart is pending.

### Later shared pool

`MemoryMin` remains excluded because overcommitted hard protection can move OOM
failure elsewhere in the host.

A later aggregate worker pool belongs on the workspace slice. If the control
plane is also launched under the instance parent, parent memory and CPU
boundaries can cap all of CoCo while child `MemoryLow` and `CPUWeight` values
express workspace protection and shares. CPU already redistributes naturally
under contention; a custom allocator is unnecessary for that case. Dynamic
memory allocation, admission based on host pressure, and promised minimums are
scheduler policy rather than cgroup plumbing and remain a later design.

Global defaults belong to future CoCo configuration, not Codex profiles. A
dynamic allocator may persist desired floors and weights, but never treat its
momentary allocation as user intent.

### Implementation slices and verification

The bounded implementation state and order is:

1. Implemented: a focused daemon execution-containment module,
   instance-scoped unit identity, capability selection, stale-scope cleanup,
   scope-aware launch, and whole-scope shutdown.
2. Implemented: cgroup-v2 accounting and event files, typed runtime JSON
   that does not conflate memory metrics, and process-tree fallback.
3. Implemented: the portable per-workspace capability/policy contract,
   revisioned SQLite intent, CLI and local RPC controls, verified launch/live
   enforcement, staged CPU-cap removal, rollback, and fail-closed activation.
4. Next policy work may evaluate an aggregate pool, admission control, and
   native non-Linux backends without changing the current workspace policy
   vocabulary. Automatic idle retirement stays lower priority until aggregate
   measurements show that it is needed.

Current unit coverage includes opaque unit identity, strict stale-unit and
cgroup-path validation, policy validation and revision conflicts, file
parsers, CPU deltas, coordinator rollback, capability failure, and staged
restart behavior. The opt-in Linux/systemd tests prove both that a child and
its descendant are born in the selected scope and that configured properties,
live updates, safe clearing, kernel-file verification, and whole-scope cleanup
work on the host. The real Codex suite proves environment registration,
truthful cgroup measurements, workspace-pool placement, and daemon-shutdown
cleanup.

Primary contracts for this design are the Linux kernel's
[cgroup-v2 interface](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html),
systemd's [`systemd-run`](https://github.com/systemd/systemd/blob/main/man/systemd-run.xml)
and [resource-control](https://github.com/systemd/systemd/blob/main/man/systemd.resource-control.xml)
manuals, and systemd's
[single-writer/delegation contract](https://github.com/systemd/systemd/blob/main/docs/CONTROL_GROUP_INTERFACE.md).

## Idle-runtime retirement priority

One real idle Codex 0.154.0 workspace runtime was observed at approximately
14 MiB of cgroup-charged memory, one process, and no measurable interval CPU.
That is a useful local data point, not a universal benchmark: model version,
loaded configuration, allocator state, plugins, and background processes can
change the footprint. It also must not be compared directly with an earlier
process-RSS/PSS probe because `memory.current` measures all memory charged to
the workspace cgroup.

The current decision is to keep automatic idle retirement out of the near-term
work. Its added lifecycle complexity is not justified by this observed idle
cost. An explicit runtime-stop command and a configurable automatic policy
remain valid future controls if many simultaneous workspaces or host-pressure
measurements demonstrate a need.

`Ready` alone can never be the automatic-stop predicate. A safe policy would
also have to exclude active turns, pending decisions, attached TUI relays, and
workspace descendants or background commands that are expected to continue.
Stopping remains safe for durable workspace/thread recovery only after those
conditions hold; a later `send` or `jump` may then lazily start a new executor.

## Token usage and cost accounting

The released App Server assessment and staged recommendation now live in
[Workspace token usage and billing estimates](workspace-usage.md). Native
thread notifications are the preferred raw-token source; backend per-thread
billing is optional and nullable. The implemented passive `coco usage` surface
keeps those model-side values separate from cgroup resource observations and
configured runtime limits.

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
