import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// The dev playground: `pnpm --filter @ppm/ui dev`.
export default defineConfig({
  root: fileURLToPath(new URL("playground", import.meta.url)),
  plugins: [react(), tailwindcss()],
  server: { port: 5199 },
  build: { outDir: fileURLToPath(new URL("dist/playground", import.meta.url)), emptyOutDir: true },
});
