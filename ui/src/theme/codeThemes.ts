// The code themes (UI-24): the only place their colors are defined, as
// src/theme/tokens.css is for the app theme (UI-23). A code theme sets the
// `--code-*` and `--diff-*` variables that the diff editors use; the added,
// removed, changed and conflict colors keep 4.5:1 against the text (UI-25,
// checked by codeThemes.test.ts).
//
// Sources, all MIT: GitHub's Primer colors, Ethan Schoonover's Solarized,
// @codemirror/theme-one-dark, Dracula, Nord, and Monokai as in
// @uiw/codemirror-theme-monokai. Diff colors are the theme's own green, red,
// blue and yellow mixed into its background. Solarized uses base02 and base2
// for text, its strongest content tones, so its diff lines can keep 4.5:1.
// For WCAG 2.2 AA (M5-10), each syntax color is lighter or darker than its
// source, with its hue kept, so it keeps 4.5:1 on the background and on every
// highlight; the word-level highlights sit 30% nearer the background.
import type { CodeTheme } from "../ipc";

export interface CodeThemeColors {
  background: string;
  text: string;
  gutterBackground: string;
  gutterText: string;
  selection: string;
  hunkBackground: string;
  hunkText: string;
  /** Line and word level highlights (UI-25). */
  added: string;
  addedWord: string;
  removed: string;
  removedWord: string;
  changed: string;
  changedWord: string;
  conflict: string;
  conflictWord: string;
  /** Syntax. */
  keyword: string;
  string: string;
  comment: string;
  number: string;
  type: string;
  function: string;
  tag: string;
}

export interface CodeThemeDef {
  label: string;
  dark: boolean;
  colors: CodeThemeColors;
}

