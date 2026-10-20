// The changes pane (UI-14 to UI-19, UI-34): a header with the diff mode,
// options and dock button, then the commit header or the uncommitted lists,
// the changed files and the selected file's diff, and for a large repo the
// offer to speed up its status (PERF-7). For a group or several repos the
// lists are sectioned by repo, each repo's files staged as that repo's, and
// one message box runs the group Commit. The core reads and diffs; this
// component lays out its answers and forwards input (ARCH-6).
import { createEffect, createMemo, For, Match, Show, Switch } from "solid-js";
import type { ChangeAction, ChangesContent, Context, FileDiff, RepoChanges, RepoId, WorkspaceId } from "../ipc";
import { ACTION_LIST, type ChangesStore, handleOf } from "../stores/changes";
import type { CommitStore } from "../stores/commit";
import type { ConflictStore } from "../stores/conflicts";
import type { ConflictAi } from "../conflict/ConflictEditor";
import type { CommandStore } from "../palette/commands";
import ConflictView from "../conflict/ConflictView";
import type { LayoutStore } from "../stores/layout";
import type { SettingsStore } from "../stores/settings";
import CommitBox from "./CommitBox";
import CommitHeader from "./CommitHeader";
import DiffView, { type HunkControls } from "./DiffView";
import DiscardDialog from "./DiscardDialog";
import FileList from "./FileList";
import { entryOf, type FileKey } from "./fileRows";
import SpecialDiff from "./SpecialDiff";
import { strings } from "../strings/en";

/** The controls of a hunk of the staged or unstaged diff (UI-15). */
const HUNK_ACTIONS: Record<"staged" | "unstaged", { action: ChangeAction; label: string }[]> = {
  staged: [{ action: "unstage", label: strings.changes.unstageHunk }],
  unstaged: [
    { action: "discard", label: strings.changes.discardHunk },
    { action: "stage", label: strings.changes.stageHunk },
  ],
};

const CONTEXTS: { value: Context; label: string }[] = [
  { value: "three", label: strings.changes.contexts.three },
  { value: "ten", label: strings.changes.contexts.ten },
  { value: "whole_file", label: strings.changes.contexts.whole_file },
];

/**
 * Whether a group Commit takes something from `repo` (UI-34): it is not
 * blocked, and has staged changes or, with "Stage all tracked changes
 * first", changed tracked files.
 */
const commits = (repo: RepoChanges, stageAll: boolean) =>
  repo.blocked === null &&
  (repo.staged.length > 0 || (stageAll && repo.unstaged.some((file) => file.status !== "untracked")));

/** The number of files `content` lists. */
function fileCount(content: ChangesContent): number {
  const lists = (shown: Pick<RepoChanges, "staged" | "unstaged" | "unmerged">) =>
    shown.staged.length + shown.unstaged.length + shown.unmerged.length;
  switch (content.kind) {
    case "commit":
      return content.files.length;
    case "uncommitted":
      return lists(content);
    case "merged":
      return content.repos.reduce((sum, repo) => sum + lists(repo), 0);
  }
}

function Toolbar(props: { workspace: WorkspaceId; settings: SettingsStore; layout: LayoutStore }) {
  const view = () => props.settings.changes();
  const dock = () => props.layout.dock(props.workspace);
  return (
    <div class="changes-toolbar" role="toolbar" aria-label={strings.changes.diffOptions}>
      <div class="segmented" role="group" aria-label={strings.changes.diffMode}>
        <button
          type="button"
          aria-pressed={view().diff_mode === "inline"}
          onClick={() => void props.settings.setChanges({ diff_mode: "inline" })}
        >
          {strings.changes.inline}
        </button>
        <button
          type="button"
          aria-pressed={view().diff_mode === "side_by_side"}
          onClick={() => void props.settings.setChanges({ diff_mode: "side_by_side" })}
        >
          {strings.changes.sideBySide}
        </button>
      </div>
      <label>
        <input
          type="checkbox"
          checked={view().ignore_whitespace}
          onChange={(event) => void props.settings.setChanges({ ignore_whitespace: event.currentTarget.checked })}
        />
        {strings.changes.ignoreWhitespace}
      </label>
      <label>
        <input
          type="checkbox"
          checked={view().wrap_lines}
          onChange={(event) => void props.settings.setChanges({ wrap_lines: event.currentTarget.checked })}
        />
        {strings.changes.wrapLines}
      </label>
      <label>
        {strings.changes.context}
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
        {dock() === "right" ? strings.changes.dockBottom : strings.changes.dockRight}
      </button>
    </div>
  );
}

