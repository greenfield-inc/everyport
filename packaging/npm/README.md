# everyport

See every dev server running on your machine, or on any box you can reach.

This package installs the `everyport` command (also available as `everyport`). The first run downloads the `everyport` release binary for your platform from [GitHub Releases](https://github.com/greenfield-inc/everyport/releases), checks it against the SHA-256 checksums packed into this package, and caches it.

```bash
npx everyport list
npm i -g everyport
everyport
```

Set `EVERYPORT_DOWNLOAD_URL` to a folder that holds the release files to download from a mirror instead.

See the [Everyport README](https://github.com/greenfield-inc/everyport#readme) for the desktop app and every command.
