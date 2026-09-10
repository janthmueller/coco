---
type: Engineering Decision
title: CoCo hooks and guards
description: Defines the boundary between native Codex hooks, CoCo-owned durable reactions, and synchronous retirement guards.
tags: [hooks, signals, persistence, security, codex]
status: stable
---

# CoCo hooks and guards

## Boundary

CoCo has two distinct extension mechanisms:

- Native Codex hooks observe or influence activity inside one Codex session.
  They own prompt, tool, permission, compaction, subagent, stop, interrupt, and
  session lifecycle events. CoCo must not mirror that vocabulary or become a
  second policy engine for the model loop.
- CoCo hooks react to facts CoCo owns across clients and daemon restarts:
  accepted agent signals and committed workspace lifecycle changes. They are
  asynchronous effects after the source fact succeeds. A failed hook never
  rolls back or changes the workspace or signal.
- CoCo guards synchronously decide whether an already checked
  `workspace.close` or `workspace.delete` request may cross its first effect
  boundary. They are command-backed policy checks, not durable events: they do
  not mutate requests, retry, or run during a dry-run or saga recovery.

The first CoCo hook and guard target is a trusted local command. HTTP delivery,
MCP-tool targets, model wakeup, workspace chaining, ticket semantics, and a
complete copy of App Server notifications remain outside this contract.

