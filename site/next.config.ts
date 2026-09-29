import type { NextConfig } from "next";
import nextra from "nextra";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// The site's public address. Every absolute URL (canonical, og:image, sitemap,
// install commands) comes from it.
const site = new URL(process.env.SITE_URL ?? "https://everyport.dev/");

// Release files are named after the workspace version in Cargo.toml.
const version = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8").match(/^version = "(.+)"$/m)![1];

const withNextra = nextra({ contentDirBasePath: "/docs" });

const nextConfig: NextConfig = {
  env: { SITE_URL: site.href, EVERYPORT_VERSION: version },
  // The pnpm workspace root, where the packages the site imports live.
  turbopack: { root: fileURLToPath(new URL("..", import.meta.url)) },
  transpilePackages: ["@everyport/ui", "@everyport/protocol"],
  async redirects() {
    return [{ source: "/:path*", has: [{ type: "host", value: `www.${site.host}` }], destination: `${site.origin}/:path*`, permanent: true }];
  },
};

export default withNextra(nextConfig);
