// Clone from the tree's add menu (section 6 "Repos"): whether the dialog is
// open, the parent folder last picked and why the last request was refused,
// per workspace (ARCH-9). The core opens the folder dialog (SEC-2), names
// the default folder, checks the target and runs the clone as a job; this
// store only sends requests and keeps the answers. A clone that ends with
// its repo in the workspace hands it to `onCloned`, which selects it. The
// clone can add the repo to a group (section 6 "Repos").
import { createStore, produce } from "solid-js/store";
import { type CloneGroup, clonePickParent, cloneStart, core, type RepoId, type WorkspaceId } from "../ipc";

interface WorkspaceClone {
  open: boolean;
  /** The parent folder last picked; kept for the next clone. */
  parent: string | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceClone => ({ open: false, parent: null, error: null, sending: false });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createCloneStore(onCloned: (id: WorkspaceId, repo: RepoId) => void) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceClone> }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceClone => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (clone: WorkspaceClone) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const isOpen = (id: WorkspaceId) => workspace(id).open;
  const parent = (id: WorkspaceId) => workspace(id).parent;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId) =>
    update(id, (s) => {
      s.open = true;
      s.error = null;
    });

  const close = (id: WorkspaceId) =>
    update(id, (s) => {
      s.open = false;
      s.error = null;
    });

  /** Asks the core to open the folder dialog; the pick arrives as `clone_parent_picked`. */
  const pickParent = (id: WorkspaceId) => {
    update(id, (s) => (s.error = null));
    clonePickParent(id).catch((failure) => update(id, (s) => (s.error = message(failure))));
  };

  /** Starts cloning `url` into folder `name` of the picked parent, then into `group`; started, the dialog closes. */
  const start = async (id: WorkspaceId, url: string, name: string, group: CloneGroup | null = null) => {
    const into = parent(id);
    if (into === null) {
      return;
    }
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      await cloneStart(id, { url, parent: into, name, repo: null, group });
      update(id, (s) => (s.open = false));
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    } finally {
      update(id, (s) => (s.sending = false));
    }
  };

  /** Follows the picked folders and the clones' ends; resolves to the unsubscribe function. */
  const subscribe = async () => {
    const stopPicked = await core.events.cloneParentPicked.listen((event) => {
      // A pick with a key is the missing repos dialog's.
      if (event.payload.key === null) {
        update(event.payload.workspace_id, (s) => (s.parent = event.payload.path));
      }
    });
    const stopReport = await core.events.cloneReport.listen((event) => {
      const { workspace_id, repo } = event.payload;
      if (repo !== null) {
        onCloned(workspace_id, repo.id);
      }
    });
    return () => {
      stopPicked();
      stopReport();
    };
  };

  return { state, isOpen, parent, error, sending, open, close, pickParent, start, subscribe };
}

export type CloneStore = ReturnType<typeof createCloneStore>;
