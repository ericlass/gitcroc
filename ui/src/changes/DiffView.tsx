// A text diff in CodeMirror (UI-17): inline, one document of the hunks with
// both line numbers, or side by side, both whole files with unchanged runs
// folded to the chosen context. The changed lines and the hunks are the
// core's. Each hunk can carry stage, unstage and discard controls (UI-15),
// which send its index; the core builds the patch.
import { Change, MergeView } from "@codemirror/merge";
import { EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, gutter, lineNumbers, WidgetType } from "@codemirror/view";
import { createEffect, on, onCleanup, Show } from "solid-js";
import type { ChangeAction, Context, DiffMode, Hunk, TextDiff } from "../ipc";
import { applyLanguage, baseExtensions, type EditorParts, newParts, wrapping } from "./codemirror";
import { changeRanges, type InlineDoc, inlineDoc } from "./diffDoc";

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

/** Hunk headers with controls above each hunk's first change in the new file, side by side. */
function sideHeaders(diff: TextDiff, state: EditorState, controls: HunkControls | null): Extension {
  const decorations = diff.hunks.map((hunk, index) => {
    const line = Math.min(Math.max(firstChangedLine(hunk), 1), state.doc.lines);
    return Decoration.widget({
      widget: new HunkHeader(hunkHeader(hunk), index, controls),
      block: true,
      side: -1,
    }).range(state.doc.line(line).from);
  });
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
}) {
  let host: HTMLDivElement | undefined;
  let parts: EditorParts = newParts();
  let views: EditorView[] = [];
  let merge: MergeView | null = null;

  const destroy = () => {
    merge?.destroy();
    for (const view of views) {
      view.destroy();
    }
    merge = null;
    views = [];
  };

  // Side by side needs both whole files; until the core sent them, the
  // diff at hand stays inline.
  const sideBySide = () => props.mode === "side_by_side" && props.diff.old_text !== null && props.diff.new_text !== null;

  const build = () => {
    destroy();
    if (!host) {
      return;
    }
    parts = newParts();
    const diff = props.diff;
    const base = () => baseExtensions(parts, props.wrap);
    if (sideBySide()) {
      const oldText = diff.old_text ?? "";
      const newText = diff.new_text ?? "";
      const changes = changeRanges(diff, oldText, newText).map(
        (range) => new Change(range.fromA, range.toA, range.fromB, range.toB),
      );
      const margin = MARGIN[props.context];
      const headers = sideHeaders(diff, EditorState.create({ doc: newText }), props.controls);
      merge = new MergeView({
        parent: host,
        a: { doc: oldText, extensions: [base(), lineNumbers()] },
        b: { doc: newText, extensions: [base(), lineNumbers(), headers] },
        highlightChanges: false,
        gutter: false,
        collapseUnchanged: margin === null ? undefined : { margin, minSize: 4 },
        diffConfig: { override: () => changes },
      });
      views = [merge.a, merge.b];
    } else {
      const doc = inlineDoc(diff);
      const start = EditorState.create({ doc: doc.text });
      const state = EditorState.create({
        doc: doc.text,
        extensions: [base(), inlineExtensions(doc, start, props.controls)],
      });
      views = [new EditorView({ state, parent: host })];
    }
    void applyLanguage(parts, props.path, views);
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

  const missing = () =>
    [props.diff.old_missing_newline && "old", props.diff.new_missing_newline && "new"].filter(Boolean).join(" and ");

  return (
    <div class="diff-text">
      <Show when={props.diff.hunks.length === 0}>
        <p class="placeholder">No lines changed.</p>
      </Show>
      <div
        class="diff-editor"
        hidden={props.diff.hunks.length === 0}
        data-mode={sideBySide() ? "side_by_side" : "inline"}
        ref={(element) => (host = element)}
      />
      <Show when={missing() !== ""}>
        <p class="diff-note">No newline at the end of the {missing()} file.</p>
      </Show>
    </div>
  );
}
