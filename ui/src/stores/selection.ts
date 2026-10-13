// What is selected in the history pane, keyed by workspace id (ARCH-9): a
// commit or a repo's uncommitted changes. The changes pane shows it (M1-09).
import { createStore } from "solid-js/store";
import type { RepoId, WorkspaceId } from "../ipc";

export type CommitSelection =
  | { kind: "commit"; repo_id: RepoId; hash: string }
  | { kind: "uncommitted"; repo_id: RepoId };

export function createSelectionStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, CommitSelection | null> }>({
    byWorkspace: {},
  });

  const selected = (workspace: WorkspaceId): CommitSelection | null => state.byWorkspace[workspace] ?? null;

  const select = (workspace: WorkspaceId, selection: CommitSelection | null) => {
    // Replaces the old selection object. Setting the path itself would merge
    // into it, and the changes store, which keeps the object it was shown,
    // would then see the new commit as the one it already shows.
    setState("byWorkspace", { [workspace]: selection });
  };

  return { selected, select };
}

export type SelectionStore = ReturnType<typeof createSelectionStore>;
