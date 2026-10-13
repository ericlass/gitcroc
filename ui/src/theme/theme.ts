// Applies the theme setting (UI-23) as `data-theme` on the root element.
// `system` follows the OS through `prefers-color-scheme`, live. The code
// theme (UI-24) and the fonts (UI-26) are CSS variables on the root too, so
// every component follows a change at once.
import { type Accessor, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { Appearance, CodeTheme, Theme } from "../ipc";
import { applyCodeTheme } from "./codeThemes";

export type ResolvedTheme = "light" | "dark";

export const DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(theme: Theme, systemDark: boolean): ResolvedTheme {
  if (theme === "system") {
    return systemDark ? "dark" : "light";
  }
  return theme;
}

/** The code theme in use: the light mode choice while the app is light, the dark one otherwise. */
export function codeThemeFor(appearance: Pick<Appearance, "code_theme_light" | "code_theme_dark">, theme: ResolvedTheme): CodeTheme {
  return theme === "light" ? appearance.code_theme_light : appearance.code_theme_dark;
}

/** Sets `data-theme` and the default code theme before the first render, so nothing paints unthemed. */
export function applyInitialTheme(root: HTMLElement = document.documentElement): void {
  const theme = resolveTheme("system", window.matchMedia(DARK_QUERY).matches);
  root.dataset.theme = theme;
  applyCodeTheme(root, theme === "light" ? "github_light" : "github_dark");
}

/** Keeps `data-theme` on `root` in step with `theme` and the OS setting; returns the theme in use. */
export function followTheme(theme: Accessor<Theme>, root: HTMLElement = document.documentElement): Accessor<ResolvedTheme> {
  const media = window.matchMedia(DARK_QUERY);
  const [systemDark, setSystemDark] = createSignal(media.matches);
  const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
  media.addEventListener("change", onChange);
  onCleanup(() => media.removeEventListener("change", onChange));

  const resolved = createMemo(() => resolveTheme(theme(), systemDark()));
  createEffect(() => {
    root.dataset.theme = resolved();
  });
  return resolved;
}

/**
 * A font family list for CSS: `name` quoted, then the system font in
 * `fallback`; only the fallback when `name` is `null`.
 */
export function fontFamily(name: string | null, fallback: string): string {
  if (name === null) {
    return fallback;
  }
  // A CSS string: backslashes and quotes escaped, line breaks as escapes.
  const quoted = name.replace(/[\\"]/g, "\\$&").replace(/[\n\r\f]/g, (c) => `\\${c.charCodeAt(0).toString(16)} `);
  return `"${quoted}", ${fallback}`;
}

/** Keeps the code theme and font variables on `root` in step with the settings and the theme in use. */
export function followAppearance(
  appearance: Accessor<Appearance>,
  theme: Accessor<ResolvedTheme>,
  root: HTMLElement = document.documentElement,
): void {
  createEffect(() => applyCodeTheme(root, codeThemeFor(appearance(), theme())));
  createEffect(() => {
    const fonts = appearance();
    root.style.setProperty("--font-ui", fontFamily(fonts.ui_font, "var(--font-ui-system)"));
    root.style.setProperty("--font-size-ui", `${fonts.ui_font_size}px`);
    root.style.setProperty("--font-code", fontFamily(fonts.code_font, "var(--font-code-system)"));
    root.style.setProperty("--font-size-code", `${fonts.code_font_size}px`);
  });
}
