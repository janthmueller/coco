---
type: Working Document
title: "feature/codex-compatibility: current native contracts"
description: Verify current Codex releases and restore AI control through proven configuration and runtime compatibility.
tags: [work, codex, compatibility, cli, testing]
status: complete
branch: feature/codex-compatibility
updated: 2026-11-22
---

# Current Codex compatibility

## Intended outcome

Resolve the user-reported exact-0.166.0 rejection in `coco ai`, verify the
native contracts CoCo actually consumes, and update affected compatibility
claims using isolated model-free proofs. Preserve safe configuration isolation,
thread/turn identity and restart behavior. No unrelated product overhaul.

Before this work the user authorized merging the completed Git tracking slice
locally: `main` fast-forwarded from `f3596fd` to `94bc27f`, clean, with no push.
This branch starts at that integrated commit, retaining the complete prior slice.

## Active work

- [x] Inspect installed/released versions and relevant upstream changes.
- [x] Verify AI config/feature/MCP isolation, native worker capabilities and
  consumed App Server/TUI/environment contracts before changing gates.
- [x] Implement the smallest justified compatibility changes and focused tests.
- [x] Run isolated native model-free proofs and ordinary bounded Rust/terminal
  gates; update public usage requirements and canonical knowledge.
- [x] Review the complete diff and leave an actionable verification handoff.

## Decisions and authority

- Use one neutral private explicit version allowlist rather than semver >=,
  version-string prefix matching or a protocol capability guessed from presence
  of turnId. Initially exercise 0.166.0 (prior evidence), installed 0.169.0 and
  official 0.171.0; release claims remain provisional until native proofs pass.
- Keep adapter-specific validation and strict interruption separate. Source
  review of 0.171.0 still finds an untargeted `Op::Interrupt` queued after its
  turn-ID check. Its delayed acknowledgement improves terminal ordering but
  does not establish atomic targeting; strict cancellation stays unavailable.
- Initial authority covered the compatibility slice without another commit or
  integration. After the completed re-review, the user explicitly authorized
  its local checkpoint and merge into `main`. No push, release,
  installed-service update or real-model invocation is authorized.
- Do not remove version guards without proving the isolation/capability contract.
  Strict/exact cancellation must not acquire a guarantee from a version bump.
- Read installed/local sources first, use official upstream releases/source for
  current facts, and keep any additional native binaries/source in temporary
  isolated locations rather than updating the user's installed Codex.
- No subagents. Cargo/Nix gates remain sequential and concurrency-bounded.

## Findings

- Installed executable reports `codex-cli 0.169.0`; GitHub's official latest
  stable release is `rust-v0.171.0`, published 2026-11-20. The existing source
  clone at `/home/jan/codex` is older and cannot establish these contracts yet.
- At the start, AI launch and inherited-MCP discovery were gated to 0.166.0. Separate exact
  guards also govern worker MCP setup, indexed turn lookup and native best-effort
  interruption; widening only the CLI guard would leave important features off.
- Downloaded official 0.171.0 package into a temporary directory, verified its
  GitHub-reported SHA-256 and inspected exact 0.166.0/0.169.0/0.171.0 source tags
  in a temporary clone borrowing the existing clone's objects without changing it.
- The unchanged native AI feature-resolution proof first passed on 0.171.0.
  Strengthened it for inherited JavaScript/file-image, message-board and experimental
  per-environment MCP flags, preserving only the sandboxed native MCP dispatcher.
- Upstream highlights: provider preservation on resume (0.168), delayed-abort
  fixes (0.169), per-environment MCP as an experimental opt-in (0.170), and
  remote exec, parallel resume and indexed-history/performance work (0.171).
  Validate behavior before replacing any CoCo responsibility or old fixture.

## Verification

- Baseline before this branch: Git tracking commit `94bc27f` has 994 passing
  model-free tests plus Clippy, formatting, terminal and static docs evidence.
- First model-free native 0.171.0 suite: 11 passed, 5 failed. Start/fork/resume,
  eager context capture, legacy boundaries, native hooks, separated worker MCP
  grants, exact-unsupported and best-effort turn control all passed.
- Two failed provider fixtures assume the pre-0.168 default-provider mismatch;
  current native resume preserves the saved provider. Update the proofs to
  cover this intentional upstream contract, retaining legacy rejection coverage.
- Two temporary-TUI fixtures never launch CoCo: their environment command exceeds
  the terminal input buffer and leaves Bash waiting for an unfinished quote.
  Reuse the private script transport already proven by the separate live fixture
  for every native TUI test. This is test harness behavior, not a product fallback.
- The scripted AI TUI trust navigation also differs; investigate its actual
  captured screen before changing assertions. No authenticated model invocation
  has been run or authorized.
- Confirmed the native trust screen now offers returning to the Agent Command
  Center instead of Quit; select/verify option position independently of that
  label. Keep actual trust acceptance and the post-trust inherited-server
  exclusion assertions unchanged.
