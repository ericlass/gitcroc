// The open workspaces as the core reports them (ARCH-6, ARCH-9). The store
// only mirrors `workspace_get` and the `workspace_changed` and
// `workspace_add_report` events; the core decides and saves.
import { createStore } from "solid-js/store";
import { type AddReport, core, type WorkspaceId, type WorkspaceView, workspaceGet } from "../ipc";

export interface WorkspacesState {
  /** In tab order; `null` until the first read. */
  views: WorkspaceView[] | null;
  /** The last add report of each workspace. */
  reports: Record<WorkspaceId, AddReport>;
  /** Why the read failed. */
  error: string | null;
}

export function createWorkspaceStore() {
  const [state, setState] = createStore<WorkspacesState>({ views: null, reports: {}, error: null });

  const record = (view: WorkspaceView) => {
    setState("views", (views) => (views ?? []).map((open) => (open.id === view.id ? view : open)));
  };

  /**
   * Starts recording workspace events, then reads the open workspaces;
   * resolves to the unsubscribe function. A change heard before the read
   * finishes is newer than the read, so it is applied again afterwards.
   * A failed read is kept in `error`.
   */
  const subscribe = async () => {
    const heard: WorkspaceView[] = [];
    let loaded = false;
    const stopChanged = await core.events.workspaceChanged.listen((event) => {
      if (loaded) {
        record(event.payload);
      } else {
        heard.push(event.payload);
      }
    });
    const stopReports = await core.events.workspaceAddReport.listen((event) => {
      setState("reports", event.payload.workspace_id, event.payload);
    });
    const unlisten = () => {
      stopChanged();
      stopReports();
    };
    try {
      setState("views", await workspaceGet());
      loaded = true;
      heard.forEach(record);
    } catch (error) {
      unlisten();
      setState("error", error instanceof Error ? error.message : String(error));
      return () => {};
    }
    return unlisten;
  };

  return { state, subscribe };
}

export type WorkspaceStore = ReturnType<typeof createWorkspaceStore>;
