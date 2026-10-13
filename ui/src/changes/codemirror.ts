// What every diff editor shares (UI-17, SEC-2, SEC-3, UI-24, UI-26):
// read-only, styled only through the code theme's and the fonts' variables,
// so a change of either restyles open editors at once, its injected styles carrying the
// page's CSP nonce, and the syntax of the file's extension. CodeMirror
// renders the document as text, never as HTML.
import { LanguageDescription, HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

/**
 * The nonce Tauri gives the page's style tags; CodeMirror adds its own
 * style tag with it, so `style-src` needs no `'unsafe-inline'` (SEC-2).
 * Empty where the page has no CSP, as in tests.
 */
export function cspNonce(): string {
  return document.querySelector<HTMLStyleElement>("style#csp-nonce")?.nonce ?? "";
}

/** Syntax colors from the code theme (src/theme/codeThemes.ts). */
const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.operatorKeyword, tags.modifier, tags.controlKeyword], color: "var(--code-keyword)" },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: "var(--code-string)" },
  { tag: [tags.comment, tags.lineComment, tags.blockComment, tags.docComment], color: "var(--code-comment)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--code-number)" },
  { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--code-type)" },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName), tags.macroName], color: "var(--code-function)" },
  { tag: [tags.tagName, tags.heading], color: "var(--code-tag)" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.emphasis, fontStyle: "italic" },
]);

const theme = EditorView.theme({
  "&": {
    color: "var(--code-text)",
    backgroundColor: "var(--code-bg)",
    fontSize: "var(--font-size-code)",
    height: "100%",
  },
  ".cm-scroller": { fontFamily: "var(--font-code)", lineHeight: "1.5" },
  ".cm-content": { caretColor: "transparent" },
  ".cm-gutters": {
    backgroundColor: "var(--code-gutter-bg)",
    color: "var(--code-gutter-text)",
    borderRight: "1px solid var(--border)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "var(--code-selection)" },
  // Inline mode.
  ".cm-diff-added": { backgroundColor: "var(--diff-added-bg)" },
  ".cm-diff-removed": { backgroundColor: "var(--diff-removed-bg)" },
  ".cm-diff-hunk": {
    backgroundColor: "var(--code-hunk-bg)",
    color: "var(--code-hunk-text)",
    fontFamily: "var(--font-code)",
    padding: "2px 8px",
  },
  ".cm-diff-number": { minWidth: "3ch", textAlign: "right", padding: "0 4px" },
  // Side-by-side mode (@codemirror/merge). Its base theme sets some rules
  // for light and dark apart; these are as specific, so the code theme wins.
  "&.cm-merge-a .cm-changedLine": { backgroundColor: "var(--diff-removed-bg)" },
  "&.cm-merge-b .cm-changedLine": { backgroundColor: "var(--diff-added-bg)" },
  "&.cm-merge-a .cm-collapsedLines, &.cm-merge-b .cm-collapsedLines": {
    background: "var(--code-hunk-bg)",
    color: "var(--code-hunk-text)",
  },
});

/** A compartment per option that changes without rebuilding the editor. */
export interface EditorParts {
  wrap: Compartment;
  language: Compartment;
}

export const newParts = (): EditorParts => ({ wrap: new Compartment(), language: new Compartment() });

/** The extensions of a read-only diff editor. */
export function baseExtensions(parts: EditorParts, wrap: boolean): Extension[] {
  const nonce = cspNonce();
  return [
    ...(nonce === "" ? [] : [EditorView.cspNonce.of(nonce)]),
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    theme,
    syntaxHighlighting(highlight),
    parts.wrap.of(wrap ? EditorView.lineWrapping : []),
    parts.language.of([]),
  ];
}

export const wrapping = (parts: EditorParts, wrap: boolean) =>
  parts.wrap.reconfigure(wrap ? EditorView.lineWrapping : []);

/**
 * Loads the language of `path` by its file name and applies it to `views`;
 * an unknown type stays plain text. Languages load on first use.
 */
export async function applyLanguage(parts: EditorParts, path: string, views: EditorView[]): Promise<void> {
  const name = path.split("/").filter(Boolean).pop() ?? path;
  const description = LanguageDescription.matchFilename(languages, name);
  if (description === null) {
    return;
  }
  try {
    const support = await description.load();
    for (const view of views) {
      view.dispatch({ effects: parts.language.reconfigure(support) });
    }
  } catch {
    // A language that fails to load leaves the diff as plain text.
  }
}
