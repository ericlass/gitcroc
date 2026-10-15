// What each workspace's changes pane shows (UI-14 to UI-18), keyed by
// workspace id (ARCH-9): the core's view of the selected commit or of the
// uncommitted changes, the selected file and its diff. The core reads the
// lists and diffs and applies whitespace and context (ARCH-6); this store
// asks for them and keeps the latest answers.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  type ChangeAction,
  changesApply,
  changesGet,
  type ChangeRequest,
  type ChangesSettings,
  type ChangeTarget,
  type ChangesTarget,
  type ChangesView,
  core,
  type DiffRequest,
  diffGet,
  type FileDiff,
  fsmonitorDecline,
  fsmonitorEnable,
  type RepoId,
  type WorkspaceId,
} from "../ipc";
import { entryOf, type FileKey, firstFile, sameFile } from "../changes/fileRows";
import type { CommitSelection } from "./selection";

export interface WorkspaceChanges {
  /** What is shown; `null` for nothing. */
  selection: CommitSelection | null;
  /** The parent a commit is compared with (UI-16). */
  parent: number;
  view: ChangesView | null;
  /** The view is being read. */
  loading: boolean;
  error: string | null;
  file: FileKey | null;
  diff: FileDiff | null;
  diffLoading: boolean;
  diffError: string | null;
  /** The user asked to load the selected file's diff although it is over the limit (UI-18). */
  loadAnyway: boolean;
  /** Folders of the tree that are closed, by row key. */
  collapsed: Record<string, boolean>;
  /** The files a stage, unstage or discard acts on: the selected file, or several of one list. */
  marked: FileKey[];
  /** A discard waiting for the user's confirmation (GRP-9), with the names the core gave. */
  confirm: { request: ChangeRequest; repo: string; files: string[] } | null;
  /** A short message about the last action, such as a hunk refused because its file changed. */
  notice: string | null;
  /** Repos whose fsmonitor offer (PERF-7) was answered this session; it is not shown again. */
  offerAnswered: Record<RepoId, boolean | undefined>;
  /** Why the last answer to the offer failed. */
  offerError: string | null;
  /** Where the file list and the diff were scrolled, for when they are shown again (UI-29). */
  listScroll: number;
  diffScroll: number;
  /**
   * The tab went to the background and the view and diff were let go
   * (UI-31): the file to select again when they are read on return.
   */
  released: { list: FileKey["list"]; path: string } | null | false;
}

