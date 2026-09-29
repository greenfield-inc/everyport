import { readFileSync } from "node:fs";
import { join } from "node:path";
import { docPages } from "../../docs.ts";
import { SITE_URL } from "../../site.ts";

export const dynamic = "force-static";

// llms.txt's summary, then every docs page in sidebar order.
export function GET() {
  const summary = readFileSync(join(process.cwd(), "../llms.txt"), "utf8").split("\n## ")[0].trim();
  const pages = docPages()
    .filter((page) => page.path !== "docs")
    .map((page) => `<!-- ${SITE_URL}${page.path} -->\n\n${page.markdown.trim()}`);
  return new Response([summary, ...pages].join("\n\n---\n\n") + "\n", { headers: { "Content-Type": "text/markdown; charset=utf-8" } });
}
