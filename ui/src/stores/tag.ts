// Tag create, delete and push from the tree and the history (section 6
// "Tags", UI-7, UI-13): which dialog is open and why the last request was
// refused, per workspace (ARCH-9). The core checks every name (SEC-4),
// refuses blocked repos (GRP-8), picks the remote or asks for one, and
// decides what needs a confirmation; this store only sends requests and
// shows what came back.
import { createStore, produce } from "solid-js/store";
import { opTag, type RepoId, type TagOp, type WorkspaceId } from "../ipc";

export type TagDialog =
  /** A new tag at `commit`, or at `HEAD` when `null`; `at` names where for the title. */
  | { kind: "create"; repo: RepoId; commit: string | null; at: string }
  | { kind: "delete"; repo: RepoId; name: string }
  /** The core asked which remote `op` goes to. */
  | { kind: "remote"; repo: RepoId; op: TagOp; remotes: string[] }
  /** The core asked for the user's confirmation of `op`. */
  | { kind: "confirm"; repo: RepoId; op: TagOp; repoName: string; actions: string[] };

interface WorkspaceTag {
  dialog: TagDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceTag => ({ dialog: null, error: null, sending: false });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createTagStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceTag> }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceTag => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (tag: WorkspaceTag) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId, opened: TagDialog) =>
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

  /**
   * Sends `op` on `repo`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a remote to pick or a confirmation opens the
   * dialog the core asked for, which sends `op` again; refused, the reason
   * is kept for the dialog or the tree.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: TagOp, confirmed = false) => {
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      const start = await opTag(id, { repo_id: repo, op, confirmed });
      update(id, (s) => {
        switch (start.kind) {
          case "started":
            s.dialog = null;
            break;
          case "confirm":
            s.dialog = { kind: "confirm", repo, op, repoName: start.repo, actions: start.actions };
            break;
          case "choose_remote":
            s.dialog = { kind: "remote", repo, op, remotes: start.remotes };
            break;
        }
      });
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    } finally {
      update(id, (s) => (s.sending = false));
    }
  };

  return { state, dialog, error, sending, open, close, dismissError, run };
}

export type TagStore = ReturnType<typeof createTagStore>;
