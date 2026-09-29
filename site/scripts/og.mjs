// Renders the /og page to public/og.png at 1200x630: `pnpm --filter @everyport/site og`.
// The same file is the repository's social preview (Settings, Social preview).
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const root = fileURLToPath(new URL("..", import.meta.url));
const port = 5197;
const server = spawn("pnpm", ["exec", "next", "dev", "--port", String(port)], { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
await new Promise((resolve) => server.stdout.on("data", (chunk) => /Ready/.test(chunk) && resolve()));
const browser = await chromium.launch({ channel: "chrome" });
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
  await page.goto(`http://localhost:${port}/og`, { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${root}public/og.png` });
  console.log("Wrote public/og.png");
} finally {
  await browser.close();
  server.kill();
}
