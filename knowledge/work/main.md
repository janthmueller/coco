---
type: Working Document
title: "main: repository foundation and first architecture"
description: Tracks repository bootstrap, the Rust baseline, and early CoCo architecture decisions on main.
tags: [work, branch, bootstrap, rust, mcp, architecture]
status: active
branch: main
updated: 2026-09-05
---

# main — repository foundation

## Intended outcome

Establish the repository, documentation workflow, and first executable and
architectural baseline for CoCo.

## Active work

- [x] Initialize the repository on `main`.
- [x] Review Wuf's public/internal documentation boundary and agent guidance.
- [x] Establish a required working document for every branch and task.
- [x] Inspect Orca's public documentation and record the intended site stack
  and visual qualities.
- [x] Read and classify the CoCo product handoff.
- [x] Switch the implementation baseline from TypeScript to Rust at the user's
  direction.
- [x] Establish the initial Rust domain, persistence, Git, RPC, Codex client,
  profile, CLI, and control-MCP modules.
- [x] Evaluate per-thread worker MCP selection and Agentgateway.
- [x] Document CoCo-owned MCP registry/bindings, native Codex as the initial
  runtime, and Agentgateway as a deferred optional adapter.
- [x] Record static export and GitHub Pages as mandatory constraints for the
  eventual user-facing documentation.
- [x] Complete the first Rust vertical slice through repository registration,
  worktree creation, and Codex thread/turn startup.
- [x] Scaffold the public Next.js/Fumadocs site and its initial user-facing
  guides, concepts, reference, and integration pages.
- [x] Verify the complete static export at both `/` and the GitHub Pages
  project subpath, including browser-side search and the public-only boundary.
- [x] Add Nix apps and development-shell tooling for installing, checking,
  building, serving, and developing the documentation site.
- [x] Record the planned detached-worktree creation mode without presenting it
  as current behavior.
- [x] Replace the first public site draft: it exposed internal architecture and
  roadmap material and did not meet the strict user-facing boundary.
- [x] Separate task preparation from execution: `coco new` must not require or
  send an instruction, while `coco send` starts the first and later turns.
- [x] Replace the overlapping `show`/`watch` UX with a concise one-shot
  `coco status` and an explicit live `coco status --follow` mode.
- [x] Add `coco jump` so an operator can open the existing Codex thread in its
  managed worktree through the same App Server observed by `cocod`.
- [x] Replace the temporary Unix-only shared App Server listener with an
  authenticated loopback-WebSocket endpoint that `coco jump` can use on Linux,
  macOS, and Windows.
- [ ] Keep CoCo's CLI-to-daemon transport behind a local-IPC boundary: Unix
  sockets on Linux/macOS and a Windows named-pipe backend, without duplicating
  protocol or coordinator logic.
- [x] Create safe checkpoint commits as the foundation and subsequent vertical
  slices become independently buildable.
- [ ] After the current App Server and CLI UX slice, research and propose a
  maintainable Rust module/workspace architecture. Evaluate when large modules
  should become submodules or separate crates, and select enforceable hygiene
  checks for code complexity, dependency health, layering, and dead code before
  performing any structural refactor.
- [ ] Design task annotations and external references as a deliberate future
  feature. Decide typed versus free-form values, mutation/audit semantics,
  privacy and display rules, fork/handoff inheritance, and explicit projection
  into Codex before adding any CLI or RPC field.

## Decisions

- 2026-09-05 — Use a progressively disclosed `knowledge/` bundle for durable
  maintainer context, keeping public documentation focused on users.
- 2026-09-05 — Mirror full branch names below `knowledge/work/` so every task
  record is unambiguous and parallel branches normally touch different files.
- 2026-09-05 — Use Next.js, Fumadocs, MDX, and Tailwind for the eventual public
  site. This follows the structure and visual restraint of Orca's docs rather
  than Wuf's Astro Starlight implementation.
- 2026-09-05 — Supersede Orca as the visual reference with nuqs' open-source
  Fumadocs package: use its monochrome notebook layout, top navigation,
  centered search, flat sidebar, reading width, and typographic rhythm while
  retaining CoCo's branding and content.
