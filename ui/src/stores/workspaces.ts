// The open workspaces as the core reports them (ARCH-6, ARCH-9). The store
// mirrors `workspace_get` and the `workspaces_changed`, `workspace_changed`
// and `workspace_add_report` events, and sends the tab changes (UI-27,
// UI-28); the core checks names, decides and saves.
import { createStore } from "solid-js/store";
import {
  type AddReport,
  core,
  type WorkspaceId,
  workspaceActivate,
  workspaceClose,
  workspaceCreate,
  workspaceDelete,
  workspaceGet,
  workspaceList,
  workspaceOpen,
  workspaceRename,
  workspaceReorder,
  workspaceSetAi,
  type WorkspaceTabs,
  type WorkspaceView,
} from "../ipc";

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

  /** Sends a tab change; the tabs it answers with are shown at once, as its event would show them. */
  const change = async (request: Promise<WorkspaceTabs>) => recordTabs(await request);

  /** Moves open workspace `id` by `step` places in the tab order; past either end it stays. */
  const move = (id: WorkspaceId, step: number) => {
    const order = (state.views ?? []).map((view) => view.id);
    const from = order.indexOf(id);
    const to = from + step;
    if (from < 0 || to < 0 || to >= order.length) {
      return Promise.resolve();
    }
    order.splice(from, 1);
    order.splice(to, 0, id);
    return change(workspaceReorder(order));
  };

  return {
    state,
    active,
    subscribe,
    /** Every workspace file, open ones marked, sorted by name. */
    list: workspaceList,
    create: (name: string) => change(workspaceCreate(name)),
    /** Opens a workspace as the last tab, or switches to it when it is open (UI-27). */
    open: (id: WorkspaceId) => change(workspaceOpen(id)),
    close: (id: WorkspaceId) => change(workspaceClose(id)),
    rename: (id: WorkspaceId, name: string) => change(workspaceRename(id, name)),
    activate: (id: WorkspaceId) => change(workspaceActivate(id)),
    reorder: (order: WorkspaceId[]) => change(workspaceReorder(order)),
    move,
    /** Turns AI on or off for workspace `id` (AI-6); its view is shown at once, as its event would show it. */
    setAi: async (id: WorkspaceId, enabled: boolean) => record(await workspaceSetAi(id, enabled)),
    /** Deletes the file of a workspace that is not open, after the user confirmed. */
    remove: (id: WorkspaceId) => workspaceDelete(id, true),
  };
}

export type WorkspaceStore = ReturnType<typeof createWorkspaceStore>;
