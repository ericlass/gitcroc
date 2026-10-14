// Branch operations from the tree and the toolbar (section 6 "Branches",
// UI-7, UI-20): which dialog is open and why the last request was refused,
// per workspace (ARCH-9). The core checks names (SEC-4), refuses blocked
// repos (GRP-8) and decides what needs a confirmation (GRP-9); this store
// only sends requests and shows what came back.
import { createStore, produce } from "solid-js/store";
import { type BranchOp, opBranch, type RepoId, type StartPoint, type WorkspaceId } from "../ipc";

export type BranchDialog =
  | { kind: "create"; repo: RepoId; start: StartPoint }
  | { kind: "rename"; repo: RepoId; name: string }
  | { kind: "delete"; repo: RepoId; name: string; tracked: boolean }
  | { kind: "upstream"; repo: RepoId; name: string; upstream: string | null }
  /** The core asked for the user's confirmation of `op` (GRP-9). */
  | { kind: "confirm"; repo: RepoId; op: BranchOp; repoName: string; actions: string[] };

interface WorkspaceBranch {
  dialog: BranchDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceBranch => ({ dialog: null, error: null, sending: false });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createBranchStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceBranch> }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceBranch => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (branch: WorkspaceBranch) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId, opened: BranchDialog) =>
    update(id, (b) => {
      b.dialog = opened;
      b.error = null;
    });

  const close = (id: WorkspaceId) =>
    update(id, (b) => {
      b.dialog = null;
      b.error = null;
    });

  const dismissError = (id: WorkspaceId) => update(id, (b) => (b.error = null));

  /**
   * Sends `op` on `repo`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a destructive one opens the confirmation the
   * core asked for; refused, the reason is kept for the dialog or the tree.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: BranchOp, confirmed = false) => {
    update(id, (b) => {
      b.error = null;
      b.sending = true;
    });
    try {
      const start = await opBranch(id, { repo_id: repo, op, confirmed });
      update(id, (b) => {
        b.dialog =
          start.kind === "confirm"
            ? { kind: "confirm", repo, op, repoName: start.repo, actions: start.actions }
            : null;
      });
    } catch (failure) {
      update(id, (b) => (b.error = message(failure)));
    } finally {
      update(id, (b) => (b.sending = false));
    }
  };

  return { state, dialog, error, sending, open, close, dismissError, run };
}

export type BranchStore = ReturnType<typeof createBranchStore>;
