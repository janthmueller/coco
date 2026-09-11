---
type: Working Document
title: "coco/docs/fuma2starlight: migrate public docs to Starlight"
description: Active scope and handoff state for the Starlight documentation migration.
tags: [work, branch, documentation]
status: complete
branch: coco/docs/fuma2starlight
updated: 2026-09-11
---

# coco/docs/fuma2starlight — migrate public docs to Starlight

## Intended outcome

Replace the Next.js and Fumadocs application with one focused Astro Starlight
documentation site. Preserve CoCo's public content boundary and produce a
fully static GitHub Pages artifact under the repository subpath.

## Active work

- [x] Review the current documentation system and the Starlight setups in
  Workfold and Azurator.
- [x] Replace the documentation toolchain and site configuration.
- [x] Move the existing user documentation into Starlight's content model and
  remove the separate landing page.
- [x] Adapt static-export verification, Nix checks, and Pages automation.
- [x] Update canonical documentation policy and contributor guidance.
- [x] Verify type checks, the production export, base-path links,
  and repository-level gates affected by the migration.

## Decisions

- 2026-09-11 — Adopt Astro Starlight following the user's explicit reversal of
  the earlier Fumadocs decision. The simpler unified documentation shell better
  fits the desired site than a separate marketing homepage plus docs app.
- 2026-09-11 — Use the established Workfold and Azurator Starlight structures
  as local references while retaining CoCo's stricter static-export and
  public-boundary verification.
- 2026-09-11 — Keep `docs/out` as the generated artifact so the existing Pages
  upload boundary and repository tooling need no unnecessary path migration.
- 2026-09-11 — Retain a dependency-free static preview helper because Astro's
  preview server mounts generated files at `/` instead of reproducing a Pages
  project subpath. The helper detects and serves the configured base path.
- 2026-09-11 — Rebase onto local `main` and carry its newly shipped `usage`
  documentation into the relocated Starlight pages. Dropping those edits with
  the retired Fumadocs paths would make public documentation stale.

## Findings

- The current public content is already task-oriented and can be retained; the
  framework-specific application shell accounts for most of the removable
  complexity.
- Astro requires `site` and a repository `base` for project-style GitHub Pages
  URLs. Starlight provides the sidebar and static search without a custom API
  route.
- Astro's built-in preview serves generated files at its local root rather than
  mounting the configured Pages base. The retained static helper reproduces
  the `/coco/` mount and successfully serves Starlight and Pagefind assets.
- An existing `out/` directory must be excluded from the Astro TypeScript
  project; otherwise `astro check` diagnoses generated Starlight bundles and
  produces irrelevant framework hints after a production build.
- A 1600-by-1000 headless browser render confirmed the intended standard
  Starlight layout: compact header, grouped sidebar, readable content column,
  and table of contents without a separate landing experience.

## Verification

- `nix run .#docs-check` — passed with zero errors, warnings, or hints.
- `nix run .#docs-build` — built and verified ten routes plus Pagefind at the
  site root.
- `DOCS_BASE_PATH=/coco nix run .#docs-build` — built and verified all routes,
  assets, links, search output, and the public-only boundary below `/coco`.
- `nix run .#docs-preview` plus HTTP requests — returned 200 for `/coco/`, the
  workspace guide, and `pagefind/pagefind.js`.
- `actionlint .github/workflows/docs-pages.yml` — passed.
- `git diff --check` — passed.
- `nix flake check` — all compatible-system checks passed.
- After rebasing, `nix run .#docs-check` again passed with zero diagnostics and
  `DOCS_BASE_PATH=/coco nix run .#docs-build` again verified all ten routes.

## Open questions and handoff

- No implementation blocker remains. The branch is ready for user review and
  a commit when requested.
- The branch was rebased onto local `main` at `eb42a61`; Main's subsequently
  added workspace-usage documentation was merged into the moved Starlight
  content.
