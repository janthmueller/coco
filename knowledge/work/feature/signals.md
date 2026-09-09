---
type: Working Document
title: "feature/signals: workspace-bound agent signals"
description: Completes the combined control-plane proof and implements a bounded, durable signal interface without ticket workflow or automatic reactions.
tags: [work, branch, signals, mcp, testing]
status: complete
branch: feature/signals
updated: 2026-09-09
---

# feature/signals — workspace-bound agent signals

## Intended outcome

The user approved the sequence recorded on `main`: finish the combined
multi-repository/multi-client proof, settle the signal contract, and implement
registration, discovery, workspace-bound emission, persistence, bounded reads,
and resumable follow. A signal reports an agent claim, not ticket completion.
No hooks, automatic model wakeup, task system, gateway, release, or push.

## Active work

- [x] Replace draft imperative type registration with explicitly selected JSON
  Schema 2020-12 files; preserve history and exact-version emission grants.
- [x] Prove atomic catalog loading, immutable revisions, bounded input and
  actionable payload rejection through focused and real-process tests.
- [x] Update public setup examples and canonical contract; run sequential gates.
- [x] Align signal reads with workspace-command syntax: optional positional
  workspace, `-a` for collections, `-g` for one global reference, and unchanged
  `list`/`ls` aliases. Update focused tests and usage docs; no signal-status API.
- [x] Complete a bounded combined core proof with isolated repositories and
  clients; retain accurate distinctions between fake-process and live-native
  evidence and do not modify the user's workspaces.
- [x] Settle and prove sender attribution and the minimal MCP configuration
  integration across activation, resume, and context forks.
- [x] Record the concrete signal contract in canonical internal knowledge.
- [x] Implement operator-owned type registration/revisions, optional payload
  schema validation, bounded durable emit/read, and idempotent retry behavior.
- [x] Add CLI registration/list/follow and read-only/explicitly enabled MCP
  capabilities without exposing registration to worker models.
- [x] Verify restart, independent-reader replay, invalid input, authority,
  lifecycle/deletion, and boundedness; update user-facing docs for actual behavior.
- [x] Run final gates sequentially and review the diff before handoff.

## Decisions

- 2026-09-09 checkpoint — User requested a local commit of the completed signal
  slice, including tests, public usage docs and the preceding design record,
  then explicitly authorized merging it into local `main`. Both branches start
  at `d90d82d`, so integration can fast-forward. No push, release or publication
  is included. The verified source is
  unchanged; only this handoff record is updated before committing.
- 2026-09-09 file-catalog follow-up — User accepted standard JSON Schema
  2020-12 after discussing file-defined signals. Load a selected directory once
  when the control MCP process starts (`--signal-catalog`), not automatically
  from worker worktrees and not through a worker registration tool. Each direct
  `NAME@VERSION.json` file is a plain schema; its standard `description` explains
  use. No custom schema wrapper, directory watcher, daemon configuration system,
  or digest-as-authentication mechanism is needed.
- Catalog startup validates all files and stores immutable repository-scoped
  snapshots in one transaction. Existing versions must compare equal as parsed
  JSON. A running MCP instance keeps its selected snapshot; file changes require
  an MCP restart and a new version for changed definitions. Removed files do not
  erase history or revoke already running instances. Exact `--allow-emit` grants
  must also exist in the selected collection. Discovery reports emission
  permission separately. The CLI retains history reads, not type registration.
- 2026-09-09 follow-up — The user approved `signal list [workspace]` instead of
  the draft `--workspace`/`-w` filter. Match status's local/default, global-name,
  globally unique ID, and invalid-scope behavior. Keep signals as historical
  occurrences, not a second workspace state. `signal type` still names the
  registered definitions; its terminology is under discussion, not being changed.
- 2026-09-09 — Work in a dedicated branch from `d90d82d`. Preserve the four
  existing knowledge-document changes from the prior discussion; their record
  stays in `knowledge/work/main.md`. This document owns implementation history.
- Remain one Cargo package with focused private modules and the existing
  CLI/MCP → typed protocol → coordinator → store/runtime direction. No subagents.
- Resource-intensive checks run one at a time with one build job. Use the Git
  flake `.`. No full legacy-schema cleanup is mixed into the signal migration.
- Native thread metadata supplies sender attribution; no synthetic session IDs,
  gateway, authentication service, or model-selected workspace argument. Explicit
  version-pinned grants live in the operator's MCP launch arguments/profile.
- Review refinement: `--allow-emit NAME@VERSION` pins an exact version; a bare
  name means version 1. A new catalog version must not expand old grants, and a
  model must not choose a previously registered weaker schema outside its grant.
- The optional schema uses `jsonschema` 0.55.1 with default features disabled:
  no HTTP/file resolution or TLS stack. Inline draft-2020-12 schemas, bounded
  JSON, immutable versions, and an operator-only catalog keep the contract small.
