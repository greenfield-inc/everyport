// Builds `ppm` and puts it where Tauri's externalBin expects it:
// src-tauri/binaries/ppm-sidecar-<target triple>[.exe]. The bundles install
// it as `ppm-sidecar`, so the Linux packages don't put a second `ppm` in
// /usr/bin next to the CLI's. Tauri sets
// TAURI_ENV_TARGET_TRIPLE for cross builds; otherwise it's the host's.
// PPM_SIDECAR names a prebuilt binary to use instead, such as one CI
// cross-built for that triple.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const release = process.argv.includes("--release");
const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (.+)$/m)[1];
const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const exe = target.includes("windows") ? ".exe" : "";
const dest = join(root, "apps/desktop/src-tauri/binaries", `ppm-sidecar-${target}${exe}`);

mkdirSync(dirname(dest), { recursive: true });
const prebuilt = process.env.PPM_SIDECAR;
if (prebuilt) {
  copyFileSync(prebuilt, dest);
  process.exit(0);
}

// Host builds skip --target so they share cargo's default output folder.
const cross = target === host ? [] : ["--target", target];
const args = ["build", "-p", "port-process-manager", ...cross, ...(release ? ["--release"] : [])];
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const out = join(process.env.CARGO_TARGET_DIR || join(root, "target.noindex"), ...cross.slice(1));
copyFileSync(join(out, release ? "release" : "debug", `ppm${exe}`), dest);
