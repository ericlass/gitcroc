// The updater as the core reports it (PLAT-4, D12). The store mirrors the
// `update_status` command and its events and forwards the user's choices;
// the core decides when to check, what a package does with an update, and
// whether Later still hides the notice. The automatic check's on/off is a
// setting, in the settings store.
import { createStore } from "solid-js/store";
import {
  core,
  updateCheckNow,
  updateInstall,
  updateLater,
  updateOpenDownloadPage,
  updateStatus,
  type UpdateView,
} from "../ipc";

export interface UpdateState {
  /** The core's view; `null` until the first read. */
  view: UpdateView | null;
  /** Why the last request was refused, such as a running job. */
  error: string | null;
}

/** The version on offer, while the notice is not put off with Later. */
export function noticeOf(view: UpdateView | null): string | null {
  return view?.status.kind === "available" && !view.later ? view.status.version : null;
}

export function createUpdateStore() {
  const [state, setState] = createStore<UpdateState>({ view: null, error: null });

  /**
   * Starts recording `update_status_changed` events, then reads the current view; resolves to the unsubscribe
   * function. An event that arrives first is newer than the read, so the read is then dropped.
   */
  const subscribe = async () => {
    let heardEvent = false;
    const unlisten = await core.events.updateStatusChanged.listen((event) => {
      heardEvent = true;
      setState("view", event.payload);
    });
    try {
      const current = await updateStatus();
      if (!heardEvent) {
        setState("view", current);
      }
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  };

  /** Runs `request`; a refusal is kept for the dialog or the Settings section to show. */
  const send = async <T>(request: () => Promise<T>): Promise<T | null> => {
    setState("error", null);
    try {
      return await request();
    } catch (error) {
      setState("error", error instanceof Error ? error.message : String(error));
      return null;
    }
  };

  return {
    state,
    subscribe,
    /** Checks at once, also while automatic checks are off; the result arrives as an event. */
    checkNow: () => send(updateCheckNow),
    /** Hides the notice until the next start or a newer version. */
    later: () => send(updateLater),
    /** Starts the install job; resolves to whether it started. */
    install: async () => (await send(updateInstall)) !== null,
    /** Opens the releases page in the browser, for the packages that only notify. */
    openDownloadPage: () => send(updateOpenDownloadPage),
    clearError: () => setState("error", null),
  };
}

export type UpdateStore = ReturnType<typeof createUpdateStore>;
