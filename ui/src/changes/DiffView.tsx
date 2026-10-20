// A text diff in CodeMirror (UI-17): inline, one document of the hunks with
// both line numbers, or side by side, both whole files with unchanged runs
// folded to the chosen context. The changed lines and the hunks are the
// core's. Each hunk can carry stage, unstage and discard controls (UI-15),
// which send its index; the core builds the patch. They are buttons, in
// the Tab order while shown; F7 and Shift+F7 scroll to the next or
// previous hunk and give its first control the focus, or its header when
// it has none (UI-22).
import { Change, MergeView } from "@codemirror/merge";
import { EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, gutter, lineNumbers, WidgetType } from "@codemirror/view";
import { createEffect, on, onCleanup, Show, untrack } from "solid-js";
import type { ChangeAction, Context, DiffMode, Hunk, TextDiff } from "../ipc";
import type { DiffHunk } from "../stores/changes";
import { applyLanguage, baseExtensions, type EditorParts, newParts, wrapping } from "./codemirror";
import { changeRanges, type InlineDoc, inlineDoc } from "./diffDoc";
import { matches, modalOpen } from "../keys";
import { strings } from "../strings/en";

/** Unchanged lines kept around each change in side-by-side mode; `null` folds nothing. */
const MARGIN: Record<Context, number | null> = { three: 3, ten: 10, whole_file: null };

class NumberMarker extends GutterMarker {
  constructor(readonly text: string) {
    super();
  }
  override eq(other: NumberMarker) {
    return other.text === this.text;
  }
  override toDOM() {
    return document.createTextNode(this.text);
  }
}

/** The controls each hunk gets, and what a click sends. */
export interface HunkControls {
  actions: { action: ChangeAction; label: string }[];
  onAction: (action: ChangeAction, hunk: number) => void;
}

/** A hunk's header line, with its controls when it has any. */
class HunkHeader extends WidgetType {
  constructor(
    readonly header: string,
    readonly index: number,
    readonly controls: HunkControls | null,
  ) {
    super();
  }
  override eq(other: HunkHeader) {
    return other.header === this.header && other.index === this.index && other.controls === this.controls;
  }
  override toDOM() {
    const element = document.createElement("div");
    element.className = "cm-diff-hunk";
    element.dataset.hunk = String(this.index);
    // F7 focuses a header without controls, which Tab passes over.
    element.tabIndex = -1;
    const text = document.createElement("span");
    // Set as text: the header is the core's numbers, but nothing here is markup (SEC-3).
    text.textContent = this.header;
    element.append(text);
    const controls = this.controls;
    if (controls !== null) {
      const group = document.createElement("span");
      group.className = "cm-diff-hunk-actions";
      for (const { action, label } of controls.actions) {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = label;
        button.setAttribute("aria-label", `${label} ${this.index + 1}`);
        button.addEventListener("click", () => controls.onAction(action, this.index));
        group.append(button);
      }
      element.append(group);
    }
    return element;
  }
  override ignoreEvent() {
    return true;
  }
}

const hunkHeader = (hunk: Hunk) => `@@ -${hunk.old_start},${hunk.old_lines} +${hunk.new_start},${hunk.new_lines} @@`;

/** The new side's line, from 1, where a hunk's first change is. */
function firstChangedLine(hunk: Hunk): number {
  let last = hunk.new_lines > 0 ? hunk.new_start - 1 : hunk.new_start;
  for (const line of hunk.lines) {
    if (line.kind !== "context") {
      return line.new_line ?? last + 1;
    }
    last = line.new_line ?? last;
  }
  return last + 1;
}

/** Where each hunk's header is in the new file, side by side: above its first change. */
const sidePositions = (diff: TextDiff, state: EditorState) =>
  diff.hunks.map((hunk) => state.doc.line(Math.min(Math.max(firstChangedLine(hunk), 1), state.doc.lines)).from);

/** Hunk headers with controls above each hunk's first change in the new file, side by side. */
function sideHeaders(diff: TextDiff, positions: number[], controls: HunkControls | null): Extension {
  const decorations = diff.hunks.map((hunk, index) =>
    Decoration.widget({
      widget: new HunkHeader(hunkHeader(hunk), index, controls),
      block: true,
      side: -1,
    }).range(positions[index] ?? 0),
  );
  return EditorView.decorations.of(Decoration.set(decorations, true));
}

