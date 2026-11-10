---
type: Working Document
title: "feature/ai-control: natural-language CLI control"
description: Native Codex control session with scoped CoCo tools and recoverable context creation.
tags: [work, branch, cli, mcp, codex]
status: complete
branch: feature/ai-control
updated: 2026-11-10
---

# feature/ai-control — natural-language CLI control

## Intended outcome

Offer `coco ai [prompt]` as a thin native Codex dialog for managing CoCo across
known repositories, with the current repository as default. Reuse control MCP, existing creation/core
logic, scope checks, receipts and native approvals. Keep normal CLI behavior.

## Completed work

- [x] Verify installed Codex 0.166.0 launcher/configuration and tool restrictions.
- [x] Extend public idempotent creation with independently selected context,
  preserving fresh-worker contracts, source isolation and retry semantics.
- [x] Add the scoped native control-session entry point and clear approvals.
- [x] Cover contracts, errors, isolation and startup with model-free tests.
- [x] Update actionable public documentation and durable architecture guidance.
- [x] Review the final diff and run sequential regression/lint/static-doc gates.

## Decisions

- 2026-11-10 — User authorizes implementation after discussing `coco ai`.
  Use a dedicated local branch from clean `main` at `aa8215b`; no commit,
  push, release or installation change is authorized. Real model use was
  initially excluded; the later live-test authorization is recorded below.
- The control conversation is distinct from managed worker workspaces.
  No automatic worktree or worker-MCP binding for the control session.
- User explicitly rejects an all-repository flag for this dialog. Discover
  registered repositories automatically; prefer the current repository for
  unqualified targets and ask about ambiguities. Keep each MCP endpoint fixed
  to its own repository, including creation-context sources. One natural-
  language request may result in separate, non-atomic repository operations.
- Native confirmation gates concrete mutating tool inputs. The assistant first
  presents a concise review with repository, name, Git base/binding, context,
  changes, profile/model and post-action. Default to human confirmation, with
  an explicit operator-selected automatic mode; the model cannot change it.
  Use Codex's existing approval UI, not a nested CLI picker inside MCP.
- Start with the operations needed to inspect workspaces, fork/create and
  send through existing MCP. Do not bypass missing tools with unrestricted
  shell access. Native interactive jump/cd handoff and broader lifecycle /
  resource MCP coverage require explicit support, not prompting alone.

## Findings

- Native CLI dotted override keys do not parse TOML quoting. The initial
  synthetic-TOML test missed this; the actual TUI rejected the quoted inherited
  server name. Use one structured `mcp_servers` value, with all names inside it.
- Current models can select Responses-Lite and native V8 tool dispatch even
  when optional legacy code-mode features are false. Disabling the V8 host
  leaves advertised tools unusable. Keep that no-filesystem/network dispatcher
  enabled and assert the actual nested registry excludes shell/foreign tools.
- Native generic MCP approval renders only a few scalar inputs and does not
  show our repository title in its main question. Use an opt-in server-originated
  MCP form for deterministic full review instead, with native UI and exact
  retained invocation inputs. Avoid a second generic approval for those three
  guarded tools. Public MCP defaults and Palm's noninteractive setup stay unchanged.
- Enabling RMCP's `elicitation` feature adds its URL/IDNA/ICU dependency family
  (26 packages), not merely one lockfile edge. No pre-existing dependency
  version changed. Keep the feature scoped to form confirmation and run the
  existing dependency policy; no HTTP/OAuth transport is enabled.
- Native UI startup persists its own screen-reader detection and model
  introduction counters. Verify control/tool, provider and project configuration
  remains unchanged semantically, rather than claiming byte-identical native
  config files. CoCo itself never persists its invocation overrides.
- The native confirmation fixture must wait for the full form and selected
  True row before Enter; the summary title alone precedes input readiness.

- Initial inspection: installed native binary reports Codex 0.166.0. Control
  MCP offered list/status/diff/send, fresh creation and receipts, turn control
  and signals. Its missing creation-context selection is now added by reusing
  the existing CLI/core capture path.
- Existing public create keys hash the typed request. Preserve old fresh
  request hashes while extending the contract; retries must not recapture.
- Worker-bound contexts cannot be forked. A worker binding plus inherited
  context remains unsupported until grant inheritance can be verified.

## Verification

- Initial repository inspection: clean main, no unrelated edits.
- Official CLI/config/MCP documentation fetched and exact native source
  inspected at the revision recorded in the canonical AI-control decision.
- Rust 1.98.1 via the Git flake; heavy gates run sequentially with two Cargo
  jobs and two ordinary test threads, one native test thread.
