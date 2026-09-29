import type { MetadataRoute } from "next";
import { SITE_URL } from "../site.ts";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: "*", allow: "/", disallow: ["/install.sh", "/install.ps1"] },
    sitemap: `${SITE_URL}sitemap.xml`,
  };
}