- Corrected 0.171.0 suite: all 16 native model-free tests passed in 175.25 s,
  including real TUI/temporary cleanup, multi-client resume, worker setup,
  provider preservation and replay across process restart, terminal metadata,
  strict rejection and best-effort native interruption. The model/auth-free
  long-command terminal regression also passed independently.
- Provider fixture diagnostics now surface synthetic provider failures and use
  one end-to-end deadline. Provider replay checks outcome metadata, not the
  generation-bound response text; a replay after restart cannot assume that
  response text remains cached. The standalone shell-only source needs an
  explicit model on resume to avoid selecting the native default model.
- Installed 0.169.0 suite: all 16 native model-free tests passed in 190.93 s.
  The strengthened hostile-feature resolution proof passes separately on both
  0.169.0 and 0.171.0, checking actual launcher arguments in ask/auto/read-only
  modes without starting MCP children or a model. The old 0.166.0 baseline is
  retained from prior evidence, not claimed as a newly rerun authenticated proof.
- Final bounded-fixture rerun: both provider tests pass on 0.171.0 (1.26 s),
  and the standalone long terminal command proof passes (0.07 s).
- `CARGO_INCREMENTAL=0 cargo test --offline --all-targets -j 2 --
  --test-threads=2`: 995 passed (947 library plus 48 integration), no failures;
  native/TTY/resource and paid-model opt-ins remain excluded from this gate.
- `CARGO_INCREMENTAL=0 cargo clippy --offline --all-targets --all-features
  -j 2 -- -D warnings`, Rustfmt and `git diff --check`: clean.
- `python3 tests/terminal_smoke.py <current-library-test-executable>`: all
  isolated picker/defaults/Back/edit/confirmation/Git-badge/follow checks pass.
- `pnpm --dir docs run check`: zero errors/warnings/hints.
  `DOCS_BASE_PATH=/coco pnpm --dir docs run build`: 21 pages and 70 static
  files verified, including subpath links, search and the public-only boundary.
- Full diff reviewed: no API/schema/dependency change, no new native worktree
  allocator, no invented exact-interrupt guarantee. Retained 0.166.0 evidence
  remains historical. No authenticated real-model run or macOS live claim.

## Handoff and remaining boundaries

- Reviewed on `feature/codex-compatibility`, based on local `main` at `94bc27f`.
  Final user approval covers committing this complete slice and fast-forwarding
  local `main`, preserving the earlier Git-tracking commit as a separate slice.
  Delivery is local-only; no publication, installation or changes to the user's
  Codex source clone are included.
- Current supported sensitive-contract set is exact 0.166.0/0.169.0/0.171.0,
  not a semver range. Untested intermediate, future, prerelease and custom
  builds remain outside it. A future release needs these same contract checks.
- Reproduce model-free native checks with `COCO_RUN_REAL_CODEX_COMPAT=1`,
  `COCO_REAL_CODEX_BINARY=/path/to/codex`, and
  `cargo test --offline --test real_codex_compat -j 2 -- --ignored
  --skip tui::ai::live --test-threads=1`. The default expected binary is
  0.171.0; explicitly set `COCO_REAL_CODEX_VERSION=0.169.0` for that baseline.
  Feature-resolution proof uses `COCO_CODEX_BINARY` and the exact library test
  `cli::ai::tests::native::installed_codex_cannot_reenable_apps_or_memories_through_legacy_aliases`.
- Native `codex exec --remote`, workspace UI changes and experimental
  per-environment MCP do not justify replacing current CoCo ownership in this
  slice. Existing stable resume/overlay behavior is verified; no new adapter
  or gateway dependency is introduced.
- No further implementation blocker remains. Any push, release or other
  publication after local integration still requires separate user approval.

## Full-slice re-review, 2026-11-22

- Re-read every tracked change plus the new private compatibility module and
  provider fixtures. Checked shared release guards, bounded/sanitized version
  reporting, invocation-only feature restrictions, inherited MCP inventory,
  fixed worker slots, exact native target IDs and preserved strict rejection.
- Rechecked the exact upstream alias implementation (unchanged between the
  baseline and 0.171.0) and 0.169.0 interrupt validation/rejection forms.
  Delayed native acknowledgements remain covered by the existing completion,
  retry, uncertainty and real-DB-reopen regressions; no new targeting guarantee
  or automatic redispatch was introduced.
- Re-ran all targets: 995 model-free tests passed, no failures. Full Clippy,
  Rustfmt and diff whitespace checks passed. Re-ran both provider contracts
  on each of 0.169.0 and 0.171.0, plus actual hostile-feature resolution on both.
- Revalidated the existing static export with its matching
  `DOCS_BASE_PATH=/coco`: all 21 pages/70 files, links/search and public boundary
  passed. The verifier requires the same base-path environment as the build;
  an initial invocation without it was a review command mismatch, not a broken
  exported link.
- No new actionable correctness or commit-blocking findings. The prior full
  16-test native suites per version and terminal/build evidence still apply to
  the unchanged product/test code. No paid-model or fresh macOS proof was run;
  0.166.0 retains its recorded historical baseline rather than new live evidence.
- Commit-ready for separate user approval. Review changes only this working
  record; no product fixes, staging, commit, merge, push or release performed.
