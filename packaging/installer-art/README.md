# installer-art

Renders the art for the macOS DMG window and the Windows NSIS and MSI wizards from one SVG template, so every Greenfield app installs with the same look: a light warm-grey ground, faint Socket-shaped outlines and film grain, the product mark and name in dark ink, and the accent only on the arrow. Text is Sora, bundled in `fonts/` (OFL).

It needs Node 22 or later and runs anywhere `@resvg/resvg-js` does. No browser.

```bash
node render.mjs --name Everyport --tagline "Every dev server. Every OS." \
  --mark brand/socket.svg --icon brand/socket-app.svg --out build/installer
```

## Inputs

| Flag | Required | What |
|---|---|---|
| `--name` | yes | Product name, drawn next to the mark |
| `--mark` | yes | The mark as an SVG with a `viewBox`. `currentColor` takes the ink color. |
| `--icon` | yes | The app icon as an SVG. The WiX banner shows it, and the installer icon adds a download badge to it. |
| `--out` | yes | Output folder, created if missing |
| `--tagline` | no | One short line under the name. Each sentence starts a new line in the Windows sidebar. |
| `--accent` | no | Accent color for the arrow and the icon badge. Defaults to Everyport green for the theme. |
| `--theme` | no | `light` (default) or `dark`. Finder labels and the Windows wizard text are black, so `dark` only suits the Windows sidebar. |

## Outputs

| File | Size | Used by |
|---|---|---|
| `dmg-background.tiff` | 660×400 and 1320×800 in one file | Finder picks the page for the display |
| `dmg-background.png`, `dmg-background@2x.png` | 660×400, 1320×800 | The same art as separate PNGs |
| `nsis-header.bmp` | 150×57, 24-bit | NSIS page header |
| `nsis-sidebar.bmp` | 164×314, 24-bit | NSIS welcome and finish pages |
| `wix-banner.bmp` | 493×58, 24-bit | MSI page banner. WiX writes the page title over the left 406 px. |
| `wix-dialog.bmp` | 493×312, 24-bit | MSI first and last dialogs. WiX writes text from x 180. |
| `installer.ico` | 16 to 256 px | Installer and uninstaller icons |

NSIS and WiX take one bitmap each and scale it on high-DPI screens, so they have no 2× files.

## DMG layout

The art expects a 660×400 window with 128 pt icons centered at these points:

| Item | x | y |
|---|---|---|
| The app | 180 | 190 |
| Applications | 480 | 190 |

The title bar and Finder's path bar can cover about 60 pt at the bottom, so the art keeps everything in the top 340 pt.

## Tauri

Keys in `tauri.conf.json`, with paths relative to `src-tauri`:

```json
"bundle": {
  "macOS": {
    "dmg": {
      "background": "installer/dmg-background.tiff",
      "windowSize": { "width": 660, "height": 400 },
      "appPosition": { "x": 180, "y": 190 },
      "applicationFolderPosition": { "x": 480, "y": 190 }
    }
  },
  "windows": {
    "nsis": {
      "headerImage": "installer/nsis-header.bmp",
      "sidebarImage": "installer/nsis-sidebar.bmp",
      "installerIcon": "installer/installer.ico",
      "uninstallerIcon": "installer/installer.ico"
    },
    "wix": {
      "bannerPath": "installer/wix-banner.bmp",
      "dialogImagePath": "installer/wix-dialog.bmp"
    }
  }
}
```

Tauri uses the app's `.icns` as the DMG volume icon and has no key for a separate one.

## electron-builder

Keys in `electron-builder.yml` (or `build` in `package.json`):

```yaml
dmg:
  background: build/installer/dmg-background.tiff
  window: { width: 660, height: 400 }
  iconSize: 128
  contents:
    - { x: 180, y: 190, type: file }
    - { x: 480, y: 190, type: link, path: /Applications }
nsis:
  oneClick: false          # the assisted wizard, which shows the images
  installerHeader: build/installer/nsis-header.bmp
  installerSidebar: build/installer/nsis-sidebar.bmp
  uninstallerSidebar: build/installer/nsis-sidebar.bmp
  installerIcon: build/installer/installer.ico
  uninstallerIcon: build/installer/installer.ico
```

electron-builder's `msi` target has no banner or dialog image keys, so the `wix-*.bmp` files are for Tauri.

## Reuse in another app

Copy this folder into the app's repository, add `@resvg/resvg-js` to its dev dependencies, and run `render.mjs` before packaging, as Everyport's `beforeBuildCommand` does. The output is generated, so ignore it in git.
