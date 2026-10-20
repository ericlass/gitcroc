// The git detection result as reported by the core (PLAT-3). The store only
// mirrors the `git_status` command and events; it decides nothing.
import { createStore } from "solid-js/store";
import { core, gitPickPath, gitRecheck, gitStatus, type GitStatus, settingsSetGitPath } from "../ipc";

export type GitResult = Exclude<GitStatus, { kind: "checking" }>;

export interface GitState {
  /** A detection is running. */
  checking: boolean;
  /** The latest finished detection; `null` until the first one is done. */
  result: GitResult | null;
  /** Why choosing a git path failed, such as read-only settings. */
  error: string | null;
}

export function createGitStore() {
  const [state, setState] = createStore<GitState>({ checking: true, result: null, error: null });

  const record = (status: GitStatus) => {
    if (status.kind === "checking") {
      setState("checking", true);
    } else {
      setState({ checking: false, result: status });
    }
  };

  /**
   * Starts recording `git_status` events, then reads the current status;
   * resolves to the unsubscribe function. An event that arrives first is
   * newer than the read, so the read is then dropped.
   */
  const subscribe = async () => {
    let heardEvent = false;
    const unlisten = await core.events.gitStatus.listen((event) => {
      heardEvent = true;
      record(event.payload);
    });
    try {
      const current = await gitStatus();
      if (!heardEvent) {
        record(current);
      }
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  };

  // The result arrives as a `git_status` event; only a refusal is answered here.
  const choosing = async (request: () => Promise<null>) => {
    try {
      await request();
      setState("error", null);
    } catch (error) {
      setState("error", error instanceof Error ? error.message : String(error));
    }
  };

  return {
    state,
    subscribe,
    recheck: gitRecheck,
    /** Opens the platform's file dialog to choose git (PLAT-3); a cancel does nothing. */
    choose: () => choosing(gitPickPath),
    /** Forgets the chosen git and detects it again. */
    useAutomatic: () => choosing(() => settingsSetGitPath(null)),
  };
}

export type GitStore = ReturnType<typeof createGitStore>;
