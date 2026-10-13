// A text diff in CodeMirror (UI-17): inline, one document of the hunks with
// both line numbers, or side by side, both whole files with unchanged runs
// folded to the chosen context. The changed lines are the core's.
import { Change, MergeView } from "@codemirror/merge";
import { EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, gutter, lineNumbers, WidgetType } from "@codemirror/view";
import { createEffect, on, onCleanup, Show } from "solid-js";
import type { Context, DiffMode, TextDiff } from "../ipc";
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

class HunkHeader extends WidgetType {
  constructor(readonly header: string) {
    super();
  }
  override eq(other: HunkHeader) {
    return other.header === this.header;
  }
  override toDOM() {
    const element = document.createElement("div");
    element.className = "cm-diff-hunk";
    element.textContent = this.header;
    return element;
  }
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

function inlineExtensions(doc: InlineDoc, state: EditorState): Extension[] {
  const decorations: Range<Decoration>[] = [];
  for (const hunk of doc.hunks) {
    const from = state.doc.line(hunk.line + 1).from;
    decorations.push(Decoration.widget({ widget: new HunkHeader(hunk.header), block: true, side: -1 }).range(from));
  }
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
      merge = new MergeView({
        parent: host,
        a: { doc: oldText, extensions: [base(), lineNumbers()] },
        b: { doc: newText, extensions: [base(), lineNumbers()] },
        highlightChanges: false,
        gutter: false,
        collapseUnchanged: margin === null ? undefined : { margin, minSize: 4 },
        diffConfig: { override: () => changes },
      });
      views = [merge.a, merge.b];
    } else {
      const doc = inlineDoc(diff);
      const start = EditorState.create({ doc: doc.text });
      const state = EditorState.create({ doc: doc.text, extensions: [base(), inlineExtensions(doc, start)] });
      views = [new EditorView({ state, parent: host })];
    }
    void applyLanguage(parts, props.path, views);
  };

  createEffect(on(() => [props.diff, sideBySide(), props.context, props.path] as const, build));
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
