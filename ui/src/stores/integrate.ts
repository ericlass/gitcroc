// Merge, rebase, cherry-pick and revert from the toolbar and the history,
// and Abort and Continue from the conflict banner (section 6 "Integrate",
// OPS-2, UI-13, UI-20): which dialog is open, why the last request was
// refused and what the banner names, per workspace (ARCH-9). The core
// checks every request, sorts a cherry-pick's commits and reads the
// conflict state from the repo (OPS-1); this store only sends requests and
// shows what came back.
import { createStore, produce } from "solid-js/store";
import {
  type ConflictDetails,
  conflictDetails,
  type IntegrateOp,
  opIntegrate,
  type RepoId,
  type WorkspaceId,
} from "../ipc";

export type IntegrateDialog =
  | { kind: "merge"; repo: RepoId }
  /** The core asked which parent of merge commit `commit` a revert keeps. */
  | { kind: "mainline"; repo: RepoId; commit: string; parents: string[] };

interface WorkspaceIntegrate {
  dialog: IntegrateDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
  /** What the banner names, by repo, as last read. */
  details: Record<RepoId, ConflictDetails | undefined>;
  /** Why the banner's last read failed, by repo. */
  detailErrors: Record<RepoId, string | undefined>;
}

const empty = (): WorkspaceIntegrate => ({
  dialog: null,
  error: null,
  sending: false,
  details: {},
  detailErrors: {},
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createIntegrateStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceIntegrate> }>({
    byWorkspace: {},
  });
  /** The latest details read of each repo; older answers are dropped. */
  const reads = new Map<string, number>();

  const workspace = (id: WorkspaceId): WorkspaceIntegrate => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (integrate: WorkspaceIntegrate) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;
  const details = (id: WorkspaceId, repo: RepoId) => workspace(id).details[repo];
  const detailError = (id: WorkspaceId, repo: RepoId) => workspace(id).detailErrors[repo];

  const open = (id: WorkspaceId, opened: IntegrateDialog) =>
    update(id, (i) => {
      i.dialog = opened;
      i.error = null;
    });

  const close = (id: WorkspaceId) =>
    update(id, (i) => {
      i.dialog = null;
      i.error = null;
    });

  const dismissError = (id: WorkspaceId) => update(id, (i) => (i.error = null));

  /**
   * Sends `op` on `repo`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a revert of a merge commit opens the choice of
   * mainline the core asked for; refused, the reason is kept for the dialog
   * or the history pane.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: IntegrateOp) => {
    update(id, (i) => {
      i.error = null;
      i.sending = true;
    });
    try {
      const start = await opIntegrate(id, { repo_id: repo, op });
      update(id, (i) => {
        i.dialog =
          start.kind === "choose_mainline"
            ? { kind: "mainline", repo, commit: start.commit, parents: start.parents }
            : null;
      });
    } catch (failure) {
      update(id, (i) => (i.error = message(failure)));
    } finally {
      update(id, (i) => (i.sending = false));
    }
  };

  /** Reads what the banner names for `repo` (OPS-2); the latest read wins. */
  const loadDetails = async (id: WorkspaceId, repo: RepoId) => {
    const key = `${id}\n${repo}`;
    const read = (reads.get(key) ?? 0) + 1;
    reads.set(key, read);
    try {
      const found = await conflictDetails(id, repo);
      if (reads.get(key) === read) {
        update(id, (i) => {
          i.details[repo] = found;
          i.detailErrors[repo] = undefined;
        });
      }
    } catch (failure) {
      if (reads.get(key) === read) {
        update(id, (i) => (i.detailErrors[repo] = message(failure)));
      }
    }
  };

  return { state, dialog, error, sending, details, detailError, open, close, dismissError, run, loadDetails };
}

export type IntegrateStore = ReturnType<typeof createIntegrateStore>;
