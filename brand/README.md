# Brand

The mark is **Socket**: a port drawn as a wall outlet. It has a rounded square face, two slots and a ground dot. The right slot lights up while servers run.

| File | Use |
|---|---|
| `socket.svg` | The mark in `currentColor`. Use it in UI and on the site. |
| `socket-app.svg` | The app icon: the mark on the dark green tile with the lit slot. Source of every file in `apps/desktop/src-tauri/icons/`. |
| `socket-template.svg` | Black on transparent for macOS template images, which the system tints. |
| `socket-animated.svg` | Motion reference: plug in, breathe, unplug and hover as CSS keyframes. |

## Colors

| Role | Dark | Light |
|---|---|---|
| Main | `#f2f4f0` | `#1b1d1b` |
| Accent (lit slot) | `#7fd89d` | `#2f9a5d` |
| Attention | `#ffb224` | `#ffb224` |
| App tile | `#1f2b23` to `#0d100e` | same |

The unlit slot is the main color at 35% opacity.

## Geometry and clear space

The mark is drawn on a 24-unit grid: the face is 18 units with a 2-unit stroke, and the ground dot sits at y 16.4. Keep at least 3 units of clear space (the face's own margin in the grid) on every side. Don't draw the mark below 16 px.

## Element ids

Every SVG uses the same ids, so the site and the app can style or animate any copy:

| Id | Element |
|---|---|
| `#face` | The rounded square outline |
| `#slot-left` | The left slot, always the main color |
| `#slot-right` | The lit slot |
| `#ground` | The ground dot |

`socket-animated.svg` wraps them in `#mark` and reads `data-state` on the root: `idle`, `running` or `attention`.

## Motion

| Name | When | What |
|---|---|---|
| Plug in | First show while servers run | The right slot fills green from the bottom up, 400 ms |
| Breathe | A leak or other attention is active | The right slot fades between amber and 60% amber, 2.4 s loop |
| Unplug | The last server stops (the state changes to idle) | The right slot fades to the unlit color, 400 ms |
| Hover | Pointer over the mark | The mark rises 1 px, like a plug seating |

All motion stops under `prefers-reduced-motion: reduce`. The app's `Socket` component in `packages/ui` implements the same states. Tray icons stay static.

## Don'ts

- Don't add a dot grid or a colon. That was WhatThePort's mark.
- Don't light the left slot, or color the face or ground dot, except in the attention state, where the whole mark turns amber.
- Don't stretch, rotate or outline the mark, or put it on a busy background without the tile.

## Regenerate

```bash
pnpm --filter @everyport/desktop tauri icon ../../brand/socket-app.svg   # app icons (delete the android/ and ios/ output)
pnpm --filter @everyport/ui screenshots && pnpm --filter @everyport/ui readme-assets   # docs/assets
```

The tray icons are drawn in code from the same geometry (`apps/desktop/src-tauri/src/tray.rs`), so the attention state can use the theme's warning color.
