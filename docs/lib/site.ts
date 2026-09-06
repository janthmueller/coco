export const site = {
  name: "CoCo",
  description: "Run Codex workspaces side by side in separate Git worktrees.",
  repositoryUrl:
    process.env.NEXT_PUBLIC_REPOSITORY_URL ??
    "https://github.com/janthmueller/coco",
} as const;

export const docsBasePath = process.env.NEXT_PUBLIC_DOCS_BASE_PATH ?? "";

export function withBasePath(path: string): string {
  const normalized = path.startsWith("/") ? path : `/${path}`;
  return `${docsBasePath}${normalized}`;
}
