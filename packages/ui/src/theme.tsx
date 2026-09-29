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
  const style: Record<string, string> = { "--ppm-sans": chosen.sans, "--ppm-mono": chosen.mono };
  for (const [key, value] of Object.entries(dark ? chosen.dark : chosen.light)) style[`--${key}`] = value;
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
