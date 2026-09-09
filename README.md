# CoCo — Codex Coordinator

Run several Codex conversations side by side without mixing their files or
losing which thread belongs to which branch.

CoCo is a local orchestration layer for the
[Codex App Server](https://developers.openai.com/codex/app-server). Codex still
reasons, asks for approval, and edits code; CoCo gives that work a durable,
named workspace with its own Git worktree.

[Documentation](https://janthmueller.github.io/coco/) ·
[Quickstart](https://janthmueller.github.io/coco/docs/getting-started/) ·
[Current limitations](https://janthmueller.github.io/coco/docs/reference/current-limitations/)

> **Alpha software:** CoCo is intended for supervised local use on Linux and
> macOS. Read the current limitations before using it on important work.

## What CoCo adds

A CoCo workspace keeps together:

- a registered Git repository;
- a separate worktree with a new, existing, or detached Git binding;
- one persistent Codex thread after work begins; and
- the model and Codex profile selected for that thread.

This lets you prepare work without starting a model turn, inspect or follow
state from another terminal, wait for a turn's response, answer Codex
approvals and questions, and later enter the same conversation through the
normal Codex terminal UI. Workspaces can
also be found across multiple repositories. Their Git base and Codex
conversation context can be selected independently, including from another
CoCo workspace or an exact native Codex thread ID.
When a worktree is no longer needed, it can be closed and later restored
without deleting its branch or conversation; permanent record, branch, and
thread deletion remain separate choices.

Agents can also publish explicitly enabled, structured signals such as
`review.requested`. Read or follow those updates from the CLI or MCP, with
optional payload validation. Signals report a claim; they do not automatically
start work or mark a task complete.

## Install

You need Git and a configured `codex` command on `PATH`. Cargo installation
requires Rust 1.98.1 or newer; Nix is an alternative that supplies the build
toolchain.

### Cargo

Install the current alpha from its Git repository:

```bash
cargo install --locked \
  --git https://github.com/janthmueller/coco \
  codex-coordinator
```

The Cargo package is named `codex-coordinator`. It installs all three commands:
`coco`, `cocod`, and `coco-mcp`.

### Nix

With Nix flakes enabled, Rust does not need to be installed separately:

```bash
nix profile add github:janthmueller/coco
```

### From a checkout

```bash
git clone https://github.com/janthmueller/coco.git
cd coco
cargo install --path . --locked
```

## Start CoCo

Keep the coordinator running in one terminal:

```bash
cocod
```

Then use `coco` from another terminal. Each CLI command exits after returning
its result; `cocod` stays alive so active Codex work remains reachable by later
commands and by `coco jump`. The alpha does not install or enable a background
service automatically.

## Create your first workspace

From a clean Git repository with at least one commit:

```bash
coco repo add .
coco model list

coco create feat/first --base HEAD \
  --send "Inspect the project and propose one focused improvement"
coco status
coco status feat/first --follow
coco send feat/first "Summarize the result" --wait
coco jump feat/first
```

`create` prepares only the separate checkout and Git binding. `--send` creates
the Codex thread with its first real turn. A first `jump` can instead create it
interactively; leaving an empty TUI keeps the workspace prepared. `status`
shows state only. Add `--wait` to `send` when that command should stay attached
and print the final response. `status --follow` keeps its live view open until
Ctrl+C. Leaving it, stopping a wait, or leaving a TUI opened by `jump` does not
cancel an active turn; stopping `cocod` while it is active does.

Continue with the
[quickstart](https://janthmueller.github.io/coco/docs/getting-started/) or the
[workspace guide](https://janthmueller.github.io/coco/docs/guides/workspaces/).
