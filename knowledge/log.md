---
type: Knowledge Bundle Log
title: Project knowledge update log
description: Records significant structural and semantic updates to the project knowledge bundle.
tags: [knowledge, maintenance]
status: stable
---

# Project knowledge update log

## 2026-09-05

- **Typed daemon seam**: Centralized the closed daemon method set and typed
  request/result contracts, made the RPC client infer wire methods and response
  types, moved dispatch/error translation into a daemon handler, and removed
  coordinator-to-RPC plus CLI-to-Codex dependency leaks before splitting hot
  modules.
- **Architecture refactor safety net**: Added pinned Rust CI, forbade unsafe
  application code, made unused-dependency checks reproducible, and added a
  binary-level daemon/CLI test that exercises task preparation and a complete
  fake App Server turn before any source modules are moved.
- **Rust architecture and hygiene plan**: Audited the first vertical slice,
  kept one Cargo package, selected nested modules plus a typed daemon seam as
  the next refactor, defined objective crate-split triggers, and classified
  Clippy, dependency, coverage, mutation, and structural-analysis tools by
  whether they should gate changes or remain diagnostic.
- **Prepared-task and interactive CLI flow**: Split task preparation from
  execution so `coco new` creates an idle worktree/thread without an implicit
  instruction, `coco send` starts the first or later turn, `coco status`
  replaces the overlapping show/watch commands, and `coco jump` opens the same
  thread and worktree in the official Codex TUI. The daemon-owned App Server is
  now shared through a capability-token-protected IPv4-loopback WebSocket;
  CoCo's own Windows client transport remains a named-pipe follow-up.
- **Task metadata boundary**: Removed the prerelease `goal` field from the CLI,
  daemon contract, and task projection rather than treating one vague string as
  both intent and metadata. A future annotation/reference model is tracked for
  deliberate design; retained legacy database values stay internal and are not
  sent to Codex.
- **Public documentation boundary and presentation**: Made README and rendered
  docs strictly user-only, removed architecture and roadmap pages from the
  public tree, and selected the nuqs notebook-style Fumadocs layout as the
  primary visual reference while keeping CoCo's own branding and prose.
- **First executable Rust slice**: Wired `cocod`, `coco`, and `coco-mcp` to the
  coordinator. Repository registration, native worktree/task creation, Codex
  thread and turn startup, durable events, observation, retry protection, and
  failure retention now run through the local daemon boundary.
- **Repository command**: Selected `coco repo add [path]` and the
  `repository.register` daemon method instead of the ambiguous `coco init`.
- **Codex execution profiles**: Named profiles now resolve from
  `[profiles.<name>]` in `$CODEX_HOME/config.toml`, while only a redacted
  effective snapshot is persisted per task.
- **Static public documentation**: Fixed GitHub Pages as the deployment target
  for the future Next.js/Fumadocs site. The site must use a fully static
  export, client-side static search, project-subpath-safe assets and routes,
  and a GitHub Actions Pages deployment of only the generated `out/` artifact.
- **Worker MCP architecture**: Made CoCo authoritative for the future MCP
  catalog, capability profiles, and immutable thread bindings. Selected native
  Codex MCP configuration as the first runtime projection and explicitly
  excluded building a custom CoCo proxy.
- **Agentgateway boundary**: Documented Agentgateway as a compatible future
  data-plane adapter for federation and independent policy enforcement. It is
  not a current dependency or planned delivery item.
- **Rust migration**: Replaced the initial TypeScript/Node implementation
  direction with a single Rust crate using Tokio while retaining SQLite,
  native Git worktrees, and the Codex App Server boundary. The later shared-TUI
  slice moved that boundary from private `stdio` to authenticated loopback
  WebSocket.
- **CoCo v0 contract**: Added the concrete product specification, including
  task/thread/worktree invariants, CLI and MCP contracts, lifecycle states,
  normalized events, safety requirements, acceptance tests, and explicit
  assumptions.
- **Architecture**: Added the headless `cocod` topology and boundaries between
  CLI/MCP adapters, SQLite, Git worktrees, and the Codex App Server.
- **Implementation baseline (superseded)**: Initially recorded Node.js 24,
  pnpm, Nix flakes, SQLite, generated local App Server bindings, and a first
  local RPC direction; the Rust migration later the same day superseded the
  language/runtime portion of this entry.
- **Initialization**: Established the internal knowledge bundle and separated
  public product documentation from maintainer and agent context.
- **Branch workflow**: Required one working document per branch, with the full
  branch name mirrored below `knowledge/work/`.
- **Public documentation**: Selected a customized Next.js, Fumadocs, MDX, and
  Tailwind direction inspired by Orca's public documentation, explicitly
  excluding Astro Starlight.