- `cargo test --locked --offline -j 2 --all-targets -q -- --test-threads=2`:
  838 passed (809 library, seven AI process, four navigation, seven process
  smoke, eleven status); deliberate manual/native/paid tests remain ignored.
- The seven AI process tests use the real MCP executable and an isolated host:
  authoritative review before RPC, exact acceptance, decline/cancel/malformed
  responses, absent capabilities/grants, transport failure, request cancellation,
  review bound and unchanged noninteractive defaults all pass.
- Opted-in native AI-TUI proof passes with Codex 0.166.0: server-generated
  review, false default, concrete human acceptance before creation, automatic
  creation in two repositories and a separate read-only conversation. The
  actual V8 nested registry excludes shell/subagents and inherited MCP; the
  inherited test executable never starts. Control/provider/project config
  remains semantically unchanged; native UI counters are allowed explicitly.
- Two existing opted-in native proofs also pass: active-source context capture
  with independent child/restart and public worker-MCP grants across
  start/resume/restart. All three native proofs use scripted loopback providers,
  isolated Codex homes/repos and disposable test processes. No real model call.
- `cargo fmt --all`, warning-denied all-target/all-feature Clippy and
  `git diff --check` pass. `cargo machete` reports no unused dependency.
- `cargo deny --locked check`: advisories, licenses, sources and bans pass;
  permitted duplicate-version warnings remain. No dependency-policy waiver.
- `cargo package --list --locked --allow-dirty --offline` succeeds and includes
  the embedded control instructions. This is a file-list check, not a publish
  or a packaged-build verification.
- `pnpm --dir docs check`: zero errors/warnings/hints.
  `DOCS_BASE_PATH=/coco pnpm --dir docs build`: 20 pages, 68 static files,
  Pagefind, links/base routing and public-only boundary verified.
- `nix flake check --no-build --offline --no-update-lock-file` passes for the
  host system; other systems are not built or evaluated by that invocation.
- Existing real-shell navigation proof passes all nine PTY cases across
  Bash/Zsh/Fish, including selection, cancellation, quoting and terminal restore.
- New files are intent-to-add so Git-flake source filtering includes them.
  No contents staged, commit, push, release, installation or service changes.

## Full-slice review — 2026-11-10

- Reviewed production changes, protocol/setup compatibility, scope and consent,
  native 0.166.0 configuration resolution, tests, dependencies and affected docs.
- **P1 found in review, resolved below: inherited feature aliases bypass two restrictions.**
  `cli/ai/config.rs` sets canonical `apps=false` and `memories=false`, but native
  0.166.0 applies inherited `connectors=true` and `memory_tool=true` afterwards.
  Its feature-map resolver gives no canonical-precedence exception to these
  aliases. Connected apps or memory processing can therefore remain enabled
  in a control conversation, including the intended read-only invocation.
- Reproduced without a model using `codex features list` in disposable homes,
  with `-c features.connectors=true -c features.apps=false
  -c features.memory_tool=true -c features.memories=false`. Effective output
  is `apps=true` and `memories=true`; shell/hooks restrictions remain false
  when explicitly set. No actual external app action or memory pipeline ran.
- Fix before commit: override the aliases too (or equivalently neutralize them
  through verified native resolution), and add a regression that checks actual
  effective native features against inherited legacy settings. Existing tests
  inspect canonical overrides and lack that configuration case.
- Reverification: 59 focused library tests, seven MCP-process tests and all
  three scripted-loopback native proofs pass. `git diff --check` is clean.
  Green standard fixtures do not discharge the independently reproduced finding.
- Review only: no implementation fix, user configuration/service change,
  real model call, commit, push or release.

## Review correction — completed

- User authorizes fixing the inherited-feature finding, without commit or push.
- Add an ordinary assertion for Ask/Auto/Read-only and a separate opted-in
  native feature-resolution proof. First reproduce the failure using actual
  launcher arguments and an isolated config with enabled legacy aliases;
  then neutralize those aliases in the invocation-only overrides.
- Reuse the existing scripted native TUI proof with inherited aliases enabled
  after the fix. No actual app, memory-pipeline or external model call is needed.
- Invocation overrides now explicitly disable `connectors` and `memory_tool`
  alongside their canonical names in every mode. No saved-config rewrite,
  config-file deletion or generic warning suppression is introduced. Native
  deprecation notices for the compatibility aliases remain possible.
- The new opted-in proof failed against the original launcher (`apps=true`),
  then passed with the fix. It verifies the inherited alias behavior first,
  tests actual launcher arguments for all three modes, and checks byte-identical
  inherited configuration afterwards. Ordinary mode tests also assert all four
  canonical/legacy flags are false.
