import { marked } from "marked";
import { releases } from "../../changelog.ts";
import { SITE_URL } from "../../site.ts";

export const dynamic = "force-static";

const escape = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// An RSS item per release, linking to its section of /changelog.
export function GET() {
  const items = releases().map(({ version, date, notes }) => {
    const link = `${SITE_URL}changelog#${version}`;
    return `    <item>
      <title>Everyport ${version}</title>
      <link>${link}</link>
      <guid>${link}</guid>
      <pubDate>${new Date(`${date}T00:00:00Z`).toUTCString()}</pubDate>
      <description>${escape(marked.parse(notes, { async: false }))}</description>
    </item>`;
  });
  const feed = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>Everyport Changelog</title>
    <link>${SITE_URL}changelog</link>
    <description>What changed in each Everyport release.</description>
    <language>en-us</language>
    <atom:link href="${SITE_URL}feed.xml" rel="self" type="application/rss+xml"/>
${items.join("\n")}
  </channel>
</rss>
`;
  return new Response(feed, { headers: { "Content-Type": "application/rss+xml; charset=utf-8" } });
}
