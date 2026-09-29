# Design spec

## Source of truth

`docs/design/` holds each Paper frame twice: as a 2x PNG, and as HTML with the exact inline CSS and SVG icons. Measure from the HTML. The live file is [Paper](https://app.paper.design/file/01M3A46SYP52NAK2H9AQEAFJA9/p-1-0) (frames `01 · Servers popover`, `02`, `02b`, `03 · Clean up`).

| Frame | View |
|---|---|
| `01-servers` | Server list, hover state on one row, leaking row in amber, dimmed row for a deleted worktree |
| `02-detail-collapsed` | Server detail: header, session and branch, memory line chart, CPU bars, collapsed processes, footer actions |
| `02b-detail-expanded` | Detail with "6 more" expanded, the session menu open, and the process tree |
| `03-clean-up` | Clean up: amount freed, checklist with reasons, protected note, Cancel and Stop buttons |
| `04-spike-detail` | Leak notification with actions, and the three tray icon states: idle, running, attention |

WhatThePort's own React copy of these views is in `reference/what-the-port/web/components/App.tsx`, and its SwiftUI views are in `reference/what-the-port/WhatThePort/Sources/WhatThePort/UI/`. Port behavior from them.

## What stays pixel-exact

Keep these exactly as the Paper frames measure:

- Layout, spacing, element sizes and order
- Type sizes and weights: 13px (500 for titles, 400 for body), 11px for secondary lines, and 28px for the headline number
- Radii: the panel is 16px. Rows, buttons and menus are 6 to 9px. Checkboxes are 4px.
- 0.5px hairline borders and dividers
- The panel shadow: `0 0 0 0.5px rgba(0,0,0,.6), 0 24px 64px rgba(0,0,0,.55)`
- Numbers in a monospace font with tabular figures

## What comes from the theme

Colors and the body font come from the active Doozy theme. The 40 themes are in `packages/ui/src/themes/registry.json` (the shadcn/tweakcn registry format, copied from Doozy). The default is `pastel-dreams-green`, and the mode follows the OS. Map Paper's hard-coded colors to theme variables:

| Paper value | Theme variable |
|---|---|
| Panel `rgba(32,32,36,.9)` | `--popover` (tinted over native blur; see the intent brief, item 18) |
| Text `#F5F5F7` | `--foreground` |
| Secondary text `rgba(235,235,245,.6)` and `.4` | `--muted-foreground`, at full and at 70% opacity |
| Row hover and buttons `rgba(255,255,255,.08)` | `--accent` |
| Hairlines `rgba(255,255,255,.08)` | `--border` |
| Amber `#FFB224` (leak, attention, threshold) | `--chart-4`, or `--warning` when a theme defines it |
| Red stop button and destructive fill | `--destructive` |
| Memory-bar segments and port colors | `--chart-1` to `--chart-5` |
| Primary button (white "Open localhost") | `--primary` / `--primary-foreground` |

Use the theme's `font-sans` for text. Doozy's app font is Sora. Numbers use the theme's `font-mono` and fall back to Geist Mono. Themes never change radii or spacing: ignore the theme's `radius`.

## What's ours

Motion (intent brief, item 13), the app icon, and the tray icon colors (the dot grid stays; the attention color is the theme's amber). Don't reuse the WhatThePort name, the colon logo, or the tomjohn.design sample data.

## Verifying a view

Render it from `fixtureSnapshot` (`@ppm/protocol`) at 2x. Put it next to the Paper PNG at the same size and check layout, spacing and type. Colors will differ by design. Include the side-by-side in the PR.
