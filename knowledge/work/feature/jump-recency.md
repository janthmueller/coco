---
type: Working Document
title: Recent-first jump picker
description: Reuses native Codex recency for a compact, stable jump selection with relative times and exceptional branch details.
tags: [work, cli, jump, recency, codex, testing]
status: complete
branch: feature/jump-recency
updated: 2026-11-05
---

# Recent-first jump picker

## Local checkpoint

The user confirms relative-only picker times and authorizes committing the
reviewed slice. The local checkpoint is titled
`feat(cli): sort jump picker by native recency` on `feature/jump-recency`,
directly above turn-control checkpoint `0946f64`. Include all reviewed source,
test and documentation children. No product or test changes follow the final
passing review; only delivery notes are updated. No push, merge, publication,
installation change or real model call is authorized.

## Full post-implementation review

The user requests a full review of the uncommitted picker slice on top of
`0946f64`. Inspect all tracked and untracked changes, native recency validation
and passive projection, ordering/identity/scope, compact branches, terminal
layout and cleanup, JSON compatibility and documentation. Use isolated
model-free checks only. No product fixes, staging, commit, push, merge,
publication or real user runtime changes are authorized by this review.

### Review conclusion

No new actionable findings. Reviewed all tracked/untracked production and test
changes against the agreed jump-only behavior and public contracts. Optional
native dates fail independently of status, unit conversion is checked, recent
ordering is global within the chosen scope, missing/tied values are stable,
and in-place row sorting preserves exact selected IDs. Other pickers and
status/list retain their policies. Narrow/default/Unicode layout, dim suffixes,
branch exceptions, noninteractive/explicit inputs and JSON schema 17 agree
with tests, command help and documentation. Future-clock clamping, frozen
choices and lower-priority time display are explicit choices, not new defects.

Verification repeated: 670 library tests, six process scenarios and eleven
status-query tests pass. All-target/all-feature warning-denied Clippy,
formatting and `git diff --check` pass. Installed Codex is rechecked as 0.166.0;
all ten isolated model-free compatibility checks pass serially in 45.64s.
Three real CLI PTY scenarios pass again: recent arrow/j/k/Enter choice,
name-order immediate numeric choice, and narrow Escape cancellation, including
the exact outgoing ID and restored terminal state. The static artifact again
passes verification of 18 pages/64 files, Pagefind, `/coco` routes and the
public-only boundary. Public sources are unchanged in this review, so the
already-passing full site build/typecheck was not repeated. Dependency inputs
are unchanged; the prior dependency gates remain applicable. Paid inference
and seven ignored manual library probes are deliberately not run.

Ready for a local checkpoint from the review perspective. Only this working
document changed during review; no product/test fixes, staging, commit, push,
merge, publication, user-runtime change or real model call.

## Intended outcome

The user approves implementing the discussed jump picker improvements and
requests thorough testing. Start from clean local checkpoint `0946f64`, keeping
the completed turn-control branch intact. No commit, push, release, installation
change, real user daemon/workspace mutation or real model call is requested.

## Active work

- [x] Carry optional native `recencyAt` through the existing passive thread
  summary projection without another read, transcript load or storage schema.
- [x] Default only the jump picker to recent-first ordering across the selected
  scope, with `--sort name` as an explicit alternative and stable tie-breaks.
- [x] Render compact dim right-aligned relative times, frozen at picker open;
  omit unknown times and hide the time before sacrificing main content.
- [x] Omit the branch only on jump rows whose actual branch exactly matches
  the workspace name; retain different/legacy branches, detached and global
  repository disambiguation. Preserve other commands' presentation/order.
- [x] Add focused adapter, projection, sorting, rendering, parser and real
  interactive CLI proofs; run full Rust and static docs checks sequentially.
- [x] Update canonical product/engineering knowledge and affected user docs.

## Decisions and findings

- Codex 0.166.0's freshly generated local schema exposes optional `recencyAt`
  in thread summaries, distinct from general `updatedAt`, and native recency
  ordering. Reuse that field; do not invent a separate last-jump history or
  present it as the timestamp of the last assistant message.
