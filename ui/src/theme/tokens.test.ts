// @vitest-environment node
// UI-23 with WCAG 2.2 AA (README "Accessibility target"): in both app themes of src/theme/tokens.css, text
// against each background it sits on at least 4.5:1 (1.4.3), and the focus ring, the borders of inputs and the
// selection mark at least 3:1 (1.4.11). Disabled text is exempt (WCAG 1.4.3).
import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

const TOKENS = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

/** The `#rrggbb` variables of the block for `theme`, by name without the dashes. */
function themeColors(theme: "light" | "dark"): Record<string, string> {
  const block = new RegExp(`:root\\[data-theme="${theme}"\\] \\{([^}]*)\\}`).exec(TOKENS)?.[1] ?? "";
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})\s*;/g)].map((m) => [m[1], m[2]]));
}

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

const BACKGROUNDS = ["bg-1", "bg-2", "bg-3", "selection-bg"];

/** The pairs to check: each foreground against each background it is drawn on, and the ratio it needs. */
const PAIRS: { fg: string[]; bg: string[]; ratio: number }[] = [
  // Text, and text in a color that carries meaning (UI-3, GRP-7), on every background level and the selection.
  {
    fg: [
      "text-primary",
      "text-secondary",
      "accent",
      "danger",
      "warning",
      "success",
      "outcome-succeeded",
      "outcome-nothing-to-do",
      "outcome-skipped",
      "outcome-failed",
      "outcome-conflict",
    ],
    bg: BACKGROUNDS,
    ratio: 4.5,
  },
  // The banners' text (OPS-2).
  { fg: ["text-primary", "text-secondary"], bg: ["warning-bg"], ratio: 4.5 },
  // Text on a filled control: a primary or danger button, a pressed toggle, the tab badge and marker.
  { fg: ["accent-text"], bg: ["accent", "danger", "outcome-failed"], ratio: 4.5 },
  // Non-text contrast (1.4.11).
  { fg: ["focus-ring", "border-input", "selection-mark"], bg: BACKGROUNDS, ratio: 3 },
];

test("UI-23 the formula is WCAG's", () => {
  expect(contrast("#ffffff", "#000000")).toBeCloseTo(21);
  expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
});

for (const theme of ["light", "dark"] as const) {
  test(`UI-23 app theme colors meet WCAG contrast, ${theme}`, () => {
    const colors = themeColors(theme);
    const failures: string[] = [];
    for (const { fg, bg, ratio } of PAIRS) {
      for (const front of fg) {
        for (const back of bg) {
          // Opaque colors only, so the ratio is the one on screen.
          expect(colors[front], `--${front}`).toMatch(/^#[0-9a-f]{6}$/);
          expect(colors[back], `--${back}`).toMatch(/^#[0-9a-f]{6}$/);
          const found = contrast(colors[front] ?? "", colors[back] ?? "");
          if (found < ratio) {
            failures.push(`--${front} on --${back}: ${found.toFixed(2)} < ${ratio}`);
          }
        }
      }
    }
    expect(failures).toEqual([]);
  });
}
