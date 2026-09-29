// Times the popover's update path with 50 servers in Chrome, before and after
// updates.rs: update size, JSON.parse, and React render with a forced layout.
//   pnpm --filter @everyport/desktop bench [updates per run]   (default 60)
// Needs Google Chrome. The delta mode applies each update with MachineUpdates,
// as the app does, and the React build is production.
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { build, preview } from "vite";

const root = fileURLToPath(new URL(".", import.meta.url));
const outDir = fileURLToPath(new URL("dist", import.meta.url));
const count = Number(process.argv[2] ?? 60);

await build({ root, configFile: false, logLevel: "warn", plugins: [react(), tailwindcss()], build: { outDir, emptyOutDir: true } });
const server = await preview({ root, configFile: false, build: { outDir }, preview: { port: 0, host: "127.0.0.1" } });
const browser = await chromium.launch({ channel: "chrome" });
try {
  const page = await browser.newPage({ viewport: { width: 420, height: 800 } });
  const kb = (bytes) => `${(bytes / 1024).toFixed(1)} KB`;
  const ms = (value) => `${value.toFixed(1)} ms`;
  console.log("| Mode | Update size, median / max | Parse, median / max | Render + layout, median / max | Rows |");
  console.log("|---|---|---|---|---|");
  for (const mode of ["full", "delta"]) {
    await page.goto(`${server.resolvedUrls.local[0]}?mode=${mode}`);
    await page.waitForFunction(() => "bench" in window && document.querySelector("[role=option]"));
    await page.evaluate(() => window.bench(10)); // warm up
    const r = await page.evaluate((n) => window.bench(n), count);
    console.log(`| ${mode} | ${kb(r.bytes.median)} / ${kb(r.bytes.max)} | ${ms(r.parse.median)} / ${ms(r.parse.max)} | ${ms(r.render.median)} / ${ms(r.render.max)} | ${r.rows} |`);
  }
} finally {
  await browser.close();
  await server.close();
}
