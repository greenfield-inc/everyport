#!/usr/bin/env node
// Renders installer art for the macOS DMG, Windows NSIS and Windows WiX from one
// SVG template. See README.md for the inputs and the config keys that use each file.
import { Resvg } from "@resvg/resvg-js";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { deflateSync } from "node:zlib";

/** Icon centers in the DMG window, in points. README.md lists them for the config. */
const DMG = { width: 660, height: 400, app: { x: 180, y: 190 }, applications: { x: 480, y: 190 } };

// Finder and the Windows wizards draw their own text in black, so light is the default.
const THEMES = {
  light: { bg: "#f4f4f1", panel: "#ebece7", header: "#ffffff", paper: "#f4f4f1", ink: "#1b1d1b", muted: "#6b7068", accent: "#2f9a5d", onAccent: "#ffffff", line: 0.08, glow: 0, grain: ["multiply", 0.06] },
  dark: { bg: "#0b0d0b", panel: "#141a15", header: "#141a15", paper: "#f5f7f3", ink: "#eef1ec", muted: "#9aa597", accent: "#7fd89d", onAccent: "#0b0d0b", line: 0.06, glow: 0.22, grain: ["screen", 0.07] },
};
const FONT = "Sora";
const FONTS = ["Sora-Regular.ttf", "Sora-SemiBold.ttf"].map((file) => fileURLToPath(new URL(`fonts/${file}`, import.meta.url)));
const ICO_SIZES = [16, 24, 32, 48, 64, 256];

const { values: args } = parseArgs({
  options: Object.fromEntries(["name", "mark", "icon", "accent", "tagline", "theme", "out"].map((key) => [key, { type: "string" }])),
});
const missing = ["name", "mark", "icon", "out"].filter((key) => !args[key]);
if (missing.length) fail(`Missing ${missing.map((key) => `--${key}`).join(", ")}.`);
const theme = THEMES[args.theme ?? "light"] ?? fail(`--theme is light or dark, not ${args.theme}.`);
render({ ...args, accent: args.accent ?? theme.accent, tagline: args.tagline ?? "", theme });
console.log(`Wrote installer art for ${args.name} to ${args.out}`);

function fail(message) {
  console.error(`${message} See packaging/installer-art/README.md.`);
  process.exit(1);
}

function render({ name, mark, icon, accent, tagline, theme, out }) {
  const art = { name, accent, tagline, theme, mark: readSvg(mark), icon: readSvg(icon) };
  mkdirSync(out, { recursive: true });
  const write = (file, data) => writeFileSync(join(out, file), data);

  const dmg = dmgBackground(art);
  const [dmg1x, dmg2x] = [1, 2].map((scale) => rasterize(dmg, DMG.width * scale));
  write("dmg-background.png", dmg1x.png);
  write("dmg-background@2x.png", dmg2x.png);
  write("dmg-background.tiff", tiff([dmg1x, dmg2x]));
  write("nsis-header.bmp", bmp(rasterize(nsisHeader(art), 150)));
  write("nsis-sidebar.bmp", bmp(rasterize(nsisSidebar(art), 164)));
  write("wix-banner.bmp", bmp(rasterize(wixBanner(art), 493)));
  write("wix-dialog.bmp", bmp(rasterize(wixDialog(art), 493)));
  write("installer.ico", ico(ICO_SIZES.map((size) => rasterize(installerIcon(art, size >= 32), size).png)));
}

// ─── Templates. Every size is in 1× pixels. ─────────────────────────────

function dmgBackground({ name, accent, tagline, mark, theme: t }) {
  // Finder's title bar and path bar can cover about 60 pt, so the art stays in the top 340.
  const { width: w, height: h, app, applications: apps } = DMG;
  // The arrow runs between the two 128 pt icons.
  const [x1, x2, y] = [app.x + 84, apps.x - 84, app.y];
  return svg(w, h, `
    ${defs(accent)}
    <rect width="${w}" height="${h}" fill="${t.bg}"/>
    ${socketShape(t, 500, -190, 300)}
    ${socketShape(t, -120, 236, 250)}
    ${glow(app.x, app.y, 120, 120, accent, t.glow)}
    ${lockup(t, mark, name, w / 2, 54, 24, 22)}
    ${tagline ? text(tagline, w / 2, 86, 13, t.muted, { anchor: "middle" }) : ""}
    <rect x="${apps.x - 66}" y="${apps.y - 66}" width="132" height="132" rx="30" fill="none" stroke="${t.ink}" stroke-opacity=".16" stroke-dasharray="6 6"/>
    <path d="M${x1} ${y}H${x2}M${x2 - 9} ${y - 9}L${x2} ${y}L${x2 - 9} ${y + 9}" stroke="url(#arrow)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
    ${grain(t, 0, 0, w, h)}`);
}

