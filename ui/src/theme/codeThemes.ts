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
      addedWord: "#b6d6bf",
      removed: "#f8dee0",
      removedWord: "#f0b8bc",
      changed: "#dae9f9",
      changedWord: "#b0cff3",
      conflict: "#f5edd9",
      conflictWord: "#ebd9ad",
      keyword: "#cf222e",
      string: "#0a3069",
      comment: "#59636e",
      number: "#0550ae",
      type: "#953800",
      function: "#8250df",
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
      addedWord: "#1a4a29",
      removed: "#301b1f",
      removedWord: "#6b2b2b",
      changed: "#13233a",
      changedWord: "#1e4273",
      conflict: "#272215",
      conflictWord: "#533d11",
      keyword: "#ff7b72",
      string: "#a5d6ff",
      comment: "#9198a1",
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
      gutterText: "#586e75",
      selection: "#e4dcc4",
      hunkBackground: "#eee8d5",
      hunkText: "#586e75",
      added: "#ebe8c1",
      addedWord: "#d7d89a",
      removed: "#f8d9c8",
      removedWord: "#f2b7a9",
      changed: "#dde6e0",
      changedWord: "#b8d4de",
      conflict: "#f2e6c1",
      conflictWord: "#e6d39a",
      keyword: "#859900",
      string: "#2aa198",
      comment: "#839496",
      number: "#d33682",
      type: "#b58900",
      function: "#268bd2",
      tag: "#cb4b16",
    },
  },
  solarized_dark: {
    label: "Solarized Dark",
    dark: true,
    colors: {
      background: "#002b36",
      text: "#eee8d5",
      gutterBackground: "#073642",
      gutterText: "#839496",
      selection: "#274642",
      hunkBackground: "#073642",
      hunkText: "#93a1a1",
      added: "#143c2e",
      addedWord: "#2f5223",
      removed: "#212c35",
      removedWord: "#4d2d34",
      changed: "#06394d",
      changedWord: "#0d4d6d",
      conflict: "#1b392e",
      conflictWord: "#3f4c23",
      keyword: "#859900",
      string: "#2aa198",
      comment: "#657b83",
      number: "#d33682",
      type: "#b58900",
      function: "#268bd2",
      tag: "#cb4b16",
    },
  },
  one_dark: {
    label: "One Dark",
    dark: true,
    colors: {
      background: "#282c34",
      text: "#abb2bf",
      gutterBackground: "#282c34",
      gutterText: "#7d8799",
      selection: "#3e4451",
      hunkBackground: "#2c313a",
      hunkText: "#7d8799",
      added: "#32393a",
      addedWord: "#3a443f",
      removed: "#3a323b",
      removedWord: "#543b44",
      changed: "#2d3845",
      changedWord: "#324254",
      conflict: "#373739",
      conflictWord: "#42413e",
      keyword: "#c678dd",
      string: "#98c379",
      comment: "#7d8799",
      number: "#d19a66",
      type: "#e5c07b",
      function: "#61afef",
      tag: "#e06c75",
    },
  },
  dracula: {
    label: "Dracula",
    dark: true,
    colors: {
      background: "#282a36",
      text: "#f8f8f2",
      gutterBackground: "#282a36",
      gutterText: "#6272a4",
      selection: "#44475a",
      hunkBackground: "#343746",
      hunkText: "#6272a4",
      added: "#2d433e",
      addedWord: "#336449",
      removed: "#422f3a",
      removedWord: "#64363f",
      changed: "#34414e",
      changedWord: "#445f6e",
      conflict: "#423b3c",
      conflictWord: "#645245",
      keyword: "#ff79c6",
      string: "#f1fa8c",
      comment: "#6272a4",
      number: "#bd93f9",
      type: "#8be9fd",
      function: "#50fa7b",
      tag: "#ff79c6",
    },
  },
  nord: {
    label: "Nord",
    dark: true,
    colors: {
      background: "#2e3440",
      text: "#d8dee9",
      gutterBackground: "#2e3440",
      gutterText: "#7b88a1",
      selection: "#434c5e",
      hunkBackground: "#3b4252",
      hunkText: "#81a1c1",
      added: "#3c4549",
      addedWord: "#4f5b55",
      removed: "#3f3945",
      removedWord: "#57414c",
      changed: "#38414f",
      changedWord: "#455364",
      conflict: "#454649",
      conflictWord: "#635e55",
      keyword: "#81a1c1",
      string: "#a3be8c",
      comment: "#7b88a1",
      number: "#b48ead",
      type: "#8fbcbb",
      function: "#88c0d0",
      tag: "#81a1c1",
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
      hunkText: "#90908a",
      added: "#363e23",
      addedWord: "#4b5c25",
      removed: "#40282c",
      removedWord: "#622738",
      changed: "#2f3d3b",
      changedWord: "#395a5b",
      conflict: "#413522",
      conflictWord: "#634721",
      keyword: "#f92672",
      string: "#e6db74",
      comment: "#75715e",
      number: "#ae81ff",
      type: "#66d9ef",
      function: "#a6e22e",
      tag: "#f92672",
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
