// Applies the theme setting (UI-23) as `data-theme` on the root element.
// `system` follows the OS through `prefers-color-scheme`, live.
import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";
import type { Theme } from "../ipc";

export type ResolvedTheme = "light" | "dark";

export const DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(theme: Theme, systemDark: boolean): ResolvedTheme {
  if (theme === "system") {
    return systemDark ? "dark" : "light";
  }
  return theme;
}

/** Sets `data-theme` before the first render, so nothing paints unthemed. */
export function applyInitialTheme(root: HTMLElement = document.documentElement): void {
  root.dataset.theme = resolveTheme("system", window.matchMedia(DARK_QUERY).matches);
}

/** Keeps `data-theme` on `root` in step with `theme` and the OS setting. */
export function followTheme(theme: Accessor<Theme>, root: HTMLElement = document.documentElement): void {
  const media = window.matchMedia(DARK_QUERY);
  const [systemDark, setSystemDark] = createSignal(media.matches);
  const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
  media.addEventListener("change", onChange);
  onCleanup(() => media.removeEventListener("change", onChange));

  createEffect(() => {
    root.dataset.theme = resolveTheme(theme(), systemDark());
  });
}
