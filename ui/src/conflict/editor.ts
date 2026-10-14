// The conflict editor's CodeMirror parts (OPS-3): the buffer's hunks as a
// state field mapped through every edit, their highlight in the code
// theme's conflict colors (UI-24, UI-25), a header per side naming it with
// its label (OPS-4), Take ours, Take theirs and Take both on each hunk as
// built, its base on request, undo that brings a taken hunk back, and the
// keys to move between hunks and resolve them. Text goes into the DOM as
// text only (SEC-3).
import { defaultKeymap, history, historyKeymap, invertedEffects } from "@codemirror/commands";
import { type EditorState, type Extension, Facet, Prec, type Range, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, keymap, WidgetType } from "@codemirror/view";
import type { ConflictLabels } from "../ipc";
import { type BufferHunk, hunkAt, hunkTarget, nextHunk, type Take, takenText, untouched } from "./buffer";

/** A hunk leaves the buffer, taken. */
const removeHunk = StateEffect.define<number>();
/**
 * A taken hunk comes back as undo restores its text at `from`. Only `from`
 * maps through later edits: they never reach into the taken text's place.
 */
const restoreHunk = StateEffect.define<BufferHunk>({
  map: (hunk, mapping) => {
    const from = mapping.mapPos(hunk.from, 1);
    return { ...hunk, from, to: from + (hunk.to - hunk.from) };
  },
});
const toggleBase = StateEffect.define<number>();

/** The buffer's hunks, in order. */
export const hunksField = StateField.define<BufferHunk[]>({
  create: () => [],
  update(hunks, transaction) {
    let next = transaction.docChanged
      ? hunks.map((hunk) => {
          const from = transaction.changes.mapPos(hunk.from, 1);
          return { ...hunk, from, to: Math.max(from, transaction.changes.mapPos(hunk.to, -1)) };
        })
      : hunks;
    for (const effect of transaction.effects) {
      if (effect.is(removeHunk)) {
        next = next.filter((hunk) => hunk.id !== effect.value);
      } else if (effect.is(restoreHunk)) {
        next = [...next.filter((hunk) => hunk.id !== effect.value.id), effect.value].sort((a, b) => a.from - b.from);
      } else if (effect.is(toggleBase)) {
        next = next.map((hunk) => (hunk.id === effect.value ? { ...hunk, showBase: !hunk.showBase } : hunk));
      }
    }
    return next;
  },
});

/** What the editor's keys and buttons call outside it. */
export interface ConflictActions {
  labels: ConflictLabels;
  markResolved: () => void;
}

export const conflictActions = Facet.define<ConflictActions, ConflictActions>({
  combine: (values) => values[0] ?? { labels: { ours: "ours", theirs: "theirs" }, markResolved: () => {} },
});

const isUntouched = (state: EditorState, hunk: BufferHunk) =>
  untouched(hunk, (from, to) => state.doc.sliceString(from, to));

/** Replaces hunk `id` with `side`'s text; undo brings the hunk back. */
export function take(view: EditorView, id: number, side: Take): boolean {
  const hunk = view.state.field(hunksField).find((candidate) => candidate.id === id);
  if (hunk === undefined || !isUntouched(view.state, hunk)) {
    return false;
  }
  const text = takenText(hunk, side);
  view.dispatch({
    changes: { from: hunk.from, to: hunk.to, insert: text },
    effects: removeHunk.of(id),
    selection: { anchor: hunk.from },
    userEvent: "input.take",
    scrollIntoView: true,
  });
  view.focus();
  return true;
}

/** Moves the cursor to the next or previous hunk (OPS-3). */
export function goToHunk(view: EditorView, direction: 1 | -1): boolean {
  const hunks = view.state.field(hunksField);
  const hunk = nextHunk(hunks, view.state.selection.main.head, direction);
  if (hunk === null) {
    return false;
  }
  view.dispatch({
    selection: { anchor: hunkTarget(hunk, isUntouched(view.state, hunk)) },
    effects: EditorView.scrollIntoView(hunk.from, { y: "center" }),
  });
  return true;
}

