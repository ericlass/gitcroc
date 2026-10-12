// Repo summaries as the core reports them (ARCH-6, ARCH-9, PERF-3). The
// store only mirrors `workspace_summaries` and the `repo_summary` events,
// keyed by workspace and repo; the core decides what is stale.
import { createStore } from "solid-js/store";
import { core, type RepoId, type SummaryView, type WorkspaceId, workspaceSummaries } from "../ipc";

export interface SummariesState {
  /** Each workspace's summaries by repo id; absent until its first read. */
  byWorkspace: Record<WorkspaceId, Record<RepoId, SummaryView>>;
  /** Why a workspace's read failed. */
  errors: Record<WorkspaceId, string>;
}

export function createSummaryStore() {
  const [state, setState] = createStore<SummariesState>({ byWorkspace: {}, errors: {} });

  const record = (view: SummaryView) => {
    setState("byWorkspace", view.workspace_id, (repos) => ({ ...repos, [view.repo_id]: view }));
  };

  /**
   * Starts recording `repo_summary` events, then reads the summaries of
   * `workspaceId`; resolves to the unsubscribe function. An event heard
   * before the read finishes is newer than the read, so it is applied again
   * afterwards. A failed read is kept in `errors`.
   */
  const subscribe = async (workspaceId: WorkspaceId) => {
    const heard: SummaryView[] = [];
    let loaded = false;
    const unlisten = await core.events.repoSummary.listen((event) => {
      if (event.payload.workspace_id !== workspaceId) {
        return;
      }
      if (loaded) {
        record(event.payload);
      } else {
        heard.push(event.payload);
      }
    });
    try {
      const views = await workspaceSummaries(workspaceId);
      setState("byWorkspace", workspaceId, Object.fromEntries(views.map((view) => [view.repo_id, view])));
      loaded = true;
      heard.forEach(record);
    } catch (error) {
      unlisten();
      setState("errors", workspaceId, error instanceof Error ? error.message : String(error));
      return () => {};
    }
    return unlisten;
  };

  return { state, subscribe };
}

export type SummaryStore = ReturnType<typeof createSummaryStore>;
