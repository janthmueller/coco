"use client";

import type { ReactNode } from "react";
import { RootProvider } from "fumadocs-ui/provider/next";

import StaticSearchDialog from "@/components/search";

export function Provider({ children }: { children: ReactNode }) {
  return (
    <RootProvider
      search={{ SearchDialog: StaticSearchDialog }}
      theme={{ defaultTheme: "system", enableSystem: true }}
    >
      {children}
    </RootProvider>
  );
}
