---
type: Engineering Contract
title: Read-only installation and workspace diagnostics
description: Defines bounded doctor probes, diagnostic ownership, privacy, coverage, and exit behavior.
tags: [cli, diagnostics, safety, codex, git, runtime]
status: stable
---

# Read-only installation and workspace diagnostics

## Contract

`coco doctor [--json]` is a system-wide finite diagnostic report. It has no
workspace selector, scope flags, repair mode, follow mode, implicit daemon
startup, model turn, native resume, or executor activation. Running it from
outside a Git repository is supported. Normal output summarizes healthy
bindings and prints failures/warnings with actionable hints. JSON includes
individual checks, selected executable paths, and the actual running daemon's
metadata using the existing public schema wrapper.

Checks use stable IDs and `ok`, `warning`, `error`, or `skipped` severities.
Binding checks additionally carry the repository/workspace ID and a display
subject. Timeout evidence is explicit. Coverage is marked incomplete when
dependencies, deadlines, or collection caps prevent inspection. Expected
closed checkouts and prepared unbound threads are skipped normally. At least
one error produces exit code 1; warnings alone produce exit code 0. The CLI
entry point translates the already printed report's failure into an exit code
without appending a second stderr error to human or JSON output.

## Ownership

- CLI checks its own build, resolves `cocod`, selected Codex, and Git on its
  environment's search path, runs only their `--version` commands, and probes
  the existing local RPC socket. It never imports Store, Coordinator, Git, or
  Codex client internals. A missing daemon still yields installation evidence
  and explicit dependent skips. An older daemon without the new `doctor` RPC
  method yields an actionable update error, not a fallback mutation.
- The daemon composition root captures its executable, configured Codex
  executable, paths, execution mode, and containment capabilities. Codex's
  already completed `initialize.userAgent` supplies the running native build
  version; it is not inferred by executing a potentially updated binary.
- The coordinator reads native models and bound thread metadata through
  WorkerRuntime. Threads are located with `thread/read(includeTurns=false)`,
  including archived conversations. No subscription, config application,
  context fork, history replay, or resource-policy mutation occurs.
- Store uses an independent read-only connection to its captured backing file.
  It performs bounded SQLite schema, quick-integrity, foreign-key, count, and
  minimal binding reads in one read transaction. Its interrupt timer targets
  that diagnostic connection only; it cannot interrupt normal daemon writes.
  The in-memory test adapter uses a non-blocking lock instead. Store::open and
  migrations are never used by diagnostics.
- Git's diagnostic adapter runs only `rev-parse` metadata with optional locks
  disabled, sanitized Git environment, bounded capture, and a cancellable
  child. Dirty changes, hooks, filters, remote configuration, and Git history
  contents are not examined or changed. Registered-repository checks inspect
  identity only and do not require a first commit. Open-workspace checkout
  checks additionally require a resolved HEAD and validate branch/detached
  state. Configuration-path overrides are cleared as in ordinary Git
  execution; protected user configuration, including `safe.directory`, remains
  effective. Diagnostics never adds an ownership exception or bypass.

## Bounds and safety

Subprocess and native/metadata probes have three-second deadlines. Diagnostic
stdout capture is capped at 32 KiB; stderr is discarded. Failed/timed-out
children are killed with bounded reaping. Cancellation synchronously requests
termination and transfers the owned child handle to a one-second reap task
on the active runtime instead of relying solely on Tokio's best-effort orphan
queue. `kill_on_drop` remains the fallback if that runtime is unavailable or
shutting down; synchronous Drop cannot guarantee reaping after runtime exit.
SQLite uses a short busy timeout and an independent
interrupt timer. Ordinary filesystem calls still depend on the OS completing
its I/O; deadlines cannot undo kernel I/O already in progress.

The daemon report has one twenty-second budget, inspects at most 32 registered
repositories and 64 workspaces, and explicitly reports unfinished coverage.
The CLI separately bounds its installation probes, three-second health probe,
and twenty-three-second report request. Requests are never blindly retried.
Timeout/cancellation releases local correlation state but does not cancel a
native metadata lookup already accepted by Codex.

The report does not include token contents, endpoint URLs, conversations,
profiles/configuration dumps, Git output, SQL errors, native raw errors, or
subprocess stderr. Private runtime files are checked by type/permission
metadata without reading credentials. Human rendering escapes terminal
control characters and bounds display text; normal NO_COLOR/TTY rules apply.
JSON may contain local paths and workspace names and must be reviewed before
sharing. It is a best-effort point-in-time diagnostic, not an atomic snapshot
across Git and Codex or proof of every future native operation's compatibility.

## Verification boundary

Focused adapter and CLI tests cover healthy/changed bindings, prepared and
closed workspaces, stale installations, absent/old/stalled daemons, bounded
output and process cleanup, independent read-only SQLite access, collection
caps, expected skips, private-file safety, and secret-free rendering. Git
regressions cover registered unborn repositories separately from valid
checkouts, branch/detached metadata, and protected trust configuration with
both trusted and untrusted ownership cases; they do not mutate the process
environment or user configuration.
Generated shell fixtures are read as data by an existing shell, not executed
as freshly written binaries: concurrent spawning can otherwise trigger
`ETXTBSY`. Cancellation tests retain strict process-disappearance assertions,
including dropping a child before any wait future is polled; an unreaped
zombie does not count as successful cleanup. Redaction tests verify that the
fixture really ran and cover both failing exit status and invalid successful
output, rather than passing merely because fixture spawning failed.
Process gates exercise the actual CLI's exit codes and verify no state is
created without a daemon. The selected model-free real-Codex gate verifies
passive native metadata and unchanged thread/runtime bindings across doctor.
