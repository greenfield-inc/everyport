import type { MDXComponents } from "mdx/types";
import type { MDXWrapper } from "nextra";
import { useMDXComponents as getDocsMDXComponents } from "nextra-theme-docs";

const docsComponents = getDocsMDXComponents();

export function useMDXComponents(components?: MDXComponents): MDXComponents {
  return { ...docsComponents, ...components };
}

export const DocsWrapper = docsComponents.wrapper as MDXWrapper;
