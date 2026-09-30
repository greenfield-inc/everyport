import type { MetadataRoute } from "next";
import { docPages } from "../docs.ts";
import { SITE_URL } from "../site.ts";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: SITE_URL, priority: 1 },
    ...docPages().map((page) => ({ url: `${SITE_URL}${page.path}`, priority: 0.8 })),
    { url: `${SITE_URL}changelog`, priority: 0.5 },
  ];
}
