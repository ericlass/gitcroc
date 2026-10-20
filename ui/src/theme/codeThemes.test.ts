// @vitest-environment node
import { expect, test } from "vitest";
import { CODE_THEME_NAMES, CODE_THEMES, type CodeThemeColors } from "./codeThemes";

test("ui_24_eight_code_themes_ship", () => {
  const shipped = Object.fromEntries(CODE_THEME_NAMES.map((name) => [name, [CODE_THEMES[name].label, CODE_THEMES[name].dark]]));
  expect(shipped).toEqual({
    github_light: ["GitHub Light", false],
    github_dark: ["GitHub Dark", true],
    solarized_light: ["Solarized Light", false],
    solarized_dark: ["Solarized Dark", true],
    one_dark: ["One Dark", true],
    dracula: ["Dracula", true],
    nord: ["Nord", true],
    monokai: ["Monokai", true],
  });
});

/** WCAG 2 relative luminance of an opaque `#rrggbb` color. */
function luminance(color: string): number {
  const [r, g, b] = [1, 3, 5].map((at) => {
    const value = Number.parseInt(color.slice(at, at + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG 2 contrast ratio. */
function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

test("ui_25_diff_colors_meet_contrast", () => {
  expect(contrast("#ffffff", "#000000")).toBeCloseTo(21);
  expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 2);

  const backgrounds: (keyof CodeThemeColors)[] = [
    "added",
    "addedWord",
    "removed",
    "removedWord",
    "changed",
    "changedWord",
    "conflict",
    "conflictWord",
  ];
  const failures: string[] = [];
  for (const name of CODE_THEME_NAMES) {
    const colors = CODE_THEMES[name].colors;
    for (const key of ["text", ...backgrounds] as const) {
      // Opaque colors only, so the ratio is the one on screen.
      expect(colors[key], `${name} ${key}`).toMatch(/^#[0-9a-f]{6}$/);
    }
    for (const key of backgrounds) {
      const ratio = contrast(colors.text, colors[key]);
      if (ratio < 4.5) {
        failures.push(`${name} ${key}: ${ratio.toFixed(2)}`);
      }
    }
  }
  expect(failures).toEqual([]);
});

test("ui_25_syntax_colors_meet_contrast", () => {
  // Every color text is drawn in, on every background it can be drawn on: the editor's, and each diff highlight
  // at line and word level (WCAG 1.4.3). Gutter and hunk header text on their own backgrounds.
  const syntax: (keyof CodeThemeColors)[] = [
    "text",
    "keyword",
    "string",
    "comment",
    "number",
    "type",
    "function",
    "tag",
  ];
  const backgrounds: (keyof CodeThemeColors)[] = [
    "background",
    "added",
    "addedWord",
    "removed",
    "removedWord",
    "changed",
    "changedWord",
    "conflict",
    "conflictWord",
  ];
  const pairs: [keyof CodeThemeColors, keyof CodeThemeColors][] = [
    ...syntax.flatMap((text) => backgrounds.map((background): [typeof text, typeof background] => [text, background])),
    ["gutterText", "gutterBackground"],
    ["hunkText", "hunkBackground"],
  ];
  const failures: string[] = [];
  for (const name of CODE_THEME_NAMES) {
    const colors = CODE_THEMES[name].colors;
    for (const [text, background] of pairs) {
      expect(colors[text], `${name} ${text}`).toMatch(/^#[0-9a-f]{6}$/);
      const ratio = contrast(colors[text], colors[background]);
      if (ratio < 4.5) {
        failures.push(`${name} ${text} on ${background}: ${ratio.toFixed(2)}`);
      }
    }
  }
  expect(failures).toEqual([]);
});