- Final verification after the fix: 838 ordinary all-target tests pass (809
  library and 29 integration; nine library native/manual cases stay opt-in).
  All-target/all-feature warning-denied Clippy and formatting pass.
- The new native feature proof and all three existing scripted-loopback
  context/worker-MCP/AI-TUI proofs pass. The TUI fixture now inherits enabled
  legacy aliases while exercising human confirmation, automatic multi-repo
  creation and read-only access. No real model call or user-state change.
- The P1 finding is closed. No dependency or public contract changes were
  needed for this correction. New native test source is intent-to-add for
  Git-flake inclusion; no contents staged, commit or push.

## Follow-up review — 2026-11-10

- Rechecked launcher restrictions/modes, exact host confirmation, cancellation,
  repository isolation, context/creation replay, private preparation and the
  affected user documentation. No new concrete finding or review blocker.
- Re-ran 59 focused library tests, seven MCP-process tests, the native legacy-
  feature proof and all three scripted-loopback compatibility proofs: 70 pass.
  The opted-in probe is skipped in the ordinary run and explicitly run apart.
- `git diff --check` remains clean. No product code changed during this review;
  the preceding full regression, formatting and Clippy results still apply.
- Existing boundaries remain: exact Codex 0.166.0 verification, possible native
  legacy notices, no real-model quality or macOS execution claim. No commit,
  push, release or user-state change; `main` is still `aa8215b`.

## Open questions and handoff

- Planned implementation and the review correction are complete on
  `feature/ai-control`. `main` remains at `aa8215b`.
  No user daemon, workspace or native configuration changed.
- Normal use: `coco ai [prompt]`; default per-action human confirmation,
  explicit `--approval auto` or `--read-only`, optional `-m MODEL`. Current repo
  is the default and registered repositories are automatically attached.
- Current mutating MCP coverage is create/fork, send and turn interruption.
  Cleanup, limits, hooks and interactive jump/cd use ordinary CLI commands.
  Public context forks remain repository-local and exclude worker-bound
  sources; broader raw/cross-repo context remains an explicit CLI capability.
- Real-model evidence is limited to the cases explicitly recorded below;
  no general intent/quality guarantee, macOS execution, other native version
  or release-package build is claimed. Future native upgrades need to reverify
  the configuration and confirmation contract.
- Live-coverage clarification at the preceding review: the executed native fixtures use the actual
  Codex/CoCo/MCP/Git stack with scripted model replies. The separate existing
  `tests/live_product_proof.rs` supports authenticated real-model CLI/MCP sends,
  idempotency and restart checks, but does not exercise natural-language
  `coco ai` planning. A small supervised real-model AI smoke test in disposable
  repos would validate that remaining interaction gap. Its subsequent explicit
  authorization and first run are recorded below; that first run is not green.
- Next action is user review/local testing or a separately authorized commit.

## Authenticated AI smoke proof — completed

- User explicitly authorizes the proposed real-model smoke test using their
  existing Codex login. No commit, push, release or installation change.
- Extend the existing native TUI harness with a separately opted-in proof.
  Two disposable repositories, private copied auth, isolated CoCo/Codex/XDG
  state and no user hooks/integrations. Never print or persist credentials in
  repository artifacts. Remove the private fixture through normal temp cleanup.
- Bound the scenario to five user turns (one source seed and four control
  prompts), low reasoning where supported, a five-minute execution deadline
  and no automatic paid rerun. Token/request totals are not a fixed-price cap.
- Assert observable creation/context identities and exact repository inventories
  for decline, acceptance, auto multi-repo and read-only; completion text is only
  synchronization, not a golden response or correctness claim.
- Added the separately ignored `tui::ai::live` test and private support beneath
  the existing native integration harness. Confirm the advertised default
  supports low reasoning before any turn; clear inherited API-key/base-URL and
  native thread identity overrides. Shared execution/process-tree containment
  keeps this functional proof separate from cgroup/executor measurements.
- First authenticated run with Codex 0.166.0 and advertised `gpt-6.1-sol`:
  source creation/send and its synthetic response succeeded. The first control
  dialog timed out after 90 seconds before the expected native confirmation;
  the scenario stopped at 93.85 seconds. Decline, accepted context, multi-repo
  and read-only real-model assertions were not reached. No success is claimed
  for them, and no automatic paid rerun occurred.
- The private fixture was stopped and dropped. Because the initial error did
  not preserve a screen classification, the root cause is not established;
  do not attribute it to the product, provider, model or harness speculatively.
