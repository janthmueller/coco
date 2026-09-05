# CoCo

CoCo stands for **Codex Coordinator**. It lets you run several Codex tasks in
one repository without mixing their files. Every task gets a separate Git
worktree, while your current checkout stays untouched.

> **Early preview:** CoCo is intended for supervised local use and currently
> installs from source. Read the
> [current limitations](https://janthmueller.github.io/coco/docs/reference/current-limitations/)
> before using it on important work.

[Read the user guide](https://janthmueller.github.io/coco/).

## What you can do

- prepare Codex tasks in separate branches and worktrees without starting
  them immediately;
- send work, follow its state, or enter the same conversation in the Codex
  terminal UI;
- choose a named Codex profile when a task starts;
- let MCP-capable applications inspect tasks, with sending disabled by
  default.

## Install from source

You need Git, Nix with flakes enabled, and a configured `codex` executable on
`PATH`.

```bash
nix develop
cargo install --path . --locked
```

## Try it

Keep CoCo running in one terminal:

```bash
cocod
```

Then, from a clean Git repository with at least one commit:

```bash
coco repo add .
coco new first-task --base HEAD
coco send first-task "Inspect the project and propose one focused improvement"
coco status first-task --follow
coco jump first-task
```

Continue with the
[getting-started guide](https://janthmueller.github.io/coco/docs/getting-started/).
