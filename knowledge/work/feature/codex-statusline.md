---
type: Working Document
title: "feature/codex-statusline: optional native workspace footer"
description: Add a passive CoCo statusline provider through the native Codex MCP UI contract.
tags: [work, codex, mcp, statusline, testing]
status: complete
branch: feature/codex-statusline
updated: 2026-11-22
---

# Optional native CoCo statusline

## Intended outcome

Let a user select a read-only CoCo footer in the native Codex TUI using its
0.171.0 MCP statusline extension. Reuse existing repository-scoped control MCP,
typed local application queries and resource observations. No model calls,
automatic installation, global config rewrite, required gateway or replacement
TUI. Failure/absence must not prevent ordinary CoCo or Codex work.

Starts from local main `8de595e`, retaining Git tracking and verified Codex
compatibility. The user's approved improvement sequence is statusline first,
then measured jump latency, then evaluation (not automatic adoption) of native
environment-scoped MCP. Keep those responsibilities separate.

## Active work

- [x] Inspect the exact native discovery/resource/state/rendering contract.
- [x] Design the smallest scoped, bounded and passive workspace projection.
- [x] Implement optional MCP resource delivery and focused negative tests.
- [x] Prove actual native footer discovery/rendering/failure behavior without a
  model, including a remote App Server and the correct displayed workspace.
- [x] Update public setup/use instructions and canonical engineering knowledge.
- [x] Measure native startup phases without weakening readiness checks.
- [x] Evaluate native environment-MCP without silently enabling it.
- [x] Run bounded Rust/terminal/docs gates and review the complete slice.

## Decisions and authority

- User authorized the implementation sequence, not another commit, merge,
  push, release, real-model invocation or changes to installed services.
- No subagents. Resource-intensive Cargo/Nix gates run sequentially, with
  bounded compilation/test concurrency and Git flake reference `.` only.
- Native 0.171.0 source: statusline providers are discovered from an MCP tool's
  `_meta.openai/ui.entrypoints` and read a fixed `ui://` resource with
  `_meta.com.openai/codex-statusline-state`. The response is one application/json
  text resource containing a bounded native UI tree, not ANSI or model output.
- Do not expose stored native recipes, credentials, conversation text or raw
  backend diagnostics. MCP repository scope remains host-owned on every read.
- Polling must not start/resume threads/executors, scan transcripts, register
  repositories, generate hook events or append an audit row on each refresh.
- Advertise the UI entrypoint on the existing read-only `workspaces.status`
  tool; ordinary tool inputs/auditing stay unchanged. Add one fixed read-only
  resource, not a new changing tool, CLI command or mutation grant.
- Resolve and pin the host-selected repository ID once per MCP process. The
  private `workspace.statusline` query accepts that ID plus the displayed native
  thread ID, requires an open ready workspace bound to exactly that thread and
  never infers a binding from a name, cwd, parent or current active turn.
- Fresh/unbound or foreign threads get a harmless native-data fallback until
  the normal adoption path establishes a durable binding; the footer never
  performs adoption itself.
- The native provider replaces the complete footer. The compact renderer uses
  its supplied model/effort/context, plus optional previously selected native
  five-hour/weekly windows; CoCo contributes the workspace name and existing
  RAM/CPU measurements. It does not reproduce every built-in footer option.
- One bounded per-process cache keys the last result by exact thread, with a
  1.5-second TTL and failed-scope backoff. Resource observation is optional with
  a 200 ms budget; the complete MCP read has a 700 ms budget inside Codex's
  one-second resource deadline. No periodic background task or audit stream.
- `coco ai` disables the native extension in its invocation, alongside inherited
  tool/plugin capabilities. Its separate administrative conversation keeps the
  built-in footer instead of trying to resolve an excluded worker endpoint.
- Label cgroup memory `RAM` and process fallback `RSS`, matching the existing
  public accounting distinction. Quota uses only Codex's named account bucket
  and selected native items, never an arbitrary first bucket.

## Findings

- Existing control MCP is tools-only; its ordinary status dispatcher audits and
  requests the full workspace projection. Do not reuse that expensive/audited
  path for an automatically refreshed footer without a passive bounded query.
- The real native provider replaces the whole footer. Native TUI tests observe
  model plus CoCo measurements; context/effort composition is separately proved
  with supplied snapshots through unit and real stdio tests. Shell-only native
  history has no token report, so it must not assert a made-up context value.
