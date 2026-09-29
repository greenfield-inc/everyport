// Screenshots every view at 2x in dark and light mode, next to its Paper frame.
//   pnpm --filter @ppm/ui screenshots [theme]
// Writes packages/ui/screenshots/*.png and, for the default theme,
// docs/assets/popover.png (the README hero). Other themes go in screenshots/<theme>/. Needs Google Chrome; Paper's HTML loads its fonts from Google Fonts.
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const root = fileURLToPath(new URL("..", import.meta.url));
const repo = fileURLToPath(new URL("../../..", import.meta.url));
const theme = process.argv[2] ?? "pastel-dreams-green";
const hero = theme === "pastel-dreams-green";
const out = hero ? `${root}screenshots` : `${root}screenshots/${theme}`;
mkdirSync(out, { recursive: true });
mkdirSync(`${repo}docs/assets`, { recursive: true });

// Each view: how to reach it in the playground, and its Paper frame.
const VIEWS = [
  { name: "01-servers", query: "", paper: "01-servers" },
  { name: "02-detail-collapsed", query: "port=3000", paper: "02-detail-collapsed" },
  {
    name: "02b-detail-expanded",
    query: "port=3000",
    paper: "02b-detail-expanded",
    steps: async (page) => {
      await page.getByRole("button", { name: /\d+ more/ }).click();
      await page.getByRole("button", { name: /Processes/ }).click();
      await page.getByRole("button", { name: /Dot-grid menu bar icon/ }).click();
    },
  },
  {
    name: "03-clean-up",
    query: "scenario=protected",
    paper: "03-clean-up",
    steps: (page) => page.getByRole("button", { name: "Clean up" }).click(),
  },
  { name: "04-notification", query: "view=notification", paper: "04-spike-detail", selector: "[role=alert]" },
  { name: "05-machines", query: "scenario=machines" },
  {
    name: "06-other-ports",
    query: "",
    steps: (page) => page.getByRole("button", { name: /Other ports/ }).click(),
  },
];

const server = await createServer({ configFile: `${root}vite.config.ts`, server: { port: 0 }, logLevel: "error" });
await server.listen();
const base = server.resolvedUrls.local[0];
const browser = await chromium.launch({ channel: "chrome" });
const context = await browser.newContext({
  deviceScaleFactor: 2,
  viewport: { width: 560, height: 1400 },
  timezoneId: "America/Los_Angeles",
  locale: "en-US",
});
const page = await context.newPage();
page.on("pageerror", (error) => console.error("page error:", error.message));

async function shoot(url, selector, path, steps, padding = 0) {
  await page.goto(url);
  const element = page.locator(selector).first();
  await element.waitFor();
  await page.evaluate(() => document.fonts.ready);
  if (steps) await steps(page);
  await page.mouse.move(0, 0);
  await page.waitForTimeout(400);
  if (!padding) return element.screenshot({ path });
  const box = await element.boundingBox();
  await page.screenshot({
    path,
    clip: { x: box.x - padding, y: box.y - padding, width: box.width + padding * 2, height: box.height + padding * 2 },
  });
}

for (const view of VIEWS) {
  const selector = view.selector ?? ".ppm-panel";
  for (const mode of ["dark", "light"]) {
    const url = `${base}?shot&theme=${theme}&mode=${mode}&${view.query}`;
    await shoot(url, selector, `${out}/${view.name}-${mode}.png`, view.steps);
  }
  if (view.paper) {
    const frame = `file://${repo}docs/design/${view.paper}.html`;
    const paperSelector = view.selector ? 'div[style*="width: 356px"]' : 'div[style*="width: 400px"]';
    await shoot(frame, paperSelector, `${out}/${view.name}-paper.png`);
  }
  console.log(view.name);
}

// Paper | dark | light, top-aligned on one sheet per view.
await page.setViewportSize({ width: 1500, height: 1400 });
for (const view of VIEWS.filter((view) => view.paper)) {
  const column = (label, file) =>
    `<figure><figcaption>${label}</figcaption><img src="data:image/png;base64,${readFileSync(`${out}/${view.name}-${file}.png`).toString("base64")}"></figure>`;
  await page.setContent(
    `<style>body{margin:0}#sheet{background:#1b1d22;font:600 13px system-ui;color:#aaa;display:flex;gap:24px;padding:20px;align-items:flex-start;width:max-content}
      figure{margin:0;display:flex;flex-direction:column;gap:8px}img{zoom:0.5}</style><div id="sheet">` +
      column("Paper", "paper") +
      column(`@ppm/ui · ${theme} · dark`, "dark") +
      column(`@ppm/ui · ${theme} · light`, "light") +
      "</div>",
  );
  await page.evaluate(() => Promise.all([...document.images].map((image) => image.decode())));
  await page.locator("#sheet").screenshot({ path: `${out}/compare-${view.name}.png` });
}

// The README hero: the list in dark mode, default theme, with a little wallpaper around it.
if (hero) await shoot(`${base}?shot&theme=${theme}&mode=dark`, ".ppm-panel", `${repo}docs/assets/popover.png`, undefined, 24);

await browser.close();
await server.close();
console.log(`Wrote ${out}${hero ? " and docs/assets/popover.png" : ""}`);