- Dependency policy review required one exact `borrow-or-share` 0.2.4 MIT-0
  exception, verified against its included LICENSE and SPDX. No general license
  permission, advisory ignore, or package ban was relaxed.
- Signal retention is count-bounded (10,000 globally), not indefinite archival.
  Reads report expired cursors; retries deduplicate only retained records. Closed
  or deleted workspaces' histories remain readable by UUID until retention.

## Findings

- The current control MCP adapter fixes repository scope, not a sender
  workspace identity. Automatic worker signal exposure therefore requires a
  deliberate binding rather than accepting a caller-supplied workspace name.
- Existing process and real-native test harnesses are the preferred starting
  point; do not create a second parallel test framework or unbounded giant file.
- The combined real-MCP process test exposed an existing capability bug:
  `#[tool_handler]` in rmcp 3.2 defaults to `Self::tool_router()`, bypassing the
  instance whose `workspaces.send` route had been disabled. The existing tests
  inspected the instance directly and missed the actual handler path. Fix uses
  `#[tool_handler(router = self.tool_router)]`; regression coverage must exercise
  both tool discovery and an attempted invocation without `--allow-send`.
- Installed native version remains `codex-cli 0.153.4`. Its checked-in matching
  protocol includes `mcpServer/tool/call` with a thread ID, allowing the scoped
  MCP preflight without model calls or copying the user's credentials.
- The matching Codex core already injects `_meta.threadId` for model tool calls;
  the direct App Server tool-call route injects the same key. Use this native
  metadata plus the existing thread/workspace binding rather than a new token
  registry or model-selected workspace argument. Keep the trusted-local-client
  limitation explicit. Concrete contract is now in `engineering/signals.md`.

## Verification

### Initial signal slice

- Initial expanded lifecycle process test failed on the ungranted MCP tool
  exposure above, as intended by its new assertion. The correction passes both
  discovery and attempted invocation. Previous baseline is 224 library and five
  process tests plus the recorded real-terminal checks at `d90d82d`.
- Expanded combined process scenario passes (two repos, actual MCP read-only
  rejection and opted-in send, stable operation/native IDs on retry/restart).
  Dynamic workspace projection is intentionally not byte-identical on retries.
- Model-free native start/fork/resume MCP isolation passes against 0.153.4.
  Initial sandbox run could not bind a port; approved external rerun used only
  temporary homes/repositories. The fork probe negotiates experimentalApi for
  its `deferGoalContinuation` field, matching production.
- Seven focused schema/store tests pass: immutable definitions, idempotency,
  independent readers and scope/stream checks, restart, closed/deleted history,
  cursor expiry and sequence monotonicity, and retry-aware rate limits.
- Expanded actual-MCP process test now also passes schema rejection, missing or
  cross-repository origin rejection, opt-in version grants, registration conflict,
  emission replay, CLI/MCP read agreement, and persistent nonduplicating follow.
- Real Codex signal test passes native metadata, two senders, an unbound native
  fork rejection, a bound CoCo fork with its own identity, and named-profile
  restoration plus original-signal retry across daemon/App Server restart.
- Final review adds a real long-running CLI follower: read its initial page,
  wait beyond a polling interval without duplicate output, emit a later update
  through MCP, receive exactly that update, stay open through another idle
  interval, then exit cleanly on Ctrl-C. Version 2 is registered but rejected
  under a bare-name/version-1 grant. Focused CLI tests cover list/ls aliases,
  combined `-afw`, page bounds, and positive versions.
- `cargo fmt --all --check` and
  `cargo clippy --locked -j1 --all-targets --all-features -- -D warnings` pass.
- `cargo test --locked -j1 --all-targets --all-features -- --test-threads=1`
  passes: 234 library tests and five process tests. Three library tests remain
  intentionally ignored (two manual PTY probes and one charged-model Git probe).
- `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked -j1 --test real_codex_compat
  -- --ignored --test-threads=1 --nocapture` passes both installed 0.153.4 tests.
  These use temporary homes/repositories and real local processes, without
  credentials or model turns. The combined workflow worker remains a fake.
- `cargo machete` passes. `cargo deny --frozen check` passes advisory, ban,
  license, and source checks against the cached advisory database; duplicate
  dependency-version warnings remain visible, not suppressed.
- `pnpm --dir docs run check` passes type generation, TypeScript, Oxlint, and
  Prettier. The production build with `DOCS_BASE_PATH=/coco` passes and verifies
  98 static files, 10 pages, search, base-path routing, and the public-only boundary.
- `git diff --check` passes. `nix flake check . --no-write-lock-file --max-jobs 1
  --cores 1` passes, including the full package build and tooling/app checks on
  x86_64-linux. Other platforms were not exercised. This first release-profile
  build of the new dependency graph ran alone and took longer than the debug
  tests; it completed successfully without changing the resource limits.

