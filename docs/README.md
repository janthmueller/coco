# CoCo documentation site

This directory contains the public CoCo documentation. It uses Astro Starlight
and MDX and exports to ordinary static files in `out/`. No Node.js server is
required after the build.

## Local development

From the repository root:

```bash
nix run .#docs-install
nix run .#docs-dev
```

Open <http://localhost:4321>. Source pages live in `src/content/docs/`.

## Production export

Build the same repository-subpath shape used by GitHub Pages:

```bash
DOCS_BASE_PATH=/coco nix run .#docs-build
nix run .#docs-preview
```

The preview detects the built base path. Open <http://localhost:3000/coco/>.
Use an empty `DOCS_BASE_PATH` for a root domain. Set `DOCS_SITE_URL` as well
when building for another origin.

`scripts/verify-export.mjs` checks the expected routes, static search output,
subpath-safe HTML references, absence of a server bundle, and the boundary that
keeps internal `knowledge/` material out of the public artifact.

## GitHub Pages

`.github/workflows/docs-pages.yml` builds pull requests without deploying and
publishes `docs/out/` from `main`. By default it derives the base path from the
repository name (for example `/coco`). To deploy at a custom root domain, set
the repository Actions variable `DOCS_BASE_PATH` to `/` and `DOCS_SITE_URL` to
the custom origin.