function nsisHeader({ name, accent, mark, theme: t }) {
  // The wizard's header strip around it is white in the light theme.
  const [w, h] = [150, 57];
  const size = fitText(name, 15, "600", w - 58);
  return svg(w, h, `
    ${defs(accent)}
    <rect width="${w}" height="${h}" fill="${t.header}"/>
    ${glow(28, h / 2, 90, 60, accent, t.glow)}
    ${place(mark, 16, (h - 24) / 2, 24, t.ink)}
    ${text(name, 50, h / 2 + size * 0.36, size, t.ink, { weight: "600" })}`);
}

function nsisSidebar(art) {
  return svg(164, 314, `${defs(art.accent)}${sidebar(art, 164, 314)}`);
}

/** The panel that NSIS shows on its welcome and finish pages, and WiX on its first and last dialogs. */
function sidebar({ name, accent, tagline, mark, theme: t }, w, h) {
  const size = fitText(name, 20, "600", w - 32);
  const lines = wrap(tagline, 11, w - 36);
  return `
    <rect width="${w}" height="${h}" fill="${t.panel}"/>
    ${socketShape(t, -70, 236, 200)}
    ${glow(w / 2, 96, 150, 150, accent, t.glow)}
    ${place(mark, (w - 52) / 2, 70, 52, t.ink)}
    ${text(name, w / 2, 160, size, t.ink, { anchor: "middle", weight: "600" })}
    ${lines.map((line, i) => text(line, w / 2, 184 + i * 16, 11, t.muted, { anchor: "middle" })).join("")}
    ${grain(t, 0, 0, w, h)}`;
}

function wixBanner({ accent, icon, theme: t }) {
  // WiX draws the dialog title and description in black over the left 406 px.
  const [w, h] = [493, 58];
  return svg(w, h, `
    ${defs(accent)}
    <rect width="${w}" height="${h}" fill="${t.paper}"/>
    ${place(icon, w - 58, 5, 48)}
    ${grain(t, 0, 0, w, h, THEMES.light.grain)}`);
}

function wixDialog(art) {
  // WiX draws the text in black from x 180, so the panel stays left of it.
  const [w, h, panel] = [493, 312, 164];
  const t = art.theme;
  return svg(w, h, `
    ${defs(art.accent)}
    <rect width="${w}" height="${h}" fill="${t.paper}"/>
    ${grain(t, panel, 0, w - panel, h, THEMES.light.grain)}
    <svg width="${panel}" height="${h}">${sidebar(art, panel, h)}</svg>`);
}

function installerIcon({ accent, icon, theme: t }, badge) {
  // The app icon with a download badge, so the setup file reads as an installer.
  return svg(1024, 1024, `
    ${place(icon, 0, 0, 1024)}
    ${badge ? `<circle cx="800" cy="800" r="200" fill="${accent}" stroke="#0b0d0b" stroke-width="40"/>
      <path d="M800 700V890M725 820L800 895L875 820" fill="none" stroke="${t.onAccent}" stroke-width="56" stroke-linecap="round" stroke-linejoin="round"/>` : ""}`);
}

// ─── SVG pieces ─────────────────────────────────────────────────────────

function svg(w, h, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`;
}

function defs(accent) {
  return `<defs>
    <linearGradient id="arrow" x1="0" x2="1"><stop offset="0" stop-color="${accent}" stop-opacity=".2"/><stop offset="1" stop-color="${accent}"/></linearGradient>
    <filter id="grain" x="0" y="0" width="1" height="1"><feTurbulence type="fractalNoise" baseFrequency=".85" numOctaves="3" stitchTiles="stitch"/><feColorMatrix type="saturate" values="0"/></filter>
  </defs>`;
}

/** A large, faint rounded square with the Socket face's proportions. */
function socketShape(t, x, y, size) {
  return `<rect x="${x}" y="${y}" width="${size}" height="${size}" rx="${size / 3}" fill="none" stroke="${t.ink}" stroke-opacity="${t.line}" stroke-width="1.5"/>`;
}

/** A soft accent glow, as on everyport.dev. The light theme leaves it out. */
function glow(cx, cy, rx, ry, accent, opacity) {
  if (!opacity) return "";
  const id = `glow-${cx}-${cy}-${rx}`;
  return `<radialGradient id="${id}"><stop offset="0" stop-color="${accent}" stop-opacity="${opacity}"/><stop offset="1" stop-color="${accent}" stop-opacity="0"/></radialGradient>
    <ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}" fill="url(#${id})"/>`;
}

