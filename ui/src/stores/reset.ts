// Reset of the current branch to a commit from the history (section 6
// "Reset", UI-13): which dialog is open and why the last request was
// refused, per workspace (ARCH-9). The core refuses blocked repos (GRP-8)
// and asks for a confirmation before a hard reset, naming what it loses
// (GRP-9); this store only sends requests and shows what came back.
import { createStore, produce } from "solid-js/store";
import { opReset, type RepoId, type ResetMode, type WorkspaceId } from "../ipc";

export type ResetDialog =
  /** `commit` as the history shows it (`text`); `branch` is `null` when `HEAD` is detached. */
  | { kind: "reset"; repo: RepoId; commit: string; text: string; branch: string | null }
  /** The core asked for the user's confirmation of a hard reset (GRP-9). */
  | { kind: "confirm"; repo: RepoId; commit: string; mode: ResetMode; repoName: string; actions: string[] };

interface WorkspaceReset {
  dialog: ResetDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceReset => ({ dialog: null, error: null, sending: false });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createResetStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceReset> }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceReset => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (reset: WorkspaceReset) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId, opened: ResetDialog) =>
    update(id, (s) => {
      s.dialog = opened;
      s.error = null;
    });

  const close = (id: WorkspaceId) =>
    update(id, (s) => {
      s.dialog = null;
      s.error = null;
    });

  /**
   * Resets `repo` to `commit`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a hard reset opens the confirmation the core
   * asked for; refused, the reason is kept for the dialog.
   */
  const run = async (id: WorkspaceId, repo: RepoId, commit: string, mode: ResetMode, confirmed = false) => {
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      const start = await opReset(id, { repo_id: repo, commit, mode, confirmed });
      update(id, (s) => {
        s.dialog =
          start.kind === "confirm"
            ? { kind: "confirm", repo, commit, mode, repoName: start.repo, actions: start.actions }
            : null;
      });
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    } finally {
      update(id, (s) => (s.sending = false));
    }
  };

  return { state, dialog, error, sending, open, close, run };
}

export type ResetStore = ReturnType<typeof createResetStore>;