### CLI consistency follow-up

- Reused the existing CLI scope helpers with visibility limited to their CLI
  parent; no daemon, storage, MCP grant, or worker-profile semantics changed.
- Parser/help tests cover visible `list`/`ls`, positional workspaces, combined
  `-af`/`-gf`, leading and trailing scope flags, global UUIDs, conflicting scope
  errors, and absence of the removed `--workspace` form and `signal status`.
- The real process scenario verifies both spellings for a local name, explicit
  repository path, global UUID from outside a repository, and all-repository
  history. Same-name repositories remain isolated; global ambiguity reports
  both paths. Changing a reader's scope rejects its old cursor explicitly.
- The first new process probe used a nonexistent fixture home as its working
  directory; changed it to the existing temporary parent outside both repos.
  No production correction was needed for that fixture failure.
- Final `cargo test --quiet --locked -j1 --all-targets --all-features --
  --test-threads=1`: 237 library and five process tests pass; three manual/live
  library probes and two opt-in native tests remain ignored. No model call.
- Format, all-target/all-feature warning-denied Clippy, `cargo machete`, and
  cached `cargo deny --frozen check` pass. The docs check and production static
  build pass again (98 files, 10 pages, `/coco` routing/search/public boundary).
- No additional Nix package or live-native run for this CLI-only follow-up;
  the earlier initial-slice evidence above remains separate. No package,
  dependency, runtime binding, schema, or release configuration changed here.

### File-catalog follow-up verification

- Twenty focused signal tests pass. Added plain-schema/boolean-schema loading,
  filename rules, bounded files/catalogs, symlink rejection, whole-batch rollback
  on version conflict or capacity, standard nested payload types/enums/required
  fields, and explicit grant/catalog parser tests. Format annotations remain
  annotations, verified rather than silently advertised as assertions.
- Actual stdio process coverage uses both `coco-mcp` and `coco mcp serve`,
  including a relative repository path. Reload preserves registered snapshots;
  an invalid revision fails before serving, with no partial definitions. A
  running discovery snapshot survives edits/removals, new instances see the
  selected files, and retained definitions survive removal. Missing grant files
  fail startup; discovery exposes `emitAllowed` separately.
- Invalid emissions identify their instance path and schema rule without
  echoing the payload value. The valid correction reuses the rejected request's
  key and produces exactly one history record. Existing retry/follow/scoping,
  cross-repository sender and restart tests continue to pass.
- `cargo test --quiet --locked -j1 --all-targets --all-features --
  --test-threads=1`: 244 library and five process tests pass, with the same three
  opt-in library probes and two native tests excluded from the ordinary suite.
- `cargo clippy --locked -j1 --all-targets --all-features -- -D warnings` passes.
- The isolated, model-free installed-Codex MCP test passes again:
  `COCO_RUN_REAL_CODEX_COMPAT=1 cargo test --locked -j1 --test real_codex_compat
  installed_codex_keeps_mcp_scopes_separate_on_start_fork_and_resume --
  --ignored --test-threads=1 --nocapture`. It now loads schemas from files and
  still proves two repository scopes, native sender metadata, bound/unbound
  forks, profile restoration and idempotent replay after restart on 0.153.4.
  The unrelated second native compatibility test was not rerun in this follow-up.
- Format, `cargo machete` and cached `cargo deny --frozen check` pass. The
  sandbox prevented the advisory-cache lock; the approved offline rerun passed
  without downloads. Existing duplicate-version warnings remain visible.
- `pnpm --dir docs run check` and the production static build with
  `DOCS_BASE_PATH=/coco` pass: 98 static files, 10 pages, routing/search and the
  public-only boundary. No release package/Nix rebuild in this follow-up; the
  initial-slice Nix result is recorded separately above. No dependency or flake
  change was needed for the file loader. Final `git diff --check` passes.

## Open questions and handoff

- The accepted file-catalog/2020-12 follow-up is complete and verified. It
  replaces the unreleased imperative registration command. The store retains
  exact parsed schemas; a separate digest would add no authentication guarantee
  and is not needed for revision equality. No new dependency or database migration.
- This checkpoint on `feature/signals` packages the complete implementation,
  tests and documentation. The four original knowledge changes from the
  preceding signal discussion are preserved and included. The user requested
  local fast-forward integration into `main`, without a push. Pre-commit
  `git diff --check` passes; the successful
  gates above apply to the unchanged implementation without repeating builds.
- Next product discussion: choose the first external signal consumer and its
  interpretation/retry policy. A notification, ticket integration, or later
  hook can consume this interface; none is automatically authorized or built.
- A supervised model-backed real-user acceptance run and release decision
  remain separate. No live user daemon/workspace, credentials, remote settings,
  Pages, or publishing configuration was changed.
