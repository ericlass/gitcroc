// The commit message box of "Uncommitted changes" (UI-15): the message the
// user is writing, kept per repo of each workspace (ARCH-9) while the app
// runs, and the commit or amend job it was sent with. The core composes
// the message, checks the commit can be made and runs it (ARCH-6); this
// store keeps the unsent text and clears it when the commit succeeded.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { changesCommit, type CommitMessage, core, type JobId, type JobView, type RepoId, type WorkspaceId } from "../ipc";
import { LIST_CHANGED } from "./changes";

/** A commit or amend the core is running. */
interface Pending {
  job: JobId;
  repo: RepoId;
  /** The message as sent; the box is cleared only if it still holds it. */
  sent: CommitMessage;
}

interface WorkspaceCommit {
  /** The unsent message of each repo. */
  drafts: Record<RepoId, CommitMessage>;
  pending: Pending | null;
  /** Why the last commit was refused or failed, by repo. */
  errors: Record<RepoId, string | null>;
}

/** How many ended jobs are kept for a start not answered yet. */
const EARLY_KEPT = 16;

const EMPTY: CommitMessage = { subject: "", body: "" };

const empty = (): WorkspaceCommit => ({ drafts: {}, pending: null, errors: {} });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const same = (a: CommitMessage, b: CommitMessage) => a.subject === b.subject && a.body === b.body;

/** No subject and no body: what Amend fills with the last commit's message. */
export const isBlank = (draft: CommitMessage) => draft.subject.trim() === "" && draft.body.trim() === "";

/** What the box says when the commit's job failed; its output is in the job list. */
export const COMMIT_FAILED = "The commit failed. The job list shows what git said.";

/**
 * `reread` reads the workspace's changes view again, after a newer read of
 * it replaced the one the user committed from.
 */
export function createCommitStore(reread: (id: WorkspaceId) => void = () => {}) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceCommit> }>({ byWorkspace: {} });
  /** Jobs that ended before their start was answered, which the core's events may do. */
  const endedEarly = new Map<JobId, JobView>();

  const workspace = (id: WorkspaceId): WorkspaceCommit => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (commit: WorkspaceCommit) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  /** The unsent message of `repo`. */
  const draft = (id: WorkspaceId, repo: RepoId): CommitMessage => workspace(id).drafts[repo] ?? EMPTY;

  /** A commit or amend of `repo` is running. */
  const busy = (id: WorkspaceId, repo: RepoId) => workspace(id).pending?.repo === repo;

  /** Why the last commit of `repo` was refused or failed. */
  const error = (id: WorkspaceId, repo: RepoId): string | null => workspace(id).errors[repo] ?? null;

  const edit = (id: WorkspaceId, repo: RepoId, change: Partial<CommitMessage>) =>
    update(id, (c) => {
      c.drafts[repo] = { ...(c.drafts[repo] ?? EMPTY), ...change };
    });

  /** Ends the pending commit if `job` is it; `false` when it is not. */
  const finish = (job: JobView): boolean => {
    const pending = untrack(() => workspace(job.workspace_id).pending);
    if (pending?.job !== job.job_id) {
      return false;
    }
    update(job.workspace_id, (c) => {
      c.pending = null;
      if (job.outcome?.kind !== "succeeded") {
        c.errors[pending.repo] = COMMIT_FAILED;
      } else if (same(c.drafts[pending.repo] ?? EMPTY, pending.sent)) {
        c.drafts[pending.repo] = EMPTY;
      }
    });
    return true;
  };

  /**
   * Commits the staged changes of view `handle`, or amends the last commit
   * (UI-15). Amend with an empty box only fills it with `last`, the last
   * commit's message, for the user to read and change first.
   */
  const commit = async (id: WorkspaceId, repo: RepoId, handle: number, amend: boolean, last: CommitMessage | null) => {
    const sent = untrack(() => draft(id, repo));
    if (amend && isBlank(sent)) {
      if (last !== null) {
        edit(id, repo, last);
      }
      return;
    }
    if (untrack(() => workspace(id).pending) !== null) {
      return;
    }
    update(id, (c) => (c.errors[repo] = null));
    try {
      const job = await changesCommit(id, { handle, message: sent, amend });
      if (job === null) {
        // A refresh read the view again while the user clicked; the core
        // ran nothing. Say so rather than drop the click; the message stays.
        update(id, (c) => (c.errors[repo] = LIST_CHANGED));
        reread(id);
        return;
      }
      update(id, (c) => (c.pending = { job, repo, sent }));
      const ended = endedEarly.get(job);
      if (ended !== undefined) {
        endedEarly.delete(job);
        finish(ended);
      }
    } catch (failure) {
      update(id, (c) => (c.errors[repo] = message(failure)));
    }
  };

  /**
   * Follows the end of each commit job: success clears the box unless the
   * user changed it meanwhile; failure, such as a hook's refusal, keeps it.
   * Resolves to the unsubscribe function.
   */
  const subscribe = async () =>
    core.events.jobEnded.listen((event) => {
      const job = event.payload;
      if (finish(job)) {
        return;
      }
      endedEarly.set(job.job_id, job);
      // Only the latest few can still be waited for.
      for (const old of endedEarly.keys()) {
        if (endedEarly.size <= EARLY_KEPT) {
          break;
        }
        endedEarly.delete(old);
      }
    });

  return { state, draft, busy, error, edit, commit, subscribe };
}

export type CommitStore = ReturnType<typeof createCommitStore>;