The selected `codex-cli 0.154.0` was checked both in a matching source clone and
through an ignored real-process compatibility test. Codex reads native hooks
from its own `hooks.json`; an App Server thread executes a pending
`SessionStart` hook when its first ordinary turn starts. Merely calling
`thread/start`, or using the separate `thread/shellCommand` path, is not that
execution boundary. The isolated proof uses Codex's test-only trust bypass to
avoid an interactive prompt; production CoCo does not set it. The
[official Codex hooks documentation](https://learn.chatgpt.com/docs/hooks)
remains authoritative for native hook configuration and behavior.

## Configuration

`cocod` loads one hook and guard configuration when it starts. The path is:

1. `COCO_HOOKS_PATH`, when explicitly set;
2. `$XDG_CONFIG_HOME/coco/hooks.json`;
3. `$HOME/.config/coco/hooks.json`; or
4. the CoCo data directory only when no home configuration root is available.

An absent file means no CoCo hooks or guards. `coco hook validate` checks the
whole file without contacting the daemon or executing commands. `coco hook
reload` loads and validates a replacement before atomically swapping it into
the daemon; failure leaves the previous snapshot active. The file is not
watched. Deliveries retain the hash of the hook definition that matched when
their event committed. The hash also covers the canonical configuration
directory, which is the command working directory. A queued delivery is
cancelled rather than executed if that exact definition is absent after a
reload or restart. A delivery or guard already executing owns its cloned
definition until that invocation ends.

The version-1 document is:

```json
{
  "version": 1,
  "hooks": [
    {
      "id": "review-notify",
      "event": "signal.emitted",
      "signal": "review.requested@1",
      "command": ["/absolute/path/to/notify-review", "--stdin"],
      "timeoutSeconds": 30,
      "maxAttempts": 3
    }
  ],
  "guards": [
    {
      "id": "protect-delete",
      "action": "workspace.delete",
      "command": ["/absolute/path/to/check-delete"],
      "timeoutSeconds": 5,
      "onError": "deny"
    }
  ]
}
```

Hook IDs are unique lowercase identifiers of at most 64 bytes. `signal` is an
optional exact `NAME@VERSION` filter and is valid only for `signal.emitted`.
Omitting it matches every emitted signal. Hook `timeoutSeconds` defaults to 30
and is bounded to 1–300; `maxAttempts` defaults to 3 and is bounded to 1–5.
Guard `timeoutSeconds` defaults to 5 and is bounded to 1–30. Every guard must
choose `onError: "allow"` or `"deny"`; silent default failure policy is
forbidden.

The configuration is at most 64 KiB, contains at most 64 hooks and 64 guards,
and uses one shared ID namespace. It must be a regular non-symlink file and on
Unix must not be group- or world-writable. Commands contain 1–33 non-empty
strings totalling at most 16 KiB. The first string must be an absolute path to
an executable regular file. The executable contents are not copied or pinned;
an operator who changes that file changes the program used for later attempts
or checks.

## Event contract

The hook command receives exactly one compact JSON object on standard input:

```json
{
  "schemaVersion": 1,
  "id": "0199...",
  "kind": "signal.emitted",
  "occurredAtMs": 1789056000000,
  "repository": {
    "id": "...",
    "name": "project",
    "path": "/absolute/path/to/project"
  },
  "workspace": {
    "id": "...",
    "name": "review/widget",
    "threadId": "...",
    "worktreePath": "/absolute/path/to/worktree",
    "branchName": "coco/review/widget"
  },
  "data": {
    "signalId": "...",
    "name": "review.requested",
    "version": 1,
    "payload": { "pr": 42 }
  }
}
```

Consumers must use the stable event `id` for idempotency and ignore unknown
object members. Version 1 emits only:

| Kind | Commit point | `data` |
| --- | --- | --- |
| `signal.emitted` | a new, non-idempotent-replay signal is accepted | `signalId`, `name`, `version`, `payload` |
| `workspace.created` | the Git worktree and ready workspace record commit | `worktreeMode`, `baseSha` |
| `workspace.closed` | the open-to-closed transition commits | normal disposition fields, or `recovered: true` after saga recovery |
| `workspace.reopened` | the closed-to-open transition commits | empty object |
| `workspace.deleted` | the closed workspace record is removed | selected thread/branch deletion plus optional recovery marker |

No event is created when no loaded hook matches it. An idempotent retry of an
already accepted signal does not create a second event or delivery.

## Guard contract

Version 1 permits guards only for `workspace.close` and `workspace.delete`.
The coordinator first resolves the exact workspace, constructs and validates
its current retirement plan, rejects built-in blockers, and verifies any
client-supplied expected plan. Immediately before beginning the stored saga or
calling Git/Codex, it evaluates matching guards in stable ID order while still
holding the repository operation lock. A dry-run returns before this point.

Each command receives one compact JSON request on stdin with `schemaVersion`,
an opaque request `id`, `action`, `requestedAtMs`, repository and workspace
snapshots, and the checked plan plus selected retirement flags under `data`.
It must exit zero and return exactly one JSON object on stdout:

```json
{"decision":"allow"}
```

or:

```json
{"decision":"deny","reason":"The branch is not merged."}
```

An allow response must omit `reason`. A deny reason is required, sanitized,
and bounded to 512 characters. Guard stdout is bounded to 8 KiB and stderr is
discarded. Unknown output fields, invalid JSON, spawn failures, non-zero exits,
and timeouts are failures. The guard's explicit `onError` policy decides
whether CoCo continues or fails closed; an explicit denial always stops
immediately. Guards never rewrite a request and never retry. The exact action
can still fail after all guards pass, and an external invariant can change
between a check and the effect, so this is not a transactional authorization
service.

Recovery does not rerun guards. Reaching a stored `closing` or `deleting`
state proves the original request crossed the guard boundary; repeating an
external policy check during compensation or convergence could make recovery
impossible. Guard allow/fail-open outcomes are logged by the daemon, while a
deny or fail-closed result is returned through stable public error codes and
structured guard metadata. A durable guard-history surface is not part of the
first guard slice.

## Delivery contract

The source mutation, event, and all matching pending deliveries commit in one
SQLite transaction. This is a narrow transactional outbox, not the legacy
normalized Codex event log. The daemon claims pending rows transactionally and
runs at most four hook commands concurrently. Deliveries for one hook ID remain
in commit order, including across retry delays; different hook IDs can run in
parallel. It invokes the configured argument array directly without a shell,
uses the directory containing `hooks.json` as the working directory, clears
the inherited environment, and restores only `PATH` when available. Standard
output and standard error are discarded.

A zero exit status succeeds. Spawn errors, non-zero exits, and timeouts retry
after 1, 2, 4, then 8 seconds until `maxAttempts`; the final failure remains
visible. A daemon restart moves an interrupted `running` delivery back to
`pending`. Shutdown aborts child tasks and their processes, so recovery may
deliver the same event again. The guarantee is therefore durable
**at-least-once** delivery, never exactly-once external effects. Timeout and
shutdown own the direct hook process only; the trusted command must not
daemonize or leave unmanaged descendants.

Terminal event groups retain the newest 10,000 groups. Pending and running
groups are never pruned. The current CLI exposes loaded definitions and the
newest 1–100 delivery summaries through `coco hook list` and
`coco hook history`; it deliberately hides command arguments and event
payloads. Delivery errors are control-character-sanitized and bounded before
storage and presentation.

## Security and future extension

Hook/guard configuration and executables are trusted operator input. The
restricted environment reduces accidental credential leakage but is not a
same-user sandbox: the executable can still read anything its operating-system
user can read and can launch other programs through absolute paths. CoCo cannot
enforce that a guard is side-effect free. Secrets must not be placed in the
configuration or command input solely for automation. A later secret provider
needs an explicit redaction and lifecycle contract.

Do not add native Codex events to the durable outbox merely for completeness.
Add another CoCo event only when its commit boundary, payload, retention,
consumer, and failure semantics are all explicit. A future live subscription
can layer replay plus bounded notification over this store without changing
the command delivery guarantee. The first slice does not detect recursion when
a trusted hook command invokes a CoCo action that produces the same event;
operators must avoid such loops until a concrete chaining contract exists.
