# Site

The landing page at <https://greenfield-inc.github.io/port-process-manager/>. It renders the real `@ppm/ui` popover over an in-page fake `PpmClient` with demo machines, so nothing it does touches a real server.

```bash
pnpm --filter @ppm/site dev      # http://localhost:5198/port-process-manager/
pnpm --filter @ppm/site build    # site/dist
pnpm --filter @ppm/site og       # renders og.html to public/og.png
```

`.github/workflows/pages.yml` builds and deploys it on every push to `main` that changes the site, `@ppm/ui` or `@ppm/protocol`.

`public/og.png` is the social preview for the site and for the GitHub repository (Settings, Social preview).

## Moving to a custom domain

1. Add `site/public/CNAME` containing the domain, such as `ppm.example.com`.
2. Set `SITE_URL` in `.github/workflows/pages.yml` to `https://ppm.example.com/`.

`SITE_URL` sets Vite's `base` (its path, here `/`) and the absolute `og:url` and `og:image` URLs. Every other asset path is relative to the base.