- RMCP moves incoming wire `_meta` into `RequestContext.meta`; typed request
  metadata is empty on normal service dispatch. Use that canonical context,
  and verify it through real stdio/native tests, not only direct unit calls.
- Measured source path: CoCo already skips resume for a loaded subscribed
  thread and reads binding metadata without turns. Native TUI owns its later
  history rendering. Upstream independent cold resumes now overlap, but CoCo's
  repository lifecycle lock still serializes same-repository preparation.
  Removing it or bypassing readiness needs separate race/safety proofs.
- Environment-MCP separates exact environment-owned server identities,
  catalogs/resources and approvals. Current one-executor immutable thread
  bindings do not require that experimental flag. Record explicit adoption
  prerequisites in `engineering/native-codex-ui.md`; leave it off in AI.

## Verification

- Prior compatibility baseline: 995 model-free tests, native 16-test suites on
  each of Codex 0.169.0/0.171.0, Clippy, terminal and static docs gates passed.
- First focused run: 13 new statusline unit tests passed; subsequently extended
  to 16 with RSS, quota-selection and cache-expiry regressions. All 16 pass
  again after final timeout-test hardening.
- Actual Codex 0.171.0 proof passed (12.34 s): real private daemon/App Server,
  repository-scoped MCP stdio resources, source/child/source exact thread
  switches, native remote TUI, shell-only inherited context and runtime RAM.
  A second launch selected an unavailable CoCo endpoint and still rendered
  the native model, permitted Ctrl+D and left daemon health intact.
- Warning-denied all-target/all-feature Clippy passed after routing the passive
  query through the existing workspace handler rather than growing the root
  dispatch beyond its established function-size gate.
- Full regular model-free gate: 1,011 passed (963 library and 48 integration),
  no failures. The final cache-timeout regression additionally checks actual
  query counts rather than a flaky sub-100 ms wall-clock cache assertion.
- Actual AI native feature resolution, including an inherited enabled footer,
  passes for 0.169.0 and 0.171.0 without changing temporary config files.
- Full selected 0.171.0 native matrix: 17 passed in 177.94 s, including the
  repeated same-provider TUI and offline fallback. Real model scenarios were
  explicitly excluded.
- An extra attempt to assert a context percentage in the shell-only TUI fixture
  correctly found no native token report, even with a configured window. Keep
  that value absent instead of fabricating zero; strengthen real MCP stdio
  projection checks with an explicit supplied percentage and clarify evidence.
- Final focused native proof passes (4.23 s): real stdio verifies supplied
  model/effort/context plus source/child/source identities, then real TUI
  verifies warm/repeated/native rendering and restarted/offline fallback.
- Warning-denied Clippy, Rustfmt, Machete and offline Cargo Deny pass on the
  final code. Deny uses the existing advisory cache and reports only reviewed
  duplicate-family warnings; no dependencies or lockfile were changed.
- Docs check has no diagnostics. Local `/coco` static build/export verifies
  70 files, all 21 pages, Pagefind search, local anchors and public-only content.
- The existing private terminal smoke matrix passes: picker layout/navigation,
  creation defaults/Back/retries, confirmations, semantic Git badges, follow
  replacement and terminal restoration. No user tmux server was used.
- Final review: fixed resource and host-pinned exact bindings, read-only default
  permissions, no activating/history/audit path, bounded cache/fallback and
  native-only opt-in remain intact. No new dependency, storage migration,
  global configuration rewrite, generated-schema copy or CLI command.
- Jump samples in the real footer proof (milliseconds, 25 ms screen polling;
  TUI observations include private tmux/CLI startup, not just Codex):
  - First isolated sample: warm attach 73; warm header/history/footer
    4,526/5,726/5,810; restarted-daemon offline-footer 522/748/841.
  - Native matrix sample: warm attach 76; first warm TUI 355/582/610;
    repeated warm TUI with the same provider 356/556/612; restarted-daemon
    offline-footer 531/759/816.
  - Small shell-only histories, different startup/cache/provider conditions,
    no model. Not evidence of a universal bottleneck or two full history loads.
    No production readiness or immutable-profile check was removed.

## Open questions and handoff

- Fresh threads deliberately show no CoCo association before ordinary adoption;
  missing/foreign/closed bindings are covered without cwd/name inference.
