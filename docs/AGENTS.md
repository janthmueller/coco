# Public documentation boundary

Everything rendered from `src/content/docs/` is strictly user-facing.

- Write for a person evaluating or using CoCo, not for its maintainers.
- Every page must answer a concrete user question or enable a concrete action.
- Prefer a short explanation and one working example over comprehensive
  internals.
- Introduce features through the problem they solve. README and overview lead
  with parallel agents, navigation, automation, and resource control. Give
  signals and hooks visibility there; explain their setup in focused guides.
- Put exact user configuration, signal schemas, hook input/output contracts,
  retry guarantees, and measurement definitions in public reference pages.
  These help integration authors use CoCo; implementation details stay internal.
- Each guide starts with an outcome and a complete example, then explains how
  to verify it. Link to reference for edge cases instead of repeating it.
- Describe CoCo positively through the outcomes and actions it enables. Do not
  define the product or a workspace by what it is not; reserve negative wording
  for an actionable limitation or safety fact that prevents user surprise.
- Define unavoidable terms in plain language when they first appear.
- Document only verified, shipped behavior. Mention unavailable behavior only
  when a user needs the limitation to avoid surprise; do not publish a
  roadmap.
- Never include architecture, daemon/RPC/database details, protocol design,
  decision rationale, implementation plans, test strategy, agent notes,
  branch work, speculative integrations, or gateway evaluations.
- Never copy from or link users to `knowledge/`. Rewrite any shared fact for
  the user and keep internal context internal.
- Keep the navigation small. Remove a page when its purpose is explaining how
  CoCo is built rather than helping someone use it.

Internal product and engineering material belongs under `../knowledge/` and
follows the root `AGENTS.md` workflow.
