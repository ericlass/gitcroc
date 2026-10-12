// The history each workspace's pane shows, as the core pages it (ARCH-8,
// PERF-2), and where the user was in each tree selection's history, all
// keyed by workspace id (ARCH-9). The core walks, lays out lanes and filters
// (ARCH-6, UI-12); this store only asks for pages and keeps them.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  type CommitRow,
  commitGraphDecline,
  commitGraphWrite,
  core,
  historyGet,
  type HistoryTarget,
  type HistoryView,
  historyMore,
  type RepoId,
  type TreeItem,
  type WorkspaceId,
} from "../ipc";
import type { SelectionStore } from "./selection";

/** The id of the "Uncommitted changes" row; commit rows use their hash. */
export const UNCOMMITTED_ID = "uncommitted";

/** What a tree selection shows in the history pane (section 8). */
export interface HistorySource {
  /** Positions are kept under it. */
  key: string;
  repo: RepoId;
  target: HistoryTarget;
  /** The "Uncommitted changes" row may lead the list (UI-10). */
  uncommitted: boolean;
}

/**
 * The history a tree node shows, or `null` for none (Ungrouped, nothing).
 * A repo and the nodes that only group its refs (Local, a folder, a remote
 * and so on) show the whole repo, under one key.
 */
export function sourceOf(nodeId: string | null, item: TreeItem | null): HistorySource | null {
  if (nodeId === null || item === null || item.kind === "ungrouped") {
    return null;
  }
  const repo = item.repo_id;
  switch (item.kind) {
    case "branch":
      return { key: nodeId, repo, target: { kind: "branch", name: item.name }, uncommitted: item.current };
    case "remote_branch":
      return { key: nodeId, repo, target: { kind: "remote_branch", name: item.name }, uncommitted: false };
    case "tag":
      return { key: nodeId, repo, target: { kind: "tag", name: item.name }, uncommitted: false };
    case "stash":
      return { key: nodeId, repo, target: { kind: "stash", index: item.index }, uncommitted: false };
    default:
      return { key: `repo:${repo}`, repo, target: { kind: "repo" }, uncommitted: true };
  }
}

/** Where the user was in one selection's history. */
export interface Position {
  scrollTop: number;
  /** The selected row's id. */
  selected: string | null;
  text: string;
  /** How many commit rows were loaded, to load as many again. */
  rows: number;
}

export interface WorkspaceHistory {
  source: HistorySource | null;
  /** The search text (UI-12). */
  text: string;
  rows: CommitRow[];
  /** The core's walk, for `history_more`. */
  handle: number | null;
  /** No page follows the loaded rows. */
  done: boolean;
  /** A page is being read. */
  loading: boolean;
  error: string | null;
  /** The selected row's id: a hash or UNCOMMITTED_ID. */
  selected: string | null;
  scrollTop: number;
  /** A scroll position for the list to apply once, after a restore. */
  restore: number | null;
  positions: Record<string, Position>;
  /** Repos whose commit-graph offer is shown. */
  offers: Record<RepoId, boolean | undefined>;
  /** Repos whose offer was answered this session; it is not shown again. */
  answered: Record<RepoId, boolean | undefined>;
  /** Why a repo's commit-graph write or answer failed. */
  offerErrors: Record<RepoId, string | undefined>;
}

