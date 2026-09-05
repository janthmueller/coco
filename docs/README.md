# CoCo documentation site

This directory contains the public CoCo documentation. It uses Next.js,
Fumadocs MDX, and Tailwind CSS, but exports to ordinary static files in `out/`.
No Node.js server is required after the build.

## Local development

From the repository root:

```bash
nix run .#docs-install
nix run .#docs-dev
```

Open <http://localhost:3000>. Source pages live in `content/docs/`.

## Production export

Build the same repository-subpath shape used by GitHub Pages:

```bash
DOCS_BASE_PATH=/coco nix run .#docs-build
nix run .#docs-preview
```

The preview command detects the base path embedded in `out/index.html`; open
<http://localhost:3000/coco/>. Use an empty `DOCS_BASE_PATH` for a root domain.

`scripts/verify-export.mjs` checks the expected routes, static search output,
subpath-safe HTML references, absence of a server bundle, and the boundary that
keeps internal `knowledge/` material out of the public artifact.

## GitHub Pages

`.github/workflows/docs-pages.yml` builds pull requests without deploying and
publishes `docs/out/` from `main`. By default it derives the base path from the
repository name (for example `/coco`). To deploy at a custom root domain, set
the repository Actions variable `DOCS_BASE_PATH` to `/`; the Next config
normalizes that sentinel to an empty base path.
