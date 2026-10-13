// What each workspace's changes pane shows (UI-14 to UI-18), keyed by
// workspace id (ARCH-9): the core's view of the selected commit or of the
// uncommitted changes, the selected file and its diff. The core reads the
// lists and diffs and applies whitespace and context (ARCH-6); this store
// asks for them and keeps the latest answers.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  changesGet,
  type ChangesSettings,
  type ChangesTarget,
  type ChangesView,
  core,
  type DiffRequest,
  diffGet,
  type FileDiff,
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
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const sameSelection = (a: CommitSelection | null, b: CommitSelection | null) =>
  a?.kind === b?.kind &&
  a?.repo_id === b?.repo_id &&
  (a?.kind !== "commit" || b?.kind !== "commit" || a.hash === b.hash);

/**
 * What the core is asked for, from the settings: side-by-side needs both
 * whole files, and folds unchanged runs to the chosen context itself.
 */
export function diffRequest(settings: ChangesSettings, loadAnyway: boolean): DiffRequest {
  return {
    ignore_whitespace: settings.ignore_whitespace,
    context: settings.diff_mode === "side_by_side" ? "whole_file" : settings.context,
    load_anyway: loadAnyway,
  };
}

export function createChangesStore(settings: () => ChangesSettings) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceChanges> }>({ byWorkspace: {} });
  /** The latest view and diff request of each workspace; older answers are dropped. */
  const viewRequests = new Map<WorkspaceId, number>();
  const diffRequests = new Map<WorkspaceId, number>();

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
      const again =
        keep === null
          ? null
          : fileKeys(view).find((file) => file.list === keep.list && entryOf(view.content, file)?.path === keep.path);
      update(id, (c) => {
        c.view = view;
        c.loading = false;
        c.file = again ?? firstFile(view.content, untrack(settings).file_list);
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

  /** Shows `selection` (UI-14, UI-15); the same selection again changes nothing. */
  const show = (id: WorkspaceId, selection: CommitSelection | null) =>
    untrack(() => {
      if (sameSelection(changes(id).selection, selection)) {
        return Promise.resolve();
      }
      update(id, (c) => {
        c.selection = selection;
        c.parent = 0;
        c.view = null;
        c.file = null;
        c.diff = null;
        c.diffError = null;
      });
      return load(id, null);
    });

  /** Compares a merge commit with another parent; its file list is read again (UI-16). */
  const setParent = (id: WorkspaceId, parent: number) => {
    update(id, (c) => (c.parent = parent));
    return load(id, null);
  };

  const selectFile = (id: WorkspaceId, file: FileKey) => {
    if (sameFile(untrack(() => changes(id).file), file)) {
      return Promise.resolve();
    }
    update(id, (c) => {
      c.file = file;
      c.loadAnyway = false;
    });
    return loadDiff(id);
  };

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

  /** Reads uncommitted changes again when the core reports the repo changed (ARCH-6). */
  const subscribe = async (id: WorkspaceId) =>
    core.events.repoSummary.listen((event) => {
      const { workspace_id, repo_id, refreshing } = event.payload;
      const { selection, view, file } = untrack(() => changes(id));
      // Only a summary just read says the working copy may have changed.
      if (refreshing || workspace_id !== id || selection?.kind !== "uncommitted" || selection.repo_id !== repo_id) {
        return;
      }
      const entry = view === null || file === null ? null : entryOf(view.content, file);
      void load(id, entry === null || file === null ? null : { list: file.list, path: entry.path });
    });

  return { state, changes, show, setParent, selectFile, loadAnyway, reloadDiff, toggleFolder, subscribe };
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
