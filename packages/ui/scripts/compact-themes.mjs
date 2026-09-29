// Writes src/themes/themes.json from registry.json (the Doozy themes, in the
// shadcn registry format), keeping only what @ppm/ui reads. Run it after
// copying a new registry.json: `pnpm --filter @ppm/ui themes`.
import { readFileSync, writeFileSync } from "node:fs";

const dir = new URL("../src/themes/", import.meta.url);
const registry = JSON.parse(readFileSync(new URL("registry.json", dir), "utf8"));

const COLORS = [
  "foreground",
  "popover",
  "primary",
  "primary-foreground",
  "muted-foreground",
  "accent",
  "border",
  "destructive",
  "chart-1",
  "chart-2",
  "chart-3",
  "chart-4",
  "chart-5",
  "warning",
];

// Doozy's `var(--font-*)` app fonts become Sora, which @ppm/ui bundles.
const sans = (stack) => stack.replace(/var\(--font-[\w-]+\)/g, "Sora");
// Numbers fall back to the bundled Geist Mono before the generic monospace.
const mono = (stack) =>
  [
    ...stack
      .replace(/var\(--font-[\w-]+\)/g, "")
      .split(",")
      .map((font) => font.trim())
      .filter((font) => font && font !== "monospace"),
    '"Geist Mono"',
    "monospace",
  ].join(", ");

const pick = (vars) =>
  Object.fromEntries(COLORS.filter((key) => vars[key]).map((key) => [key, vars[key]]));

const themes = Object.fromEntries(
  registry.items.map((item) => {
    const { theme, light, dark } = item.cssVars;
    return [
      item.name,
      {
        title: item.title,
        sans: sans(theme["font-sans"] ?? light["font-sans"]),
        mono: mono(theme["font-mono"] ?? light["font-mono"]),
        light: pick(light),
        dark: pick(dark),
      },
    ];
  }),
);

writeFileSync(new URL("themes.json", dir), `${JSON.stringify(themes)}\n`);
console.log(`Wrote ${Object.keys(themes).length} themes`);
