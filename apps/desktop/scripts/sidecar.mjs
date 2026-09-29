// Builds `ppm` and puts it where Tauri's externalBin expects it:
// src-tauri/binaries/ppm-<target triple>[.exe]. Tauri sets
// TAURI_ENV_TARGET_TRIPLE for cross builds; otherwise it's the host's.
// With --release, a binary already there (such as one CI cross-built) is kept.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const release = process.argv.includes("--release");
const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (.+)$/m)[1];
const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const exe = target.includes("windows") ? ".exe" : "";
const dest = join(root, "apps/desktop/src-tauri/binaries", `ppm-${target}${exe}`);

if (release && existsSync(dest)) process.exit(0);

// Host builds skip --target so they share cargo's default output folder.
const cross = target === host ? [] : ["--target", target];
const args = ["build", "-p", "port-process-manager", ...cross, ...(release ? ["--release"] : [])];
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const out = join(process.env.CARGO_TARGET_DIR || join(root, "target.noindex"), ...cross.slice(1));
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(join(out, release ? "release" : "debug", `ppm${exe}`), dest);
