import type { Metadata } from "next";
import type { ReactNode } from "react";
import { DocsShell } from "../../DocsShell.tsx";
import { baseMetadata } from "../../site.ts";

export const metadata: Metadata = {
  ...baseMetadata,
  title: { template: "%s | Everyport Docs", default: "Everyport Docs" },
};

export default function DocsLayout({ children }: { children: ReactNode }) {
  return <DocsShell>{children}</DocsShell>;
}
