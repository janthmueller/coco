import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";

import { Brand } from "@/components/brand";
import { site } from "@/lib/site";

export function baseOptions(): BaseLayoutProps {
  return {
    githubUrl: site.repositoryUrl,
    links: [
      {
        active: "nested-url",
        text: "Docs",
        url: "/docs",
      },
    ],
    nav: {
      title: <Brand />,
    },
  };
}
