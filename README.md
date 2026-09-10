# CoCo — Codex Coordinator

A local control plane for Codex work that you want to start, leave running,
and return to.

CoCo gives each piece of work a durable name. Its workspace keeps a Git
worktree, persistent Codex thread, model, and profile together. By default,
activation gives that workspace one dedicated `codex exec-server` that is
reused for later turns. Start from a short-lived CLI command, inspect or
continue from another terminal or MCP client, and return through the native
Codex terminal UI.

Codex still performs the reasoning, tool calls, approvals, and file changes.
CoCo coordinates where that work lives and how you reach it.

[Quickstart](docs/content/docs/getting-started.mdx) ·
[Workspace guide](docs/content/docs/guides/workspaces.mdx) ·
[Command reference](docs/content/docs/reference/cli.mdx) ·
[Limitations](docs/content/docs/reference/current-limitations.mdx)

> **Alpha software:** CoCo is intended for supervised local use on Linux and
> macOS. Keep `cocod` running while work is active and review the limitations
> before relying on it for important work.

## Why CoCo

Codex owns each conversation and performs the work. CoCo adds a stable layer
around several pieces of Codex work so they remain addressable outside the
client that started them:

- **Leave the initiating command.** `coco send` can return as soon as Codex
  accepts a turn while `cocod` keeps the work available.
- **Return to the exact place.** A named workspace keeps the Codex thread,
  worktree, repository, and selected settings together.
- **Supervise work from another client.** Check state, inspect current resource
  use on Linux, answer supported approvals and questions, send another
  instruction, or enter the native TUI.
- **Work across repositories.** List everything together or address one
  workspace without first changing directories.
- **Control the lifecycle safely.** Prepare without starting a model turn,
  review changes, close and reopen a worktree, or explicitly choose what to
  delete.
- **Connect the work.** Agents can publish schema-validated signals, and
  trusted local commands can react afterward or guard workspace retirement.

## Install

You need Git and a configured `codex` command on `PATH`. The current alpha is
tested with `codex-cli 0.154.0`.

### Cargo

Cargo is the recommended installation method and requires Rust 1.98.1 or
newer:

```bash
cargo install --locked \
  --git https://github.com/janthmueller/coco \
  codex-coordinator
```

The package installs `coco`, `cocod`, and `coco-mcp`.

### Nix

With Nix flakes enabled, no separate Rust toolchain is required:

```bash
nix profile add github:janthmueller/coco
```

### From a checkout

```bash
git clone https://github.com/janthmueller/coco.git
cd coco
cargo install --path . --locked
```

## Run the core workflow

Keep the coordinator open in one terminal:

```bash
cocod
```

Then, from a clean Git repository with at least one commit:

```bash
coco repo add .
coco create fix/login --base main \
  --send "Fix the login redirect and run the relevant tests"

coco status fix/login --follow
coco jump fix/login
```

`create --send` prepares a separate checkout and starts the first Codex turn.
The command returns after Codex accepts the work. `status --follow` watches
state without printing chat messages; stop it with Ctrl+C. `jump` opens the
same conversation in the native Codex UI and the workspace's worktree.

Leaving the status view or the TUI does not cancel an active turn. Add `--wait`
to `send` when you want that command to remain attached and print the final
response. Stopping `cocod` while a turn is active interrupts that turn.

By default, the first `send` or `jump` starts one dedicated
`codex exec-server` for that workspace. CoCo reuses it for later turns and
stops it when the workspace is closed or `cocod` exits normally.
Detailed `status` shows the process and, on Linux, its current process count,
memory, and CPU use. These are observations, not resource limits.

## Work with a workspace

The common commands follow one lifecycle:

| Intent                                | Command                                |
| ------------------------------------- | -------------------------------------- |
| Prepare a separate workspace          | `coco create <name>`                   |
| Start or continue a Codex turn        | `coco send <workspace> <message>`      |
| Inspect or follow current state       | `coco status [<workspace>] [--follow]` |
| Answer a supported request            | `coco decide <decision-id>`            |
| Enter the existing Codex conversation | `coco jump <workspace>`                |
| Review changes from the fixed base    | `coco diff <workspace>`                |
| Free and later restore the worktree   | `coco close` / `coco reopen`           |
| Permanently retire a closed record    | `coco delete`                          |

By default, `create` makes a `coco/<workspace>` branch. The workspace remains
prepared without a Codex thread or exec server until the first `send` or
`jump`. A fresh `jump` binds the thread only after your first interactive
action. Code base and conversation context are independent: a new workspace
can use one Git revision while inheriting context from another CoCo workspace
or exact Codex thread ID.

## Connect another application

`coco-mcp` exposes the same repository-scoped workspaces to an MCP-capable
application. Inspection is read-only by default; starting turns requires an
explicit `--allow-send` capability. See the
[MCP guide](docs/content/docs/guides/mcp.mdx).

Agents can also publish explicitly enabled, schema-validated signals such as
`review.requested`. Signals are retained updates for humans or integrations;
they do not change workspace state or wake another model by themselves.
Operator-configured hooks can run a trusted local command after a matching
signal or workspace lifecycle event. Synchronous guards can stop a checked
workspace close or deletion before it changes anything. See the
[signals guide](docs/content/docs/guides/signals.mdx) and
[hooks guide](docs/content/docs/guides/hooks.mdx).
