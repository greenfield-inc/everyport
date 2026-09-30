import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const brand = (file) => fileURLToPath(new URL(`../../brand/${file}`, import.meta.url));
const out = mkdtempSync(join(tmpdir(), "installer-art-"));
execFileSync(process.execPath, [
  fileURLToPath(new URL("render.mjs", import.meta.url)),
  ...["--name", "Everyport", "--tagline", "Every dev server. Every OS."],
  ...["--mark", brand("socket.svg"), "--icon", brand("socket-app.svg"), "--out", out],
]);
const read = (file) => readFileSync(join(out, file));

test("Windows bitmaps are uncompressed 24-bit BMPs at the sizes NSIS and WiX show", () => {
  for (const [file, width, height] of [
    ["nsis-header.bmp", 150, 57],
    ["nsis-sidebar.bmp", 164, 314],
    ["wix-banner.bmp", 493, 58],
    ["wix-dialog.bmp", 493, 312],
  ]) {
    const bmp = read(file);
    assert.equal(bmp.toString("ascii", 0, 2), "BM", file);
    assert.deepEqual([bmp.readInt32LE(18), bmp.readInt32LE(22), bmp.readUInt16LE(28), bmp.readUInt32LE(30)], [width, height, 24, 0], file);
  }
});

test("the DMG background comes at 1x and 2x, and as one TIFF with both", () => {
  const size = (png) => [png.readUInt32BE(16), png.readUInt32BE(20)];
  assert.deepEqual(size(read("dmg-background.png")), [660, 400]);
  assert.deepEqual(size(read("dmg-background@2x.png")), [1320, 800]);

  const tiff = read("dmg-background.tiff");
  const pages = [];
  for (let ifd = tiff.readUInt32LE(4); ifd; ifd = tiff.readUInt32LE(ifd + 2 + tiff.readUInt16LE(ifd) * 12)) {
    const tag = (id) => {
      for (let i = 0; i < tiff.readUInt16LE(ifd); i++) {
        const entry = ifd + 2 + i * 12;
        if (tiff.readUInt16LE(entry) === id) return tiff.readUInt32LE(entry + 8);
      }
    };
    pages.push([tag(256), tag(257), tiff.readUInt32LE(tag(282))]);
  }
  assert.deepEqual(pages, [[660, 400, 72], [1320, 800, 144]]);
});

test("the installer icon has every size Windows asks for", () => {
  const ico = read("installer.ico");
  const sizes = Array.from({ length: ico.readUInt16LE(4) }, (_, i) => ico[6 + i * 16] || 256);
  assert.deepEqual(sizes, [16, 24, 32, 48, 64, 256]);
});