/** Takes `side` of the hunk the cursor is in. */
const takeAtCursor = (side: Take) => (view: EditorView) => {
  const hunk = hunkAt(view.state.field(hunksField), view.state.selection.main.head);
  return hunk !== null && take(view, hunk.id, side);
};

const button = (text: string, label: string, onClick: () => void) => {
  const element = document.createElement("button");
  element.type = "button";
  element.textContent = text;
  element.setAttribute("aria-label", label);
  element.addEventListener("mousedown", (event) => event.preventDefault());
  element.addEventListener("click", onClick);
  return element;
};

/** A marker line shown as the header of a side, with the hunk's buttons on the first. */
class SideHeader extends WidgetType {
  constructor(
    readonly side: "ours" | "theirs" | "end",
    readonly label: string,
    readonly hunk: BufferHunk,
    readonly number: number,
  ) {
    super();
  }
  override eq(other: SideHeader) {
    return (
      other.side === this.side &&
      other.label === this.label &&
      other.hunk.id === this.hunk.id &&
      other.hunk.showBase === this.hunk.showBase &&
      other.number === this.number
    );
  }
  override toDOM(view: EditorView) {
    const element = document.createElement("span");
    element.className = `cm-conflict-head cm-conflict-head-${this.side}`;
    const text = document.createElement("span");
    // Labels are repo text: set as text (SEC-3).
    text.textContent =
      this.side === "end"
        ? `End of conflict ${this.number}`
        : `${this.side === "ours" ? "Ours" : "Theirs"}: ${this.label}`;
    element.append(text);
    if (this.side === "ours") {
      const id = this.hunk.id;
      const actions = document.createElement("span");
      actions.className = "cm-conflict-actions";
      const which = `conflict ${this.number}`;
      actions.append(
        button("Take ours", `Take ours in ${which}`, () => take(view, id, "ours")),
        button("Take theirs", `Take theirs in ${which}`, () => take(view, id, "theirs")),
        button("Take both", `Take both in ${which}`, () => take(view, id, "both")),
        button(this.hunk.showBase ? "Hide base" : "Show base", `${this.hunk.showBase ? "Hide" : "Show"} base of ${which}`, () =>
          view.dispatch({ effects: toggleBase.of(id) }),
        ),
      );
      element.append(actions);
    }
    return element;
  }
  override ignoreEvent() {
    return true;
  }
}

/** A hunk's base, read-only, between ours and theirs. */
class BaseBlock extends WidgetType {
  constructor(readonly base: string) {
    super();
  }
  override eq(other: BaseBlock) {
    return other.base === this.base;
  }
  override toDOM() {
    const element = document.createElement("div");
    element.className = "cm-conflict-base";
    const head = document.createElement("div");
    head.className = "cm-conflict-head";
    head.textContent = "Base";
    const text = document.createElement("pre");
    text.textContent = this.base === "" ? "(nothing)" : this.base.replace(/\n$/, "");
    element.append(head, text);
    return element;
  }
  override ignoreEvent() {
    return true;
  }
}

interface HunkDecorations {
  all: DecorationSet;
  /** The marker lines shown as headers: the cursor steps over them whole. */
  atomic: DecorationSet;
}

const line = (name: string) => Decoration.line({ class: `cm-conflict-line ${name}` });

/** Line decorations for every line of `[from, to)`. */
function lines(state: EditorState, from: number, to: number, name: string, into: Range<Decoration>[]) {
  if (to <= from) {
    return;
  }
  const last = state.doc.lineAt(to - 1).number;
  for (let number = state.doc.lineAt(from).number; number <= last; number++) {
    into.push(line(name).range(state.doc.line(number).from));
  }
}

const cache = new WeakMap<EditorState, HunkDecorations>();

