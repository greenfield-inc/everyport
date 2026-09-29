import { type CSSProperties, type ReactNode, useSyncExternalStore } from "react";
import themes from "./themes/themes.json";

type Theme = {
  title: string;
  sans: string;
  mono: string;
  light: Record<string, string>;
  dark: Record<string, string>;
};

const THEMES = themes as Record<string, Theme>;

export const DEFAULT_THEME = "pastel-dreams-green";

/** Every Doozy theme, for a settings picker. */
export const themeList = Object.entries(THEMES).map(([name, theme]) => ({ name, title: theme.title }));

export type Appearance = "system" | "light" | "dark";

/** Paper's amber (#FFB224) for dark panels, and a darker amber (#B45309) for light ones. */
const AMBER = { dark: "oklch(0.817 0.164 75.8)", light: "oklch(0.555 0.146 49)" };

/**
 * The attention color: the theme's `warning`, or a fixed amber. The desktop
 * app uses it for the tray's attention icon.
 */
export function warningColor(theme: string, dark: boolean): string {
  const chosen = THEMES[theme] ?? THEMES[DEFAULT_THEME];
  return (dark ? chosen.dark : chosen.light).warning ?? (dark ? AMBER.dark : AMBER.light);
}

type Oklch = { l: number; c: number; h: number };

function parse(color: string): Oklch {
  const [l = 0, c = 0, h = 0] = (color.match(/[\d.]+%?/g) ?? []).map((part) => (part.endsWith("%") ? parseFloat(part) / 100 : Number(part)));
  return { l, c, h };
}

const format = ({ l, c, h }: Oklch) => `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)})`;

/** WCAG relative luminance of an OKLCH color, clipped to sRGB. */
function luminance({ l, c, h }: Oklch): number {
  const a = c * Math.cos((h * Math.PI) / 180);
  const b = c * Math.sin((h * Math.PI) / 180);
  const long = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const medium = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const short = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const clip = (value: number) => Math.min(1, Math.max(0, value));
  const red = clip(4.0767416621 * long - 3.3077115913 * medium + 0.2309699292 * short);
  const green = clip(-1.2684380046 * long + 2.6097574011 * medium - 0.3413193965 * short);
  const blue = clip(-0.0041960863 * long - 0.7034186147 * medium + 1.707614701 * short);
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

const contrast = (x: number, y: number) => (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);

/** 4.5:1 (WCAG AA for text), with margin for rounding. */
const MIN_CONTRAST = 4.6;
const RED = 27;
const ORANGE = 60;

/** Steps lightness away from `background`, keeping the hue, until the color reads on it. */
function against(color: Oklch, background: number): string {
  const step = background > 0.18 ? -0.01 : 0.01;
  const moved = { ...color };
  while (contrast(luminance(moved), background) < MIN_CONTRAST && (step < 0 ? moved.l > 0 : moved.l < 1)) {
    moved.l = Math.min(1, Math.max(0, moved.l + step));
  }
  return format(moved);
}

/** A semantic color with some saturation. A gray input takes `grayHue`, so danger always looks like danger. */
function semantic(color: string, grayHue: number): Oklch {
  const input = parse(color);
  return { l: input.l, c: Math.max(input.c, 0.15), h: input.c < 0.04 ? grayHue : input.h };
}

/** Warning and danger colors that read on the panel, and a danger fill that takes white text. */
function semanticColors(warning: string, destructive: string, panel: string) {
  const background = luminance(parse(panel));
  return {
    "--ppm-warn": against(semantic(warning, ORANGE), background),
    "--ppm-danger": against(semantic(destructive, RED), background),
    "--ppm-danger-fill": against(semantic(destructive, RED), 1),
  };
}

export type ThemeProps = {
  /** A name from `themeList`. */
  theme?: string;
  appearance?: Appearance;
};

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

function subscribe(onChange: () => void) {
  const query = darkQuery();
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

function useSystemDark() {
  return useSyncExternalStore(subscribe, () => darkQuery().matches, () => false);
}

/** Root of every @ppm/ui surface: sets the theme's variables and app chrome. */
export function Themed({
  theme = DEFAULT_THEME,
  appearance = "system",
  className,
  children,
}: ThemeProps & { className?: string; children: ReactNode }) {
  const systemDark = useSystemDark();
  const dark = appearance === "dark" || (appearance === "system" && systemDark);
  const chosen = THEMES[theme] ?? THEMES[DEFAULT_THEME];
  const colors = dark ? chosen.dark : chosen.light;
  const style: Record<string, string> = {
    "--ppm-sans": chosen.sans,
    "--ppm-mono": chosen.mono,
    ...semanticColors(warningColor(theme, dark), colors.destructive, colors.popover),
  };
  for (const [key, value] of Object.entries(colors)) style[`--${key}`] = value;
  return (
    <div
      className={`ppm-root ${className ?? ""}`}
      data-appearance={dark ? "dark" : "light"}
      style={style as CSSProperties}
      onContextMenu={(event) => event.preventDefault()}
    >
      {children}
    </div>
  );
}