/** The offer to turn on the settings `keys` that speed up a large repo's status (PERF-7). */
function FsmonitorOffer(props: { keys: string[]; onEnable: () => void; onDecline: () => void }) {
  return (
    <div class="changes-offer" role="group" aria-label={strings.changes.fsmonitorOffer}>
      <p>{strings.changes.fsmonitorWhy(props.keys)}</p>
      <div class="changes-offer-actions">
        <button type="button" onClick={() => props.onEnable()}>
          {strings.changes.turnOn}
        </button>
        <button type="button" onClick={() => props.onDecline()}>
          {strings.changes.dontAskAgain}
        </button>
      </div>
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
  /** AI in the conflict editor; none without it. */
  ai?: ConflictAi;
  /** What the palette's operations run (UI-22); the lists, commit box and conflict editor provide theirs. */
  commands: CommandStore;
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
  /** The repo of a single repo's view; a merged one has none. */
  const repo = (): RepoId | null => {
    const selection = changes().selection;
    return selection === null || selection.kind === "merged_uncommitted" ? null : selection.repo_id;
  };
  /**
   * The unmerged file shown in the conflict editor, as the core's conflict
   * calls name it (OPS-3); none in a merged view, whose repos in conflict are
   * blocked (GRP-8).
   */
  const conflictTarget = createMemo(() => {
    const shown = changes().view;
    const file = changes().file;
    const selected = repo();
    const path = entry()?.path;
    return shown === null || file?.list !== "unmerged" || selected === null || path === undefined
      ? null
      : { repo: selected, handle: shown.handle, id: file.id, path };
  });
  // Once the lists are read, the editor's state of files no longer unmerged is dropped.
  createEffect(() => {
    const content = changes().view?.content;
    const selection = changes().selection;
    if (content?.kind === "uncommitted" && selection?.kind === "uncommitted") {
      props.conflicts.prune(
        props.workspace,
        selection.repo_id,
        content.unmerged.map((file) => file.path),
      );
    }
  });
  /** Uncommitted changes, of one repo or several: their files and hunks can be staged, unstaged and discarded. */
  const uncommitted = () => {
    const kind = changes().view?.content.kind;
    return kind === "uncommitted" || kind === "merged";
  };
  /** Stages, unstages or discards `files`; in a merged view, one job per repo (UI-34). */
  const onFiles = (action: ChangeAction, files: FileKey[]) => {
    const list = ACTION_LIST[action];
    const byRepo = new Map<RepoId | undefined, { list: typeof list; id: number }[]>();
    for (const file of files) {
      if (file.list === list) {
        byRepo.set(file.repo, [...(byRepo.get(file.repo) ?? []), { list, id: file.id }]);
      }
    }
    for (const [owner, refs] of byRepo) {
      void props.changes.apply(props.workspace, action, { kind: "files", files: refs }, owner);
    }
  };
  /** Why the selected file's repo cannot be changed in a merged view (GRP-8), or `null`. */
  const blocked = () => {
    const content = changes().view?.content;
    const owner = changes().file?.repo;
    if (content?.kind !== "merged") {
      return null;
    }
    return content.repos.find((shown) => shown.repo_id === owner)?.blocked ?? null;
  };
  /** The hunk controls of the shown diff; a new object only when the file or the options change. */
  const controls = createMemo((): HunkControls | null => {
    const ref = fileRef();
    if (!uncommitted() || ref === null || ref.list === "commit" || view().ignore_whitespace || blocked() !== null) {
      return null;
    }
    return {
      actions: HUNK_ACTIONS[ref.list],
      // The file whose diff is shown; the controls are rebuilt when it changes.
      onAction: (action, hunk) => {
        const file = fileRef();
        if (file !== null && file.list !== "commit") {
          void props.changes.apply(props.workspace, action, { kind: "hunk", file, hunk }, changes().file?.repo);
        }
      },
    };
  });
  /** The keys the fsmonitor offer turns on, while it is shown (PERF-7), with its repo. */
  const offer = () => {
    const content = changes().view?.content;
    const shown = repo();
    return content?.kind === "uncommitted" &&
      content.offer_fsmonitor !== null &&
      shown !== null &&
      !changes().offerAnswered[shown]
      ? { keys: content.offer_fsmonitor, repo: shown }
      : null;
  };
  /** The uncommitted lists, while the repo can take a commit: not in conflict state (M2-08 continues those). */
  const committable = () => {
    const content = changes().view?.content;
    const shown = repo();
    return content?.kind === "uncommitted" && !content.in_conflict && shown !== null
      ? { content, repo: shown }
      : null;
  };
  /** A merged view, with the target set its one message box commits (UI-34). */
  const merged = () => {
    const content = changes().view?.content;
    const selection = changes().selection;
    return content?.kind === "merged" && selection?.kind === "merged_uncommitted" ? { content, selection } : null;
  };
  const count = () => {
    const content = changes().view?.content;
    return content ? fileCount(content) : 0;
  };

  return (
    <div class="changes" data-dock={props.layout.dock(props.workspace)}>
      <Toolbar workspace={props.workspace} settings={props.settings} layout={props.layout} />
      <Show
        when={changes().selection}
        fallback={<p class="placeholder">{strings.changes.noSelection}</p>}
      >
        <Show when={changes().error}>{(error) => <p role="alert">{strings.changes.cannotRead(error())}</p>}</Show>
        <Show when={changes().view}>
          {(shown) => (
            <div class="changes-body">
              <div class="changes-side">
                <Show when={offer()}>
                  {(shown) => (
                    <FsmonitorOffer
                      keys={shown().keys}
                      onEnable={() => void props.changes.answerOffer(props.workspace, shown().repo, true)}
                      onDecline={() => void props.changes.answerOffer(props.workspace, shown().repo, false)}
                    />
                  )}
                </Show>
                <Show when={changes().offerError}>
                  {(error) => (
                    <div class="changes-offer error" role="alert">
                      <p>{error()}</p>
                      <button type="button" onClick={() => props.changes.dismissOfferError(props.workspace)}>
                        {strings.common.dismiss}
                      </button>
                    </div>
                  )}
                </Show>
                <Show when={commit()} fallback={<h2 class="changes-title">{strings.changes.uncommitted}</h2>}>
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
                  <span>{strings.changes.files(count())}</span>
                  <div class="segmented" role="group" aria-label={strings.changes.fileList}>
                    <button
                      type="button"
                      aria-pressed={view().file_list === "flat"}
                      onClick={() => void props.settings.setChanges({ file_list: "flat" })}
                    >
                      {strings.changes.flat}
                    </button>
                    <button
                      type="button"
                      aria-pressed={view().file_list === "tree"}
                      onClick={() => void props.settings.setChanges({ file_list: "tree" })}
                    >
                      {strings.changes.tree}
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
                    const shown = repo();
                    return shown !== null && props.conflicts.unsaved(props.workspace, shown, path);
                  }}
                  scrollTop={changes().listScroll}
                  onScroll={(top) => props.changes.setScroll(props.workspace, "listScroll", top)}
                  commands={{ store: props.commands, workspace: props.workspace }}
                />
                <Show when={committable()}>
                  {(target) => (
                    <CommitBox
                      commands={{ store: props.commands, workspace: props.workspace }}
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
                <Show when={merged()}>
                  {(group) => {
                    const key = () => group().selection.key;
                    const options = () => props.commit.groupOptions(props.workspace, key());
                    const repos = () =>
                      group().content.repos.filter((shown) => commits(shown, options().stage_all)).length;
                    return (
                      <CommitBox
                        commands={{ store: props.commands, workspace: props.workspace }}
                        draft={props.commit.groupDraft(props.workspace, key())}
                        staged={repos() > 0}
                        amendable={false}
                        busy={props.commit.groupBusy(props.workspace, key())}
                        error={props.commit.groupError(props.workspace, key())}
                        label={strings.changes.commitRepos(repos())}
                        onEdit={(change) => props.commit.editGroup(props.workspace, key(), change)}
                        onCommit={() => props.commit.commitGroup(props.workspace, key(), group().selection.target)}
                      >
                        <label class="choice">
                          <input
                            type="checkbox"
                            checked={options().stage_all}
                            onChange={(event) =>
                              props.commit.setGroupOptions(props.workspace, key(), {
                                stage_all: event.currentTarget.checked,
                              })
                            }
                          />{" "}
                          {strings.changes.stageAllFirst}
                        </label>
                        <label class="choice">
                          <input
                            type="checkbox"
                            checked={options().push}
                            onChange={(event) =>
                              props.commit.setGroupOptions(props.workspace, key(), {
                                push: event.currentTarget.checked,
                              })
                            }
                          />{" "}
                          {strings.changes.pushAfter}
                        </label>
                      </CommitBox>
                    );
                  }}
                </Show>
              </div>
              <section
                class="changes-diff"
                aria-label={conflictTarget() === null ? strings.changes.diff : strings.changes.conflict}
              >
                <Show when={entry()} fallback={<p class="placeholder">{strings.changes.noFile}</p>}>
                  {(file) => (
                    <>
                      <div class="diff-header" title={file().path}>
                        {file().oldPath === null ? file().path : `${file().oldPath} → ${file().path}`}
                        <Show when={file().modeChange}>
                          {(mode) => (
                            <span class="diff-mode">
                              {" "}
                              {strings.changes.mode(mode().old, mode().new)}
                            </span>
                          )}
                        </Show>
                      </div>
                      <Show when={changes().diffError}>
                        {(error) => <p role="alert">{strings.changes.cannotReadDiff(error())}</p>}
                      </Show>
                      <Show when={changes().notice}>{(notice) => <p role="status">{notice()}</p>}</Show>
                      <Show when={uncommitted() && view().ignore_whitespace && textDiff() !== null}>
                        <p class="diff-note">{strings.changes.hunksIgnoreWhitespace}</p>
                      </Show>
                      <Switch>
                        <Match when={conflictTarget()}>
                          {(target) => (
                            <ConflictView
                              workspace={props.workspace}
                              target={target()}
                              conflicts={props.conflicts}
                              wrap={view().wrap_lines}
                              ai={props.ai}
                              commands={props.commands}
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
                              scrollTop={changes().diffScroll}
                              onScroll={(top) => props.changes.setScroll(props.workspace, "diffScroll", top)}
                              hunk={changes().diffHunk}
                              onHunk={(hunk) => props.changes.setDiffHunk(props.workspace, hunk)}
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
                                  handle={handleOf(shown(), changes().file ?? {}) ?? shown().handle}
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
