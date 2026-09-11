---
type: Documentation Policy
title: Documentation boundaries and presentation
description: Defines the audiences, responsibilities, workflow, and public documentation direction for this repository.
tags: [documentation, maintenance, design, static, github-pages]
status: stable
---

# Documentation boundaries and presentation

## Purpose

The repository separates product documentation from project memory. Someone
learning the product should not have to read implementation history, agent
notes, or unfinished design discussion, while maintainers should not have to
reconstruct important decisions from commits or chat transcripts.

## Surfaces

| Surface | Audience | Responsibility |
| --- | --- | --- |
| `README.md` | First-time visitors | Product purpose, installation, smallest useful example, project status, and links onward |
| `docs/` | Product users | Task-oriented setup, guides, command reference, limitations, and troubleshooting |
| `knowledge/` | Maintainers and coding agents | Durable product semantics, architecture, constraints, rationale, roadmaps, and documentation policy |
| `knowledge/work/` | The current task owner and future maintainers | Branch-scoped work in progress, findings, decisions, checks, open questions, and handoff state |
| `AGENTS.md` | Coding agents | Short operational rules and pointers into this bundle |

“Internal” means maintainer-facing, not confidential. The repository may
become public; no documentation surface may contain secrets or private user
data.

## Public documentation rules

- Lead with what a reader can accomplish and provide a concrete next step.
- Prefer focused examples and task-oriented pages over implementation tours.
- Explain CoCo positively through the outcomes and actions it enables. Do not
  position the product or define a workspace by listing what it is not;
  negative wording belongs only in an actionable limitation or safety note
  that prevents user surprise.
- State limitations when they affect correctness, safety, privacy, cost, or a
  successful workflow.
- Keep public claims aligned with implemented and verified behavior. Describe
  unavailable behavior only when a user needs the limitation to avoid
  surprise; do not turn the public site into a roadmap.
- Avoid duplicating exhaustive reference material across the README and site.
  Choose one authoritative surface and link to it.
- Update nearby public documentation in the same change as user-visible
  behavior.
- Keep design rationale, speculative alternatives, and agent bookkeeping out
  of public pages.
- Explain the product with the smallest useful technical model: CoCo is a local
  orchestration layer around Codex App Server, and a workspace binds a Git
  worktree, Codex thread, and selected configuration. This helps users
  understand prerequisites and process lifetime; storage, RPC, schema, and
  adapter details remain internal.
- Lead public entry points with the durable user benefit: a named Codex
  workspace remains addressable outside the client that started it and can be
  supervised through CLI, the native TUI, or MCP. Worktree creation supports
  that control-plane promise but is not the product definition. Treat signals
  and their optional local hooks as an advanced integration rather than a
  competing onboarding story.
- Call commands that run after saved events **hooks** and checks that run before
  a destructive action **guards**. Do not expose `reaction` as a third public
  category; it is an internal distinction within the hook implementation.

These presentation rules apply only to `README.md` and `docs/`. Internal
knowledge should continue to state ownership boundaries, exclusions, and
non-goals explicitly whenever they are needed for sound design decisions.

## Strict public/internal boundary

`README.md` and the rendered site below `docs/` are exclusively user-facing.
Their purpose is to help a person decide whether CoCo is useful and then
install, use, and safely troubleshoot the behavior that exists. Public pages
may contain only:

- the concise product promise and intended user;
- prerequisites, installation, and a smallest successful workflow;
- user-visible commands, configuration, output, and operational concepts;
- safety notes, current limitations, and actionable troubleshooting.

The following belong exclusively under `knowledge/` and must never be copied
or linked into the public site:

- component architecture, daemon/RPC/database internals, schemas, and adapter
  design;
- decision records, trade-off analysis, implementation sequencing, and test
  strategy;
- agent coordination, branch work logs, handoff state, and maintenance notes;
- speculative integrations, future protocol design, and roadmap material.

A technical term is appropriate publicly only when the user must see or act on
it, such as a CLI command, worktree, profile, or MCP tool. Define it in plain
language at first use and omit the underlying implementation discussion. If a
topic matters to both audiences, write two audience-specific explanations;
never expose the internal document as the user explanation.

Before accepting a public documentation change, verify each page against this
gate:

