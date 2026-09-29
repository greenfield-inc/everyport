# Site

<https://everyport.dev>: the landing page and the docs, built with Next.js and Nextra and hosted on Vercel.

```bash
pnpm --filter @everyport/site dev      # http://localhost:5198
pnpm --filter @everyport/site build    # also builds the search index
pnpm --filter @everyport/site og       # renders /og to public/og.png
```

## Landing page

`/` renders the real `@everyport/ui` popover over an in-page fake `EveryportClient` with demo machines (`src/client.ts`), so nothing it does touches a real server. It renders in the browser only, since it picks the visitor's OS and a desk or phone layout from the window.

`public/og.png` is the social preview for the site and for the GitHub repository (Settings, Social preview).

## Docs

`/docs` shows the Markdown files in the repository's `docs/`. Edit them there: `scripts/docs.mjs` copies them into `content/` before `dev`, `build` and `typecheck`, adds front matter, and turns links between them into site links. Its `PAGES` list sets each page's address, sidebar title and order. `content/index.mdx`, the docs home, is the only page that lives here.

## Routes

- `/install.sh` and `/install.ps1`: the one-command installers, `scripts/install-app.sh` and `scripts/install-app.ps1`, as they were at build time.
- `/llms.txt`: the repository's `llms.txt`. `/llms-full.txt`: its summary and every docs page.
- `/robots.txt` and `/sitemap.xml`, with every docs page.

## Deploy

Vercel project `everyport` (team `parsas`), with `site` as the root directory. Pushes to `main` deploy to production and pull requests get previews. `vercel.json` skips the build when nothing the site reads changed.

`SITE_URL` (default `https://everyport.dev/`) sets the canonical URL, `og:image`, the sitemap and the install commands. `www.` redirects to it.
