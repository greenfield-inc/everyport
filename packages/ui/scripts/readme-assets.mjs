// Builds the README images from the view screenshots.
//   pnpm --filter @ppm/ui screenshots && pnpm --filter @ppm/ui readme-assets
// Writes docs/assets/{banner,hero,logo}.png and docs/assets/screens/*.png.
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const shots = fileURLToPath(new URL("../screenshots/", import.meta.url));
const repo = fileURLToPath(new URL("../../../", import.meta.url));
const assets = `${repo}docs/assets/`;
mkdirSync(`${assets}screens`, { recursive: true });

const img = (name) => `data:image/png;base64,${readFileSync(`${shots}${name}.png`).toString("base64")}`;

// The tray icon: a 5x5 dot grid with the colon lit.
const grid = (size, dot, on, off) => {
  const gap = size / 5;
  const cells = [];
  for (let r = 0; r < 5; r++)
    for (let c = 0; c < 5; c++) {
      const lit = c === 2 && (r === 1 || r === 3);
      cells.push(`<circle cx="${c * gap + gap / 2}" cy="${r * gap + gap / 2}" r="${dot}" fill="${lit ? on : off}"/>`);
    }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">${cells.join("")}</svg>`;
};

const base = `
  * { box-sizing: border-box; margin: 0; }
  body { font-family: -apple-system, "SF Pro Display", "Segoe UI", system-ui, sans-serif; -webkit-font-smoothing: antialiased; }
`;

const pages = {
  banner: {
    size: [1280, 360],
    html: `<style>${base}
      body { width: 1280px; height: 360px; background: radial-gradient(120% 140% at 15% 0%, #1d2a22 0%, #101310 55%, #0b0d0b 100%); color: #f2f4f0; display: flex; align-items: center; padding: 0 96px; gap: 56px; overflow: hidden; position: relative; }
      .dots { position: absolute; inset: 0; background-image: radial-gradient(#ffffff14 1.2px, transparent 1.3px); background-size: 22px 22px; mask-image: linear-gradient(90deg, transparent 35%, #000 100%); }
      .ports { position: absolute; right: 64px; top: 40px; font: 500 22px ui-monospace, "SF Mono", Menlo, monospace; color: #ffffff2e; line-height: 2.1; text-align: right; }
      .ports b { color: #7fd89d; font-weight: 500; } .ports i { color: #ffb224; font-style: normal; }
      h1 { font-size: 64px; font-weight: 700; letter-spacing: -1.5px; }
      p { margin-top: 14px; font-size: 25px; color: #b9c2b8; }
      .logo { flex: none; position: relative; }
    </style>
    <div class="dots"></div>
    <div class="ports">:3000 <b>next dev</b><br>:5173 <b>vite</b><br>:6006 <i>storybook</i><br>:8000 uvicorn<br>:5432 postgres</div>
    <div class="logo">${grid(150, 11, "#7fd89d", "#ffffff33")}</div>
    <div><h1>Port Process Manager</h1><p>Every dev server on every machine, one click from your menu bar.</p></div>`,
  },
  hero: {
    size: [1600, 820],
    html: `<style>${base}
      body { width: 1600px; height: 820px; background: linear-gradient(160deg, #1b2620 0%, #0e110f 60%, #151a24 100%); overflow: hidden; position: relative; }
      .bar { height: 34px; background: #0009; display: flex; align-items: center; justify-content: flex-end; gap: 22px; padding: 0 22px; color: #eee; font-size: 15px; font-weight: 500; }
      .pill { display: flex; align-items: center; gap: 7px; background: #ffffff26; padding: 3px 9px; border-radius: 6px; }
      .row { position: absolute; top: 70px; left: 0; right: 0; display: flex; justify-content: center; align-items: flex-start; gap: 32px; }
      .panel { width: 440px; border-radius: 16px; box-shadow: 0 30px 80px #000a, 0 0 0 0.5px #fff2; display: block; }
      .crop { width: 440px; height: 700px; overflow: hidden; border-radius: 16px; margin-top: 30px; -webkit-mask-image: linear-gradient(#000 78%, transparent); box-shadow: 0 30px 80px #000a; }
      .crop img { width: 440px; display: block; }
      .stack { display: flex; flex-direction: column; gap: 22px; margin-top: 30px; }
      .note { width: 420px; margin-left: 10px; border-radius: 18px; box-shadow: 0 24px 60px #0009; }
    </style>
    <div class="bar"><span class="pill">${grid(16, 1.3, "#fff", "#ffffff66")} 5</span><span>Tue 9:41 AM</span></div>
    <div class="row">
      <div class="crop"><img src="${img("02b-detail-expanded-dark")}"></div>
      <img class="panel" src="${img("01-servers-dark")}">
      <div class="stack"><img class="note" src="${img("04-notification-dark")}"><img class="panel" src="${img("03-clean-up-light")}"></div>
    </div>`,
  },
  logo: {
    size: [240, 240],
    html: `<style>${base} body { width: 240px; height: 240px; display: grid; place-items: center; background: transparent; }
      .tile { width: 216px; height: 216px; border-radius: 48px; background: linear-gradient(160deg, #1f2b23, #0d100e); display: grid; place-items: center; box-shadow: inset 0 0 0 1px #ffffff1a; }</style>
      <div class="tile">${grid(130, 10, "#7fd89d", "#ffffff38")}</div>`,
    transparent: true,
  },
};

const browser = await chromium.launch({ channel: "chrome" });
const page = await browser.newPage({ deviceScaleFactor: 2 });
for (const [name, { size, html, transparent }] of Object.entries(pages)) {
  await page.setViewportSize({ width: size[0], height: size[1] });
  await page.setContent(`<!doctype html><meta charset="utf-8">${html}`);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: `${assets}${name}.png`, omitBackground: Boolean(transparent) });
}
await browser.close();

// The gallery reuses the view screenshots as they are.
for (const name of ["02b-detail-expanded-dark", "03-clean-up-dark", "05-machines-dark", "07-protected-stop-dark", "06-other-ports-light", "04-notification-dark"])
  copyFileSync(`${shots}${name}.png`, `${assets}screens/${name}.png`);
console.log(`Wrote ${assets}{banner,hero,logo}.png and screens/`);
