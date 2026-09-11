# CoCo — Codex Coordinator

Run parallel Codex agents. Keep their work organized.

CoCo gives each agent a named **workspace** with its own checkout and
conversation. Start work across repositories, see which agents need attention,
and jump back into any conversation.

- **Work in parallel.** Give fixes, investigations, and features separate
  workspaces while keeping your current checkout available.
- **Move quickly.** Find work across repositories and return to it with
  `coco jump`.
- **Stay informed.** Follow agent activity, requests for input, token usage,
  and available cost estimates.
- **Connect your tools.** Agents publish structured signals; hooks run your
  programs in response.
- **Stay in control.** Add guards before workspace close or deletion, and
  set CPU, memory, and process/thread limits on supported Linux systems.

CoCo uses Codex App Server and your existing Codex login and configuration.

[Documentation](https://janthmueller.github.io/coco/) ·
[Quickstart](docs/src/content/docs/getting-started.mdx) ·
[Automation](docs/src/content/docs/guides/signals.mdx) ·
[Resource limits](docs/src/content/docs/guides/resources.mdx)

## Install

You need Git, configured Codex, and Rust 1.98.1 or newer. Install the current
repository version with Cargo:

```bash
cargo install --locked --git https://github.com/janthmueller/coco codex-coordinator
```

See [Installation](docs/src/content/docs/installation.mdx) for published
releases, Nix, and setup.

## Start two agents

Run `cocod` in another terminal and leave it running. Inside a Git repository
with at least one commit:

```bash
coco repo add .
coco create fix/login -s "Fix the login redirect and add a regression test"
coco create review/cache -s "Review the cache for correctness issues"

coco status -a
coco jump fix/login
```

Each agent works in a separate Git checkout, called a worktree. The start
commands return while work continues. Use `jump` to read the conversation,
answer questions, or continue in the Codex terminal UI. Leaving that UI keeps
active work running.

## Build your workflow

Use [signals](docs/src/content/docs/guides/signals.mdx) to have an agent report
that a change is ready for review. Attach a
[hook](docs/src/content/docs/guides/hooks.mdx) to notify you or update another
tool. Add a [guard](docs/src/content/docs/guides/guards.mdx) when closing or
deleting work should require your own checks.

Watch [resource and token usage](docs/src/content/docs/guides/resources.mdx)
as you run more agents, and set workspace limits to control their use of your
machine.

## Project status

CoCo is alpha software for Linux and macOS, tested with Codex 0.154.0.
Resource measurements and limits are Linux features; limits require a
compatible systemd user session.

Keep `cocod` running during active work. Stopping it interrupts running turns;
your saved conversations and workspaces remain available.
See [Troubleshooting](docs/src/content/docs/reference/current-limitations.mdx)
for compatibility notes.
