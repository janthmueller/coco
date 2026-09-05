import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { createRelativeLink } from "fumadocs-ui/mdx";
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
} from "fumadocs-ui/layouts/notebook/page";

import { getMDXComponents } from "@/components/mdx";
import { source } from "@/lib/source";

type DocumentationPageProps = {
  params: Promise<{ slug?: string[] }>;
};

export const dynamicParams = false;

export default async function DocumentationPage({
  params,
}: DocumentationPageProps) {
  const { slug } = await params;
  const page = source.getPage(slug);
  if (!page) notFound();

  const Body = page.data.body;

  return (
    <DocsPage
      className="coco-docs-page pt-8 xl:pt-8 *:mx-auto *:w-full"
      toc={page.data.toc}
      full={page.data.full}
      tableOfContentPopover={{ list: { thumbBox: false } }}
    >
      <DocsTitle className="coco-docs-title">{page.data.title}</DocsTitle>
      <DocsDescription className="coco-docs-description">
        {page.data.description}
      </DocsDescription>
      <DocsBody className="coco-docs-body">
        <Body
          components={getMDXComponents({
            a: createRelativeLink(source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata({
  params,
}: DocumentationPageProps): Promise<Metadata> {
  const { slug } = await params;
  const page = source.getPage(slug);
  if (!page) notFound();

  return {
    description: page.data.description,
    title: page.url === "/docs" ? { absolute: "coco" } : page.data.title,
  };
}
