import "@everyport/ui/styles.css";
import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { baseMetadata } from "../../site.ts";
import "../../site.css";

export const metadata: Metadata = {
  ...baseMetadata,
  title: "Everyport: every dev server, every OS",
  alternates: { canonical: "/" },
  openGraph: { ...baseMetadata.openGraph, url: "/" },
};

export const viewport: Viewport = { themeColor: "#0b0d0b", colorScheme: "dark" };

export default function HomeLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
