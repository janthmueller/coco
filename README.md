# CoCo

CoCo stands for **Codex Coordinator**. It gives each Codex workspace a separate
Git worktree and keeps the matching conversation with it. This lets several
pieces of work move forward without mixing their files or disturbing your
current checkout.

> **Early preview:** CoCo is intended for supervised local use and currently
> installs from source. Read the
> [current limitations](https://janthmueller.github.io/coco/docs/reference/current-limitations/)
> before using it on important work.

[Read the user guide](https://janthmueller.github.io/coco/).

## What you can do

- prepare Codex workspaces in separate branches and worktrees without starting
  them immediately;
- send work, follow its state, answer approvals or questions, or enter the same
  conversation in the Codex terminal UI;
- work across several registered repositories from one daemon;
- choose a named Codex profile when a workspace starts;
- let MCP-capable applications inspect workspaces, with sending disabled by
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
coco create feat/first --base HEAD
coco send feat/first "Inspect the project and propose one focused improvement"
coco status feat/first --follow
coco jump feat/first
```

Continue with the
[getting-started guide](https://janthmueller.github.io/coco/docs/getting-started/).
