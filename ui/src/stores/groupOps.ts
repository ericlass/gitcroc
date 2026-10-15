// A group operation as the user follows it (section 5): the store asks the
// core for a preflight, shows its dialog when the core says the user must
// confirm, and starts the job, confirmed or at once. It decides nothing
// (ARCH-6): the classes, whether to confirm, and Retry failed's target set
// (GRP-11, GRP-5) are the core's. One preflight is open at a time.
import { createStore } from "solid-js/store";
import {
  core,
  groupDrop,
  groupPreflight,
  groupRetryFailed,
  groupStart,
  type GroupOp,
  type JobId,
  type PreflightId,
  type PreflightReady,
  type TargetSet,
  type WorkspaceId,
} from "../ipc";

/** The group operations of the toolbar's Fetch, Pull and Push (UI-20). */
export type SyncKind = "fetch" | "pull" | "push";

/** A click on Fetch, Pull or Push for a group or several repos: the defaults of section 5. */
export const DEFAULT_GROUP_OPS = {
  fetch: { kind: "fetch", prune: "config" },
  pull: { kind: "pull", strategy: "config" },
  push: { kind: "push", set_upstream: true, force_with_lease: false },
} as const satisfies Record<SyncKind, GroupOp>;

/**
 * The dialogs of the group operations on a name X typed or picked once
 * (section 5 "Checkout branch X", "Create branch X", "Delete branch X",
 * "Merge X into current", "Rebase current onto X" and "Create tag X"):
 * what they run on, how they name it, and the name they start with.
 */
export interface GroupNameDialog {
  workspaceId: WorkspaceId;
  target: TargetSet;
  /** Such as `Backend` or `3 repositories`. */
  targetName: string;
  kind: "checkout" | "create" | "delete" | "merge" | "rebase" | "tag";
  name: string;
}

export interface PendingPreflight {
  workspaceId: WorkspaceId;
  /** `null` until the core answered the request. */
  preflightId: PreflightId | null;
  /** The core's classes, once the preflight finished and the user must confirm. */
  ready: PreflightReady | null;
  /** The job is being started. */
  starting: boolean;
  /** Why the preflight or the start was refused. */
  error: string | null;
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createGroupOpStore() {
  const [state, setState] = createStore<{ pending: PendingPreflight | null; nameDialog: GroupNameDialog | null }>({
    pending: null,
    nameDialog: null,
  });
  /** Results that arrived before their request's answer, by preflight. */
  const early = new Map<PreflightId, PreflightReady>();
  /** Counts requests, so an answer to one the user left is dropped. */
  let request = 0;

  const start = async (id: PreflightId, confirmed: boolean) => {
    const mine = request;
    setState("pending", { starting: true, error: null });
    try {
      await groupStart(id, confirmed);
      if (mine === request) {
        setState("pending", null);
      }
    } catch (error) {
      if (mine === request) {
        setState("pending", { starting: false, error: message(error) });
      }
    }
  };

  /** A preflight finished: the dialog shows when the core says so, else the job starts at once. */
  const finished = (ready: PreflightReady) => {
    const pending = state.pending;
    if (pending === null) {
      return;
    }
    if (pending.preflightId === null) {
      early.set(ready.preflight_id, ready);
      return;
    }
    if (pending.preflightId !== ready.preflight_id) {
      return;
    }
    if (ready.needs_confirmation) {
      setState("pending", "ready", ready);
    } else {
      void start(ready.preflight_id, false);
    }
  };

  const begin = async (workspaceId: WorkspaceId, asked: Promise<PreflightId>) => {
    const mine = ++request;
    early.clear();
    setState("pending", { workspaceId, preflightId: null, ready: null, starting: false, error: null });
    try {
      const id = await asked;
      if (mine !== request) {
        // Cancelled before the core answered.
        void groupDrop(id);
        return;
      }
      setState("pending", "preflightId", id);
      const ready = early.get(id);
      early.clear();
      if (ready !== undefined) {
        finished(ready);
      }
    } catch (error) {
      if (mine === request) {
        setState("pending", "error", message(error));
      }
    }
  };

  /** Starts mirroring `preflight_ready`; resolves to the unsubscribe function. */
  const subscribe = () => core.events.preflightReady.listen((event) => finished(event.payload));

  return {
    state,
    subscribe,
    /** Runs `op` on `target`, after the preflight dialog when the core asks for it. */
    run: (workspaceId: WorkspaceId, target: TargetSet, op: GroupOp) =>
      begin(workspaceId, groupPreflight(workspaceId, target, op)),
    /** Opens a group branch dialog; its Run goes through `run`. */
    openName: (dialog: GroupNameDialog) => setState("nameDialog", dialog),
    closeName: () => setState("nameDialog", null),
    /** Retry failed (GRP-11) of ended group job `jobId`; the core takes its failed repos from the report. */
    retry: (workspaceId: WorkspaceId, jobId: JobId) => begin(workspaceId, groupRetryFailed(jobId)),
    /** The user confirmed the dialog (GRP-9). */
    confirm: () => {
      const ready = state.pending?.ready;
      return ready ? start(ready.preflight_id, true) : Promise.resolve();
    },
    /** The user cancelled the dialog: the core drops the preflight. */
    cancel: () => {
      const id = state.pending?.preflightId ?? null;
      request++;
      early.clear();
      setState("pending", null);
      if (id !== null) {
        void groupDrop(id);
      }
    },
  };
}

export type GroupOpStore = ReturnType<typeof createGroupOpStore>;