1. It answers a concrete user question or enables a concrete user action.
2. A first-time user can understand it without reading architecture material.
3. It describes shipped behavior; unavailable behavior appears only as a
   concise limitation, not as a roadmap.
4. It contains no internal rationale, design exploration, work tracking, or
   links into `knowledge/`.
5. Removing any paragraph that does not help the user would make the page less
   useful; otherwise remove that paragraph.

## Internal knowledge rules

- Use index pages for progressive disclosure; link concepts rather than
  duplicating them.
- Give every non-index knowledge document YAML frontmatter with a non-empty
  `type`, a descriptive title, a short description, tags, and a status.
- Record meaningful structural or semantic changes in `knowledge/log.md`;
  ordinary typo fixes do not need log entries.
- Promote decisions that outlive a branch from its working document into a
  canonical product or engineering concept. Keep the working document as the
  historical account of the task.
- A roadmap describes candidates and sequencing, not shipped behavior.

## User-facing site direction

The public site uses **Astro Starlight + MDX**. The user explicitly replaced
the earlier Next.js and Fumadocs decision because its separate landing page and
custom application shell added more interface than this documentation needs.
Pin exact package versions and keep framework customization deliberately small.

The primary structural references are the Starlight documentation sites in the
Workfold and Azurator repositories. CoCo should feel like the same family of
focused product manuals: the overview is the site root, every other route is a
documentation page, and one sidebar provides the complete navigation. Preserve
CoCo's own wording and content rather than copying either product's prose.

Preserve these qualities:

- Starlight's restrained responsive documentation shell, built-in light and
  dark themes, accessible navigation, and static search;
- one narrow readable content column, grouped left navigation, and a quiet
  right-side table of contents;
- concise page introductions, clear “when to use” guidance, obvious next
  steps, and uncluttered prose and code blocks;
- no separate marketing landing page, oversized display heading, duplicate
  top-level navigation, or decorative component that does not help a user act;
- responsive navigation, visible focus states, semantic markup, sufficient
  contrast, and reduced-motion support.

Use Starlight's standard components before writing a custom component. Keep
site-level CSS limited to intentional product tokens or readability fixes and
do not recreate an application-style landing shell around the documentation.

## Static GitHub Pages deployment contract

CoCo is published as a public alpha. The GitHub repository is public, and the
active Documentation workflow deploys the static site to GitHub Pages from the
default branch. Changes to repository visibility or publication still require
an explicit user decision.

The user-facing documentation must continue to build as static HTML, CSS,
JavaScript, and assets. Its **GitHub Pages** deployment must not depend on a
long-running Node.js server. This remains a product requirement, not an
optional deployment optimization.

The site scaffold must therefore:

- keep Astro's static output mode and emit the complete deployable site to
  `out/`;
- use Starlight's build-time Pagefind index so search requires no server;
- pre-render every documentation route and avoid on-demand routes, server
  adapters, middleware, request-time authentication, or other runtime-only
  features;
- support both a repository project path such as `/coco/` and a later custom
  domain. Astro's build-time `base` and `site` values must come from deployment
  context rather than being scattered through content or components;
- include `.nojekyll` in the generated artifact and keep internal links safe
  under the configured repository base path;
- treat every emitted file as public. Internal `knowledge/`, working documents,
  credentials, local paths, and private operational metadata must never be
  copied into the Pages artifact.

Deployment uses a GitHub Actions Pages workflow rather than committing build
output to a `gh-pages` branch. The workflow builds
the pinned documentation dependencies, verifies the static export, uploads
only `out/` with `actions/upload-pages-artifact`, and deploys it with
`actions/deploy-pages` from the protected default branch. Pull requests build
and validate the same artifact but do not deploy it.

The initial site milestone is not complete until CI proves all of the
following:

1. a clean checkout can produce `out/index.html` and the complete route set;
2. the export contains no server bundle requirement;
3. navigation, assets, and search work when served below the repository
   subpath, not only at `/`;
4. internal-only documentation is absent from the artifact;
5. a simple local static file server can render the production export.

Primary implementation references:

- [Starlight configuration](https://starlight.astro.build/reference/configuration/)
- [Starlight sidebar navigation](https://starlight.astro.build/guides/sidebar/)
- [Astro deployment to GitHub Pages](https://docs.astro.build/en/guides/deploy/github/)
- [GitHub Pages publishing sources](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
- [GitHub Pages custom workflows](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
