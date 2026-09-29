#!/usr/bin/env node
// Runs the ppm release binary that matches this package's version. The first
// run downloads it from GitHub Releases, checks its SHA-256 against the
// SHA256SUMS packed into this package at release time, and caches it.
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');

const { version } = require('../package.json');
const SUMS = path.join(__dirname, '..', 'SHA256SUMS');
const RELEASES = 'https://github.com/greenfield-inc/port-process-manager/releases/download';

function fail(message) {
  console.error(`ppm: ${message}`);
  process.exit(1);
}

function assetName() {
  const arch = { x64: 'x86_64', arm64: 'aarch64' }[process.arch];
  const target = {
    darwin: 'apple-darwin',
    linux: 'unknown-linux-musl',
    win32: 'pc-windows-msvc.exe',
  }[process.platform];
  if (!arch || !target) fail(`no ppm build for ${process.platform} ${process.arch}`);
  return `ppm-${arch}-${target}`;
}

function cacheDir() {
  const root =
    process.platform === 'win32'
      ? process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local')
      : process.platform === 'darwin'
        ? path.join(os.homedir(), 'Library', 'Caches')
        : process.env.XDG_CACHE_HOME || path.join(os.homedir(), '.cache');
  return path.join(root, 'port-process-manager', version);
}

async function download(url) {
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  return Buffer.from(await response.arrayBuffer());
}

function expectedHash(sums, asset) {
  for (const line of sums.split(/\r?\n/)) {
    const [hash, name] = line.trim().split(/\s+/);
    if (name && name.replace(/^\*/, '') === asset) return hash.toLowerCase();
  }
  return undefined;
}

async function install(binary) {
  const asset = assetName();
  const base = process.env.PPM_DOWNLOAD_URL || `${RELEASES}/v${version}`;
  console.error(`ppm: downloading ${asset} ${version}`);
  const expected = expectedHash(fs.readFileSync(SUMS, 'utf8'), asset);
  if (!expected) fail(`SHA256SUMS has no entry for ${asset}`);
  const data = await download(`${base}/${asset}`);
  const actual = crypto.createHash('sha256').update(data).digest('hex');
  if (actual !== expected) fail(`checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);

  fs.mkdirSync(path.dirname(binary), { recursive: true });
  const partial = `${binary}.${process.pid}.partial`;
  fs.writeFileSync(partial, data, { mode: 0o755 });
  fs.renameSync(partial, binary);
}

async function main() {
  const binary = path.join(cacheDir(), process.platform === 'win32' ? 'ppm.exe' : 'ppm');
  if (!fs.existsSync(binary)) await install(binary);

  const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) fail(result.error.message);
  if (result.signal) process.kill(process.pid, result.signal);
  process.exit(result.status ?? 1);
}

main().catch((error) => fail(error.message));