/** A gutter of one side's line numbers. */
function numbers(name: string, values: (number | null)[]): Extension {
  const widest = String(Math.max(0, ...values.map((value) => value ?? 0)));
  return gutter({
    class: `cm-diff-number cm-diff-${name}`,
    lineMarker: (view, line) => {
      const value = values[view.state.doc.lineAt(line.from).number - 1];
      return value == null ? null : new NumberMarker(String(value));
    },
    initialSpacer: () => new NumberMarker(widest),
  });
}

function inlineExtensions(doc: InlineDoc, state: EditorState, controls: HunkControls | null): Extension[] {
  const decorations: Range<Decoration>[] = [];
  doc.hunks.forEach((hunk, index) => {
    const from = state.doc.line(hunk.line + 1).from;
    decorations.push(
      Decoration.widget({ widget: new HunkHeader(hunk.header, index, controls), block: true, side: -1 }).range(from),
    );
  });
  doc.kinds.forEach((kind, index) => {
    if (kind !== "context") {
      const from = state.doc.line(index + 1).from;
      decorations.push(Decoration.line({ class: `cm-diff-${kind}` }).range(from));
    }
  });
  return [
    numbers("old", doc.oldNumbers),
    numbers("new", doc.newNumbers),
    EditorView.decorations.of(Decoration.set(decorations, true)),
  ];
}

