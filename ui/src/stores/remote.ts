// Remote add, rename, remove and URL edits from the tree (section 6
// "Remotes", UI-7): which dialog is open and why the last request was
// refused, per workspace (ARCH-9). The core checks every name (SEC-4) and
// URL, refuses blocked repos (GRP-8) and names the upstreams a remove
// unsets; this store only sends requests and shows what came back.
import { createStore, produce } from "solid-js/store";
import { opRemote, type RemoteOp, type RemoteUrlsView, remoteUrls, type RepoId, type WorkspaceId } from "../ipc";

export type RemoteDialog =
  | { kind: "add"; repo: RepoId }
  | { kind: "rename"; repo: RepoId; name: string }
  /** `urls` is `null` while the core reads them. */
  | { kind: "url"; repo: RepoId; name: string; urls: RemoteUrlsView | null }
  /** The core asked for the user's confirmation of `op`. */
  | { kind: "confirm"; repo: RepoId; op: RemoteOp; repoName: string; actions: string[] };

interface WorkspaceRemote {
  dialog: RemoteDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceRemote => ({ dialog: null, error: null, sending: false });

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createRemoteStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceRemote> }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceRemote => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (remote: WorkspaceRemote) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId, opened: RemoteDialog) =>
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

  /** Opens the URL dialog of remote `name`, its URLs read by the core; a failed read stays as the dialog's error. */
  const openUrl = async (id: WorkspaceId, repo: RepoId, name: string) => {
    open(id, { kind: "url", repo, name, urls: null });
    try {
      const urls = await remoteUrls(id, repo, name);
      update(id, (s) => {
        if (s.dialog?.kind === "url" && s.dialog.repo === repo && s.dialog.name === name) {
          s.dialog.urls = urls;
        }
      });
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    }
  };

  /**
   * Sends `op` on `repo`. Started, the dialog closes and the job's end
   * arrives as `job_ended`; a confirmation opens the dialog the core asked
   * for, which sends `op` again; refused, the reason is kept for the dialog
   * or the tree.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: RemoteOp, confirmed = false) => {
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      const start = await opRemote(id, { repo_id: repo, op, confirmed });
      update(id, (s) => {
        s.dialog =
          start.kind === "started"
            ? null
            : { kind: "confirm", repo, op, repoName: start.repo, actions: start.actions };
      });
    } catch (failure) {
      update(id, (s) => (s.error = message(failure)));
    } finally {
      update(id, (s) => (s.sending = false));
    }
  };

  return { state, dialog, error, sending, open, openUrl, close, dismissError, run };
}

export type RemoteStore = ReturnType<typeof createRemoteStore>;
