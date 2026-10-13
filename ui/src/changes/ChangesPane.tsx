// The changes pane (UI-14 to UI-19): a header with the diff mode, options
// and dock button, then the commit header or the uncommitted lists, the
// changed files and the selected file's diff. The core reads and diffs;
// this component lays out its answers and forwards input (ARCH-6).
import { createMemo, For, Match, Show, Switch } from "solid-js";
import type { Context, FileDiff, WorkspaceId } from "../ipc";
import type { ChangesStore } from "../stores/changes";
import type { LayoutStore } from "../stores/layout";
import type { SettingsStore } from "../stores/settings";
import CommitHeader from "./CommitHeader";
import DiffView from "./DiffView";
import FileList, { CONFLICTS } from "./FileList";
import { entryOf } from "./fileRows";
import SpecialDiff from "./SpecialDiff";

const CONTEXTS: { value: Context; label: string }[] = [
  { value: "three", label: "3 lines" },
  { value: "ten", label: "10 lines" },
  { value: "whole_file", label: "Whole file" },
];

function Toolbar(props: { workspace: WorkspaceId; settings: SettingsStore; layout: LayoutStore }) {
  const view = () => props.settings.changes();
  const dock = () => props.layout.dock(props.workspace);
  return (
    <div class="changes-toolbar" role="toolbar" aria-label="Diff options">
      <div class="segmented" role="group" aria-label="Diff mode">
        <button
          type="button"
          aria-pressed={view().diff_mode === "inline"}
          onClick={() => void props.settings.setChanges({ diff_mode: "inline" })}
        >
          Inline
        </button>
        <button
          type="button"
          aria-pressed={view().diff_mode === "side_by_side"}
          onClick={() => void props.settings.setChanges({ diff_mode: "side_by_side" })}
        >
          Side by side
        </button>
      </div>
      <label>
        <input
          type="checkbox"
          checked={view().ignore_whitespace}
          onChange={(event) => void props.settings.setChanges({ ignore_whitespace: event.currentTarget.checked })}
        />
        Ignore whitespace
      </label>
      <label>
        <input
          type="checkbox"
          checked={view().wrap_lines}
          onChange={(event) => void props.settings.setChanges({ wrap_lines: event.currentTarget.checked })}
        />
        Wrap lines
      </label>
      <label>
        Context
        <select
          value={view().context}
          onChange={(event) => void props.settings.setChanges({ context: event.currentTarget.value as Context })}
        >
          <For each={CONTEXTS}>{(option) => <option value={option.value}>{option.label}</option>}</For>
        </select>
      </label>
      <button
        type="button"
        class="dock-button"
        onClick={() => props.layout.setDock(props.workspace, dock() === "right" ? "bottom" : "right")}
      >
        {dock() === "right" ? "Dock at bottom" : "Dock right"}
      </button>
    </div>
  );
}

