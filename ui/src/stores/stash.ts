// Stash, apply, pop and drop from the toolbar and the tree (section 6
// "Stash", UI-7, UI-20): which dialog is open, why the last request was
// refused, and the offer to drop a stash git kept, per workspace (ARCH-9).
// The core checks every request, names the entry by the hash the tree read
// and decides what needs a confirmation; this store only sends requests
// and shows what came back. A pop whose job ended *conflict* leaves its
// stash in place: once the repo is out of conflict state the app offers to
// drop it, unless the apply was aborted.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { core, type JobId, type JobView, opStash, type RepoId, type StashOp, type StashRef, type WorkspaceId } from "../ipc";

export type StashDialog =
  | { kind: "push"; repo: RepoId }
  /** The core asked for the user's confirmation of `op`. */
  | { kind: "confirm"; repo: RepoId; op: StashOp; repoName: string; actions: string[] };

/** A stash git kept because its pop stopped in conflicts. */
export interface KeptStash {
  stash: StashRef;
  message: string;
}

interface WorkspaceStash {
  dialog: StashDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
  /** By repo: the stash a conflicting pop kept. */
  kept: Record<RepoId, KeptStash | undefined>;
}

const empty = (): WorkspaceStash => ({ dialog: null, error: null, sending: false, kept: {} });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Job ends kept until their request returns, at most this many. */
const EARLY_KEPT = 16;

export function createStashStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceStash> }>({ byWorkspace: {} });
  /** Pop jobs not ended yet: their repo and stash. */
  const pops = new Map<JobId, { id: WorkspaceId; repo: RepoId; kept: KeptStash }>();
  /** Job ends that arrived before their request returned. */
  const endedEarly = new Map<JobId, JobView>();

  const workspace = (id: WorkspaceId): WorkspaceStash => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (stash: WorkspaceStash) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;
  /** Every repo of the workspace with a stash a conflicting pop kept. */
  const kept = (id: WorkspaceId) =>
    Object.entries(workspace(id).kept).flatMap(([repo, stash]) => (stash === undefined ? [] : [{ repo, ...stash }]));

  const open = (id: WorkspaceId, opened: StashDialog) =>
    update(id, (s) => {
      s.dialog = opened;
      s.error = null;
    });

  const close = (id: WorkspaceId) =>
    update(id, (s) => {
      s.dialog = null;
      s.error = null;
    });

  const dismissError = (id: WorkspaceId) => update(id, (s) => (s.error = null));

  /** No offer to drop `repo`'s kept stash: dropped, kept on purpose, or its apply aborted. */
  const forget = (id: WorkspaceId, repo: RepoId) => update(id, (s) => (s.kept[repo] = undefined));

  /** A pop's job ended: *conflict* keeps its stash for the offer. */
  const popEnded = (job: JobView) => {
    const pop = pops.get(job.job_id);
    if (pop === undefined) {
      return false;
    }
    pops.delete(job.job_id);
    if (job.outcome?.kind === "conflict") {
      update(pop.id, (s) => (s.kept[pop.repo] = pop.kept));
    }
    return true;
  };

  /**
   * Sends `op` on `repo`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a drop opens the confirmation the core asked
   * for; refused, the reason is kept for the dialog or the banner.
   * `message` names a popped stash for the offer to drop it.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: StashOp, confirmed = false, popped = "") => {
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      const start = await opStash(id, { repo_id: repo, op, confirmed });
      update(id, (s) => {
        s.dialog =
          start.kind === "confirm"
            ? { kind: "confirm", repo, op, repoName: start.repo, actions: start.actions }
            : null;
      });
      if (start.kind === "started" && op.kind === "drop") {
        forget(id, repo);
      }
      if (start.kind === "started" && op.kind === "pop") {
        pops.set(start.job_id, { id, repo, kept: { stash: op.stash, message: popped } });
        const ended = endedEarly.get(start.job_id);
        if (ended !== undefined) {
          endedEarly.delete(start.job_id);
          popEnded(ended);
        }
      }
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    } finally {
      update(id, (s) => (s.sending = false));
    }
  };

  /** Follows the end of each pop; resolves to the unsubscribe function. */
  const subscribe = async () =>
    core.events.jobEnded.listen((event) => {
      const job = event.payload;
      if (untrack(() => popEnded(job))) {
        return;
      }
      endedEarly.set(job.job_id, job);
      for (const old of endedEarly.keys()) {
        if (endedEarly.size <= EARLY_KEPT) {
          break;
        }
        endedEarly.delete(old);
      }
    });

  return { state, dialog, error, sending, kept, open, close, dismissError, forget, run, subscribe };
}

export type StashStore = ReturnType<typeof createStashStore>;
