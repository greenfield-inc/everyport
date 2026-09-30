import type { Metadata } from "next";
import type { ComponentProps } from "react";
import { compileMdx } from "nextra/compile";
import { evaluate } from "nextra/evaluate";
import { useMDXComponents as getDocsMDXComponents } from "nextra-theme-docs";
import { DocsWrapper } from "../../../mdx-components";
import { longDate, releases, releaseUrl } from "../../changelog.ts";
import { baseMetadata, REPO } from "../../site.ts";

const title = "Changelog";
const description = "What changed in each Everyport release, newest first.";

export const metadata: Metadata = {
  ...baseMetadata,
  title: "Everyport Changelog",
  description,
  alternates: {
    canonical: "/changelog",
    types: { "application/rss+xml": "/feed.xml", "text/plain": "/changelog.txt" },
  },
  openGraph: { ...baseMetadata.openGraph, title: "Everyport Changelog", description, url: "/changelog" },
};

// CHANGELOG.md as a page, with a heading per version.
const markdown = [
  `# ${title}\n\n${description} Follow it with the [RSS feed](/feed.xml), or read it as [plain text](/changelog.txt).`,
  ...releases().map(
    ({ version, date, notes }) =>
      `## ${version}\n\n<p className="changelog-date">${longDate(date)} · [GitHub release](${releaseUrl(version)})</p>\n\n${notes}`,
  ),
].join("\n\n");

const docsComponents = getDocsMDXComponents();
const Heading = docsComponents.h2!;
// Anchors each version at its number, as in /changelog#0.1.2, which the updater links to.
// Nextra's own heading ids drop the dots ("012").
const components = { ...docsComponents, h2: (props: ComponentProps<"h2">) => <Heading {...props} id={String(props.children)} /> };

export default async function ChangelogPage() {
  const { default: Content, toc } = evaluate(await compileMdx(markdown), components);
  return (
    <DocsWrapper
      toc={toc.filter((heading) => heading.depth === 2).map((heading) => ({ ...heading, id: String(heading.value) }))}
      metadata={{ title, description, filePath: `${REPO}/edit/main/CHANGELOG.md` }}
      sourceCode={markdown}
    >
      <Content />
    </DocsWrapper>
  );
}
