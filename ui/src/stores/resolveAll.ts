// Resolve all with AI from the conflict banner (AI-3, AI-13), keyed by
// workspace and repo (ARCH-9): the request on its way, why it was refused,
// and the job it started, whose view carries each file's progress. The core
// reads every unmerged file and decides what is sent (AI-4); this store
// names the files the editor has a buffer for, with their untouched hunks
// (OPS-9). Files are named by the changes view that lists them (PLAT-2), so
// the repo's uncommitted changes are shown first when they are not. A run
// keeps going in a background tab (UI-30); its proposals are the core's and
// are read when a file is shown. Nothing is accepted for the user (D9).
import { createEffect, createRoot, on, untrack } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import {
  aiResolveAll,
  type AiResolveAllFile,
  type AiResolveAllStart,
  type ChangesView,
  type JobId,
  type JobView,
  jobCancel,
  type RepoId,
  type UnmergedFile,
  type WorkspaceId,
} from "../ipc";
import { untouchedHunks } from "../conflict/editor";
import type { ChangesStore } from "./changes";
import { type AiAsk, type ConflictStore, refusalText } from "./conflicts";
import { type HistoryStore, UNCOMMITTED_ID } from "./history";
import type { JobStore } from "./jobs";
import type { CommitSelection } from "./selection";

/** A repo's Resolve all. */
export interface RepoRun {
  /** A request is on its way, or waits for the repo's lists. */
  starting: boolean;
  /** Why the last request was refused or did not go out. */
  refusal: string | null;
  /** The job the last request started. */
  job: JobId | null;
}

const idle = (): RepoRun => ({ starting: false, refusal: null, job: null });

