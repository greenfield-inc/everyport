import type { Metadata } from "next";

/** The site's public address, ending in "/" (next.config.ts). */
export const SITE_URL = process.env.SITE_URL!;

/** The workspace version, which names the release files (next.config.ts). */
export const VERSION = process.env.EVERYPORT_VERSION!;

export const REPO = "https://github.com/greenfield-inc/everyport";
export const RELEASES = `${REPO}/releases/latest`;

export const DESCRIPTION = "See every dev server on macOS, Windows, Linux and the boxes you SSH into, from your menu bar or tray. Free and open source.";

/** Shared by the landing page and the docs. */
export const baseMetadata: Metadata = {
  metadataBase: new URL(SITE_URL),
  description: DESCRIPTION,
  icons: { icon: { url: "/favicon.svg", type: "image/svg+xml" } },
  openGraph: {
    type: "website",
    siteName: "Everyport",
    title: "Every dev server. Every OS.",
    description: "See what's running on your ports on macOS, Windows, Linux and every box you SSH into. Free and open source.",
    images: [{ url: "/og.png", width: 1200, height: 630, alt: "Every dev server. Every OS. The Everyport popover on macOS, Windows and Linux." }],
  },
  twitter: { card: "summary_large_image" },
};
