// Writes the docs pages into content/ from the repository's docs/*.md, so the
// site and GitHub show the same text. Runs before dev, build and typecheck.
// Each page gets front matter with its title, description and source file, and
// links between docs become site links.
import { readFileSync, writeFileSync } from "node:fs";

const REPO = "https://github.com/greenfield-inc/everyport";

/** Site slug, sidebar title and source file in docs/, in sidebar order. */
const PAGES = [
  ["getting-started", "Getting started", "getting-started.md"],
  ["machines", "Machines", "remote-machines.md"],
  ["settings", "Settings and clean up", "settings.md"],
  ["cli", "CLI reference", "cli.md"],
  ["protocol", "Protocol", "protocol.md"],
  ["troubleshooting", "Troubleshooting", "troubleshooting.md"],
  ["faq", "FAQ", "faq.md"],
];

const docs = new URL("../../docs/", import.meta.url);
const content = new URL("../content/", import.meta.url);
const slugOf = new Map(PAGES.map(([slug, , file]) => [file, slug]));

/** Points a link in docs/ at the site page, the README or the file on GitHub. */
function rewrite(target) {
  if (/^[a-z]+:|^#/.test(target)) return target;
  const [path, hash = ""] = target.split("#");
  const anchor = hash && `#${hash}`;
  if (slugOf.has(path)) return `/docs/${slugOf.get(path)}${anchor}`;
  if (path === "../README.md") return `${REPO}${anchor}`;
  return `${REPO}/blob/main/${new URL(path, "https://x/docs/").pathname.slice(1)}${anchor}`;
}

const quote = (text) => JSON.stringify(text);

for (const [slug, , file] of PAGES) {
  const source = readFileSync(new URL(file, docs), "utf8");
  const title = source.match(/^# (.+)$/m)[1];
  // The first paragraph under the title, when there is one.
  const lead = source.split(/\n\n/)[1]?.trim();
  const description = lead && !/^[#|`<-]/.test(lead) ? lead.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/`/g, "") : undefined;
  const body = source.replace(/\]\(([^)\s]+)\)/g, (_, target) => `](${rewrite(target)})`);
  const front = [`title: ${quote(title)}`, description && `description: ${quote(description)}`, `source: docs/${file}`].filter(Boolean);
  writeFileSync(new URL(`${slug}.md`, content), `---\n${front.join("\n")}\n---\n\n${body}`);
}

writeFileSync(
  new URL("_meta.ts", content),
  `// Written by scripts/docs.mjs.\nexport default ${JSON.stringify({ index: "Overview", ...Object.fromEntries(PAGES.map(([slug, title]) => [slug, title])), changelog: { title: "Changelog", href: "/changelog" } }, null, 2)};\n`,
);
