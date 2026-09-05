import { createMDX } from "fumadocs-mdx/next";

function normalizeBasePath(value) {
  if (!value || value === "/") return "";

  const normalized = value.endsWith("/") ? value.slice(0, -1) : value;
  if (!normalized.startsWith("/") || normalized.includes("//")) {
    throw new Error(
      `DOCS_BASE_PATH must be empty, '/', or a single absolute path such as '/coco'; received ${JSON.stringify(value)}`,
    );
  }
  return normalized;
}

const basePath = normalizeBasePath(process.env.DOCS_BASE_PATH);

/** @type {import('next').NextConfig} */
const config = {
  basePath,
  env: {
    NEXT_PUBLIC_DOCS_BASE_PATH: basePath,
  },
  images: {
    unoptimized: true,
  },
  output: "export",
  reactStrictMode: true,
  trailingSlash: true,
};

export default createMDX()(config);
