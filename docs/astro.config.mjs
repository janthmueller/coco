import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const base = normalizeBasePath(process.env.DOCS_BASE_PATH);
const repositoryUrl =
  process.env.DOCS_REPOSITORY_URL ?? 'https://github.com/janthmueller/coco';
const site = process.env.DOCS_SITE_URL ?? 'https://janthmueller.github.io';

export default defineConfig({
  site,
  base: base || '/',
  outDir: './out',
  scopedStyleStrategy: 'where',
  integrations: [
    starlight({
      title: 'CoCo',
      description:
        'Coordinate persistent Codex workspaces across terminals and repositories.',
      disable404Route: true,
      customCss: ['./src/styles/custom.css'],
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: repositoryUrl,
        },
      ],
      sidebar: [
        {
          label: 'Getting started',
          items: [
            { label: 'Overview', link: '/' },
            { label: 'Installation', slug: 'installation' },
            { label: 'Quickstart', slug: 'getting-started' },
          ],
        },
        {
          label: 'Everyday work',
          items: [
            { label: 'Run and navigate agents', slug: 'guides/workspaces' },
            { label: 'Choose code and context', slug: 'guides/context' },
            { label: 'Models and profiles', slug: 'guides/execution-profiles' },
            { label: 'Monitor usage and set limits', slug: 'guides/resources' },
            { label: 'Review and clean up', slug: 'guides/cleanup' },
          ],
        },
        {
          label: 'Automation',
          items: [
            { label: 'Publish agent updates', slug: 'guides/signals' },
            { label: 'Run commands with hooks', slug: 'guides/hooks' },
            { label: 'Protect actions with guards', slug: 'guides/guards' },
            { label: 'Use CoCo through MCP', slug: 'guides/mcp' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Command reference', slug: 'reference/cli' },
            { label: 'Automation reference', slug: 'reference/automation' },
            { label: 'Signal reference', slug: 'reference/signals' },
            { label: 'Resource accounting', slug: 'reference/resources' },
            {
              label: 'Troubleshooting',
              slug: 'reference/current-limitations',
            },
          ],
        },
      ],
    }),
  ],
});

function normalizeBasePath(value) {
  if (!value || value === '/') return '';

  const normalized = value.endsWith('/') ? value.slice(0, -1) : value;
  if (!normalized.startsWith('/') || normalized.includes('//')) {
    throw new Error(
      `DOCS_BASE_PATH must be empty, '/', or a single absolute path such as '/coco'; received ${JSON.stringify(value)}`,
    );
  }
  return normalized;
}