export const CODE_THEMES: Record<CodeTheme, CodeThemeDef> = {
  github_light: {
    label: "GitHub Light",
    dark: false,
    colors: {
      background: "#ffffff",
      text: "#1f2328",
      gutterBackground: "#f6f8fa",
      gutterText: "#59636e",
      selection: "#cce5ff",
      hunkBackground: "#ddf4ff",
      hunkText: "#59636e",
      added: "#ddece1",
      addedWord: "#cce2d2",
      removed: "#f8dee0",
      removedWord: "#f4cdd0",
      changed: "#dae9f9",
      changedWord: "#c8ddf7",
      conflict: "#f5edd9",
      conflictWord: "#f1e4c6",
      keyword: "#b51e28",
      string: "#0a3069",
      comment: "#545e68",
      number: "#0550ae",
      type: "#953800",
      function: "#6e34da",
      tag: "#116329",
    },
  },
  github_dark: {
    label: "GitHub Dark",
    dark: true,
    colors: {
      background: "#0d1117",
      text: "#e6edf3",
      gutterBackground: "#151b23",
      gutterText: "#9198a1",
      selection: "#264f78",
      hunkBackground: "#121d2f",
      hunkText: "#9198a1",
      added: "#12261e",
      addedWord: "#163924",
      removed: "#301b1f",
      removedWord: "#4f2325",
      changed: "#13233a",
      changedWord: "#193357",
      conflict: "#272215",
      conflictWord: "#3e3013",
      keyword: "#ff7b72",
      string: "#a5d6ff",
      comment: "#959ca4",
      number: "#79c0ff",
      type: "#ffa657",
      function: "#d2a8ff",
      tag: "#7ee787",
    },
  },
  solarized_light: {
    label: "Solarized Light",
    dark: false,
    colors: {
      background: "#fdf6e3",
      text: "#073642",
      gutterBackground: "#eee8d5",
      gutterText: "#566b72",
      selection: "#e4dcc4",
      hunkBackground: "#eee8d5",
      hunkText: "#566b72",
      added: "#ebe8c1",
      addedWord: "#e2e1b0",
      removed: "#f8d9c8",
      removedWord: "#f5caba",
      changed: "#dde6e0",
      changedWord: "#cddee0",
      conflict: "#f2e6c1",
      conflictWord: "#eddeb0",
      keyword: "#546000",
      string: "#1a6560",
      comment: "#505d5e",
      number: "#a62463",
      type: "#725600",
      function: "#1a5f90",
      tag: "#9e3a11",
    },
  },
  solarized_dark: {
    label: "Solarized Dark",
    dark: true,
    colors: {
      background: "#002b36",
      text: "#eee8d5",
      gutterBackground: "#073642",
      gutterText: "#8c9c9e",
      selection: "#274642",
      hunkBackground: "#073642",
      hunkText: "#93a1a1",
      added: "#143c2e",
      addedWord: "#214629",
      removed: "#212c35",
      removedWord: "#362c35",
      changed: "#06394d",
      changedWord: "#09435c",
      conflict: "#1b392e",
      conflictWord: "#2c4229",
      keyword: "#9eb600",
      string: "#31bdb2",
      comment: "#9cadb3",
      number: "#e78fba",
      type: "#d6a200",
      function: "#68b1e4",
      tag: "#ef926b",
    },
  },
  one_dark: {
    label: "One Dark",
    dark: true,
    colors: {
      background: "#282c34",
      text: "#abb2bf",
      gutterBackground: "#282c34",
      gutterText: "#8b94a4",
      selection: "#3e4451",
      hunkBackground: "#2c313a",
      hunkText: "#919aa9",
      added: "#32393a",
      addedWord: "#353d3c",
      removed: "#3a323b",
      removedWord: "#47373f",
      changed: "#2d3845",
      changedWord: "#2f3b4a",
      conflict: "#373739",
      conflictWord: "#3a3b3b",
      keyword: "#cf8ee2",
      string: "#98c379",
      comment: "#9fa6b4",
      number: "#d19a66",
      type: "#e5c07b",
      function: "#61afef",
      tag: "#e78d94",
    },
  },
  dracula: {
    label: "Dracula",
    dark: true,
    colors: {
      background: "#282a36",
      text: "#f8f8f2",
      gutterBackground: "#282a36",
      gutterText: "#8591b8",
      selection: "#44475a",
      hunkBackground: "#343746",
      hunkText: "#96a1c2",
      added: "#2d433e",
      addedWord: "#305343",
      removed: "#422f3a",
      removedWord: "#52323c",
      changed: "#34414e",
      changedWord: "#3c4f5d",
      conflict: "#423b3c",
      conflictWord: "#524640",
      keyword: "#ffa0d7",
      string: "#f1fa8c",
      comment: "#b7bed5",
      number: "#cfb0fb",
      type: "#8be9fd",
      function: "#50fa7b",
      tag: "#ffa0d7",
    },
  },
  nord: {
    label: "Nord",
    dark: true,
    colors: {
      background: "#2e3440",
      text: "#d8dee9",
      gutterBackground: "#2e3440",
      gutterText: "#929db1",
      selection: "#434c5e",
      hunkBackground: "#3b4252",
      hunkText: "#98b2cc",
      added: "#3c4549",
      addedWord: "#454f4f",
      removed: "#3f3945",
      removedWord: "#4b3d48",
      changed: "#38414f",
      changedWord: "#3e4a59",
      conflict: "#454649",
      conflictWord: "#53514f",
      keyword: "#b3c7da",
      string: "#b6cca4",
      comment: "#bec5d1",
      number: "#d4bfd0",
      type: "#a9cccb",
      function: "#9eccd9",
      tag: "#b3c7da",
    },
  },
  monokai: {
    label: "Monokai",
    dark: true,
    colors: {
      background: "#272822",
      text: "#f8f8f2",
      gutterBackground: "#272822",
      gutterText: "#90908a",
      selection: "#49483e",
      hunkBackground: "#3e3d32",
      hunkText: "#a8a8a3",
      added: "#363e23",
      addedWord: "#404c24",
      removed: "#40282c",
      removedWord: "#502731",
      changed: "#2f3d3b",
      changedWord: "#344b4a",
      conflict: "#413522",
      conflictWord: "#513e21",
      keyword: "#fc99bd",
      string: "#e6db74",
      comment: "#bab7a9",
      number: "#c6a7ff",
      type: "#66d9ef",
      function: "#a6e22e",
      tag: "#fc99bd",
    },
  },
};

/** The themes in the order the Settings dialog lists them. */
export const CODE_THEME_NAMES = Object.keys(CODE_THEMES) as CodeTheme[];

/** The CSS variable each color sets. */
const VARIABLES: Record<keyof CodeThemeColors, string> = {
  background: "--code-bg",
  text: "--code-text",
  gutterBackground: "--code-gutter-bg",
  gutterText: "--code-gutter-text",
  selection: "--code-selection",
  hunkBackground: "--code-hunk-bg",
  hunkText: "--code-hunk-text",
  added: "--diff-added-bg",
  addedWord: "--diff-added-word-bg",
  removed: "--diff-removed-bg",
  removedWord: "--diff-removed-word-bg",
  changed: "--diff-changed-bg",
  changedWord: "--diff-changed-word-bg",
  conflict: "--diff-conflict-bg",
  conflictWord: "--diff-conflict-word-bg",
  keyword: "--code-keyword",
  string: "--code-string",
  comment: "--code-comment",
  number: "--code-number",
  type: "--code-type",
  function: "--code-function",
  tag: "--code-tag",
};

/** Sets the variables of `theme` on `element`; its descendants follow at once. */
export function applyCodeTheme(element: HTMLElement, theme: CodeTheme): void {
  const colors = CODE_THEMES[theme].colors;
  for (const [key, variable] of Object.entries(VARIABLES) as [keyof CodeThemeColors, string][]) {
    element.style.setProperty(variable, colors[key]);
  }
}
