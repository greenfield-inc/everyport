import { readFileSync } from "node:fs";
import { join } from "node:path";

export const dynamic = "force-static";

// The repository's llms.txt, which links every docs page as raw Markdown.
const text = readFileSync(join(process.cwd(), "../llms.txt"), "utf8");

export function GET() {
  return new Response(text, { headers: { "Content-Type": "text/markdown; charset=utf-8" } });
}