export default function DiffView(props: {
  diff: TextDiff;
  /** The file's path, for its syntax. */
  path: string;
  mode: DiffMode;
  wrap: boolean;
  context: Context;
  /** The hunks' controls; `null` for none, as for a commit's diff. */
  controls: HunkControls | null;
  /** Where the diff was scrolled when it was last shown (UI-29). */
  scrollTop: number;
  onScroll: (scrollTop: number) => void;
  /** The hunk F7 goes on from, kept while the diff is shown again (UI-22). */
  hunk: DiffHunk;
  onHunk: (hunk: DiffHunk) => void;
}) {
  let host: HTMLDivElement | undefined;
  let parts: EditorParts = newParts();
  let views: EditorView[] = [];
  let merge: MergeView | null = null;
  /** The editor that shows the hunk headers, and where each header is in it. */
  let headers: { view: EditorView; positions: number[] } | null = null;
  /** The hunk F7 last went to, or whose control has the focus; -1 for none yet. */
  const current = () => props.hunk.index;
  const setCurrent = (index: number, focused: boolean) => props.onHunk({ index, focused });

  const destroy = () => {
    merge?.destroy();
    for (const view of views) {
      view.destroy();
    }
    merge = null;
    views = [];
    headers = null;
  };

  // Side by side needs both whole files; until the core sent them, the
  // diff at hand stays inline.
  const sideBySide = () => props.mode === "side_by_side" && props.diff.old_text !== null && props.diff.new_text !== null;

  const build = () => {
    // A hunk's control that had the focus gets it back in the new diff, as after Stage hunk by keyboard, or in
    // this diff shown again, as after a refresh (UI-22).
    const kept = untrack(() => props.hunk);
    const focusHere = host?.contains(document.activeElement) === true;
    const focused = focusHere || (kept.focused && document.activeElement === document.body) ? kept.index : -1;
    destroy();
    if (!host) {
      return;
    }
    parts = newParts();
    const diff = props.diff;
    const base = () => baseExtensions(parts, props.wrap);
    /** The editor's name, as its content is a text box to assistive technology. */
    const named = (label: string) => EditorView.contentAttributes.of({ "aria-label": label });
    if (sideBySide()) {
      const oldText = diff.old_text ?? "";
      const newText = diff.new_text ?? "";
      const changes = changeRanges(diff, oldText, newText).map(
        (range) => new Change(range.fromA, range.toA, range.fromB, range.toB),
      );
      const margin = MARGIN[props.context];
      const positions = sidePositions(diff, EditorState.create({ doc: newText }));
      merge = new MergeView({
        parent: host,
        a: { doc: oldText, extensions: [base(), named(strings.changes.diffBefore(props.path)), lineNumbers()] },
        b: {
          doc: newText,
          extensions: [
            base(),
            named(strings.changes.diffAfter(props.path)),
            lineNumbers(),
            sideHeaders(diff, positions, props.controls),
          ],
        },
        highlightChanges: false,
        gutter: false,
        collapseUnchanged: margin === null ? undefined : { margin, minSize: 4 },
        diffConfig: { override: () => changes },
      });
      views = [merge.a, merge.b];
      headers = { view: merge.b, positions };
    } else {
      const doc = inlineDoc(diff);
      const start = EditorState.create({ doc: doc.text });
      const state = EditorState.create({
        doc: doc.text,
        extensions: [base(), named(strings.changes.diffOf(props.path)), inlineExtensions(doc, start, props.controls)],
      });
      const view = new EditorView({ state, parent: host });
      views = [view];
      headers = { view, positions: doc.hunks.map((hunk) => state.doc.line(hunk.line + 1).from) };
    }
    void applyLanguage(parts, props.path, views);
    follow(merge?.dom ?? views[0]?.scrollDOM);
    const count = headers?.positions.length ?? 0;
    if (focused >= 0 && count > 0) {
      goTo(Math.min(focused, count - 1));
    } else if (kept.index >= count) {
      setCurrent(-1, false);
    }
  };

  /** Scrolls `scroller` to where the diff was, and keeps where it goes. */
  const follow = (scroller: HTMLElement | undefined) => {
    if (scroller === undefined) {
      return;
    }
    const top = props.scrollTop;
    scroller.scrollTop = top;
    // The editor measures its lines in the next frame; until then the height may fall short.
    if (scroller.scrollTop !== top) {
      requestAnimationFrame(() => (scroller.scrollTop = top));
    }
    scroller.addEventListener("scroll", () => props.onScroll(scroller.scrollTop));
  };

  /** Scrolls to hunk `index` and focuses its first control, or its header, once the editor has drawn it. */
  const goTo = (index: number) => {
    const shown = headers;
    const position = shown?.positions[index];
    if (shown === null || position === undefined) {
      return;
    }
    setCurrent(index, true);
    shown.view.dispatch({ effects: EditorView.scrollIntoView(position, { y: "center" }) });
    // The editor draws what scrolls into view in a later frame.
    let frames = 0;
    const focus = () => {
      const header = host?.querySelector<HTMLElement>(`.cm-diff-hunk[data-hunk="${index}"]`);
      if (header) {
        (header.querySelector<HTMLElement>("button:not(:disabled)") ?? header).focus();
      } else if (frames++ < 10) {
        requestAnimationFrame(focus);
      }
    };
    focus();
  };

  // F7 and Shift+F7, wherever the focus is, while this diff is shown and no dialog is open (UI-22).
  const onWindowKey = (event: KeyboardEvent) => {
    const step = matches("nextHunk", event) ? 1 : matches("previousHunk", event) ? -1 : 0;
    const count = headers?.positions.length ?? 0;
    if (step === 0 || count === 0 || host === undefined || host.getClientRects().length === 0 || modalOpen()) {
      return;
    }
    event.preventDefault();
    // From the hunk whose control has the focus, else from the one F7 last went to.
    const focused = host.contains(document.activeElement)
      ? document.activeElement?.closest<HTMLElement>(".cm-diff-hunk")?.dataset.hunk
      : undefined;
    const at = focused === undefined ? current() : Number(focused);
    goTo(at < 0 ? (step === 1 ? 0 : count - 1) : (at + step + count) % count);
  };
  window.addEventListener("keydown", onWindowKey);
  onCleanup(() => window.removeEventListener("keydown", onWindowKey));

  /** A hunk's control that takes the focus, by Tab or a click, makes it the hunk F7 goes on from. */
  const onFocusIn = (event: FocusEvent) => {
    const hunk = (event.target as HTMLElement).closest<HTMLElement>(".cm-diff-hunk")?.dataset.hunk;
    if (hunk !== undefined) {
      setCurrent(Number(hunk), true);
    }
  };
  /** The focus going elsewhere; a control taken away with the diff, which has nowhere to go, keeps it. */
  const onFocusOut = (event: FocusEvent) => {
    if (event.relatedTarget instanceof Node && !host?.contains(event.relatedTarget)) {
      setCurrent(current(), false);
    }
  };

  createEffect(on(() => [props.diff, sideBySide(), props.context, props.path, props.controls] as const, build));
  createEffect(
    on(
      () => props.wrap,
      (wrap) => {
        for (const view of views) {
          view.dispatch({ effects: wrapping(parts, wrap) });
        }
      },
      { defer: true },
    ),
  );
  onCleanup(destroy);

  return (
    <div class="diff-text">
      <Show when={props.diff.hunks.length === 0}>
        <p class="placeholder">{strings.changes.noLines}</p>
      </Show>
      <div
        class="diff-editor"
        hidden={props.diff.hunks.length === 0}
        data-mode={sideBySide() ? "side_by_side" : "inline"}
        ref={(element) => (host = element)}
        onFocusIn={onFocusIn}
        onFocusOut={onFocusOut}
      />
      <Show when={props.diff.old_missing_newline || props.diff.new_missing_newline}>
        <p class="diff-note">
          {strings.changes.noNewline(props.diff.old_missing_newline, props.diff.new_missing_newline)}
        </p>
      </Show>
    </div>
  );
}
