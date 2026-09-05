# Repository guidance

- Before substantial product or architecture work, read `knowledge/index.md`
  and only the linked concepts relevant to the task.
- Before changing worker-facing MCP selection, profiles, or gateway support,
  read `knowledge/engineering/mcp-runtime.md`. Do not conflate that data flow
  with CoCo's own control MCP server; Agentgateway is a deferred optional
  adapter, not a current dependency.
- Before changing Rust module layout, crate boundaries, public visibility, or
  code-health gates, read `knowledge/engineering/rust-architecture.md` and
  preserve its staged dependency direction unless the same change records why
  it is superseded.
- Every branch has one working document. At the start of a task, resolve the
  current branch and create or resume `knowledge/work/<branch>.md`; preserve
  slashes as directories (for example, `feature/login` maps to
  `knowledge/work/feature/login.md`). Follow `knowledge/work/index.md`.
- Keep the branch working document current while working. Record the active
  scope, meaningful findings, decisions with rationale, verification, and
  unresolved follow-up. Never put credentials, private data, or other secrets
  in it.
- Promote durable product and engineering knowledge into the appropriate
  canonical document under `knowledge/`. The branch document remains the
  chronological task record, not the sole home of lasting decisions.
- Treat `README.md` and the rendered content below `docs/` as a strict public
  user boundary. Include only what a user needs to understand, install, use,
  or safely troubleshoot CoCo. Do not publish architecture, implementation
  internals, decision rationale, protocol/storage design, agent bookkeeping,
  speculative integrations, or roadmaps there; keep all of that under
  `knowledge/`. Do not copy or link internal knowledge into the public site.
- Every public documentation page must answer a concrete user question or
  enable a concrete user action. Use ordinary language, define unavoidable
  product terms once, and remove pages whose main purpose is explaining how
  CoCo is built rather than how it is used. Follow the scoped rules in
  `docs/AGENTS.md` for every change below `docs/`.
- Build the user-facing documentation with Next.js, Fumadocs, and MDX in the
  visual direction recorded in `knowledge/documentation.md`. It must remain a
  fully static GitHub Pages export with no runtime server dependency. Do not
  introduce Astro Starlight unless the user explicitly reverses this decision.
- Treat implemented behavior, automated tests, generated schemas, and command
  help as authoritative. Update affected public docs and internal concepts
  when behavior changes; a roadmap item is not evidence that a feature exists.
- Use the Git flake reference `.` for local Nix commands, never `path:.`:
  `path:.` includes ignored build output such as `target/` and can create
  multi-gigabyte source copies. Run resource-intensive Nix and Cargo gates
  sequentially rather than launching them in parallel.
- Before handing work over, update the branch working document so another
  session can continue without reconstructing context from chat history.
