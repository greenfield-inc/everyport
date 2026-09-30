import { releases, releaseUrl } from "../../changelog.ts";
import { REPO, SITE_URL } from "../../site.ts";

export const dynamic = "force-static";

// The changelog as plain text, for agents and scripts.
export function GET() {
  const header = `# Everyport changelog\n\nWhat changed in each Everyport release, newest first.\n\nWeb: ${SITE_URL}changelog\nFeed: ${SITE_URL}feed.xml\nReleases: ${REPO}/releases`;
  const body = releases().map(({ version, date, notes }) => `## ${version} - ${date}\n${releaseUrl(version)}\n\n${notes}`);
  return new Response([header, ...body].join("\n\n") + "\n", { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
