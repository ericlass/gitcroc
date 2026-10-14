// The conflict editor's state (OPS-3, OPS-5, OPS-6, OPS-8, OPS-9), keyed by
// workspace, repo and path (ARCH-9, UI-29): each conflicted file as the core
// read it, whether its edited-outside question was answered, and its unsaved
// buffer, kept while the app runs so that switching files, repos and tabs
// and back restores it. Nothing is written before Mark resolved; the core
// writes then (ARCH-1). A repo's state is dropped when its conflict state
// ends, by Abort or in a terminal, and a file's when it is no longer
// unmerged. The core checks every request; this store sends them and keeps
// what came back.
import type { EditorState, StateEffect, Text } from "@codemirror/state";
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  type Conflict,
  type ConflictChoice,
  conflictChoose,
  conflictGet,
  conflictResolve,
  type ConflictStart,
  core,
  type RepoId,
  type WorkspaceId,
} from "../ipc";

/** What the editor starts from: the conflict's hunks, or the file as it is on disk (OPS-8). */
export type Start = "conflict" | "disk";

export interface FileConflict {
  conflict: Conflict;
  /** Edited outside and not answered yet: keep the file or start over (OPS-8). `null` once answered. */
  start: Start | null;
  /** The buffer differs from what it started with. */
  unsaved: boolean;
  /** Mark resolved was refused: the first conflict marker line, from 1 (OPS-5). */
  markers: number | null;
  /** A resolution is on its way to the core. */
  sending: boolean;
  error: string | null;
}

interface WorkspaceConflicts {
  /** By repo, then by path. */
  files: Record<RepoId, Record<string, FileConflict | undefined> | undefined>;
  /** The file being read. */
  loading: { repo: RepoId; path: string } | null;
  /** Why the last read failed. */
  error: string | null;
}

/** A buffer kept while its editor is closed: the whole editor state, undo included, and where it was scrolled. */
export interface SavedEditor {
  state: EditorState;
  initial: Text;
  scroll: StateEffect<unknown> | null;
}

const empty = (): WorkspaceConflicts => ({ files: {}, loading: null, error: null });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const keyOf = (id: WorkspaceId, repo: RepoId, path: string) => `${id}\n${repo}\n${path}`;

/** A file of the shown changes view: what the core's conflict calls name it by. */
export interface ViewFile {
  repo: RepoId;
  /** The changes view's handle and the file's id in its unmerged list. */
  handle: number;
  id: number;
  path: string;
}

/**
 * `onResolvedElsewhere` is called with the workspace when the core says a
 * file is no longer in conflict, so that the changes view is read again.
 */