const empty = (): WorkspaceHistory => ({
  source: null,
  text: "",
  rows: [],
  handle: null,
  done: true,
  loading: false,
  error: null,
  selected: null,
  scrollTop: 0,
  restore: null,
  positions: {},
  offers: {},
  answered: {},
  offerErrors: {},
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export function createHistoryStore(selection: SelectionStore) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceHistory> }>({ byWorkspace: {} });
  /** The latest request of each workspace; older answers are dropped. */
  const requests = new Map<WorkspaceId, number>();

  const history = (id: WorkspaceId): WorkspaceHistory => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (history: WorkspaceHistory) => void) => {
    setState(
      produce((draft) => {
        change((draft.byWorkspace[id] ??= empty()));
      }),
    );
  };

  const begin = (id: WorkspaceId) => {
    const request = (requests.get(id) ?? 0) + 1;
    requests.set(id, request);
    return request;
  };

  const isLatest = (id: WorkspaceId, request: number) => requests.get(id) === request;

  /** Tells the selection store what the selected row is. */
  const announce = (id: WorkspaceId) => {
    const [source, selected] = untrack(() => [history(id).source, history(id).selected] as const);
    if (source === null || selected === null) {
      selection.select(id, null);
    } else if (selected === UNCOMMITTED_ID) {
      selection.select(id, { kind: "uncommitted", repo_id: source.repo });
    } else {
      selection.select(id, { kind: "commit", repo_id: source.repo, hash: selected });
    }
  };

  const append = (id: WorkspaceId, view: HistoryView) => {
    update(id, (h) => {
      h.rows.push(...view.rows);
      h.handle = view.handle;
      h.done = view.done;
    });
  };

  /** Reads the next page; `false` when there was none to read or it was dropped. */
  const readMore = async (id: WorkspaceId, request: number): Promise<boolean> => {
    const [handle, done] = untrack(() => [history(id).handle, history(id).done] as const);
    if (handle === null || done) {
      return false;
    }
    update(id, (h) => (h.loading = true));
    try {
      const view = await historyMore(id, handle);
      if (view === null || !isLatest(id, request)) {
        return false;
      }
      append(id, view);
      return true;
    } catch (error) {
      if (isLatest(id, request)) {
        update(id, (h) => (h.error = message(error)));
      }
      return false;
    } finally {
      if (isLatest(id, request)) {
        update(id, (h) => (h.loading = false));
      }
    }
  };

  /**
   * Reads the first page of the shown source with the current text, then
   * more pages until `rows` commits are loaded, then asks the list to
   * scroll to `scrollTop`.
   */
  const load = async (id: WorkspaceId, rows: number, scrollTop: number) => {
    const request = begin(id);
    const [source, text] = untrack(() => [history(id).source, history(id).text] as const);
    if (source === null) {
      return;
    }
    update(id, (h) => {
      h.rows = [];
      h.handle = null;
      h.done = false;
      h.loading = true;
      h.error = null;
      h.restore = null;
    });
    try {
      const view = await historyGet(id, source.repo, source.target, text);
      if (view === null || !isLatest(id, request)) {
        return;
      }
      update(id, (h) => {
        h.rows = view.rows;
        h.handle = view.handle;
        h.done = view.done;
        h.loading = false;
        if (view.offer_commit_graph && !h.answered[source.repo]) {
          h.offers[source.repo] = true;
        }
      });
    } catch (error) {
      if (isLatest(id, request)) {
        update(id, (h) => {
          h.error = message(error);
          h.done = true;
          h.loading = false;
        });
      }
      return;
    }
    while (untrack(() => history(id).rows.length) < rows && (await readMore(id, request))) {
      // Each page brings the restored position closer.
    }
    if (isLatest(id, request)) {
      update(id, (h) => (h.restore = scrollTop));
    }
  };

  /** Shows `source`, keeping where the user was in the previous one. */
  const show = (id: WorkspaceId, source: HistorySource | null) => untrack(() => showSource(id, source));

  const showSource = (id: WorkspaceId, source: HistorySource | null) => {
    const current = history(id);
    if (current.source?.key === source?.key) {
      if (source !== null) {
        // The same history; only whether uncommitted changes apply may differ.
        update(id, (h) => (h.source = source));
      }
      return;
    }
    const saved = source === null ? undefined : current.positions[source.key];
    update(id, (h) => {
      if (h.source !== null) {
        h.positions[h.source.key] = {
          scrollTop: h.scrollTop,
          selected: h.selected,
          text: h.text,
          rows: h.rows.length,
        };
      }
      h.source = source;
      h.text = saved?.text ?? "";
      h.selected = saved?.selected ?? null;
      h.scrollTop = saved?.scrollTop ?? 0;
      h.rows = [];
      h.handle = null;
      h.done = true;
      h.error = null;
    });
    announce(id);
    if (source === null) {
      begin(id);
      update(id, (h) => (h.loading = false));
      return;
    }
    void load(id, saved?.rows ?? 0, saved?.scrollTop ?? 0);
  };

  /** Reads the next page, unless one is being read or none follows. */
  const more = async (id: WorkspaceId) => {
    if (untrack(() => history(id).loading || history(id).done)) {
      return;
    }
    await readMore(id, requests.get(id) ?? 0);
  };

  /** Searches the history (UI-12); each change supersedes the previous one. */
  const setText = (id: WorkspaceId, text: string) => {
    update(id, (h) => {
      h.text = text;
      h.scrollTop = 0;
    });
    return load(id, 0, 0);
  };

  const select = (id: WorkspaceId, rowId: string | null) => {
    update(id, (h) => (h.selected = rowId));
    announce(id);
  };

  const setScrollTop = (id: WorkspaceId, scrollTop: number) => {
    update(id, (h) => {
      h.scrollTop = scrollTop;
      h.restore = null;
    });
  };

  const answer = (id: WorkspaceId, repo: RepoId) => {
    update(id, (h) => {
      h.offers[repo] = false;
      h.answered[repo] = true;
      h.offerErrors[repo] = undefined;
    });
  };

  /** Accepts the commit-graph offer: the core writes the file as a job. */
  const writeCommitGraph = async (id: WorkspaceId, repo: RepoId) => {
    answer(id, repo);
    try {
      await commitGraphWrite(id, repo);
    } catch (error) {
      update(id, (h) => (h.offerErrors[repo] = `Cannot write the commit-graph file: ${message(error)}`));
    }
  };

  /** "Don't ask again": the core keeps the answer in the workspace. */
  const declineCommitGraph = async (id: WorkspaceId, repo: RepoId) => {
    answer(id, repo);
    try {
      await commitGraphDecline(id, repo);
    } catch (error) {
      update(id, (h) => (h.offerErrors[repo] = `Cannot keep the answer: ${message(error)}`));
    }
  };

  const dismissOfferError = (id: WorkspaceId, repo: RepoId) => {
    update(id, (h) => (h.offerErrors[repo] = undefined));
  };

  /** Reports failed commit-graph writes of workspace `id`. */
  const subscribe = async (id: WorkspaceId) =>
    core.events.commitGraphWritten.listen((event) => {
      const { workspace_id, repo_id, error, git_stderr } = event.payload;
      if (workspace_id === id && error !== null) {
        const detail = git_stderr ? `${error}\n${git_stderr}` : error;
        update(id, (h) => (h.offerErrors[repo_id] = `Cannot write the commit-graph file: ${detail}`));
      }
    });

  return {
    state,
    history,
    show,
    more,
    setText,
    select,
    setScrollTop,
    writeCommitGraph,
    declineCommitGraph,
    dismissOfferError,
    subscribe,
  };
}

export type HistoryStore = ReturnType<typeof createHistoryStore>;
