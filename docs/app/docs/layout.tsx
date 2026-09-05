import type { ReactNode } from "react";
import { DocsLayout } from "fumadocs-ui/layouts/notebook";

import { baseOptions } from "@/lib/layout.shared";
import { source } from "@/lib/source";

export default function DocumentationLayout({
  children,
}: {
  children: ReactNode;
}) {
  const shared = baseOptions();

  return (
    <DocsLayout
      {...shared}
      tree={source.getPageTree()}
      nav={{ ...shared.nav, mode: "top" }}
      sidebar={{ collapsible: false }}
    >
      {children}
    </DocsLayout>
  );
}
