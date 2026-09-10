import type { ReactNode } from "react";
import { HomeLayout } from "fumadocs-ui/layouts/home";

import { baseOptions } from "@/lib/layout.shared";

export default function Home({ children }: { children: ReactNode }) {
  return (
    <HomeLayout className="coco-home-layout" {...baseOptions()}>
      {children}
    </HomeLayout>
  );
}
