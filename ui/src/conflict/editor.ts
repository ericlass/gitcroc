// The conflict editor's CodeMirror parts (OPS-3): the buffer's hunks as a
// state field mapped through every edit, their highlight in the code
// theme's conflict colors (UI-24, UI-25), a header per side naming it with
// its label (OPS-4), Take ours, Take theirs and Take both on each hunk as
// built, its base on request, undo that brings a taken hunk back, and the
// keys to move between hunks and resolve them. Under each hunk an AI
// proposal covers, the proposal with Accept, Reject and Edit (AI-11);
// Accept replaces the hunk in the buffer as a take does and nowhere else
// (AI-12), and only while the hunk is untouched. Text goes into the DOM as
// text only (SEC-3).
import { defaultKeymap, history, historyKeymap, invertedEffects } from "@codemirror/commands";
import { type EditorState, type Extension, Facet, Prec, type Range, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, keymap, WidgetType } from "@codemirror/view";
import type { ConflictLabels, ProposalConfidence, ProposedHunk } from "../ipc";
import type { HunkView } from "../stores/conflicts";
import { type BufferHunk, hunkAt, hunkTarget, nextHunk, normalize, type Take, takenText, untouched } from "./buffer";
import { strings } from "../strings/en";

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
  /** Reject of proposed hunk `id` (AI-11), counted from 1. */
  reject: (id: number) => void;
  /** Opens proposed hunk `id`'s inline editor with `text`, keeps what is typed, or closes it with `null`. */
  edit: (id: number, text: string | null) => void;
}

export const conflictActions = Facet.define<ConflictActions, ConflictActions>({
  combine: (values) =>
    values[0] ?? {
      labels: { ours: "ours", theirs: "theirs" },
      markResolved: () => {},
      reject: () => {},
      edit: () => {},
    },
});

/** A file's proposal as the editor shows it (AI-11): the proposed hunks and what the user did with each. */
export interface ProposalView {
  hunks: ProposedHunk[];
  views: Record<number, HunkView | undefined>;
}

const setProposal = StateEffect.define<ProposalView | null>();

/** The proposal shown, or `null`: none, or AI features are off (AI-16). */
export const proposalField = StateField.define<ProposalView | null>({
  create: () => null,
  update(proposal, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setProposal)) {
        proposal = effect.value;
      }
    }
    return proposal;
  },
});

/** Shows `proposal`; `null` shows none. */
export const showProposal = (view: EditorView, proposal: ProposalView | null) =>
  view.dispatch({ effects: setProposal.of(proposal) });

/** The proposal for buffer hunk `hunk` while shown: proposed hunks count from 1 over the file's hunks (AI-5). */
function proposalFor(state: EditorState, hunk: BufferHunk): { proposed: ProposedHunk; view: HunkView | null } | null {
  const proposal = state.field(proposalField, false);
  const proposed = proposal?.hunks.find((candidate) => candidate.id === hunk.id + 1);
  const view = proposed === undefined ? undefined : proposal?.views[proposed.id];
  return proposed === undefined || view?.rejected ? null : { proposed, view: view ?? null };
}

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

/** Replaces untouched hunks with their proposed `text` in one change; undo brings them back. */
function acceptHunks(view: EditorView, accepted: { hunk: BufferHunk; text: string }[]): boolean {
  const untouchedOnes = accepted.filter(({ hunk }) => isUntouched(view.state, hunk));
  if (untouchedOnes.length === 0) {
    return false;
  }
  const first = untouchedOnes[0]?.hunk.from ?? 0;
  view.dispatch({
    changes: untouchedOnes.map(({ hunk, text }) => ({ from: hunk.from, to: hunk.to, insert: normalize(text) })),
    effects: untouchedOnes.map(({ hunk }) => removeHunk.of(hunk.id)),
    selection: { anchor: first },
    userEvent: "input.accept",
    scrollIntoView: true,
  });
  view.focus();
  return true;
}

/**
 * Accept (AI-12): replaces buffer hunk `id`, markers included, with its
 * proposed text, or with `text` from the inline editor. Refused once the
 * hunk was changed.
 */