- The current generic picker inherits Store's general workspace-update order;
  list/status apply their own repository/name sorting. Keep those commands
  unchanged and confine the new policy to jump selection.
- Missing/malformed native recency must not make a workspace unreadable. Known
  native dates sort first, missing values use deterministic name/repository/ID
  tie-breaks; do not substitute management timestamps as activity evidence.
- English compact relative labels match the existing CLI language. Sorting and
  labels are a snapshot, not live status or a countdown.
- Optional presentation data is validated independently of native status and
  normalized to Unix milliseconds. Public CLI JSON advances to version 17 for
  `threadRuntime.recencyAtMs`; RPC and SQLite versions/semantics stay unchanged.
  Unknown values omit the label and sort last; future clock skew shows just
  now rather than a negative age. The time is lower priority than the complete
  existing label and details, including global repository disambiguation.

## Verification

- Seventeen new permanent automated tests cover native missing/malformed/
  overflowing recency, passive projection without persistence or loading,
  cross-repository primary ordering, natural-name/missing/tie behavior, branch
  exceptions, compact time boundaries, Unicode/narrow/default/color rendering,
  scope and exact ID selection, cancellation, explicit/noninteractive targets,
  name override and unchanged status/list ordering and columns.
- `cargo test --locked --offline --all-targets --jobs 2 --quiet --
  --test-threads=2`: 670 library tests, six process scenarios and eleven
  status-query tests pass. A new mock originally expected explicit false flags;
  the request correctly omits them. The assertion now checks the decoded typed
  defaults rather than inventing a wire requirement.
- `cargo clippy --locked --offline --all-targets --all-features --jobs 2 --
  -D warnings`, formatting and `git diff --check`: pass. Reused one existing
  jump-test thread fixture instead of expanding that test beyond the existing
  function-size gate; no lint exception or behavior change.
- All ten opt-in model-free native compatibility checks pass serially against
  installed Codex 0.166.0 in 46.33 seconds. Native recency is present in fresh,
  restarted/unloaded and resumed projections, normalized to milliseconds and
  unchanged by passive reads or quiet resume. No inference or real account
  authentication was used.
- Three real-binary PTY scenarios use a temporary fake RPC socket and stop at
  a deliberate attach error before starting any real Codex UI/workspace:
  recent ordering with arrow/j/k/Enter selection; name ordering with immediate
  numeric selection; and Escape cancellation at 34 columns. Relative times
  align at 100 columns and disappear when narrow, equal branches are omitted,
  exceptional branch/global path stay visible, selection sends the correct
  exact ID, and canonical input/echo/cursor visibility are restored. The
  temporary driver is not added to the repository or the dependency list.
- `cargo machete`: no unused dependencies. Offline deny passes advisories,
  bans, licenses and sources with unchanged duplicate-version warnings. No
  dependency changes. Advisory cache locking and isolated PTY/loopback fixtures
  used scoped sandbox access; no real user runtime or repository was touched.
- Docs check: zero errors, warnings and hints. Static `/coco` build verifies
  18 pages/64 files, Pagefind, local routes and the public-only boundary. Build
  required package-manager cache access; no deployment. Rendered CLI synopsis
  also checked for the literal `recent|name` alternatives.
- Seven manual library probes and paid/authenticated inference remain skipped;
  the independent real CLI PTY checks above exercise the new interaction.

## Open questions and handoff

- Scope is complete locally on `feature/jump-recency`, based on `0946f64`.
  The prior turn-control branch is unchanged. Existing workspace branches,
  status/list presentation, other pickers, user project files and runtime
  services were not migrated or reconfigured. Native recency represents Codex's recent
  use, not a promise about last assistant-message time or last jump.
- The user subsequently authorizes the local checkpoint described above.
  Its source/test children are included, so Git-flake source capture can see
  them. Matching CLI and daemon builds are needed to receive native recency;
  no running user service was switched by this task. No push, merge, release
  or installation change is included.
