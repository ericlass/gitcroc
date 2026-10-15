// The open workspaces as the core reports them (ARCH-6, ARCH-9). The store
// only mirrors `workspace_get` and the `workspaces_changed`,
// `workspace_changed` and `workspace_add_report` events; the core decides
// and saves.
import { createStore } from "solid-js/store";
import { type AddReport, core, type WorkspaceId, type WorkspaceTabs, type WorkspaceView, workspaceGet } from "../ipc";

export interface WorkspacesState {
  /** In tab order; `null` until the first read. */
  views: WorkspaceView[] | null;
  /** The active tab; `null` when no workspace is open (UI-27). */
  active: WorkspaceId | null;
  /** The saved tabs whose workspace file was gone at start (UI-33). */
  missing: string[];
  /** The last add report of each workspace. */
  reports: Record<WorkspaceId, AddReport>;
  /** Why the read failed. */
  error: string | null;
}

/** The change the core sent, kept in arrival order until the first read is in. */
type Heard = { kind: "tabs"; tabs: WorkspaceTabs } | { kind: "view"; view: WorkspaceView };

export function createWorkspaceStore() {
  const [state, setState] = createStore<WorkspacesState>({
    views: null,
    active: null,
    missing: [],
    reports: {},
    error: null,
  });

  const record = (view: WorkspaceView) => {
    setState("views", (views) => (views ?? []).map((open) => (open.id === view.id ? view : open)));
  };

  const recordTabs = (tabs: WorkspaceTabs) => {
    setState({ views: tabs.open, active: tabs.active, missing: tabs.missing });
  };

  const apply = (heard: Heard) => (heard.kind === "tabs" ? recordTabs(heard.tabs) : record(heard.view));

  /** The active workspace's view, if one is open. */
  const active = () => state.views?.find((view) => view.id === state.active);

  /**
   * Starts recording workspace events, then reads the open workspaces;
   * resolves to the unsubscribe function. A change heard before the read
   * finishes is newer than the read, so it is applied again afterwards.
   * A failed read is kept in `error`.
   */
  const subscribe = async () => {
    const heard: Heard[] = [];
    let loaded = false;
    const hear = (change: Heard) => (loaded ? apply(change) : heard.push(change));
    const stopTabs = await core.events.workspacesChanged.listen((event) => {
      hear({ kind: "tabs", tabs: event.payload });
    });
    const stopChanged = await core.events.workspaceChanged.listen((event) => {
      hear({ kind: "view", view: event.payload });
    });
    const stopReports = await core.events.workspaceAddReport.listen((event) => {
      setState("reports", event.payload.workspace_id, event.payload);
    });
    const unlisten = () => {
      stopTabs();
      stopChanged();
      stopReports();
    };
    try {
      recordTabs(await workspaceGet());
      loaded = true;
      heard.forEach(apply);
    } catch (error) {
      unlisten();
      setState("error", error instanceof Error ? error.message : String(error));
      return () => {};
    }
    return unlisten;
  };

  return { state, active, subscribe };
}

export type WorkspaceStore = ReturnType<typeof createWorkspaceStore>;
