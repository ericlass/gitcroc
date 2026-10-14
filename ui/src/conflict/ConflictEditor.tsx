// The editor of a text conflict (OPS-3, OPS-5, OPS-9): the buffer in an
// editable CodeMirror with its hunks, next and previous conflict, and Mark
// resolved, which sends the whole buffer to the core. The editor state is
// kept in the conflict store on every change and restored with its cursor,
// undo and scroll when the file is shown again. Nothing is written here.
import { EditorState, StateEffect, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { createEffect, createSignal, on, onCleanup, onMount, Show, untrack } from "solid-js";
import { applyLanguage, baseExtensions, newParts, wrapping } from "../changes/codemirror";
import type { WorkspaceId } from "../ipc";
import type { ConflictStore, FileConflict, Start, ViewFile } from "../stores/conflicts";
import { buildBuffer, normalize } from "./buffer";
import { conflictExtensions, goToHunk, hunkCount } from "./editor";

/** The buffer a file starts with: the conflict's hunks, or the file on disk without any. */
function startBuffer(file: FileConflict, start: Start) {
  const content = file.conflict.content;
  if (start === "disk" || content.kind !== "text") {
    return { text: normalize(file.conflict.on_disk ?? ""), hunks: [] };
  }
  return buildBuffer(content.chunks, file.conflict.labels);
}

export default function ConflictEditor(props: {
  workspace: WorkspaceId;
  target: ViewFile;
  file: FileConflict;
  start: Start;
  conflicts: ConflictStore;
  wrap: boolean;
  /** Takes the focus when shown, as when the file before it was resolved from the keyboard. */
  focus: boolean;
  /** Called on closing, with whether the editor had the focus. */
  onClose: (focused: boolean) => void;
}) {
  let parent: HTMLDivElement | undefined;
  let view: EditorView | undefined;
  const parts = newParts();
  const [remaining, setRemaining] = createSignal(0);
  // The file this editor was opened for; the parent remounts it for another.
  const { workspace, conflicts, repo, path } = untrack(() => ({
    workspace: props.workspace,
    conflicts: props.conflicts,
    repo: props.target.repo,
    path: props.target.path,
  }));

  const markResolved = (override: boolean) => {
    if (view !== undefined && !props.file.sending) {
      void conflicts.resolve(workspace, props.target, view.state.doc.toString(), override);
    }
  };

  onMount(() => {
    const saved = conflicts.editor(workspace, repo, path);
    const actions = { labels: props.file.conflict.labels, markResolved: () => markResolved(false) };
    const wrap = props.wrap;
    // What the buffer started with, to tell unsaved edits (OPS-9).
    let initial: Text;
    const extensions = (hunks: Parameters<typeof conflictExtensions>[1]) => [
      ...baseExtensions(parts, wrap, true),
      ...conflictExtensions(actions, hunks),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.transactions.length > 0) {
          setRemaining(hunkCount(update.state));
          conflicts.keep(workspace, repo, path, { state: update.state, initial, scroll: null });
        }
      }),
      EditorView.contentAttributes.of({ "aria-label": `Resolution of ${path}` }),
    ];
    let state: EditorState;
    if (saved !== undefined) {
      initial = saved.initial;
      state = saved.state.update({ effects: StateEffect.reconfigure.of(extensions(null)) }).state;
    } else {
      const buffer = startBuffer(props.file, props.start);
      state = EditorState.create({ doc: buffer.text, extensions: extensions(buffer.hunks) });
      initial = state.doc;
      conflicts.keep(workspace, repo, path, { state, initial, scroll: null });
    }
    view = new EditorView({ state, parent });
    setRemaining(hunkCount(state));
    if (saved?.scroll) {
      view.dispatch({ effects: saved.scroll });
    }
    void applyLanguage(parts, path, [view]);
    if (props.focus) {
      view.focus();
    }
    const opened = view;
    onCleanup(() => {
      props.onClose(opened.hasFocus);
      conflicts.keep(workspace, repo, path, {
        state: opened.state,
        initial,
        scroll: opened.scrollSnapshot(),
      });
      opened.destroy();
    });
  });

  createEffect(
    on(
      () => props.wrap,
      (wrap) => view?.dispatch({ effects: wrapping(parts, wrap) }),
      { defer: true },
    ),
  );

  const go = (direction: 1 | -1) => {
    if (view !== undefined) {
      goToHunk(view, direction);
      view.focus();
    }
  };

  return (
    <div class="conflict-editor-pane">
      <div class="conflict-toolbar" role="toolbar" aria-label="Conflict">
        <span class="conflict-count" role="status">
          {remaining() === 0 ? "No conflict left" : remaining() === 1 ? "1 conflict left" : `${remaining()} conflicts left`}
        </span>
        <button type="button" title="Alt+↑" disabled={remaining() === 0} onClick={() => go(-1)}>
          Previous conflict
        </button>
        <button type="button" title="Alt+↓" disabled={remaining() === 0} onClick={() => go(1)}>
          Next conflict
        </button>
        <button
          type="button"
          class="primary"
          title="Write this text to the file and stage it (Ctrl+Enter)"
          disabled={props.file.sending}
          onClick={() => markResolved(false)}
        >
          Mark resolved
        </button>
      </div>
      <Show when={props.file.markers}>
        {(line) => (
          <p role="alert" class="conflict-refusal">
            Line {line()} is still a conflict marker line, so the file was not marked resolved.
            <button type="button" disabled={props.file.sending} onClick={() => markResolved(true)}>
              Mark resolved anyway
            </button>
          </p>
        )}
      </Show>
      <Show when={props.file.error}>{(error) => <p role="alert">Cannot mark it resolved: {error()}</p>}</Show>
      <div class="conflict-editor" ref={(element) => (parent = element)} />
    </div>
  );
}
