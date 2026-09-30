import { readFileSync } from "node:fs";
import { join } from "node:path";
import { REPO } from "./site.ts";

export type Release = { version: string; date: string; notes: string };

/** Every release with notes in the repository's CHANGELOG.md, newest first, read at build time. */
export function releases(): Release[] {
  const text = readFileSync(join(process.cwd(), "../CHANGELOG.md"), "utf8");
  return text
    .split(/^## /m)
    .slice(1)
    .map((section) => {
      const [heading, ...body] = section.split("\n");
      const [, version, date] = heading.match(/^(\S+) - (\d{4}-\d{2}-\d{2})$/) ?? [];
      if (!version) throw new Error(`CHANGELOG.md: expected "## x.y.z - YYYY-MM-DD", found "## ${heading}"`);
      return { version, date, notes: body.join("\n").trim() };
    })
    .filter((release) => release.notes);
}

export const releaseUrl = (version: string) => `${REPO}/releases/tag/v${version}`;

/** "2026-09-29" as "September 29, 2026". */
export const longDate = (date: string) =>
  new Date(`${date}T00:00:00Z`).toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric", timeZone: "UTC" });