- Added bounded allowlisted Boolean screen diagnostics, separate confirmation/
  completion timeout labels, and unconditional runtime-cleanup verification.
  No raw terminal text, login cache or arbitrary provider error body is output.
- Model-free native recheck passes with an added current-pane assertion:
  Codex's confirmation title/footer are visible, approval creates only after
  confirmation, automatic creation reaches both repos, and read-only works.
- Requested fresh approval for one diagnostic paid rerun while completing
  model-free checks. No commit, push, release or user-service/config change.
- User explicitly approves that one diagnostic rerun. Start only after the
  sequential regression suite finishes; retain the five-minute execution bound
  and the prohibition on an additional automatic paid rerun.
- The second run again reached the source seed but not the control UI; at
  timeout all form/working/error/login/completion markers were absent, and the
  pane's foreground process was still Bash. No control-model call was made.
- Reproduced the launch failure without Codex/auth/model using the same private
  environment and a 6-KiB synthetic prompt with the harmless `true` executable:
  the long typed command never reached the child. A longer 32-KiB probe also
  hits tmux's command-size limit. Keep the ordinary-length regression opt-in.
- Fix the live harness, not production: put the environment/command in a private
  disposable script and type only its short path. Clear `BASH_ENV`/`ENV` and
  isolate the XDG runtime directory too. The identical 6-KiB regression now
  passes in 0.07 seconds. No authentication or model was used for this fix.
- User explicitly approves one final live execution with that corrected harness.
  The preceding complete ordinary suite passes 840 tests, including the two
  new diagnostic/form-readiness tests; paid/native tests remain ignored normally.
- With the short-script fix, real native control now runs. Declining creation
  leaves inventory unchanged; confirmed context creation produces a distinct
  native child, the expected parent and branch, and inherited synthetic history.
  Both real-model cases pass. The auto phase times out waiting for the requested
  assistant-text completion marker (126.56 seconds for the partial run). Its
  actual creation result and read-only were not asserted, so remain unverified.
- Remove assistant-text completion synchronization entirely. A private sibling
  `ai/progress` observes bounded fixture-only native rollout records, correlates
  start/completion turn IDs, ignores older sessions/different CWDs/partial final
  lines and rejects terminal errors without exposing their contents. No user
  history is imported; no production contract or persistence reader changes.
- Reuse that observer in the existing scripted native AI proof: all three
  native dialog completions are detected successfully without model calls.
  Requested explicit approval before another paid run; none starts automatically.
- Final model-free verification of the current harness: 843 ordinary all-target
  tests pass (809 library plus 34 integration); nine library manual/native tests,
  eighteen native/live compatibility cases and the separate paid product proof
  remain ignored. Warning-denied all-target/all-feature Clippy, formatting and
  `git diff --check` pass. The scripted native proof passes with the observer;
  the model-free long-command terminal regression also passed after its fix.
- Real-model observed results so far: source send, declined creation and
  accepted captured-context creation pass on Codex 0.166.0 / `gpt-6.1-sol`.
  The full paid scenario with native completion synchronization has not yet
  run; automatic multi-repository and read-only model behavior are not claimed.
  Await the latest explicit rerun approval, or hand off with that limit intact.
- No commit/push/release, installation or user-service/config change. All
  private paid fixtures were stopped and dropped; auth is not in repository
  artifacts. Main is unchanged; the feature and test work remains uncommitted.
- Make the shared private tmux harness disable Bash history explicitly, so its
  temporary launch commands cannot be saved in the operator's shell history.
- User now explicitly approves one full live rerun with the corrected short
  launch script and native completion observer. Reuse the existing exact
  ignored test, private auth/repositories and five-minute execution bound.
  No automatic additional paid run, commit, push or release is authorized.
- Full authenticated proof now passes on Codex 0.166.0 with advertised
  `gpt-6.1-sol` / low reasoning, in 78.41 seconds. The exact opted-in test is
  `tui::ai::live::authenticated_ai_handles_consent_context_multiple_repositories_and_readonly`
  in `tests/real_codex_compat.rs`, run with `COCO_RUN_LIVE_AI_PROOF=1` and an
  explicitly selected auth home. One source user turn plus four control prompts:
  declined creation has no effect; confirmed creation captures the correct
  independent native context and branch; auto creates the prepared workspace
  in both repositories without sending worker instructions; read-only leaves
  the exact repository/workspace inventory unchanged despite a write request.
- Native completion correlation, successful TUI exits, coordinator shutdown and
  private runtime-file cleanup all pass. Disposable state/auth is dropped.
  This closes the preceding pending real-model coverage gap for these observed
  scenarios only; it is not a general language-understanding/model guarantee.