const empty = (): WorkspaceChanges => ({
  selection: null,
  parent: 0,
  view: null,
  loading: false,
  error: null,
  file: null,
  diff: null,
  diffLoading: false,
  diffError: null,
  loadAnyway: false,
  collapsed: {},
  marked: [],
  confirm: null,
  notice: null,
  offerAnswered: {},
  offerError: null,
  listScroll: 0,
  diffScroll: 0,
  released: false,
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const sameSelection = (a: CommitSelection | null, b: CommitSelection | null) =>
  a?.kind === b?.kind &&
  a?.repo_id === b?.repo_id &&
  (a?.kind !== "commit" || b?.kind !== "commit" || a.hash === b.hash);

/**
 * What the core is asked for, from the settings: side-by-side needs both
 * whole files as well as the hunks, and folds unchanged runs to the chosen
 * context itself.
 */
export function diffRequest(settings: ChangesSettings, loadAnyway: boolean): DiffRequest {
  return {
    ignore_whitespace: settings.ignore_whitespace,
    context: settings.context,
    load_anyway: loadAnyway,
    full_text: settings.diff_mode === "side_by_side",
  };
}

/** The list a stage, unstage or discard takes its files from, as the core checks it. */
export const ACTION_LIST: Record<ChangeAction, "staged" | "unstaged"> = {
  stage: "unstaged",
  unstage: "staged",
  discard: "unstaged",
};

/** The message shown when the core refused a hunk because its file changed. */
export const FILE_CHANGED = "The file changed; showing it again.";

/** The message shown when a newer read of the list replaced the one the user acted on. */
export const LIST_CHANGED = "The list changed; showing it again.";

export function createChangesStore(settings: () => ChangesSettings) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceChanges> }>({ byWorkspace: {} });
  /** The latest view and diff request of each workspace; older answers are dropped. */
  const viewRequests = new Map<WorkspaceId, number>();
  const diffRequests = new Map<WorkspaceId, number>();
  /** The file a view being read selects again, by workspace, until it is read. */
  const keeping = new Map<WorkspaceId, { list: FileKey["list"]; path: string }>();

  const changes = (id: WorkspaceId): WorkspaceChanges => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (changes: WorkspaceChanges) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const next = (requests: Map<WorkspaceId, number>, id: WorkspaceId) => {
    const request = (requests.get(id) ?? 0) + 1;
    requests.set(id, request);
    return request;
  };

  /** Reads the selected file's diff; unmerged files have none until the conflict editor (M2). */
  const loadDiff = async (id: WorkspaceId) => {
    const request = next(diffRequests, id);
    const { view, file, loadAnyway } = untrack(() => changes(id));
    if (view === null || file === null || file.list === "unmerged") {
      update(id, (c) => {
        c.diff = null;
        c.diffLoading = false;
        c.diffError = null;
      });
      return;
    }
    update(id, (c) => {
      c.diffLoading = true;
      c.diffError = null;
    });
    const options = untrack(() => diffRequest(settings(), loadAnyway));
    try {
      const diff = await diffGet(id, view.handle, { list: file.list, id: file.id }, options);
      if (diff === null || diffRequests.get(id) !== request) {
        return;
      }
      update(id, (c) => {
        c.diff = diff;
        c.diffLoading = false;
      });
    } catch (error) {
      if (diffRequests.get(id) === request) {
        update(id, (c) => {
          c.diff = null;
          c.diffLoading = false;
          c.diffError = message(error);
        });
      }
    }
  };

  /**
   * Reads the view of the shown selection. `keep` is the file to select
   * again when the new lists still have one with its path, as after a
   * refresh; otherwise the first file is selected (UI-14).
   */
  const load = async (id: WorkspaceId, keep: { list: FileKey["list"]; path: string } | null) => {
    const request = next(viewRequests, id);
    next(diffRequests, id);
    if (keep === null) {
      keeping.delete(id);
    } else {
      keeping.set(id, keep);
    }
    const { selection, parent } = untrack(() => changes(id));
    if (selection === null) {
      update(id, (c) => {
        c.view = null;
        c.loading = false;
        c.file = null;
        c.diff = null;
      });
      return;
    }
    const target: ChangesTarget =
      selection.kind === "commit" ? { kind: "commit", hash: selection.hash, parent } : { kind: "uncommitted" };
    update(id, (c) => {
      c.loading = true;
      c.error = null;
    });
    try {
      const view = await changesGet(id, selection.repo_id, target);
      if (view === null || viewRequests.get(id) !== request) {
        return;
      }
      keeping.delete(id);
      // The same file in the same list, else in the other list, as after
      // staging it whole.
      const files = fileKeys(view);
      const withPath = (file: FileKey) => keep !== null && entryOf(view.content, file)?.path === keep.path;
      const again =
        keep === null
          ? null
          : (files.find((file) => file.list === keep.list && withPath(file)) ?? files.find(withPath));
      update(id, (c) => {
        c.view = view;
        c.loading = false;
        c.file = again ?? firstFile(view.content, untrack(settings).file_list);
        c.marked = c.file === null ? [] : [c.file];
        c.loadAnyway = false;
      });
    } catch (error) {
      if (viewRequests.get(id) === request) {
        update(id, (c) => {
          c.view = null;
          c.file = null;
          c.diff = null;
          c.loading = false;
          c.error = message(error);
        });
      }
      return;
    }
    await loadDiff(id);
  };

  /**
   * Shows `selection` (UI-14, UI-15); the same selection again changes
   * nothing, unless the view was released: then it is read again with the
   * same file and scroll positions.
   */
  const show = (id: WorkspaceId, selection: CommitSelection | null) =>
    untrack(() => {
      const { selection: shown, released } = changes(id);
      if (sameSelection(shown, selection)) {
        if (released === false) {
          return Promise.resolve();
        }
        update(id, (c) => (c.released = false));
        return load(id, released);
      }
      update(id, (c) => {
        c.selection = selection;
        c.released = false;
        c.parent = 0;
        c.view = null;
        c.file = null;
        c.marked = [];
        c.diff = null;
        c.diffError = null;
        c.confirm = null;
        c.notice = null;
        c.listScroll = 0;
        c.diffScroll = 0;
      });
      return load(id, null);
    });

  /** Compares a merge commit with another parent; its file list is read again (UI-16). */
  const setParent = (id: WorkspaceId, parent: number) => {
    update(id, (c) => (c.parent = parent));
    return load(id, null);
  };

  /**
   * Selects `file` and shows its diff. `marked` is the multi-selection it
   * belongs to, by default the file alone.
   */
  const selectFile = (id: WorkspaceId, file: FileKey, marked: FileKey[] = [file]) => {
    update(id, (c) => (c.marked = marked));
    if (sameFile(untrack(() => changes(id).file), file)) {
      return Promise.resolve();
    }
    update(id, (c) => {
      c.file = file;
      c.loadAnyway = false;
      c.diffScroll = 0;
    });
    return loadDiff(id);
  };

  /** Re-reads the view, keeping the selected file where it still exists (OPS-7). */
  const reload = (id: WorkspaceId) => {
    const { view, file } = untrack(() => changes(id));
    const entry = view === null || file === null ? null : entryOf(view.content, file);
    // While a view is read, the file it was to select again.
    const pending = view === null ? (keeping.get(id) ?? null) : null;
    return load(id, entry === null || file === null ? pending : { list: file.list, path: entry.path });
  };

  /**
   * Lets go of the view and diff of workspace `id`, whose tab went to the
   * background (UI-31); the core has ended its view. The selection, the
   * selected file's path, the folders and the scroll positions are kept for
   * `show`.
   */
  const release = (id: WorkspaceId) => {
    next(viewRequests, id);
    next(diffRequests, id);
    const { view, file, released } = untrack(() => changes(id));
    if (released !== false) {
      return;
    }
    const entry = view === null || file === null ? null : entryOf(view.content, file);
    const pending = keeping.get(id) ?? null;
    keeping.delete(id);
    update(id, (c) => {
      c.released = entry === null || file === null ? pending : { list: file.list, path: entry.path };
      c.view = null;
      c.diff = null;
      c.loading = false;
      c.diffLoading = false;
      c.confirm = null;
    });
  };

  /** Sends `request`; a discard comes back asking for confirmation (GRP-9). */
  const send = async (id: WorkspaceId, request: ChangeRequest) => {
    update(id, (c) => (c.notice = null));
    try {
      const start = await changesApply(id, request);
      if (start === null) {
        // A refresh read the view again while the user clicked; the core
        // ran nothing. Say so rather than drop the click.
        update(id, (c) => (c.notice = LIST_CHANGED));
        await reload(id);
        return;
      }
      switch (start.kind) {
        case "confirm":
          update(id, (c) => (c.confirm = { request, repo: start.repo, files: start.files }));
          break;
        case "file_changed":
          update(id, (c) => (c.notice = FILE_CHANGED));
          await reload(id);
          break;
        case "started":
          // The job's end re-reads the repo, and `subscribe` the view.
          break;
      }
    } catch (error) {
      update(id, (c) => (c.notice = message(error)));
    }
  };

  /** Stages, unstages or discards `target` of the shown view (UI-15). */
  const apply = (id: WorkspaceId, action: ChangeAction, target: ChangeTarget) => {
    const view = untrack(() => changes(id).view);
    if (view === null) {
      return Promise.resolve();
    }
    return send(id, { handle: view.handle, action, target, confirmed: false });
  };

  /** Runs the discard the user confirmed. */
  const confirmDiscard = (id: WorkspaceId) => {
    const confirm = untrack(() => changes(id).confirm);
    update(id, (c) => (c.confirm = null));
    return confirm === null ? Promise.resolve() : send(id, { ...confirm.request, confirmed: true });
  };

  const cancelDiscard = (id: WorkspaceId) => update(id, (c) => (c.confirm = null));

  /** Loads a diff over the line limit after all (UI-18). */
  const loadAnyway = (id: WorkspaceId) => {
    update(id, (c) => (c.loadAnyway = true));
    return loadDiff(id);
  };

  /** Reads the diff again, after a diff setting changed. */
  const reloadDiff = (id: WorkspaceId) => loadDiff(id);

  const toggleFolder = (id: WorkspaceId, key: string) => {
    update(id, (c) => (c.collapsed[key] = !c.collapsed[key]));
  };

  /**
   * Answers the fsmonitor offer of `repo` (PERF-7): the core turns the
   * settings on as a job, or keeps "don't ask again".
   */
  const answerOffer = async (id: WorkspaceId, repo: RepoId, accept: boolean) => {
    update(id, (c) => {
      c.offerAnswered[repo] = true;
      c.offerError = null;
    });
    try {
      await (accept ? fsmonitorEnable(id, repo) : fsmonitorDecline(id, repo));
    } catch (error) {
      const what = accept ? "Cannot turn the settings on" : "Cannot keep the answer";
      update(id, (c) => (c.offerError = `${what}: ${message(error)}`));
    }
  };

  const dismissOfferError = (id: WorkspaceId) => update(id, (c) => (c.offerError = null));

  /** Reads uncommitted changes again when the core reports the repo changed (ARCH-6). */
  const subscribe = async (id: WorkspaceId) =>
    core.events.repoSummary.listen((event) => {
      const { workspace_id, repo_id, refreshing } = event.payload;
      const { selection } = untrack(() => changes(id));
      // Only a summary just read says the working copy may have changed.
      if (refreshing || workspace_id !== id || selection?.kind !== "uncommitted" || selection.repo_id !== repo_id) {
        return;
      }
      void reload(id);
    });

  const setScroll = (id: WorkspaceId, which: "listScroll" | "diffScroll", scrollTop: number) =>
    update(id, (c) => (c[which] = scrollTop));

  return {
    state,
    changes,
    show,
    release,
    setParent,
    selectFile,
    loadAnyway,
    reloadDiff,
    reload,
    toggleFolder,
    subscribe,
    apply,
    confirmDiscard,
    cancelDiscard,
    answerOffer,
    dismissOfferError,
    setScroll,
  };
}

/** Every file of a view, unmerged ones included. */
function fileKeys(view: ChangesView): FileKey[] {
  const content = view.content;
  if (content.kind === "commit") {
    return content.files.map((file) => ({ list: "commit", id: file.id }));
  }
  return [
    ...content.staged.map((file) => ({ list: "staged" as const, id: file.id })),
    ...content.unmerged.map((file) => ({ list: "unmerged" as const, id: file.id })),
    ...content.unstaged.map((file) => ({ list: "unstaged" as const, id: file.id })),
  ];
}

export type ChangesStore = ReturnType<typeof createChangesStore>;