export default function ChangesPane(props: {
  workspace: WorkspaceId;
  changes: ChangesStore;
  settings: SettingsStore;
  layout: LayoutStore;
  /** Selects a commit in the history, as a parent link asks. */
  onSelectCommit: (hash: string) => void;
}) {
  const changes = () => props.changes.changes(props.workspace);
  const view = () => props.settings.changes();
  const entry = createMemo(() => {
    const shown = changes().view;
    const file = changes().file;
    return shown === null || file === null ? null : entryOf(shown.content, file);
  });
  const commit = () => {
    const content = changes().view?.content;
    return content?.kind === "commit" ? content : null;
  };
  const textDiff = () => {
    const diff = changes().diff;
    return diff?.kind === "text" ? diff : null;
  };
  const otherDiff = () => {
    const diff = changes().diff;
    return diff !== null && diff.kind !== "text" ? (diff as Exclude<FileDiff, { kind: "text" }>) : null;
  };
  const fileRef = () => {
    const file = changes().file;
    return file === null || file.list === "unmerged" ? null : { list: file.list, id: file.id };
  };
  const count = () => {
    const content = changes().view?.content;
    if (!content) {
      return 0;
    }
    return content.kind === "commit"
      ? content.files.length
      : content.staged.length + content.unstaged.length + content.unmerged.length;
  };

  return (
    <div class="changes" data-dock={props.layout.dock(props.workspace)}>
      <Toolbar workspace={props.workspace} settings={props.settings} layout={props.layout} />
      <Show
        when={changes().selection}
        fallback={<p class="placeholder">Select a commit or the uncommitted changes to see what changed.</p>}
      >
        <Show when={changes().error}>{(error) => <p role="alert">Cannot read the changes: {error()}</p>}</Show>
        <Show when={changes().view}>
          {(shown) => (
            <div class="changes-body">
              <div class="changes-side">
                <Show when={commit()} fallback={<h2 class="changes-title">Uncommitted changes</h2>}>
                  {(content) => (
                    <CommitHeader
                      details={content().details}
                      parent={content().parent}
                      onSelectParent={(hash) => props.onSelectCommit(hash)}
                      onCompareWith={(parent) => void props.changes.setParent(props.workspace, parent)}
                    />
                  )}
                </Show>
                <div class="files-header">
                  <span>
                    {count()} {count() === 1 ? "file" : "files"}
                  </span>
                  <div class="segmented" role="group" aria-label="File list">
                    <button
                      type="button"
                      aria-pressed={view().file_list === "flat"}
                      onClick={() => void props.settings.setChanges({ file_list: "flat" })}
                    >
                      Flat
                    </button>
                    <button
                      type="button"
                      aria-pressed={view().file_list === "tree"}
                      onClick={() => void props.settings.setChanges({ file_list: "tree" })}
                    >
                      Tree
                    </button>
                  </div>
                </div>
                <FileList
                  content={shown().content}
                  mode={view().file_list}
                  collapsed={changes().collapsed}
                  selected={changes().file}
                  onSelect={(file) => void props.changes.selectFile(props.workspace, file)}
                  onToggleFolder={(key) => props.changes.toggleFolder(props.workspace, key)}
                />
              </div>
              <section class="changes-diff" aria-label="Diff">
                <Show when={entry()} fallback={<p class="placeholder">No file changed.</p>}>
                  {(file) => (
                    <>
                      <div class="diff-header" title={file().path}>
                        {file().oldPath === null ? file().path : `${file().oldPath} → ${file().path}`}
                        <Show when={file().modeChange}>
                          {(mode) => (
                            <span class="diff-mode">
                              {" "}
                              mode {mode().old} → {mode().new}
                            </span>
                          )}
                        </Show>
                      </div>
                      <Show when={changes().diffError}>
                        {(error) => <p role="alert">Cannot read the diff: {error()}</p>}
                      </Show>
                      <Switch>
                        <Match when={file().conflict}>
                          {(conflict) => (
                            <p class="diff-special">
                              Conflict: {CONFLICTS[conflict()]}. A conflicted file shows no diff here.
                            </p>
                          )}
                        </Match>
                        <Match when={textDiff()}>
                          {(diff) => (
                            <DiffView
                              diff={diff()}
                              path={file().path}
                              mode={view().diff_mode}
                              wrap={view().wrap_lines}
                              context={view().context}
                            />
                          )}
                        </Match>
                        <Match when={otherDiff()}>
                          {(diff) => (
                            <Show when={fileRef()}>
                              {(ref) => (
                                <SpecialDiff
                                  diff={diff()}
                                  workspace={props.workspace}
                                  handle={shown().handle}
                                  file={ref()}
                                  onLoadAnyway={() => void props.changes.loadAnyway(props.workspace)}
                                />
                              )}
                            </Show>
                          )}
                        </Match>
                      </Switch>
                    </>
                  )}
                </Show>
              </section>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}