- No production or test code changed in this last rerun. The preceding 843
  ordinary tests, warning-denied Clippy, formatting, scripted native and terminal
  regressions remain applicable; final `git diff --check` passes. No commit,
  push, release, installation or user-service/config change. Next action is
  user review or a separately authorized local commit.

## Full-slice rereview — complete, one follow-up open

- User requests another complete review of the current uncommitted slice.
  Preserve product code and all existing edits; no commit, push, release or
  additional real-model call is authorized by this review request.
- Recheck launcher/native restrictions, repository authority, exact consent,
  context create/replay/recovery, protocol compatibility, public docs and the
  expanded model-free/native/live test coverage. Apply security guidance as
  source-first review, not a separately commissioned security audit.
- Retain the verified 843 ordinary tests, native scripted/terminal proofs,
  warning-denied Clippy and the authenticated 78.41-second full smoke as prior
  evidence. Do not describe an unchanged prior run as a newly executed test.
- Reviewed the full diff: launcher/configuration/instructions, native 0.166.0
  feature precedence/tool gates, scoped repository preparation, public setup
  context and replay paths, confirmation/dispatch cancellation, public schemas,
  tests, dependencies and public documentation. No new concrete production
  defect or additional repository/consent bypass was found. This source review
  is not a full security audit or a general safety guarantee.
- **P2 output-contract defect, live-test cleanup:** the new authenticated path
  calls the shared `close_tui` at `tui/ai/live/support.rs:264`; that helper emits
  the complete captured screen on nonzero exit or timeout (`tui.rs:108,113`).
  The live runner also propagates shared `stop_daemon` errors directly
  (`tui/ai/live.rs:26-27`), whose unsuccessful-exit error embeds the daemon log
  (`real_codex_compat.rs:1426-1427`). These paths bypass the live fixture's
  allowlisted diagnostics and contradict its no-transcript/raw-error-output
  contract. This is a statically established output path, not an observed
  credential disclosure; the successful live run emitted no such dump.
- Recommended follow-up before committing: give authenticated cleanup a
  sanitized error boundary while retaining shutdown/reaping, and add a dummy-
  private-marker regression for failed TUI/daemon cleanup. Keep rich raw
  diagnostics confined to model-free synthetic fixtures. No fix was applied
  under the review-only request; branch status stays active for this follow-up.
- Verification this review: read-only source/diff inspection and
  `git diff --check` pass; no target code, ordinary/native/paid tests or builds
  were executed again. Existing test results remain prior evidence. No contents
  staged, commit, push, release or service/config change.

## Live cleanup correction — completed

- User authorizes implementing the review finding without commit/push or
  additional real-model use. Preserve existing uncommitted work.
- Add a small private, dependency-free asynchronous error boundary in the live
  fixture only. Await the existing cleanup, then replace any failure with a
  fixed stage label and no retained source/body; keep shared model-free
  diagnostics and process shutdown/reaping unchanged.
- Route both live TUI and daemon cleanup through that boundary. Regress with
  synthetic private screen/log markers, error-chain/debug formatting, pending
  cleanup completion and unchanged success values. No real credentials or
  network are needed. Validate target-controlled test execution only in a
  resource-limited, empty-environment OS sandbox with read-only source/toolchain
  and private writable scratch, following the focused security guidance.
- Added private `live/privacy`: an awaited generic cleanup boundary returns
  only the fixed terminal/coordinator failure enum. The underlying error is
  neither formatted nor retained as a source, so downstream anyhow formatting
  cannot redisclose its raw screen/log. Both authenticated cleanup call sites
  now use it; shared model-free helpers and shutdown/reaping remain unchanged.
- Three new ordinary regressions pass: terminal and coordinator dummy private
  markers never appear through Display/Debug/source chains, and a pending
  cleanup is still awaited with its success value preserved. They also pass
  as part of the freshly compiled complete `real_codex_compat` target:
  eight passed, eighteen native/paid cases deliberately ignored.
- Verification uses Rust 1.98.1 and the exact dependency artifacts selected by
  the existing successful Cargo fingerprint, compiled directly inside
  Bubblewrap rather than executing target builds on the host. Source/cache/
  toolchain are read-only; no host home, authentication, runtime sockets or
  external network is exposed. Environment starts empty with explicit safe
  build/path/temp values. Private scratch tmpfs is capped at 512 MiB, per-process
  address space at 2 GiB, CPU at 60 seconds, processes at 96, file size at
  128 MiB, file descriptors at 256 and wall time at 150 seconds. Process limits
  apply after entering the private user namespace; LLD workers are capped at two.
  Namespace-local scratch is discarded when the check exits. No target-generated
  artifact is retained or copied into the repository.