export function accept(view: EditorView, id: number, text?: string): boolean {
  const hunk = view.state.field(hunksField).find((candidate) => candidate.id === id);
  const shown = hunk === undefined ? null : proposalFor(view.state, hunk);
  const proposed = text ?? shown?.proposed.resolution ?? null;
  if (hunk === undefined || shown === null || shown.proposed.status !== "resolved" || proposed === null) {
    return false;
  }
  return acceptHunks(view, [{ hunk, text: proposed }]);
}

/** The untouched hunks proposed with high confidence, with their proposed text. */
export function highConfidence(state: EditorState): { hunk: BufferHunk; text: string }[] {
  return state.field(hunksField, false)?.flatMap((hunk) => {
    const shown = proposalFor(state, hunk);
    const { proposed } = shown ?? {};
    return proposed?.status === "resolved" &&
      proposed.confidence === "high" &&
      proposed.resolution !== null &&
      isUntouched(state, hunk)
      ? [{ hunk, text: proposed.resolution }]
      : [];
  }) ?? [];
}

/** Accept all high-confidence (AI-11): this file's only. */
export const acceptAllHigh = (view: EditorView) => acceptHunks(view, highConfidence(view.state));

/** The hunks still as built, counted from 1 as the core counts them (AI-5): what Resolve with AI sends. */
export const untouchedHunks = (state: EditorState): number[] =>
  state.field(hunksField, false)?.flatMap((hunk) => (isUntouched(state, hunk) ? [hunk.id + 1] : [])) ?? [];

/** Accepts, rejects or edits the proposal of the hunk the cursor is in. */
type ProposalAct = (view: EditorView, hunk: BufferHunk, proposed: ProposedHunk) => boolean;

const atCursor = (act: ProposalAct) => (view: EditorView) => {
  const hunk = hunkAt(view.state.field(hunksField), view.state.selection.main.head);
  const shown = hunk === null ? null : proposalFor(view.state, hunk);
  return hunk !== null && shown !== null && act(view, hunk, shown.proposed);
};

/** The inline editor to focus once drawn: the one just opened. */
let focusEditor: number | null = null;

const editProposal: ProposalAct = (view, hunk, proposed) => {
  if (proposed.resolution === null || !isUntouched(view.state, hunk)) {
    return false;
  }
  focusEditor = proposed.id;
  view.state.facet(conflictActions).edit(proposed.id, proposed.resolution);
  return true;
};

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