- 2026-09-05 — Defer the actual site scaffold, package versions, navigation,
  and branding to a separately scheduled documentation milestone after the
  first public behavior is stable.
- 2026-09-05 — Use a single Rust crate with Tokio instead of the initial
  TypeScript/Node direction.
- 2026-09-05 — Keep worker execution profiles and MCP capability profiles as
  separate immutable snapshot dimensions, with optional presets joining them
  for user-facing convenience.
- 2026-09-05 — CoCo owns the future MCP catalog and per-thread bindings. The
  first runtime projection uses native Codex MCP configuration; CoCo will not
  build a custom MCP proxy.
- 2026-09-05 — Preserve an adapter boundary for Agentgateway, but do not add it
  as a dependency or roadmap item yet.
- 2026-09-05 — The Fumadocs application must use Next.js static export and be
  deployed through GitHub Actions to GitHub Pages. It may not require a Node.js
  server at runtime.
- 2026-09-05 — Spell repository registration `coco repo add [path]`; the nested
  namespace leaves room for later `repo list`, `repo show`, and `repo remove`
  commands without an ambiguous top-level `init`.
- 2026-09-05 — Resolve a named execution profile from
  `[profiles.<name>]` in `$CODEX_HOME/config.toml` and project that overlay into
  `thread/start.config`. The full overlay stays in memory; SQLite receives only
  its identity/hash and the App Server's allowlisted effective settings.
- 2026-09-05 — Use cursor-based `event.list` polling for the first `watch`
  implementation. Live subscription remains a compatible later transport
  optimization rather than a prerequisite for durable observation.
- 2026-09-05 — Position CoCo concretely as the coordinator of Codex tasks and
  their Git working areas. Codex App Server owns threads and turns in a caller-
  supplied `cwd`; CoCo owns repository registration, worktree lifecycle, the
  durable task binding, and client-independent orchestration.
- 2026-09-05 — Make the public/internal documentation boundary an explicit
  acceptance gate. Every rendered page must help a user evaluate, operate, or
  troubleshoot shipped behavior; architecture, rationale, roadmaps, gateway
  evaluation, and agent work remain exclusively under `knowledge/`.
- 2026-09-05 — Use one local Codex App Server listener over a private Unix
  socket for both `cocod` and interactive `coco jump` clients. A standalone
  `codex resume` process would bypass CoCo's event projection; sharing the App
  Server lets the TUI own interaction while the daemon remains subscribed to
  thread and turn notifications. Pin and contract-test the Codex version
  because the remote transport is currently documented as experimental.
- 2026-09-05 — Supersede the Unix-only App Server endpoint with an
  authenticated loopback WebSocket. Codex CLI supports the same `--remote`
  endpoint on native Windows, Linux, and macOS, whereas Codex does not expose a
  Windows named-pipe listener. Keep the capability token in private local
  runtime state and pass it to interactive clients through an environment
  variable rather than command-line arguments.
- 2026-09-05 — Do not force one transport across unrelated boundaries. CoCo's
  own privileged local RPC should use OS-native local IPC through one abstract
  interface (Unix sockets or Windows named pipes); the shared Codex App Server
  uses its cross-platform loopback-WebSocket surface because Codex TUI must
  connect to it directly.
- 2026-09-05 — Make preparation explicit: `new` creates the worktree and Codex
  thread and returns it `idle`; it accepts no instruction or open-ended
  metadata field. `send` is the only non-interactive command that starts the
  first or a later turn.
- 2026-09-05 — Remove the prerelease `goal` field instead of renaming it to a
  generic `metadata` bag. Preserve old local values only in hidden
  compatibility storage and design annotations/references separately before
  exposing them through CLI, RPC, or prompts.
- 2026-09-05 — Replace the overlapping `show` and `watch` commands with
  `status` and `status --follow`. `jump` launches the official Codex TUI in the
  recorded worktree and resumes the recorded thread through the daemon-owned
  App Server rather than creating a parallel conversation.

## Findings

- Wuf separates its concise README and user site from an internal OKF knowledge
  bundle, with `AGENTS.md` acting as a short router into that material.