- Warning-denied Clippy on the complete affected integration target, including
  the package's nesting/function-size lints, and pinned rustfmt checking the
  integration target and its children pass. `git diff --check` passes. The
  unrelated full-suite/all-target results and the authenticated 78.41-second
  proof remain prior evidence, not newly executed checks for this correction.
- The P2 cleanup-output finding is closed. No product/API/dependency change or
  real-model rerun was needed. The new test source is intent-to-add for local
  Git-flake inclusion; no contents staged, commit, push, release, installation
  or user-service/config change. Next action is user review or an explicitly
  authorized local commit.

## Post-correction rereview — complete, one follow-up open

- User requests another review after the cleanup-output correction. Review the
  corrected boundary/call sites and related authenticated failure propagation,
  plus unchanged core consent/scope/replay contracts. No product fix, commit,
  push, release or real-model use is authorized by this review request.
- Keep the prior eight model-free integration tests, affected-target Clippy,
  formatting and earlier full/live results distinct from this source review.
- The cleanup correction is sound: both call sites await the shared helper,
  replace failures with fixed stage values and retain no raw error source.
  Unit cases cover both stages and pending/success behavior; production
  consent, repository ownership and creation replay remain unchanged.
- **P2 remaining authenticated output path:** `live.rs:36` propagates the inner
  `verify()` error directly after successful sanitized cleanup. In particular,
  `verify_context` forwards `RemoteAppServer::request("thread/read", ...)`
  errors (`live.rs:153-158`); that shared helper formats the complete native
  response frame into the error (`real_codex_compat.rs:199-202`). A rejected
  native read need not stop the coordinator, so successful cleanup does not
  mask this raw-body path. The cleanup-only guard therefore does not enforce
  the live fixture's full no-raw-error-output contract. This is a source-proven
  output path, not a claim that credentials leaked in the successful run.
- Recommended correction: put a sanitized error boundary around the entire
  authenticated proof, including setup/verification, while preserving cleanup
  execution and useful fixed stage diagnostics. Add a synthetic native-error
  regression instead of guarding individual external helpers piecemeal.
- No new production defect found in the rechecked consent/scope/replay paths.
  Verification here is source/diff inspection plus `git diff --check`; no
  build/test/model rerun or implementation fix. No contents staged, commit,
  push, release or service change. Branch stays active for the remaining guard.

## Complete live-proof output boundary — completed

- User authorizes the remaining output-boundary correction; preserve all
  existing edits. No real-model request, commit, push or release.
- Guard the entire authenticated async proof at its test entry, including
  prerequisites/setup, verification and shutdown. Unknown returned errors
  become a fixed proof-failure value with no retained source; existing safe
  cleanup stage values can survive without their enclosing raw contexts.
- Regress an actual shared native-request rejection from a private synthetic
  WebSocket peer, followed by successful cleanup. Also cover setup errors and
  preservation of safe cleanup categories without private wrapper messages.
  Use the existing resource-limited OS sandbox, no credentials/model/backend.
- The test entry now wraps the entire authenticated async proof. Unknown
  returned errors are consumed, never formatted, and replaced by a fixed
  source-free value. Existing typed cleanup failures retain only their safe
  stage; surrounding anyhow contexts are discarded. Shutdown/reaping and
  success values are unchanged. No public/product API or dependency changes.
- Red/green verification: with an identity boundary, all three new regressions
  fail at their intended assertions (eight existing tests pass). This includes
  the actual shared `thread/read` helper receiving a synthetic native error,
  followed by successful awaited cleanup. With the correction, the freshly
  compiled complete `real_codex_compat` target passes eleven ordinary tests;
  eighteen opted-in native/paid scenarios remain deliberately ignored.
- Reused the preceding empty-environment, private-network Bubblewrap setup and
  exact cached Cargo dependencies with the same resource limits. The dummy
  WebSocket peer uses isolated loopback only; no backend, auth or model call.
  Warning-denied Clippy including size/nesting checks, recursive rustfmt and
  `git diff --check` pass. No full-suite/native/paid rerun is claimed; their
  previously recorded results remain prior evidence.
- Updated canonical test-boundary/layout guidance. The remaining P2 finding is
  closed; no known required follow-up remains for this correction. Nothing
  committed, pushed, released, installed or changed in user services. Next
  action is user review or separately authorized local commit.

## Further full-slice review — complete, launch-policy follow-up open

