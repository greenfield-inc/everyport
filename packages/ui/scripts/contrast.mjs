// Measures warning and destructive contrast in every theme and mode, as
// Chrome renders them: warning and danger text on the panel, and white text
// on the danger fill. Fails below 4.5:1 (WCAG AA).
//   pnpm --filter @everyport/ui contrast
import { readFileSync } from "node:fs";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const root = new URL("..", import.meta.url);
const themes = Object.keys(JSON.parse(readFileSync(new URL("src/themes/themes.json", root), "utf8")));
const server = await createServer({ configFile: new URL("vite.config.ts", root).pathname, server: { port: 0 }, logLevel: "error" });
await server.listen();
const browser = await chromium.launch({ channel: "chrome" });
const page = await browser.newPage();

const rows = [];
for (const theme of themes) {
  for (const mode of ["light", "dark"]) {
    await page.goto(`${server.resolvedUrls.local[0]}?shot&scenario=empty&theme=${theme}&mode=${mode}`);
    await page.waitForSelector(".everyport-panel");
    const ratios = await page.evaluate(() => {
      const root = document.querySelector(".everyport-root");
      const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
      // Resolve a variable to sRGB by painting it.
      const rgb = (variable) => {
        const probe = document.createElement("span");
        probe.style.color = `var(${variable})`;
        root.append(probe);
        context.fillStyle = getComputedStyle(probe).color;
        probe.remove();
        context.fillRect(0, 0, 1, 1);
        return [...context.getImageData(0, 0, 1, 1).data].slice(0, 3);
      };
      const channel = (value) => ((value /= 255) <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
      const luminance = ([r, g, b]) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
      const ratio = (a, b) => {
        const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
        return (high + 0.05) / (low + 0.05);
      };
      const panel = rgb("--popover");
      return {
        warning: ratio(rgb("--everyport-warn"), panel),
        danger: ratio(rgb("--everyport-danger"), panel),
        "white on danger": ratio([255, 255, 255], rgb("--everyport-danger-fill")),
      };
    });
    rows.push({ theme, mode, ...ratios });
  }
}
await browser.close();
await server.close();

let failed = false;
for (const mode of ["light", "dark"]) {
  for (const key of ["warning", "danger", "white on danger"]) {
    const worst = rows.filter((row) => row.mode === mode).reduce((a, b) => (b[key] < a[key] ? b : a));
    failed ||= worst[key] < 4.5;
    console.log(`${mode.padEnd(5)} ${key.padEnd(15)} lowest ${worst[key].toFixed(2)}:1 (${worst.theme})`);
  }
}
if (failed) {
  console.error("Below 4.5:1");
  process.exit(1);
}