- Controlled large-history and built-in/provider latency comparisons remain
  optional follow-up before optimizing readiness or repository lock scope.
  Environment-MCP adoption remains a later independently proved change.
- The implementation is local on `feature/codex-statusline`, based on
  `8de595e`. Main and installed services/config remain untouched. No staging,
  commit, merge, push, release or real model invocation was performed.

## Full-slice review — 2026-11-22

User requested review only at this stage. Production files were unchanged;
the findings below reopened completion until the subsequent authorized fixes.

- [x] P2: The new 200 ms resource timeout only bounds yielding futures. The real
  backend calls synchronous `Runtime::resources` on the Tokio worker, including
  cgroup reads or a complete `/proc` fallback scan. That work cannot be preempted
  by `tokio::time::timeout`; periodic footer reads can therefore block a daemon
  runtime thread beyond the intended budget. The separate 700 ms MCP-process
  timeout still protects the footer, not the daemon worker. Move/bound actual
  collection appropriately and test a genuinely blocking sampler rather than
  only the existing async sleep fixture.
  - Isolated Tokio mechanism proof with existing compiled dependency: a 750 ms
    synchronous body under a 200 ms timeout returns `Ok(42)` after 750 ms;
    the equivalent async sleep times out after 201 ms. No real model or user
    state involved.
- [x] P2: The new native footer proof is in the shared compatibility target but
  unconditionally requires expected version 0.171.0. The documented baseline
  command for 0.169.0 now fails before verifying/running the actual binary,
  although that release remains supported. Keep the actual footer proof on
  0.171.0 while explicitly handling feature applicability for older supported
  matrix entries (or isolate the target/update its invocation contract).
  - Reproduced with `COCO_REAL_CODEX_VERSION=0.169.0` and the exact ignored
    footer test: immediate failure, `this extension proof selects Codex 0.171.0`.
- All 16 focused statusline unit tests pass again. Repository/thread isolation,
  no activating/audit path, failure cache keys and text rendering show no further
  substantiated defect in this review. Neither finding had been implemented or
  waived at this stage; the following section records their resolution.
- The intended exact native 0.171.0 footer proof passes again in 4.09 s;
  `git diff --check` is clean. The 0.169.0 failure is the matrix applicability
  defect above, not evidence that its ordinary workspace operations fail.

## Authorized review fixes — 2026-11-22

User authorized fixing both findings, without a new commit or publication.

- [x] Offload physical resource sampling to Tokio's blocking pool, after
  acquiring the exact workspace lock. Transfer that lock and lifecycle lease
  into the owned job so caller timeout cannot overlap sampling with stop,
  replacement or close. Repeated timed-out reads must not queue more jobs for
  that workspace.
- [x] Add deterministic genuinely blocking sampler/cancellation regressions,
  unrelated-workspace responsiveness, lifecycle ordering and error cleanup.
- [x] Make footer-proof applicability explicit for the verified older native
  baselines; unknown versions must still fail rather than silently pass.
- [x] Re-run focused and full model-free Rust gates, selected real native
  compatibility proofs and code health. Update canonical lifecycle knowledge.

The synchronous sampler remains private to the executor adapter. No worker
port, API, dependency, schema or global configuration changes are needed.

- Implemented sampling offload after acquiring the exact operation lock. The
  owned job retains both guards through completion; waiting cancelled readers
  never enqueue more sampling jobs. Shutdown intentionally waits for the real
  sample rather than pretending that caller cancellation kills kernel I/O.
- All four new single-thread-runtime executor tests pass (0.31 s): genuinely
  blocking sampling respects the caller's 200 ms wait, repeated timed-out reads
  create no extra jobs, another workspace remains responsive, close/stop stay
  ordered, error/panic releases guards, and inactive reads never sample/start.
- Footer applicability now reuses the neutral verified release set: exact
  0.171.0 runs the proof; older verified selections report non-applicability;
  unknown/custom/future selections fail. Ordinary positive/negative tests cover
  this distinction. Actual selected native runs are pending verification.
- Full regular gate passes: 1,017 model-free tests (967 library and 50
  integration), no failures. The four blocking-sampler tests and both ordinary
  footer applicability/identity tests run in this gate. Latest all-target
  compilation and library execution took 110 s and 66.83 s respectively; no
  parallel heavy gates or real model tests were started.