- User requests another review, not implementation. Rechecked launcher,
  native restrictions, fixed repository authority, confirmations, public
  context/replay/recovery, tests, dependencies, docs and the complete live-proof
  error boundary. The latter now covers all returned setup/verification errors
  and retains no private source chain. No further defect found in those paths.
- **P2 project-trust transition:** `daemon/worker/control.rs:23` reads only
  effective native configuration (`includeLayers=false`). A server in a
  disabled untrusted project layer is absent from this inventory, so
  `cli/ai/config.rs:81-93` cannot add its explicit `enabled=false` override.
  Native TUI project approval reloads configuration while preserving the CLI
  overrides; recursive table merging can then activate that omitted server.
  This contradicts the control conversation's inherited-integration exclusion,
  including read-only mode. The fixture is not evidence of an actual secret
  disclosure or an unauthorized CoCo mutation.
- Model-free reproduction with installed Codex 0.166.0: start
  `codex -c 'mcp_servers={}' app-server --listen stdio://` against a synthetic
  Git repository whose `.codex/config.toml` defines an enabled dummy MCP server
  with a nonexistent executable. `config/read` with layers reports the server
  only in a disabled layer. Write `projects.<fixture>.trust_level=trusted` via
  native `config/value/write`, then repeat `config/read`: the unchanged CLI
  table no longer excludes that enabled server. No thread/model/tool was run.
- Check ran with empty allowlisted environment, private HOME/CODEX_HOME/XDG/tmp,
  read-only Nix tools and probe, no host home/auth/services, isolated network,
  128 MiB scratch, 2 GiB address space, CPU 20 seconds, processes 96, file size
  32 MiB, FDs 128 and wall time 40 seconds. Printed only version and three
  synthetic booleans; no target artifacts retained.
- Recommended narrow correction: enumerate and explicitly disable names from
  disabled project layers as well as effective configuration, preserving
  bounds, names-only transport and fail-closed parsing. Add a native
  trust-transition regression, not just a trusted-project fixture.
- `git diff --check` passes. The eleven affected-target tests/Clippy and earlier
  full/native/live results remain prior evidence, not new executions. No
  product code changed, contents staged, commit, push, release or real-model
  request. Branch is active pending the user's decision on this finding.

## Project-trust MCP exclusion correction — completed

- User authorizes the narrow correction. Include names from effective and all
  returned native config layers, including disabled project layers. Keep
  bounded sorted/deduplicated names-only transport and fail closed on missing
  or malformed layer evidence; do not alter worker MCP/profile selection.
- Extend parser regressions for disabled layers, malformed/missing evidence
  and aggregate limits. Extend the existing native scripted AI-TUI proof to
  start in an untrusted project, verify preparation, accept native folder trust
  and assert its inherited executable never starts. Exercise actual launch
  overrides and ordinary CoCo tools without a real model or operator history.
- Preserve current edits; no commit/push/release or paid rerun. Run checks in
  the existing empty-environment, resource-limited OS sandbox with read-only
  sources/tools, private scratch/auth-free HOME and isolated loopback only.
- Red native regression with the old binary fails exactly because preparation
  omits the disabled project server. After names-only enumeration, 26 focused
  ordinary tests and warning-denied Clippy pass, but the real TUI exposes an
  additional native constraint: a disabled entry without transport is invalid
  before folder trust. A names-only disable is therefore not a working fix.
- Keep a tiny names/transport-kind DTO in private `domain/control`, shared by
  the worker port and protocol without exposing native recipes. Infer kinds
  across effective/all-layer data, allow partial overlays, reject missing or
  conflicting kinds. The CLI supplies same-kind harmless declarations with
  `enabled=false`: current CoCo `--version` or an inert loopback URL. These
  remain valid after trust without copying commands, URLs, env or credentials.
- Extend the native fixture with both hidden stdio and HTTP entries, explicit
  folder approval, marker/connection checks and exact expected trust-write
  validation. The normal worker-MCP/profile path is unchanged. Follow-up
  verification is recorded below.
- Final verification passes with fresh library/all three binary builds and
  Codex 0.166.0: four adapter parser cases, six CLI launcher cases, five
  coordinator inventory cases and eleven ordinary compatibility-target cases
  pass (26 focused ordinary tests). Warning-denied Clippy for the library
  including tests and the full compatibility target, with the existing
  size/nesting gates, and recursive pinned rustfmt/diff checks pass.