export function createConflictStore(onResolvedElsewhere: (id: WorkspaceId) => void) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceConflicts> }>({ byWorkspace: {} });
  /** Editor states are CodeMirror's objects, kept out of the reactive store. */
  const editors = new Map<string, SavedEditor>();

  const workspace = (id: WorkspaceId): WorkspaceConflicts => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (conflicts: WorkspaceConflicts) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const updateFile = (id: WorkspaceId, repo: RepoId, path: string, change: (file: FileConflict) => void) =>
    update(id, (c) => {
      const file = c.files[repo]?.[path];
      if (file !== undefined) {
        change(file);
      }
    });

  const file = (id: WorkspaceId, repo: RepoId, path: string): FileConflict | undefined =>
    workspace(id).files[repo]?.[path];

  const loading = (id: WorkspaceId) => workspace(id).loading;
  const error = (id: WorkspaceId) => workspace(id).error;

  /** The file has unsaved edits, for the file list's mark. */
  const unsaved = (id: WorkspaceId, repo: RepoId, path: string) => file(id, repo, path)?.unsaved ?? false;

  /**
   * Reads `target` unless its state is kept; a kept buffer is shown as it
   * was left. A file resolved meanwhile reads the changes view again.
   */
  const open = async (id: WorkspaceId, target: ViewFile) => {
    if (untrack(() => file(id, target.repo, target.path)) !== undefined) {
      return;
    }
    update(id, (c) => {
      c.loading = { repo: target.repo, path: target.path };
      c.error = null;
    });
    const current = () => {
      const now = untrack(() => loading(id));
      return now?.repo === target.repo && now.path === target.path;
    };
    try {
      const read = await conflictGet(id, target.handle, target.id);
      if (!current()) {
        return;
      }
      update(id, (c) => {
        c.loading = null;
        if (read?.kind === "conflict") {
          const conflict = read.conflict;
          (c.files[target.repo] ??= {})[target.path] = {
            conflict,
            start: conflict.edited_outside && conflict.content.kind === "text" ? null : "conflict",
            unsaved: false,
            markers: null,
            sending: false,
            error: null,
          };
        }
      });
      if (read?.kind === "already_resolved") {
        onResolvedElsewhere(id);
      }
    } catch (failure) {
      if (current()) {
        update(id, (c) => {
          c.loading = null;
          c.error = message(failure);
        });
      }
    }
  };

  /** Answers the edited-outside question (OPS-8). */
  const answer = (id: WorkspaceId, repo: RepoId, path: string, start: Start) =>
    updateFile(id, repo, path, (f) => (f.start = start));

  const editor = (id: WorkspaceId, repo: RepoId, path: string) => editors.get(keyOf(id, repo, path));

  /** Keeps the editor state of a file; called on every change and when the editor closes. */
  const keep = (id: WorkspaceId, repo: RepoId, path: string, saved: SavedEditor) => {
    // An editor closing because its file was dropped keeps nothing.
    if (untrack(() => file(id, repo, path)) === undefined) {
      return;
    }
    editors.set(keyOf(id, repo, path), saved);
    const changed = !saved.state.doc.eq(saved.initial);
    if (untrack(() => file(id, repo, path)?.unsaved) !== changed) {
      updateFile(id, repo, path, (f) => (f.unsaved = changed));
    }
  };

  /** What a resolution's answer does: a job, a refusal naming a marker line, or a stale view. */
  const settle = (id: WorkspaceId, target: ViewFile, start: ConflictStart | null) => {
    if (start === null) {
      // A newer view replaced the request's; it is on its way.
      updateFile(id, target.repo, target.path, (f) => (f.sending = false));
      return;
    }
    switch (start.kind) {
      case "started":
        // The job's end re-reads the repo, and the file leaves the list.
        updateFile(id, target.repo, target.path, (f) => (f.sending = false));
        break;
      case "markers_remain":
        updateFile(id, target.repo, target.path, (f) => {
          f.sending = false;
          f.markers = start.line;
        });
        break;
      case "already_resolved":
        forget(id, target.repo, target.path);
        onResolvedElsewhere(id);
        break;
    }
  };

  const send = async (id: WorkspaceId, target: ViewFile, request: () => Promise<ConflictStart | null>) => {
    updateFile(id, target.repo, target.path, (f) => {
      f.sending = true;
      f.markers = null;
      f.error = null;
    });
    try {
      settle(id, target, await request());
    } catch (failure) {
      updateFile(id, target.repo, target.path, (f) => {
        f.sending = false;
        f.error = message(failure);
      });
    }
  };

  /** Mark resolved with `text` (OPS-5); `override` resolves although marker lines remain. */
  const resolve = (id: WorkspaceId, target: ViewFile, text: string, override: boolean) =>
    send(id, target, () =>
      conflictResolve(id, { handle: target.handle, file: target.id, text, override_markers: override }),
    );

  /** Resolves with one side (OPS-6). */
  const choose = (id: WorkspaceId, target: ViewFile, side: ConflictChoice) =>
    send(id, target, () => conflictChoose(id, { handle: target.handle, file: target.id, side }));

  const forget = (id: WorkspaceId, repo: RepoId, path: string) => {
    editors.delete(keyOf(id, repo, path));
    update(id, (c) => {
      const files = c.files[repo];
      if (files !== undefined) {
        files[path] = undefined;
      }
    });
  };

  /** Drops every file of `repo`: its conflict state ended (OPS-9). */
  const drop = (id: WorkspaceId, repo: RepoId) => {
    for (const key of [...editors.keys()]) {
      if (key.startsWith(keyOf(id, repo, ""))) {
        editors.delete(key);
      }
    }
    update(id, (c) => {
      c.files[repo] = undefined;
      if (c.loading?.repo === repo) {
        c.loading = null;
      }
    });
  };

  /** Drops the files of `repo` that are no longer unmerged, after its lists were read. */
  const prune = (id: WorkspaceId, repo: RepoId, unmerged: readonly string[]) => {
    const files = untrack(() => workspace(id).files[repo]);
    for (const [path, kept] of Object.entries(files ?? {})) {
      if (kept !== undefined && !unmerged.includes(path)) {
        forget(id, repo, path);
      }
    }
  };

  /** Drops a repo's state when a fresh read finds it out of conflict state (OPS-9). */
  const subscribe = async (id: WorkspaceId) =>
    core.events.repoSummary.listen((event) => {
      const { workspace_id, repo_id, refreshing, conflict, summary } = event.payload;
      if (workspace_id === id && !refreshing && summary !== null && !conflict) {
        drop(id, repo_id);
      }
    });

  return {
    state,
    file,
    loading,
    error,
    unsaved,
    open,
    answer,
    editor,
    keep,
    resolve,
    choose,
    drop,
    prune,
    subscribe,
  };
}

export type ConflictStore = ReturnType<typeof createConflictStore>;
