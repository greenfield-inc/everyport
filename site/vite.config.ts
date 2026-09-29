import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";

// The site's public address. Every absolute URL (og:url, og:image) comes from
// it, and Vite's base is its path, so a custom domain needs only SITE_URL.
const site = new URL(process.env.SITE_URL ?? "https://greenfield-inc.github.io/everyport/");
if (!site.pathname.endsWith("/")) site.pathname += "/";

// Release files are named after the workspace version in Cargo.toml.
const version = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8").match(/^version = "(.+)"$/m)![1];

export default defineConfig({
  base: site.pathname,
  define: { __SITE_URL__: JSON.stringify(site.href), __EVERYPORT_VERSION__: JSON.stringify(version) },
  plugins: [
    react(),
    tailwindcss(),
    { name: "site-url", transformIndexHtml: (html) => html.replaceAll("%SITE_URL%", site.href) },
  ],
  server: { port: 5198 },
});