const button = (text: string, label: string, onClick: () => void, disabled = false, title = "") => {
  const element = document.createElement("button");
  element.type = "button";
  element.textContent = text;
  element.disabled = disabled;
  element.title = title;
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
        ? strings.conflict.endOf(this.number)
        : this.side === "ours"
          ? strings.conflict.ours(this.label)
          : strings.conflict.theirs(this.label);
    element.append(text);
    if (this.side === "ours") {
      const id = this.hunk.id;
      const actions = document.createElement("span");
      actions.className = "cm-conflict-actions";
      const which = strings.conflict.which(this.number);
      actions.append(
        button(strings.conflict.takeOurs, strings.conflict.takeOursIn(which), () => take(view, id, "ours")),
        button(strings.conflict.takeTheirs, strings.conflict.takeTheirsIn(which), () => take(view, id, "theirs")),
        button(strings.conflict.takeBoth, strings.conflict.takeBothIn(which), () => take(view, id, "both")),
        button(
          this.hunk.showBase ? strings.conflict.hideBase : strings.conflict.showBase,
          this.hunk.showBase ? strings.conflict.hideBaseOf(which) : strings.conflict.showBaseOf(which),
          () => view.dispatch({ effects: toggleBase.of(id) }),
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
    head.textContent = strings.conflict.base;
    const text = document.createElement("pre");
    text.textContent = this.base === "" ? strings.conflict.nothing : this.base.replace(/\n$/, "");
    element.append(head, text);
    return element;
  }
  override ignoreEvent() {
    return true;
  }
}

/** The confidence as a word and a marker whose shape tells the level without color (UI-25). */
const CONFIDENCE: Record<ProposalConfidence, { word: string; marker: string }> = {
  high: { word: strings.conflict.confidence.high, marker: "●●●" },
  medium: { word: strings.conflict.confidence.medium, marker: "●●○" },
  low: { word: strings.conflict.confidence.low, marker: "●○○" },
};

const textOf = (tag: string, className: string, text: string) => {
  const element = document.createElement(tag);
  element.className = className;
  element.textContent = text;
  return element;
};

const sameProposal = (a: ProposedHunk, b: ProposedHunk) =>
  a.id === b.id &&
  a.status === b.status &&
  a.resolution === b.resolution &&
  a.explanation === b.explanation &&
  a.confidence === b.confidence;

/** A hunk's proposal, under it (AI-11): read-only text, the explanation, the confidence, and its buttons. */
class ProposalBlock extends WidgetType {
  constructor(
    readonly proposed: ProposedHunk,
    readonly hunk: number,
    readonly number: number,
    readonly editing: string | null,
    readonly changed: boolean,
  ) {
    super();
  }
  override eq(other: ProposalBlock) {
    // What is typed in the inline editor is the textarea's own; drawing it again would lose the cursor.
    return (
      sameProposal(other.proposed, this.proposed) &&
      other.hunk === this.hunk &&
      other.number === this.number &&
      (other.editing === null) === (this.editing === null) &&
      other.changed === this.changed
    );
  }
  override toDOM(view: EditorView) {
    const { proposed, hunk, changed } = this;
    const which = strings.conflict.which(this.number);
    const actions = view.state.facet(conflictActions);
    const element = document.createElement("div");
    element.className = "cm-proposal";
    element.setAttribute("role", "group");
    element.setAttribute("aria-label", strings.conflict.proposalFor(which));

    const head = document.createElement("div");
    head.className = "cm-proposal-head";
    const { word, marker } = CONFIDENCE[proposed.confidence];
    const confidence = textOf("span", `cm-proposal-confidence cm-proposal-confidence-${proposed.confidence}`, "");
    const shape = textOf("span", "cm-proposal-marker", marker);
    shape.setAttribute("aria-hidden", "true");
    confidence.append(shape, ` ${word}`);
    head.append(textOf("span", "", strings.conflict.aiProposal), confidence);
    if (proposed.status === "unresolved") {
      head.append(textOf("span", "cm-proposal-note", strings.conflict.notResolved));
    }
    if (changed) {
      head.append(textOf("span", "cm-proposal-note", strings.conflict.hunkChanged));
    }
    element.append(head);

    const buttons = document.createElement("div");
    buttons.className = "cm-conflict-actions cm-proposal-actions";
    const reject = button(
      strings.conflict.reject,
      strings.conflict.rejectFor(which),
      () => actions.reject(proposed.id),
      false,
      strings.conflict.keys.reject,
    );
    if (proposed.resolution !== null && this.editing !== null) {
      const box = document.createElement("textarea");
      box.className = "cm-proposal-editor";
      box.setAttribute("aria-label", strings.conflict.editedProposalFor(which));
      box.value = this.editing;
      box.rows = Math.min(12, Math.max(2, this.editing.split("\n").length));
      box.spellcheck = false;
      const acceptEdited = () => accept(view, hunk, box.value);
      const cancel = () => {
        actions.edit(proposed.id, null);
        view.focus();
      };
      box.addEventListener("input", () => actions.edit(proposed.id, box.value));
      box.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
          event.preventDefault();
          acceptEdited();
        } else if (event.key === "Escape") {
          event.preventDefault();
          cancel();
        }
        event.stopPropagation();
      });
      buttons.append(
        button(
          strings.conflict.accept,
          strings.conflict.acceptEditedFor(which),
          acceptEdited,
          changed,
          strings.conflict.keys.acceptEdited,
        ),
        button(
          strings.common.cancel,
          strings.conflict.cancelEditFor(which),
          cancel,
          false,
          strings.conflict.keys.cancelEdit,
        ),
      );
      element.append(box);
      if (focusEditor === proposed.id) {
        focusEditor = null;
        queueMicrotask(() => box.focus());
      }
    } else if (proposed.resolution !== null) {
      element.append(textOf("pre", "cm-proposal-text", proposed.resolution.replace(/\n$/, "")));
      buttons.append(
        button(
          strings.conflict.accept,
          strings.conflict.acceptFor(which),
          () => accept(view, hunk),
          changed,
          strings.conflict.keys.accept,
        ),
        reject,
        button(
          strings.conflict.edit,
          strings.conflict.editFor(which),
          () => {
            focusEditor = proposed.id;
            actions.edit(proposed.id, proposed.resolution);
          },
          changed,
          strings.conflict.keys.edit,
        ),
      );
    } else {
      buttons.append(reject);
    }
    // The model's words, as text (SEC-3).
    element.append(textOf("p", "cm-proposal-explanation", proposed.explanation), buttons);
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
    const shown = proposalFor(state, hunk);
    if (shown !== null) {
      // Under the hunk's last line, its end marker while untouched.
      const end = state.doc.lineAt(Math.max(hunk.from, hunk.to - 1)).to;
      const changed = !isUntouched(state, hunk);
      const widget = new ProposalBlock(shown.proposed, hunk.id, number, shown.view?.editing ?? null, changed);
      all.push(Decoration.widget({ widget, block: true, side: 1 }).range(end));
    }
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
  // Proposal colors are the code theme's added colors, with its text (UI-25).
  ".cm-proposal": {
    backgroundColor: "var(--diff-added-bg)",
    color: "var(--code-text)",
    borderLeft: "3px solid var(--diff-added-word-bg)",
    padding: "4px 8px",
    fontFamily: "var(--font-ui)",
  },
  ".cm-proposal-head": { display: "flex", flexWrap: "wrap", gap: "8px", fontWeight: "600" },
  ".cm-proposal-confidence": { padding: "0 6px", borderRadius: "3px", fontWeight: "normal" },
  ".cm-proposal-confidence-high": { backgroundColor: "var(--diff-added-word-bg)" },
  ".cm-proposal-confidence-medium": { backgroundColor: "var(--diff-changed-word-bg)" },
  ".cm-proposal-confidence-low": { backgroundColor: "var(--diff-removed-word-bg)" },
  ".cm-proposal-note": { fontWeight: "normal", fontStyle: "italic" },
  ".cm-proposal-text": { margin: "4px 0", fontFamily: "var(--font-code)", whiteSpace: "pre-wrap" },
  ".cm-proposal-editor": {
    display: "block",
    width: "100%",
    boxSizing: "border-box",
    margin: "4px 0",
    fontFamily: "var(--font-code)",
    fontSize: "inherit",
    backgroundColor: "var(--code-bg)",
    color: "var(--code-text)",
  },
  ".cm-proposal-explanation": { margin: "4px 0", whiteSpace: "pre-wrap" },
  ".cm-proposal-actions": { marginLeft: "0" },
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
    proposalField,
    EditorView.decorations.compute(
      ["doc", hunksField, proposalField, conflictActions],
      (state) => decorations(state).all,
    ),
    EditorView.atomicRanges.of((view) => decorations(view.state).atomic),
    Prec.high(
      keymap.of([
        { key: "Alt-ArrowDown", run: (view) => goToHunk(view, 1) },
        { key: "Alt-ArrowUp", run: (view) => goToHunk(view, -1) },
        { key: "Alt-1", run: takeAtCursor("ours") },
        { key: "Alt-2", run: takeAtCursor("theirs") },
        { key: "Alt-3", run: takeAtCursor("both") },
        { key: "Alt-4", run: atCursor((view, hunk) => accept(view, hunk.id)) },
        {
          key: "Alt-5",
          run: atCursor((view, _hunk, proposed) => {
            view.state.facet(conflictActions).reject(proposed.id);
            return true;
          }),
        },
        { key: "Alt-6", run: atCursor(editProposal) },
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
