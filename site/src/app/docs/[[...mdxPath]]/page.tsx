import { generateStaticParamsFor, importPage } from "nextra/pages";
import { DocsWrapper } from "../../../../mdx-components";
import { baseMetadata, REPO } from "../../../site.ts";

export const generateStaticParams = generateStaticParamsFor("mdxPath");

type PageProps = { params: Promise<{ mdxPath?: string[] }> };

export async function generateMetadata(props: PageProps) {
  const { mdxPath = [] } = await props.params;
  const { metadata } = await importPage(mdxPath);
  const canonical = mdxPath.length ? `/docs/${mdxPath.join("/")}` : "/docs";
  return {
    ...metadata,
    alternates: { canonical },
    openGraph: { ...baseMetadata.openGraph, title: metadata.title, description: metadata.description, url: canonical, type: "article" },
  };
}

export default async function Page(props: PageProps) {
  const params = await props.params;
  const { default: MDXContent, toc, metadata, sourceCode } = await importPage(params.mdxPath);
  // Pages written from docs/ (scripts/docs.mjs) are edited there.
  const { source } = metadata as { source?: string };
  const filePath = source ? `${REPO}/edit/main/${source}` : metadata.filePath;
  return (
    <DocsWrapper toc={toc} metadata={{ ...metadata, filePath }} sourceCode={sourceCode}>
      <MDXContent {...props} params={params} />
    </DocsWrapper>
  );
}
