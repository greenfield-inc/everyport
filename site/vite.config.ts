import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The site's public address. Every absolute URL (og:url, og:image) comes from
// it, and Vite's base is its path, so a custom domain needs only SITE_URL.
const site = new URL(process.env.SITE_URL ?? "https://greenfield-inc.github.io/port-process-manager/");
if (!site.pathname.endsWith("/")) site.pathname += "/";

export default defineConfig({
  base: site.pathname,
  plugins: [
    react(),
    tailwindcss(),
    { name: "site-url", transformIndexHtml: (html) => html.replaceAll("%SITE_URL%", site.href) },
  ],
  server: { port: 5198 },
});
