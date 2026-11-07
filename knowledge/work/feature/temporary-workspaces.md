---
type: Working Document
title: "Temporary workspaces"
description: Adds opt-in one-session workspaces with explicit successful-exit cleanup and the existing retirement safeguards.
tags: [work, cli, workspaces, cleanup, safety]
status: complete
branch: feature/temporary-workspaces
updated: 2026-11-07
---

# Temporary workspaces

## Intended outcome

Create an explicitly temporary workspace, with or without opening its TUI.
After the first regular exit of the last tracked TUI, attempt safe cleanup
using the existing delete policy. Include the create walkthrough and focused
public documentation. The initial implementation excluded commit/push/release;
the subsequent 2026-11-07 delivery authorization is completed below. No
installation or user-runtime change is part of this work.

## Active work

- [x] Fix the reviewed auxiliary-thread cleanup hole conservatively: persist
      disarm before forwarding additional-conversation traffic, including
      shared execution, and explain retention after TUI exit.
- [x] Cover fresh/bound sessions, reconnects, several clients, failed persistence,
      and real DB reopen without model calls; rerun the relevant full gates.
- [x] Define and persist the create-time cleanup policy and safe one-shot claim.
- [x] Add explicit successful TUI completion without making lease expiry,
      disconnect, failed launch, send, polling or recovery trigger deletion.
- [x] Reuse guarded deletion; retain blocked workspaces with a clear message.
- [x] Add `--temporary`, optional thread retention and walkthrough defaults.
- [x] Test isolated persistence, CLI, multi-client, error and deletion cases.
- [x] Update canonical knowledge, command help and focused user docs.

## Decisions

- 2026-11-06 — Start from clean main `4c59eb9`, containing released 0.5.0 and
  Palm turn control. Keep Compose and other branches separate.
- 2026-11-06 — Creation without jump is valid: the policy waits for a later
  regular TUI session. Send alone never triggers cleanup. Failed launches,
  abnormal exits, expired presence and daemon restart are not cleanup events.
- 2026-11-06 — Only the last live tracked client may trigger cleanup. Preserve
  the existing native-work, file, commit, dependency, path and hook guards.
  A blocked cleanup retains the workspace and must not become a delayed
  destructive retry when it later becomes idle or clean.
- 2026-11-06 — Keep the package/module dependency direction; no Codex feature,
  worker MCP change, runtime scheduler or new dependency is required.
- 2026-11-06 — Use a separate `workspace.attach.finish` request after successful
  TUI/relay completion. Validate the exact still-live lease, preserve old release
  semantics, and let checked deletion catch newly admitted clients. SQLite 18
  adds nullable typed policy; public JSON 18 exposes it only while armed.
  Claim consumes it before effects; immutable create input still supports
  same-key replay and rejects changed retention policy after consumption.
- 2026-11-07 — Conservatively consume temporary permission before forwarding
  another start, a fork or a different/unknown thread reference through the
  relay. Reuse the existing durable claim under a validated live exact lease;
  keep its registry mutex until commit so another completion cannot claim in
  between. Do not forward on unconfirmed persistence. Keep the native binding
  unchanged and report retention after normal exit. Temporary shared-mode
  resumes now use the relay; persistent shared-mode resumes remain direct.

## Findings

- 2026-11-07 — An isolated review probe reproduces deletion of a clean
  worktree while an independent auxiliary native thread using it is active.
  Bound relays deliberately forward new conversations without replacing the
  workspace binding; existing deletion checks inspect only that binding and
  its descendants. The approved fix is conservative durable opt-out, not a
  new native-thread registry: disarm before forwarding extra-conversation
  requests, preserve the original binding, and retain the workspace thereafter.
  Temporary shared-mode resumes also need relay observation; persistent
  shared-mode behavior must stay unchanged.
- `cli/jump.rs` already waits for the TUI and releases the exact attachment.
  The same release path also runs after errors, so ordinary release itself
  cannot be used as evidence of a successful session.
- Attachments are renewable, generation-local and workspace-to-many. The
  current release is synchronous and expiry only changes presence.
- Existing checked deletion already owns destructive policy, guard execution,
  active-runtime and context-dependency checks, durable intent and recovery.

## Verification

- Review correction: all 39 focused temporary-workspace tests pass, including
  13 new cases for native-before-ack gating, failed RPC/storage persistence,
  fresh versus bound and shared versus workspace execution, repeated disarm,
  invalid/expired/released authority, multiple clients, and a real closed and
  reopened SQLite connection/new coordinator generation. The first socket
  fixture run was denied by sandbox restrictions; it passes with isolated
  local-socket permission. Warning-denied all-target/all-feature Clippy passes.
  The additional full-relay case checks durable-disarm dispatch, retained
  original binding and completion-notice propagation; the persistent relay
  regression proves there is no new cleanup RPC for ordinary workspaces.