function decorations(state: EditorState): HunkDecorations {
  const known = cache.get(state);
  if (known !== undefined) {
    return known;
  }
  const { labels } = state.facet(conflictActions);
  const all: Range<Decoration>[] = [];
  const atomic: Range<Decoration>[] = [];
  state.field(hunksField).forEach((hunk, index) => {
    const number = index + 1;
    if (!isUntouched(state, hunk)) {
      lines(state, hunk.from, hunk.to, "cm-conflict-changed", all);
      return;
    }
    const { head, ours, separator, theirs } = hunk.layout;
    const oursFrom = hunk.from + head;
    const separatorFrom = oursFrom + ours;
    const theirsFrom = separatorFrom + separator;
    const endFrom = theirsFrom + theirs;
    const header = (from: number, length: number, widget: SideHeader) => {
      const replace = Decoration.replace({ widget }).range(from, from + length - 1);
      all.push(line("cm-conflict-marker").range(from), replace);
      atomic.push(replace);
    };
    header(hunk.from, head, new SideHeader("ours", labels.ours, hunk, number));
    lines(state, oursFrom, separatorFrom, "cm-conflict-ours", all);
    if (hunk.showBase) {
      all.push(Decoration.widget({ widget: new BaseBlock(hunk.base), block: true, side: -1 }).range(separatorFrom));
    }
    header(separatorFrom, separator, new SideHeader("theirs", labels.theirs, hunk, number));
    lines(state, theirsFrom, endFrom, "cm-conflict-theirs", all);
    header(endFrom, hunk.to - endFrom, new SideHeader("end", labels.theirs, hunk, number));
  });
  const result = { all: Decoration.set(all, true), atomic: Decoration.set(atomic, true) };
  cache.set(state, result);
  return result;
}

const theme = EditorView.theme({
  ".cm-content": { caretColor: "var(--code-text)" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--code-text)" },
  ".cm-conflict-line": { backgroundColor: "var(--diff-conflict-bg)" },
  ".cm-conflict-marker": { backgroundColor: "var(--diff-conflict-word-bg)" },
  ".cm-conflict-head": { fontFamily: "var(--font-ui)", fontWeight: "600" },
  ".cm-conflict-head-end": { fontWeight: "normal" },
  ".cm-conflict-actions": { display: "inline-flex", flexWrap: "wrap", gap: "4px", marginLeft: "12px", fontWeight: "normal" },
  ".cm-conflict-actions button": { padding: "0 6px", fontSize: "0.9em" },
  ".cm-conflict-base": { backgroundColor: "var(--diff-conflict-bg)", padding: "0 0 0 4px", borderLeft: "3px solid var(--diff-conflict-word-bg)" },
  ".cm-conflict-base pre": { margin: "0", fontFamily: "var(--font-code)" },
});

/** The conflict editor's own extensions; `hunks` seeds a new buffer, `null` keeps a restored one's. */
export function conflictExtensions(actions: ConflictActions, hunks: BufferHunk[] | null): Extension[] {
  return [
    hunks === null ? hunksField : hunksField.init(() => hunks),
    conflictActions.of(actions),
    history(),
    invertedEffects.of((transaction) => {
      const removed = transaction.effects.filter((effect) => effect.is(removeHunk)).map((effect) => effect.value);
      return transaction.startState
        .field(hunksField)
        .filter((hunk) => removed.includes(hunk.id))
        .map((hunk) => restoreHunk.of(hunk));
    }),
    EditorView.decorations.compute(["doc", hunksField, conflictActions], (state) => decorations(state).all),
    EditorView.atomicRanges.of((view) => decorations(view.state).atomic),
    Prec.high(
      keymap.of([
        { key: "Alt-ArrowDown", run: (view) => goToHunk(view, 1) },
        { key: "Alt-ArrowUp", run: (view) => goToHunk(view, -1) },
        { key: "Alt-1", run: takeAtCursor("ours") },
        { key: "Alt-2", run: takeAtCursor("theirs") },
        { key: "Alt-3", run: takeAtCursor("both") },
        {
          key: "Mod-Enter",
          run: (view) => {
            view.state.facet(conflictActions).markResolved();
            return true;
          },
        },
      ]),
    ),
    keymap.of([...defaultKeymap, ...historyKeymap]),
    theme,
  ];
}

/** The hunks still in the buffer. */
export const hunkCount = (state: EditorState) => state.field(hunksField, false)?.length ?? 0;
