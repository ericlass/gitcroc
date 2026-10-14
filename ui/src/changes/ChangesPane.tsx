// The changes pane (UI-14 to UI-19): a header with the diff mode, options
// and dock button, then the commit header or the uncommitted lists, the
// changed files and the selected file's diff. The core reads and diffs;
// this component lays out its answers and forwards input (ARCH-6).
import { createEffect, createMemo, For, Match, Show, Switch } from "solid-js";
import type { ChangeAction, Context, FileDiff, WorkspaceId } from "../ipc";
import { ACTION_LIST, type ChangesStore } from "../stores/changes";
import type { CommitStore } from "../stores/commit";
import type { ConflictStore } from "../stores/conflicts";
import ConflictView from "../conflict/ConflictView";
import type { LayoutStore } from "../stores/layout";
import type { SettingsStore } from "../stores/settings";
import CommitBox from "./CommitBox";
import CommitHeader from "./CommitHeader";
import DiffView, { type HunkControls } from "./DiffView";
import DiscardDialog from "./DiscardDialog";
import FileList from "./FileList";
import { entryOf } from "./fileRows";
import SpecialDiff from "./SpecialDiff";

/** The controls of a hunk of the staged or unstaged diff (UI-15). */
const HUNK_ACTIONS: Record<"staged" | "unstaged", { action: ChangeAction; label: string }[]> = {
  staged: [{ action: "unstage", label: "Unstage hunk" }],
  unstaged: [
    { action: "discard", label: "Discard hunk" },
    { action: "stage", label: "Stage hunk" },
  ],
};

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
  commit: CommitStore;
  conflicts: ConflictStore;
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
  /** The unmerged file shown in the conflict editor, as the core's conflict calls name it (OPS-3). */
  const conflictTarget = createMemo(() => {
    const shown = changes().view;
    const file = changes().file;
    const selection = changes().selection;
    const path = entry()?.path;
    return shown === null || file?.list !== "unmerged" || selection === null || path === undefined
      ? null
      : { repo: selection.repo_id, handle: shown.handle, id: file.id, path };
  });
  // Once the lists are read, the editor's state of files no longer unmerged is dropped.
  createEffect(() => {
    const content = changes().view?.content;
    const selection = changes().selection;
    if (content?.kind === "uncommitted" && selection !== null) {
      props.conflicts.prune(
        props.workspace,
        selection.repo_id,
        content.unmerged.map((file) => file.path),
      );
    }
  });
  /** Uncommitted changes: their files and hunks can be staged, unstaged and discarded. */
  const uncommitted = () => changes().view?.content.kind === "uncommitted";
  const onFiles = (action: ChangeAction, files: { list: string; id: number }[]) => {
    const list = ACTION_LIST[action];
    const refs = files.flatMap((file) => (file.list === list ? [{ list, id: file.id }] : []));
    if (refs.length > 0) {
      void props.changes.apply(props.workspace, action, { kind: "files", files: refs });
    }
  };
  /** The hunk controls of the shown diff; a new object only when the file or the options change. */
  const controls = createMemo((): HunkControls | null => {
    const ref = fileRef();
    if (!uncommitted() || ref === null || ref.list === "commit" || view().ignore_whitespace) {
      return null;
    }
    return {
      actions: HUNK_ACTIONS[ref.list],
      // The file whose diff is shown; the controls are rebuilt when it changes.
      onAction: (action, hunk) => {
        const file = fileRef();
        if (file !== null && file.list !== "commit") {
          void props.changes.apply(props.workspace, action, { kind: "hunk", file, hunk });
        }
      },
    };
  });
  /** The uncommitted lists, while the repo can take a commit: not in conflict state (M2-08 continues those). */
  const committable = () => {
    const content = changes().view?.content;
    const selection = changes().selection;
    return content?.kind === "uncommitted" && !content.in_conflict && selection !== null
      ? { content, repo: selection.repo_id }
      : null;
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
                  marked={changes().marked}
                  onSelect={(file, marked) => void props.changes.selectFile(props.workspace, file, marked)}
                  onToggleFolder={(key) => props.changes.toggleFolder(props.workspace, key)}
                  onAction={uncommitted() ? onFiles : undefined}
                  unsaved={(path) => {
                    const repo = changes().selection?.repo_id;
                    return repo !== undefined && props.conflicts.unsaved(props.workspace, repo, path);
                  }}
                />
                <Show when={committable()}>
                  {(target) => (
                    <CommitBox
                      draft={props.commit.draft(props.workspace, target().repo)}
                      staged={target().content.staged.length > 0}
                      amendable={target().content.last_message !== null}
                      busy={props.commit.busy(props.workspace, target().repo)}
                      error={props.commit.error(props.workspace, target().repo)}
                      onEdit={(change) => props.commit.edit(props.workspace, target().repo, change)}
                      onCommit={() =>
                        void props.commit.commit(props.workspace, target().repo, shown().handle, false, null)
                      }
                      onAmend={() =>
                        void props.commit.commit(
                          props.workspace,
                          target().repo,
                          shown().handle,
                          true,
                          target().content.last_message,
                        )
                      }
                    />
                  )}
                </Show>
              </div>
              <section class="changes-diff" aria-label={conflictTarget() === null ? "Diff" : "Conflict"}>
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
                      <Show when={changes().notice}>{(notice) => <p role="status">{notice()}</p>}</Show>
                      <Show when={uncommitted() && view().ignore_whitespace && textDiff() !== null}>
                        <p class="diff-note">Hunks can be staged with "Ignore whitespace" off.</p>
                      </Show>
                      <Switch>
                        <Match when={conflictTarget()}>
                          {(target) => (
                            <ConflictView
                              workspace={props.workspace}
                              target={target()}
                              conflicts={props.conflicts}
                              wrap={view().wrap_lines}
                            />
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
                              controls={controls()}
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
      <Show when={changes().confirm}>
        {(confirm) => (
          <DiscardDialog
            repo={confirm().repo}
            files={confirm().files}
            hunk={confirm().request.target.kind === "hunk"}
            onConfirm={() => void props.changes.confirmDiscard(props.workspace)}
            onCancel={() => props.changes.cancelDiscard(props.workspace)}
          />
        )}
      </Show>
    </div>
  );
}