- The actual 0.169.0 model-free matrix passes in 170.22 s: all 16 baseline
  native scenarios execute successfully, and the footer scenario explicitly
  reports non-applicability without executing a native footer. Rust reports
  17 passed test functions; this is not 17 native feature proofs. Start/fork/
  resume, real resources, restart, worker grants, TUI cleanup and turn control
  continue to pass. Latest warning-denied Clippy and Rustfmt are also green.
- Actual 0.171.0 native footer proof passes again in 4.51 s after the sampling
  fix. It still checks real stdio identity/model/context projection, warm and
  repeated native UI, daemon restart/offline fallback, resources and cleanup.
- Final Machete and offline Cargo Deny pass with the same reviewed duplicate
  warnings; no manifest/lockfile/dependency changed. `git diff --check` is clean.
- Both review findings are resolved. The executor lifecycle/sampling contract,
  narrow test seam and release-specific proof applicability are recorded in
  canonical engineering knowledge. This remains a local uncommitted slice on
  `feature/codex-statusline`; no staging, merge, push, publication, user service
  or global configuration change was performed.

## Re-review after fixes — 2026-11-22

- Re-read the complete production delta, including the new passive protocol/
  application/MCP children, renderer, sampling ownership, native applicability
  guard and related tests/documentation. No new substantiated finding or
  remaining commit blocker was identified.
- Re-ran all 16 statusline unit tests and four blocking-sampling regressions:
  all pass. The applicability regression also passes, the selected 0.169.0
  footer invocation explicitly reports non-applicability, and the actual
  0.171.0 native footer proof passes again in 4.13 s. `git diff --check` is clean.
- The full 1,017-test gate and 16 executed 0.169.0 native scenarios from the fix
  remain applicable to unchanged product code; they were not redundantly
  rerun in this review. No real model calls were used.
- Residual boundary remains explicit: an already running kernel sampling read
  cannot be forcibly cancelled and can delay its own retirement/daemon close.
  Caller deadlines, per-workspace job admission and async-worker responsiveness
  are separate from that physical I/O boundary; this is documented, not a new
  regression or a promise to kill the read.
- Review changed only this task record. No product edits, staging, commit,
  merge, push, release or installed configuration changes.

## Authorized delivery and fresh dependency gate — 2026-11-22

User authorized committing, merging, pushing and publishing through the existing
GitHub Actions process, subject to the release checks. This supersedes the
earlier no-commit/no-publication scope; it does not waive those checks.

- Fetched `origin` and tags. Before the feature checkpoint, local `main` and
  the feature branch both point to `8de595e`; `origin/main` is `f3596fd`.
  Main is two commits ahead and none behind. The existing Git-tracking and
  verified-Codex compatibility checkpoints would accompany this feature.
- GitHub reports the latest publication as `v0.12.0` and
  `COCO_RELEASE_ENABLED=false`. No channel, variable or workflow setting was
  changed. A later publication still requires successful exact-SHA CI and
  a fresh nonpublishing rehearsal before the publishing dispatch.
- The mandatory **fresh** `cargo deny check` failed after refreshing the
  advisory database. Earlier successful offline checks used cached advisories
  and are not current release clearance:
  - [`RUSTSEC-2026-0592`](https://rustsec.org/advisories/RUSTSEC-2026-0592.html):
    the locked `url 2.5.8` has an origin-recursion stack-overflow vulnerability;
    the advisory requires `url >=2.5.9`. The package is present through RMCP.
    This dependency finding alone does not establish a reachable exploit in
    CoCo, and no exploit was executed.
  - [`RUSTSEC-2026-0543`](https://rustsec.org/advisories/RUSTSEC-2026-0543.html):
    `chrono` is marked unmaintained, with no patched version. This is an
    informational maintenance advisory, not a newly demonstrated vulnerability.
    `chrono 0.4.45` is used directly and through RMCP/schemars, so replacing
    only CoCo's direct use would not remove it from the dependency graph.
- The unchanged `deny.toml` rejects both findings. No advisory ignore,
  dependency update, manifest/lockfile edit or policy weakening was made.
  Focused security triage was read-only and used the refreshed RustSec data;
  no full audit or model invocation was started.
- Delivery is paused before merge/push/release. Checkpoint the reviewed feature
  locally, then obtain direction on dependency remediation or an explicitly
  reviewed maintenance exception before resuming delivery. The feature review
  and model-free test results above remain valid; publication readiness is
  blocked by this newly refreshed dependency gate.
