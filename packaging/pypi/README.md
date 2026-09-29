# port-process-manager

See every dev server running on your machine, or on any box you can reach.

This package installs the `ppm` command (also available as `port-process-manager`). The first run downloads the `ppm` release binary for your platform from [GitHub Releases](https://github.com/greenfield-inc/port-process-manager/releases), checks its SHA-256 checksum, and caches it.

```bash
uvx port-process-manager list
pipx install port-process-manager
ppm
```

Set `PPM_DOWNLOAD_URL` to a folder that holds the release files to download from a mirror instead.

See the [Port Process Manager README](https://github.com/greenfield-inc/port-process-manager#readme) for the desktop app and every command.
