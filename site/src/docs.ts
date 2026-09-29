import { readFileSync } from "node:fs";
import { join } from "node:path";
import meta from "../content/_meta.ts";

export type DocPage = { path: string; title: string; description?: string; markdown: string };

const field = (front: string, name: string) => {
  const value = front.match(new RegExp(`^${name}: (.+)$`, "m"))?.[1];
  return value?.startsWith('"') ? (JSON.parse(value) as string) : value;
};

/** Every docs page, in sidebar order, read from content/ at build time. */
export function docPages(): DocPage[] {
  return Object.keys(meta).map((slug) => {
    const file = slug === "index" ? "index.mdx" : `${slug}.md`;
    const [, front, markdown] = readFileSync(join(process.cwd(), "content", file), "utf8").match(/^---\n([\s\S]*?)\n---\n+([\s\S]*)$/)!;
    return { path: slug === "index" ? "docs" : `docs/${slug}`, title: field(front, "title")!, description: field(front, "description"), markdown };
  });
}