/** A changes view's list of a repo's unmerged files, and the handle that names them. */
interface Lists {
  handle: number;
  unmerged: UnmergedFile[];
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * The unmerged list of `repo` in `view` of `selection`: the whole view's,
 * or its part of a merged view (UI-34).
 */
function listsIn(view: ChangesView, selection: CommitSelection | null, repo: RepoId): Lists | null {
  const content = view.content;
  if (content.kind === "merged") {
    const part = content.repos.find((known) => known.repo_id === repo);
    return part === undefined ? null : { handle: part.handle, unmerged: part.unmerged };
  }
  return content.kind === "uncommitted" && selection?.kind === "uncommitted" && selection.repo_id === repo
    ? { handle: view.handle, unmerged: content.unmerged }
    : null;
}

export function createResolveAllStore(deps: {
  history: HistoryStore;
  changes: ChangesStore;
  conflicts: ConflictStore;
  jobs: JobStore;
  /** Called when the core says no file is in conflict any more, so the lists are read again. */
  onResolvedElsewhere: (id: WorkspaceId) => void;
}) {
  const [state, setState] = createStore<{
    byWorkspace: Record<WorkspaceId, Record<RepoId, RepoRun | undefined> | undefined>;
  }>({ byWorkspace: {} });

  const run = (id: WorkspaceId, repo: RepoId): RepoRun => state.byWorkspace[id]?.[repo] ?? idle();

  const update = (id: WorkspaceId, repo: RepoId, change: (run: RepoRun) => void) =>
    setState(
      produce((draft) => {
        const repos = (draft.byWorkspace[id] ??= {});
        change((repos[repo] ??= idle()));
      }),
    );

  /** The job of the repo's last run, as the core last reported it. */
  const view = (id: WorkspaceId, repo: RepoId): JobView | undefined => {
    const job = run(id, repo).job;
    return job === null ? undefined : deps.jobs.state.byId[job];
  };

  /** The repo's unmerged list when the changes pane shows it read. */
  const shownLists = (id: WorkspaceId, repo: RepoId): Lists | null => {
    const shown = deps.changes.changes(id);
    return shown.view === null || shown.loading ? null : listsIn(shown.view, shown.selection, repo);
  };

  /**
   * The repo's unmerged list, once the changes pane shows it: its
   * uncommitted changes are selected first when they are not shown.
   * `null` when the user selects something else meanwhile, or the read
   * fails.
   */
  const lists = (id: WorkspaceId, repo: RepoId): Promise<Lists | null> => {
    const now = untrack(() => shownLists(id, repo));
    if (now !== null) {
      return Promise.resolve(now);
    }
    if (untrack(() => deps.history.history(id).source?.repo) === repo) {
      deps.history.select(id, UNCOMMITTED_ID);
    }
    return new Promise((resolve) =>
      createRoot((dispose) => {
        createEffect(
          on(
            () => [shownLists(id, repo), deps.changes.changes(id)] as const,
            ([found, shown]) => {
              const selection = shown.selection;
              const elsewhere =
                selection === null ||
                (selection.kind === "uncommitted" && selection.repo_id !== repo) ||
                selection.kind === "commit" ||
                (shown.error !== null && !shown.loading);
              if (found !== null || elsewhere) {
                dispose();
                resolve(found);
              }
            },
          ),
        );
      }),
    );
  };

  /** The files with an editor buffer and their untouched hunks, counted from 1 (OPS-9). */
  const buffers = (id: WorkspaceId, repo: RepoId, unmerged: UnmergedFile[]): AiResolveAllFile[] =>
    unmerged.flatMap((file) => {
      const kept = deps.conflicts.editor(id, repo, file.path);
      return kept === undefined ? [] : [{ file: file.id, hunks: untouchedHunks(kept.state) }];
    });

  /** What the core answered: a job, or a refusal, which asks for consent and sends again once (AI-6). */
  const settle = async (id: WorkspaceId, repo: RepoId, start: AiResolveAllStart | null, ask: AiAsk) => {
    if (start === null) {
      // A newer view replaced the request's; the user can send again.
      update(id, repo, (r) => (r.starting = false));
      return;
    }
    switch (start.kind) {
      case "started":
        update(id, repo, (r) => {
          r.starting = false;
          r.job = start.job_id;
        });
        return;
      case "already_resolved":
        update(id, repo, (r) => (r.starting = false));
        deps.onResolvedElsewhere(id);
        return;
      case "refused": {
        const refused = start.refused;
        update(id, repo, (r) => (r.starting = false));
        if (refused.kind === "no_provider") {
          ask.openSettings();
        } else if (refused.kind !== "consent_needed") {
          update(id, repo, (r) => (r.refusal = refusalText(refused)));
        }
      }
    }
  };

  /**
   * Resolve all with AI for `repo` (AI-3): every unmerged file goes, but
   * those the core finds ineligible and those whose buffer has no hunk left
   * untouched. Opens the provider settings when there is no provider, and
   * asks for consent before the run (AI-6).
   */
  const start = async (id: WorkspaceId, repo: RepoId, ask: AiAsk) => {
    update(id, repo, (r) => {
      r.starting = true;
      r.refusal = null;
    });
    try {
      const found = await lists(id, repo);
      if (found === null) {
        update(id, repo, (r) => (r.starting = false));
        return;
      }
      const request = () =>
        aiResolveAll(id, { repo, handle: found.handle, files: buffers(id, repo, found.unmerged) });
      let answer = await request();
      if (answer?.kind === "refused" && answer.refused.kind === "consent_needed") {
        answer = (await ask.askConsent(answer.refused.consent.provider_id)) ? await request() : null;
      }
      await settle(id, repo, answer, ask);
    } catch (failure) {
      update(id, repo, (r) => {
        r.starting = false;
        r.refusal = message(failure);
      });
    }
  };

  /** Cancels the repo's run: waiting files never go, running requests are dropped (AI-13). */
  const cancel = (id: WorkspaceId, repo: RepoId) => {
    const job = untrack(() => run(id, repo).job);
    if (job !== null) {
      jobCancel(job).catch((failure: unknown) => update(id, repo, (r) => (r.refusal = message(failure))));
    }
  };

  /** Opens `path` of `repo` in the conflict editor, which reads its proposal (AI-13). */
  const open = async (id: WorkspaceId, repo: RepoId, path: string) => {
    const found = await lists(id, repo);
    const file = found?.unmerged.find((known) => known.path === path);
    const shown = untrack(() => deps.changes.changes(id).view);
    if (file === undefined || shown === null) {
      return;
    }
    const key = shown.content.kind === "merged" ? { list: "unmerged" as const, id: file.id, repo } : null;
    await deps.changes.selectFile(id, key ?? { list: "unmerged", id: file.id });
  };

  /** Forgets every run: AI features were turned off, and their proposals are gone (AI-16). */
  const dropAll = () => setState("byWorkspace", reconcile({}));

  /** Forgets workspace `id`'s runs: its tab was closed. */
  const forgetWorkspace = (id: WorkspaceId) => setState("byWorkspace", id, undefined);

  return { state, run, view, start, cancel, open, dropAll, forgetWorkspace };
}

export type ResolveAllStore = ReturnType<typeof createResolveAllStore>;