- The relevant Orca reference is `stablyai/orca`. Its public docs render with
  Next.js and Fumadocs conventions and use MDX-oriented, grouped navigation.
- The installed `codex-cli 0.147.0` App Server does not accept CLI
  `--profile`; CoCo must resolve named profiles into per-thread configuration
  overlays when sharing one App Server process.
- Locally generated App Server schemas expose `config` on thread start,
  resume, and fork. Runtime isolation of different MCP selections still needs
  a pinned-version contract test before implementation depends on it.
- Agentgateway can federate MCP targets and filter tools by tool/target and
  authenticated claims, but strict per-thread policy on one endpoint requires
  a thread-distinguishing identity.
- Fumadocs documents a static search/build path, while Next.js emits the
  deployable `out/` tree with `output: "export"`. A GitHub project Pages site
  also requires build-time subpath handling rather than root-relative
  assumptions.
- The official Codex SDK/App Server surface starts, resumes, and forks thread
  history in a caller-supplied `cwd`; the documented managed-worktree creation,
  handoff, snapshot, and cleanup behavior belongs to the desktop application.
- The initial Fumadocs draft violated the repository's existing audience
  boundary by exposing architecture and future-integration material. A clean
  static build did not make that content suitable for users; the public
  information architecture and link verification both require correction.
- Native Codex CLI now supports Windows directly. The present CoCo executable
  remains Unix-only because its RPC listener is implemented with Tokio Unix
  sockets; the daemon concept itself is not platform-specific.
- The installed Codex CLI accepts a capability-token-protected loopback
  WebSocket and can initialize successfully with an isolated temporary Codex
  home. This proves the shared transport without consuming a model turn.
- The executable coordinator now serializes creation and turn-start operations
  per repository, resolves client paths back to a registered Git common
  directory, and treats Git/Codex effects as persisted saga steps.
- App Server requests are surfaced as redacted durable events and are never
  auto-approved. The approval response workflow remains intentionally open.

## Verification

- `git rev-parse --is-inside-work-tree` returned `true`.
- `git branch --show-current` returned `main`.
- All expected local documentation targets exist.
- Every non-index knowledge document has the required frontmatter and a
  non-empty `type`.
- The new MCP architecture document is linked from the engineering index and
  `AGENTS.md`; targeted trailing-whitespace checks are clean.
- `cargo test --locked --offline --all-targets` passes all 44 tests, including
  native Git worktrees, legacy-goal retirement, task preparation without a
  turn, externally started TUI turns, authenticated WebSocket bridging,
  private capability files, daemon singleton locking, exact `jump` invocation,
  SQLite recovery, RPC sockets, MCP delegation, event normalization, and
  failure retention.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` and
  `cargo fmt --all -- --check` pass.
- `nix flake check path:. --no-write-lock-file` passes.
- A process-level smoke test with isolated temporary state started `cocod`,
  initialized the installed Codex App Server over an authenticated IPv4-
  loopback WebSocket, verified `0600` runtime/state files, registered this
  checkout via `coco repo add .`, returned a versioned empty task list over the
  real Unix socket, and removed endpoint/token/socket files on shutdown. No
  model turn was started.
- The documentation TypeScript, Oxlint, and Prettier checks pass. Both the
  root and `/coco` builds export 88 static files across eight pages with static
  search, valid local links, no server artifact, and no internal knowledge.

## Open questions and handoff

- Prove native Codex per-thread MCP isolation across start, resume, and fork
  before scheduling the registry feature.
- Agentgateway is deliberately not scheduled. Reconsider it only when
  federation, centralized credential custody, independent enforcement, or
  gateway observability becomes an actual requirement.
- Add a safe approval-response command and durable pending-request model before
  calling v0 generally usable.
- Add daemon recovery through `thread/resume`; the first slice conservatively
  marks in-flight work interrupted after daemon loss.
- When the public site is scheduled, validate its production export under the
  GitHub Pages project subpath before enabling deployment from `main`.
- Revisit source layout after the current vertical slice. The review should
  distinguish ordinary Rust module directories from true Cargo package/crate
  boundaries and avoid a premature multi-crate workspace without measurable
  coupling or build-time benefits.