- Corrected-production all-target suite passes: 708 library tests, seven
  process scenarios and eleven status-query tests; seven manual library probes,
  paid inference and ten native opt-in cases remain intentionally excluded
  from that default invocation. The later additional relay-only test passes
  with the 39-case targeted rerun. All ten Codex 0.166.0 model-free native
  compatibility checks pass separately in 93.22 seconds. No model was invoked.
- Final format and diff checks pass. Machete finds no unused dependencies;
  offline dependency policy passes with the cached advisory database and
  unchanged permitted duplicate-version warnings. Astro check reports zero
  errors/warnings/hints; the `/coco` static export validates 18 pages, 64 files,
  search/routing and the strict public boundary. Cache/sockets checks required
  narrowly scoped sandbox access; no installation or publication occurred.
- The four new correction files are intent-to-add for Git-flake source
  capture only; no file content is staged. `main` and `origin/main` still point
  to `4c59eb9`, and no commit, push or user service update was made.
- All-target offline compilation passes. Eighteen focused tests pass for
  deferred create, safe deletion, thread retention, multiple clients, aborted
  sessions, active work, tracked/untracked/ignored files, unretained commits,
  dependencies, request replay and real DB reopen/concurrent claims.
- New built-CLI/daemon process test passes for deferred create, actual daemon
  restart, successful jump/create-jump, failed launch and abnormal exit. Its
  initial mock accepted only the daemon connection, stalling the real relay
  handshake. The fixture now accepts relay connections too; no production
  timeout or behavior was weakened. It initiates no native model work.
- Additional guard, adopted-branch, protocol and live-capability tests pass.
  Full all-target suite passes: 695 library cases, seven process scenarios and
  eleven status-query cases. Seven manual library probes and paid inference
  remain intentionally ignored. Three initial old migration-test assertions
  still expected the previous current schema 17; they now expect 18 while
  preserving all loss/record assertions, and the full rerun passes.
- All 25 focused cases and all-target/all-feature warning-denied Clippy pass.
  All ten installed Codex 0.166.0 model-free compatibility tests pass serially
  in 70.45s. Astro check has no errors/warnings/hints; the `/coco` export verifies
  18 pages/64 files and the public-only boundary. Machete and refreshed
  dependency policy pass, with unchanged permitted duplicate-version warnings.
- Final review additionally checks durable recovery state for generic cleanup
  failures: an early Git-removal I/O error can have partial effects without the
  later deletion-incomplete wrapper. Closed/deleting/non-open evidence is now
  reported incomplete, not retained; absent records report removed. A focused
  classification regression is added.
- Final all-target rerun after that correction passes 696 library, seven
  process and eleven status-query tests; all-target/all-feature Clippy and
  formatting pass. The slice adds 26 permanent library tests and one process
  scenario. Native compatibility covers the unchanged worker/TUI contracts;
  the new cleanup behavior is proved by isolated application/process fixtures,
  including real database reopen and a real test-daemon restart, without
  paid inference.
- `nix flake check --no-build --no-update-lock-file` passes for the local
  system. All twelve new implementation/test files compare byte-for-byte
  with the Git-flake snapshot. New files are intent-to-add, not committed;
  Git source capture therefore includes them for local-coco. No full Nix
  binary build or installation was performed. `git diff --check` passes.

## Open questions and handoff

- 2026-11-07 — User authorized committing this reviewed slice, integrating
  it into `main`, pushing, and publishing the next regular release through
  guarded GitHub Actions. This supersedes the earlier no-publication boundary
  for delivery only; keep Compose/other branches and installed runtimes
  unchanged. Latest read-only review finds no remaining blockers: 709 library,
  seven process and eleven status-query tests pass, as do all 39 focused
  cases, formatting and warning-denied Clippy. Delivery is recorded in main's
  working document.
- Complete and released as stable `0.6.0`. Feature commit `1a71545` was
  integrated and pushed by fast-forward; release automation produced tag
  `v0.6.0` at `ea0c70f`. Exact-source Rust and Pages gates, stable rehearsal
  and publishing workflow passed, including native Linux/macOS and crates.io.
  Public registry availability, both archive checksums/content and the tagged
  lockfile were independently verified. Compose/other branches are unchanged.
- Use matching CLI and daemon builds; published 0.6.0 includes the new flags
  and schema-18 policy. Existing workspaces migrate as persistent and ordinary
  release behavior is unchanged. No installation or live-runtime change was
  made by this delivery.
- Headless turn-completion cleanup, TTL expiration and forced automatic file
  or commit discard are intentionally outside this feature.
