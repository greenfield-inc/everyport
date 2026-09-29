# Site

The landing page at <https://everyport.dev>. It renders the real `@everyport/ui` popover over an in-page fake `EveryportClient` with demo machines, so nothing it does touches a real server.

```bash
pnpm --filter @everyport/site dev      # http://localhost:5198/everyport/
pnpm --filter @everyport/site build    # site/dist
pnpm --filter @everyport/site og       # renders og.html to public/og.png
```

`.github/workflows/pages.yml` builds and deploys it on every push to `main` that changes the site, `@everyport/ui` or `@everyport/protocol`.

The install section's one-line command fetches `install.sh` and `install.ps1` from the site root. They come from `scripts/install-app.sh` and `scripts/install-app.ps1`. Direct downloads link to the release files for the version in `Cargo.toml`.

`public/og.png` is the social preview for the site and for the GitHub repository (Settings, Social preview).

## Domain

The `SITE_URL` repository variable (Settings, Secrets and variables, Actions) holds the site's address, `https://everyport.dev`, and the custom domain is set in Settings, Pages. `SITE_URL` sets Vite's `base` (its path, here `/`), the canonical, `og:url` and `og:image` URLs, and the install commands. Every other asset path is relative to the base. Without the variable, the site builds for `https://greenfield-inc.github.io/everyport/`.
