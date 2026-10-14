// Fetch, pull and push from the toolbar (section 6 "Sync", UI-20): which
// options dialog is open and why the last request was refused, per
// workspace (ARCH-9). The core refuses blocked repos (GRP-8), picks the
// remote or asks for one, and decides what needs a confirmation (GRP-9);
// this store only sends requests and shows what came back.
import { createStore, produce } from "solid-js/store";
import { opSync, type RepoId, type SummaryView, type SyncOp, type WorkspaceId } from "../ipc";
import { isBlocked } from "./summaries";

export type SyncDialog =
  | { kind: "pull"; repo: RepoId }
  /** `remotes` is set when the core asked which remote to push to. */
  | { kind: "push"; repo: RepoId; remotes: string[] | null }
  /** The core asked for the user's confirmation of `op` (GRP-9). */
  | {
      kind: "confirm";
      repo: RepoId;
      op: SyncOp;
      repoName: string;
      actions: string[];
    };

interface WorkspaceSync {
  dialog: SyncDialog | null;
  /** Why the last request was refused. */
  error: string | null;
  /** A request is on its way to the core. */
  sending: boolean;
}

const empty = (): WorkspaceSync => ({
  dialog: null,
  error: null,
  sending: false,
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** A repo a fetch may start on (GRP-8): not busy and not missing; conflict state is allowed. */
export const canFetch = (view: SummaryView | undefined) => !(view?.busy || view?.missing);

/** A repo a pull or push may start on: not blocked (GRP-8) and on a branch. */
export const canPullOrPush = (view: SummaryView | undefined) =>
  !isBlocked(view) && view?.summary?.head.kind !== "detached";

/** A click on Fetch, Pull or Push: the defaults of section 5. */
export const DEFAULT_OPS = {
  fetch: { kind: "fetch", prune: "config" },
  pull: { kind: "pull", strategy: "config" },
  push: {
    kind: "push",
    set_upstream: true,
    remote: null,
    force_with_lease: false,
  },
} as const satisfies Record<string, SyncOp>;

export function createSyncStore() {
  const [state, setState] = createStore<{
    byWorkspace: Record<WorkspaceId, WorkspaceSync>;
  }>({ byWorkspace: {} });

  const workspace = (id: WorkspaceId): WorkspaceSync => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (sync: WorkspaceSync) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const dialog = (id: WorkspaceId) => workspace(id).dialog;
  const error = (id: WorkspaceId) => workspace(id).error;
  const sending = (id: WorkspaceId) => workspace(id).sending;

  const open = (id: WorkspaceId, opened: SyncDialog) =>
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
   * Sends `op` on `repo`. Started, the dialog closes and the job's progress
   * and end arrive as events; a force push opens the confirmation the core
   * asked for, a push without a remote to go to opens the push options with
   * the remotes to pick from; refused, the reason is kept for the dialog or
   * the toolbar.
   */
  const run = async (id: WorkspaceId, repo: RepoId, op: SyncOp, confirmed = false) => {
    update(id, (s) => {
      s.error = null;
      s.sending = true;
    });
    try {
      const start = await opSync(id, { repo_id: repo, op, confirmed });
      update(id, (s) => {
        switch (start.kind) {
          case "started":
            s.dialog = null;
            break;
          case "confirm":
            s.dialog = {
              kind: "confirm",
              repo,
              op,
              repoName: start.repo,
              actions: start.actions,
            };
            break;
          case "choose_remote":
            s.dialog = { kind: "push", repo, remotes: start.remotes };
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

export type SyncStore = ReturnType<typeof createSyncStore>;
