// The git detection result as reported by the core (PLAT-3). The store only
// mirrors the `git_status` command and events; it decides nothing.
import { createStore } from "solid-js/store";
import { core, gitRecheck, gitStatus, type GitStatus } from "../ipc";

export type GitResult = Exclude<GitStatus, { kind: "checking" }>;

export interface GitState {
  /** A detection is running. */
  checking: boolean;
  /** The latest finished detection; `null` until the first one is done. */
  result: GitResult | null;
}

export function createGitStore() {
  const [state, setState] = createStore<GitState>({ checking: true, result: null });

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

  return { state, subscribe, recheck: gitRecheck };
}

export type GitStore = ReturnType<typeof createGitStore>;