- The updated opted-in native AI-TUI scenario passes in 16.38 seconds. It
  accepts folder trust after preparation, completes server-confirmed creation,
  automatic creation in both repositories and read-only inspection, verifies
  native completions, exact configuration changes and shutdown. No inherited
  stdio marker or HTTP connection appears. The proof uses the scripted
  isolated-loopback provider, not a real model or operator authentication.
  Exact compiled-test selection: `/scratch/real-compat-tests --ignored --exact
  tui::ai::native_ai_creation_requires_human_approval_and_auto_handles_two_repositories
  --nocapture --test-threads=1`. Opt-in is `COCO_RUN_REAL_CODEX_COMPAT=1`, with
  `COCO_REAL_CODEX_BINARY` selecting the installed 0.166.0 executable. Relevant
  safe overrides are `COCO_WORKSPACE_EXECUTION=shared`,
  `COCO_WORKSPACE_CONTAINMENT=process-tree`, `LC_ALL=C`, and
  `RAYON_NUM_THREADS=TOKIO_WORKER_THREADS=2`; HOME/tmp/XDG, manifest/version/bin
  compile-time values and tool PATH explicitly point to the private fixture
  or read-only pinned tool/cache paths, never the inherited user environment.
- The harness acknowledges idempotent Down/Up navigation before its single
  trust confirmation: native startup intentionally drains early keys after
  drawing that screen. No production input behavior or consent retry changes.
- Build/test isolation keeps source/toolchain/cache read-only and all outputs
  in a discarded 768 MiB scratch tmpfs, with an empty safe environment, private
  HOME/tmp/XDG, synthetic passwd/group and isolated network. Two CPU affinities,
  CPU 120 seconds, 96 processes, file size 256 MiB, FDs 256 and wall 240 seconds
  bound each check. Initial stricter build limits failed without repository
  output; splitting codegen and removing debug sections keeps linking bounded.
  Native V8 needs a large uncommitted virtual reservation: the final native
  run allows 2 TiB virtual address space while enforcing a 2 GiB data limit and
  16 MiB stack limit. No user service, credentials or live control plane is
  exposed. These are test-runner limits, not product runtime-policy changes.
- Canonical launch/metadata architecture is updated; public user behavior and
  worker setup remain unchanged, so no implementation details were added to
  user docs. The P2 trust-transition finding is closed. Other full-suite and
  real-model results remain earlier evidence; no paid rerun is claimed.
  Nothing committed, pushed, released or installed. The new domain source is
  intent-to-add only for local Git-flake inclusion; no contents staged. Next
  action is user review or a separately authorized local commit.

## Commit/merge/publication readiness assessment

- User asks for readiness only. Source/diff, local refs and release policy
  reviewed; no commit, merge, push or workflow dispatch is authorized here.
- No known unresolved finding remains. Diff whitespace is clean; the index
  contains no staged contents. Local branch/main and cached origin/main still
  share `aa8215b`; remote freshness must be checked before an actual push.
- Commit/local merge/push are technically ready. Latest changes have 27 focused
  model-free/native checks plus applicable warning-denied Clippy and formatting;
  the earlier full-suite/docs/paid results remain prior evidence, not a claim
  that every gate was rerun after the last metadata correction.
- Publication is conditional on the exact pushed main SHA passing complete
  hosted Rust/docs checks, package dry-run and Linux/macOS binary smoke, then
  the selected channel's non-publishing rehearsal. A feature commit should
  propose the next minor (expected 0.9.0); CI owns the actual version stamp.
  Keep publication separate and explicitly authorized, not an incidental push.

## Authorized delivery — completed

- User authorizes commit, merge, push and release after the readiness review.
  Preserve the reviewed slice and normal versioning; no manual version stamp,
  force push, automatic-alpha switch change or real-model rerun.
- Refresh origin, commit the feature, merge locally to main without discarding
  remote work and push. Require full Rust (including package/native binaries)
  and static-doc CI on that exact revision. Then dispatch stable rehearsal
  without publication, inspect the proposed version and publish through the
  same guarded GitHub workflow. Verify registry, tag, release assets and local
  clean-main synchronization before handing over.
- Delivered as feature commit `4abf44c` and stable v0.9.0, tagged at CI version
  stamp `b58bce1` with the exact tested feature commit as parent. Full hosted
  CI passes 852 ordinary tests, fresh dependency policy, package/Flake checks,
  native Linux/macOS binary smoke and static Pages delivery. Stable rehearsal
  `42906948761` and publishing workflow `42907430227` both pass.
- The public registry confirms codex-coordinator 0.9.0 is available and not
  yanked. Both published archive checksum files match GitHub's corresponding
  archive SHA-256 digests. Automatic alpha remains disabled. Delivery details
  are recorded in the main branch document; no production installation,
  credentials/settings change or additional real-model call was performed.