/** Static film grain, the same feTurbulence tile the onboarding draws. */
function grain(t, x, y, w, h, [blend, opacity] = t.grain) {
  return `<rect x="${x}" y="${y}" width="${w}" height="${h}" filter="url(#grain)" opacity="${opacity}" style="mix-blend-mode:${blend}"/>`;
}

/** The mark and the name side by side, centered on `cx`. */
function lockup(t, mark, name, cx, cy, markSize, textSize) {
  const gap = markSize * 0.45;
  const width = markSize + gap + textWidth(name, textSize, "600");
  const x = cx - width / 2;
  return `${place(mark, x, cy - markSize / 2, markSize, t.ink)}
    ${text(name, x + markSize + gap, cy + textSize * 0.36, textSize, t.ink, { weight: "600" })}`;
}

function text(value, x, y, size, fill, { anchor = "start", weight = "400" } = {}) {
  return `<text x="${x}" y="${y}" font-family="${FONT}" font-size="${size}" font-weight="${weight}" fill="${fill}" text-anchor="${anchor}">${escape(value)}</text>`;
}

/** Nests an SVG in a square box. `currentColor` in the source takes `color`. */
function place({ viewBox, attributes, body }, x, y, size, color = "#000") {
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="${viewBox}" color="${color}"${attributes}>${body}</svg>`;
}

/** Reads an SVG's viewBox, its root presentation attributes such as `fill`, and its content. */
function readSvg(path) {
  const source = readFileSync(path, "utf8");
  const open = source.match(/<svg\b([^>]*)>/);
  const viewBox = open?.[1].match(/viewBox="([^"]+)"/)?.[1];
  if (!viewBox) throw new Error(`${path} needs to be an SVG with a viewBox`);
  const attributes = open[1].replace(/\s(xmlns(:\w+)?|viewBox|width|height|x|y)="[^"]*"/g, "").trimEnd();
  return { viewBox, attributes, body: source.slice(open.index + open[0].length, source.lastIndexOf("</svg>")) };
}

function escape(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// ─── Text measuring, with the same fonts the art renders with ───────────

function textWidth(value, size, weight = "400") {
  const probe = new Resvg(svg(4000, 200, text(value, 0, 100, size, "#000", { weight })), fontOptions());
  return probe.getBBox()?.width ?? 0;
}

/** The largest size up to `size` at which `value` fits in `width`. */
function fitText(value, size, weight, width) {
  return Math.min(size, Math.floor((size * width) / textWidth(value, size, weight)));
}

/** Breaks `value` into lines that fit `width`, starting each sentence on a new line. */
function wrap(value, size, width) {
  const lines = [];
  for (const sentence of value.split(/(?<=[.!?])\s+/).filter(Boolean)) {
    let line = "";
    for (const word of sentence.split(/\s+/)) {
      if (line && textWidth(`${line} ${word}`, size) > width) {
        lines.push(line);
        line = word;
      } else line = line ? `${line} ${word}` : word;
    }
    lines.push(line);
  }
  return lines;
}

// ─── Rasterizing and file formats ───────────────────────────────────────

function fontOptions() {
  return { font: { fontFiles: FONTS, loadSystemFonts: false, defaultFontFamily: FONT } };
}

function rasterize(source, width) {
  const image = new Resvg(source, { ...fontOptions(), fitTo: { mode: "width", value: width } }).render();
  return { width: image.width, height: image.height, rgba: image.pixels, png: image.asPng() };
}

/** 24-bit uncompressed BMP, the only kind NSIS and WiX show reliably. */
function bmp({ width, height, rgba }) {
  const stride = Math.ceil((width * 3) / 4) * 4;
  const file = Buffer.alloc(54 + stride * height);
  file.write("BM", 0, "ascii");
  file.writeUInt32LE(file.length, 2);
  file.writeUInt32LE(54, 10);
  file.writeUInt32LE(40, 14);
  file.writeInt32LE(width, 18);
  file.writeInt32LE(height, 22);
  file.writeUInt16LE(1, 26);
  file.writeUInt16LE(24, 28);
  file.writeUInt32LE(stride * height, 34);
  file.writeInt32LE(2835, 38); // 72 dpi
  file.writeInt32LE(2835, 42);
  for (let y = 0; y < height; y++) {
    const row = 54 + (height - 1 - y) * stride;
    for (let x = 0; x < width; x++) {
      const [src, dst] = [(y * width + x) * 4, row + x * 3];
      file[dst] = rgba[src + 2];
      file[dst + 1] = rgba[src + 1];
      file[dst + 2] = rgba[src];
    }
  }
  return file;
}

/** An ICO of PNG entries, as `tauri icon` writes. */
function ico(pngs) {
  const header = Buffer.alloc(6 + 16 * pngs.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(pngs.length, 4);
  let offset = header.length;
  pngs.forEach((png, i) => {
    const size = png.readUInt32BE(16);
    const entry = 6 + 16 * i;
    header[entry] = size % 256;
    header[entry + 1] = size % 256;
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(png.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += png.length;
  });
  return Buffer.concat([header, ...pngs]);
}

/**
 * A multi-page RGB TIFF, one page per scale at 72 dpi times the scale, which is
 * what `tiffutil -cathidpicheck` writes. Finder picks the page for the display.
 */
function tiff(images) {
  const pages = images.map(({ width, height, rgba }) => {
    // Deflate with horizontal differencing (Predictor 2) keeps the file small.
    const rows = Buffer.alloc(height * width * 3);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        for (let c = 0; c < 3; c++) {
          const here = rgba[(y * width + x) * 4 + c];
          const left = x ? rgba[(y * width + x - 1) * 4 + c] : 0;
          rows[(y * width + x) * 3 + c] = (here - left) & 0xff;
        }
      }
    }
    return { width, height, data: deflateSync(rows, { level: 9 }) };
  });

  const tags = 14;
  const ifdSize = 2 + tags * 12 + 4;
  const extraSize = 6 + 8 + 8; // BitsPerSample, XResolution, YResolution
  let offset = 8;
  const layout = pages.map((page) => {
    const at = { ifd: offset, extra: offset + ifdSize, data: offset + ifdSize + extraSize };
    offset = at.data + page.data.length + (page.data.length % 2);
    return at;
  });

  const file = Buffer.alloc(offset);
  file.write("II", 0, "ascii");
  file.writeUInt16LE(42, 2);
  file.writeUInt32LE(layout[0].ifd, 4);
  pages.forEach((page, i) => {
    const at = layout[i];
    const dpi = 72 * (page.width / pages[0].width);
    let cursor = at.ifd;
    file.writeUInt16LE(tags, cursor);
    cursor += 2;
    const tag = (id, type, count, value) => {
      file.writeUInt16LE(id, cursor);
      file.writeUInt16LE(type, cursor + 2);
      file.writeUInt32LE(count, cursor + 4);
      if (type === 3 && count === 1) file.writeUInt16LE(value, cursor + 8);
      else file.writeUInt32LE(value, cursor + 8);
      cursor += 12;
    };
    const [SHORT, LONG, RATIONAL] = [3, 4, 5];
    tag(256, LONG, 1, page.width);
    tag(257, LONG, 1, page.height);
    tag(258, SHORT, 3, at.extra);
    tag(259, SHORT, 1, 8); // Deflate
    tag(262, SHORT, 1, 2); // RGB
    tag(273, LONG, 1, at.data);
    tag(277, SHORT, 1, 3);
    tag(278, LONG, 1, page.height);
    tag(279, LONG, 1, page.data.length);
    tag(282, RATIONAL, 1, at.extra + 6);
    tag(283, RATIONAL, 1, at.extra + 14);
    tag(284, SHORT, 1, 1);
    tag(296, SHORT, 1, 2); // Inches
    tag(317, SHORT, 1, 2); // Horizontal differencing
    file.writeUInt32LE(i + 1 < pages.length ? layout[i + 1].ifd : 0, cursor);
    [8, 8, 8].forEach((bits, j) => file.writeUInt16LE(bits, at.extra + j * 2));
    for (const rational of [at.extra + 6, at.extra + 14]) {
      file.writeUInt32LE(dpi, rational);
      file.writeUInt32LE(1, rational + 4);
    }
    page.data.copy(file, at.data);
  });
  return file;
}
