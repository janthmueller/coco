import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { createServer } from 'node:http';
import { dirname, extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputDirectory = join(projectDirectory, 'out');
const indexHtml = await readFile(join(outputDirectory, 'index.html'), 'utf8');
const basePath = detectBasePath(indexHtml);
const port = Number.parseInt(process.env.PORT ?? '3000', 10);

const mimeTypes = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.pagefind', 'application/wasm'],
  ['.svg', 'image/svg+xml'],
  ['.txt', 'text/plain; charset=utf-8'],
  ['.webp', 'image/webp'],
  ['.woff2', 'font/woff2'],
  ['.xml', 'application/xml; charset=utf-8'],
]);

const server = createServer(async (request, response) => {
  try {
    const requestUrl = new URL(request.url ?? '/', 'http://localhost');
    const pathname = decodeURIComponent(requestUrl.pathname);

    if (basePath && pathname === basePath) {
      response.writeHead(308, { location: `${basePath}/` });
      response.end();
      return;
    }

    if (basePath && !pathname.startsWith(`${basePath}/`)) {
      respondNotFound(response);
      return;
    }

    const publicPath = basePath ? pathname.slice(basePath.length) : pathname;
    const candidate = await resolveStaticFile(publicPath);
    if (!candidate) {
      respondNotFound(response);
      return;
    }

    response.writeHead(200, {
      'cache-control': 'no-store',
      'content-type':
        mimeTypes.get(extname(candidate)) ?? 'application/octet-stream',
    });
    createReadStream(candidate).pipe(response);
  } catch (error) {
    response.writeHead(500, { 'content-type': 'text/plain; charset=utf-8' });
    response.end(
      error instanceof Error ? error.message : 'Unexpected preview error',
    );
  }
});

server.listen(port, '127.0.0.1', () => {
  console.log(
    `Serving the static CoCo docs at http://127.0.0.1:${port}${basePath || ''}/`,
  );
});

async function resolveStaticFile(pathname) {
  const requested = pathname.endsWith('/') ? `${pathname}index.html` : pathname;
  const candidates = [requested, `${requested}/index.html`, `${requested}.html`];

  for (const candidate of candidates) {
    const absolutePath = resolve(outputDirectory, `.${candidate}`);
    if (
      absolutePath !== outputDirectory &&
      !absolutePath.startsWith(`${outputDirectory}${sep}`)
    ) {
      return undefined;
    }

    try {
      if ((await stat(absolutePath)).isFile()) return absolutePath;
    } catch (error) {
      if (
        error &&
        typeof error === 'object' &&
        'code' in error &&
        error.code === 'ENOENT'
      ) {
        continue;
      }
      throw error;
    }
  }

  return undefined;
}

function detectBasePath(contents) {
  const match = contents.match(
    /(?:href|src)=["'](\/[^"']*)\/(?:_astro|pagefind)\//,
  );
  if (!match || match[1] === '') return '';
  return match[1];
}

function respondNotFound(response) {
  response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
  response.end('Not found');
}
