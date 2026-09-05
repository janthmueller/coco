import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

import { Provider } from "@/components/provider";
import { site } from "@/lib/site";

import "./global.css";

export const metadata: Metadata = {
  description: site.description,
  title: {
    default: "CoCo — Codex Coordinator",
    template: "%s · CoCo",
  },
};

export const viewport: Viewport = {
  colorScheme: "dark light",
  themeColor: [
    { color: "#fafafa", media: "(prefers-color-scheme: light)" },
    { color: "#050505", media: "(prefers-color-scheme: dark)" },
  ],
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
