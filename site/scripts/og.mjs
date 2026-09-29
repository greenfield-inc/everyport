// Renders og.html to public/og.png at 1200x630: `pnpm --filter @ppm/site og`.
// The same file is the repository's social preview (Settings, Social preview).
import { chromium } from "playwright-core";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const root = fileURLToPath(new URL("..", import.meta.url));
const server = await createServer({ root, server: { port: 5197, strictPort: true } });
await server.listen();
const browser = await chromium.launch({ channel: "chrome" });
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
  await page.goto(`http://localhost:5197${server.config.base}og.html`, { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${root}public/og.png` });
  console.log("Wrote public/og.png");
} finally {
  await browser.close();
  await server.close();
}
