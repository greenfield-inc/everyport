# port-process-manager

See every dev server running on your machine, or on any box you can reach.

This package installs the `ppm` command (also available as `port-process-manager`). The first run downloads the `ppm` release binary for your platform from [GitHub Releases](https://github.com/greenfield-inc/port-process-manager/releases), checks it against the SHA-256 checksums packed into this package, and caches it.

```bash
npx port-process-manager list
npm i -g port-process-manager
ppm
```

Set `PPM_DOWNLOAD_URL` to a folder that holds the release files to download from a mirror instead.

See the [Port Process Manager README](https://github.com/greenfield-inc/port-process-manager#readme) for the desktop app and every command.
