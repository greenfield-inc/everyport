import type { ReactNode } from "react";
import { Footer, Layout, Navbar } from "nextra-theme-docs";
import { Head } from "nextra/components";
import { getPageMap } from "nextra/page-map";
import { Socket } from "./Socket.tsx";
import { REPO } from "./site.ts";
import "nextra-theme-docs/style.css";
import "./docs.css";

/** The docs' page: navbar, sidebar and footer. The docs and the changelog use it. */
export async function DocsShell({ children }: { children: ReactNode }) {
  return (
    <html lang="en" dir="ltr" suppressHydrationWarning>
      {/* The site's green, as Nextra's primary color. */}
      <Head color={{ hue: 142, saturation: 55, lightness: { dark: 67, light: 32 } }} />
      <body>
        <Layout
          navbar={
            <Navbar
              logo={
                <span className="docs-logo">
                  <Socket size={20} accent="var(--docs-green)" />
                  Everyport <span>Docs</span>
                </span>
              }
              logoLink="/"
              projectLink={REPO}
            >
              <a className="docs-nav-link" href="/changelog">
                Changelog
              </a>
            </Navbar>
          }
          footer={
            <Footer>
              <span>MIT {new Date().getFullYear()} © Dcouple, Inc.</span>
              <nav className="docs-footer">
                <a href="/">Home</a>
                <a href={REPO}>GitHub</a>
                <a href="/changelog">Changelog</a>
                <a href={`${REPO}/releases`}>Releases</a>
                <a href="/llms.txt">llms.txt</a>
              </nav>
            </Footer>
          }
          docsRepositoryBase={`${REPO}/blob/main/site`}
          pageMap={await getPageMap("/docs")}
          sidebar={{ defaultMenuCollapseLevel: 1 }}
        >
          {children}
        </Layout>
      </body>
    </html>
  );
}
