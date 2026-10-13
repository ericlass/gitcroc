// Refresh on window focus and the file watcher's warning (OPS-7, PERF-6),
// keyed by workspace id (ARCH-9). The core watches the files and reads the
// repos; this store only asks it to when the window regains focus, and keeps
// the warning it gives.
import { untrack } from "solid-js";
import { createStore } from "solid-js/store";
import { core, type WatchWarning, watchWarning, type WorkspaceId, workspaceRefresh } from "../ipc";

export interface WorkspaceWatch {
  /** Why watching failed; changes then show on focus only. */
  warning: WatchWarning | null;
  /** The user closed the warning; it is shown once (OPS-7). */
  dismissed: boolean;
}

export function createWatchStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceWatch> }>({ byWorkspace: {} });

  const watch = (id: WorkspaceId): WorkspaceWatch => state.byWorkspace[id] ?? { warning: null, dismissed: false };

  const warn = (id: WorkspaceId, warning: WatchWarning) => {
    if (untrack(() => watch(id).warning) === null) {
      setState("byWorkspace", id, { warning, dismissed: false });
    }
  };

  /** Hides the warning for the rest of the run. */
  const dismiss = (id: WorkspaceId) => {
    setState("byWorkspace", id, { ...untrack(() => watch(id)), dismissed: true });
  };

  /**
   * While workspace `id` is open: reads every repo again when the window
   * regains focus, and records the watcher's warning, also one given before
   * this subscribed. Resolves to the unsubscribe function.
   */
  const subscribe = async (id: WorkspaceId) => {
    const onFocus = () => {
      void workspaceRefresh(id).catch(() => {
        // No usable git yet: its detection reads every repo when it ends.
      });
    };
    window.addEventListener("focus", onFocus);
    const unlisten = await core.events.watchWarning.listen((event) => {
      if (event.payload.workspace_id === id) {
        warn(id, event.payload);
      }
    });
    try {
      const given = await watchWarning(id);
      if (given !== null) {
        warn(id, given);
      }
    } catch {
      // The event still brings a warning given later.
    }
    return () => {
      window.removeEventListener("focus", onFocus);
      unlisten();
    };
  };

  return { state, watch, dismiss, subscribe };
}

export type WatchStore = ReturnType<typeof createWatchStore>;
