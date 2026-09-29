import { readFileSync } from "node:fs";
import { join } from "node:path";

export const dynamic = "force-static";

// The one-command app install, from scripts/install-app.sh at build time.
const script = readFileSync(join(process.cwd(), "../scripts/install-app.sh"), "utf8");

export function GET() {
  return new Response(script, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
