import { readdir, readFile } from 'node:fs/promises';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputDirectory = join(projectDirectory, 'out');
const configuredBasePath = normalizeBasePath(process.env.DOCS_BASE_PATH);

const requiredPages = [
  'index.html',
  'installation/index.html',
  'getting-started/index.html',
  'guides/workspaces/index.html',
  'guides/execution-profiles/index.html',
  'guides/mcp/index.html',
  'guides/signals/index.html',
  'guides/hooks/index.html',
  'reference/cli/index.html',
  'reference/current-limitations/index.html',
];

const retiredInternalPages = [
  'concepts/daemon-and-events/index.html',
  'concepts/tasks-and-worktrees/index.html',
  'guides/tasks/index.html',
  'contributing/index.html',
  'integrations/control-mcp/index.html',
  'integrations/worker-mcp-and-agentgateway/index.html',
  'reference/paths-and-security/index.html',
];

const entries = await walk(outputDirectory);
const files = entries
  .filter((entry) => entry.kind === 'file')
  .map((entry) => entry.path);
const links = entries.filter((entry) => entry.kind === 'link');

assert(
  links.length === 0,
  `the Pages artifact contains symbolic links: ${links.join(', ')}`,
);
assert(files.includes('.nojekyll'), 'the Pages artifact is missing .nojekyll');

for (const page of requiredPages) {
  assert(files.includes(page), `the Pages artifact is missing ${page}`);
}

for (const page of retiredInternalPages) {
  assert(
    !files.includes(page) && !files.includes(`docs/${page}`),
    `the retired internal-facing public page is still exported: ${page}`,
  );
}

assert(
  files.includes('pagefind/pagefind.js') &&
    files.includes('pagefind/pagefind-entry.json'),
  'the static Pagefind search index was not exported',
);

const forbiddenPaths = files.filter((file) =>
  file
    .split('/')
    .some((segment) => segment === 'knowledge' || segment === '.agents'),
);
assert(
  forbiddenPaths.length === 0,
  `internal documentation leaked into the Pages artifact: ${forbiddenPaths.join(', ')}`,
);

const htmlFiles = files.filter((file) => file.endsWith('.html'));
const forbiddenInternalMarkers = [
  'type: Working Document',
  'main — repository foundation',
  'Agentgateway',
  'SQLite',
  'versioned RPC',
  'worker MCP architecture',
];
const fileSet = new Set(files);

for (const file of htmlFiles) {
  const contents = await readFile(join(outputDirectory, file), 'utf8');

  for (const marker of forbiddenInternalMarkers) {
    assert(
      !contents.includes(marker),
      `internal marker ${JSON.stringify(marker)} leaked into ${file}`,
    );
  }

  if (configuredBasePath) {
    for (const reference of localReferences(contents)) {
      assert(
        reference === configuredBasePath ||
          reference.startsWith(`${configuredBasePath}/`),
        `${file} contains root-local reference ${reference} outside ${configuredBasePath}`,
      );
    }
  }

  for (const reference of anchorReferences(contents)) {
    assertLocalAnchorExists(file, reference, fileSet);
  }
}

const serverArtifacts = files.filter(
  (file) =>
    file.includes('/server/') ||
    file.endsWith('required-server-files.json') ||
    file.endsWith('server.js'),
);
assert(
  serverArtifacts.length === 0,
  `server runtime artifacts were exported: ${serverArtifacts.join(', ')}`,
);

console.log(
  `Verified ${files.length} static files, ${requiredPages.length} pages, Pagefind search, ${
    configuredBasePath || 'root'
  } routing, and the public-only boundary.`,
);

function normalizeBasePath(value) {
  if (!value || value === '/') return '';
  return value.endsWith('/') ? value.slice(0, -1) : value;
}

function localReferences(contents) {
  const values = [];
  const pattern = /(?:href|src)=["'](\/[^"'#?]*)/g;
  for (const match of contents.matchAll(pattern)) {
    values.push(match[1]);
  }
  return values;
}

function anchorReferences(contents) {
  const values = [];
  const pattern = /<a\b[^>]*?\bhref=(['"])(.*?)\1/gi;
  for (const match of contents.matchAll(pattern)) {
    values.push(match[2]);
  }
  return values;
}

function assertLocalAnchorExists(sourceFile, reference, fileSet) {
  if (!reference || reference.startsWith('#')) return;

  const sourceRoute = routeForHtmlFile(sourceFile);
  const sourceUrl = new URL(
    `${configuredBasePath}${sourceRoute}`,
    'https://coco.invalid',
  );
  const target = new URL(reference, sourceUrl);
  if (target.origin !== sourceUrl.origin) return;

  assert(
    !configuredBasePath ||
      target.pathname === configuredBasePath ||
      target.pathname.startsWith(`${configuredBasePath}/`),
    `${sourceFile} links outside ${configuredBasePath}: ${reference}`,
  );

  const publicPath = configuredBasePath
    ? target.pathname.slice(configuredBasePath.length)
    : target.pathname;
  let decodedPath;
  try {
    decodedPath = decodeURIComponent(publicPath);
  } catch {
    throw new Error(`${sourceFile} contains an invalid link: ${reference}`);
  }

  const relativePath = decodedPath.replace(/^\/+|\/+$/g, '');
  const candidates = relativePath
    ? [relativePath, `${relativePath}.html`, `${relativePath}/index.html`]
    : ['index.html'];

  assert(
    candidates.some((candidate) => fileSet.has(candidate)),
    `${sourceFile} contains a broken local link ${reference}`,
  );
}

function routeForHtmlFile(file) {
  if (file === 'index.html') return '/';
  if (file.endsWith('/index.html')) {
    return `/${file.slice(0, -'index.html'.length)}`;
  }
  return `/${file}`;
}

async function walk(directory, root = directory) {
  const directoryEntries = await readdir(directory, { withFileTypes: true });
  const found = [];

  for (const entry of directoryEntries) {
    const absolutePath = join(directory, entry.name);
    const path = relative(root, absolutePath).split(sep).join('/');
    if (entry.isSymbolicLink()) {
      found.push({ kind: 'link', path });
    } else if (entry.isDirectory()) {
      found.push(...(await walk(absolutePath, root)));
    } else if (entry.isFile()) {
      found.push({ kind: 'file', path });
    }
  }

  return found;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
