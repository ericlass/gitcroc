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
 * The history a tree node shows, or `null` for none (Ungrouped, a group's
 * nodes, nothing). A repo and the nodes that only group its refs (Local, a
 * folder, a remote and so on) show the whole repo. Each history has one key
 * wherever the repo appears (UI-2), so both places show the same one.
 */
export function sourceOf(nodeId: string | null, item: TreeItem | null): HistorySource | null {
  if (nodeId === null || item === null || !("repo_id" in item)) {
    return null;
  }
  const repo = item.repo_id;
  switch (item.kind) {
    case "branch":
      return {
        key: `branch:${repo}:${item.name}`,
        repo,
        target: { kind: "branch", name: item.name },
        uncommitted: item.current,
      };
    case "remote_branch":
      return {
        key: `remote_branch:${repo}:${item.name}`,
        repo,
        target: { kind: "remote_branch", name: item.name },
        uncommitted: false,
      };
    case "tag":
      return { key: `tag:${repo}:${item.name}`, repo, target: { kind: "tag", name: item.name }, uncommitted: false };
    case "stash":
      return {
        key: `stash:${repo}:${item.index}`,
        repo,
        target: { kind: "stash", index: item.index },
        uncommitted: false,
      };
    default:
      return { key: `repo:${repo}`, repo, target: { kind: "repo" }, uncommitted: true };
  }
}

/**
 * The loaded commits of a history. The store keeps them as one value and
 * does not wrap them: rows never change once read, and the store would keep
 * a proxy and a signal per property of every row the list ever showed, for
 * as long as the history stays loaded (M1-14).
 */
export class Commits {
  constructor(readonly list: readonly CommitRow[]) {}

  get length(): number {
    return this.list.length;
  }
}

const NO_COMMITS = new Commits([]);

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
  rows: Commits;
  /** The core's walk, for `history_more`. */
  handle: number | null;
  /** No page follows the loaded rows. */
  done: boolean;
  /** A page is being read. */
  loading: boolean;
  error: string | null;
  /** The selected row's id: a hash or UNCOMMITTED_ID. */
  selected: string | null;
  /**
   * The commit rows selected with Ctrl/Cmd and Shift, for a cherry-pick
   * (UI-13): the selected row among them, or empty when it is not a commit.
   */
  marked: string[];
  scrollTop: number;
  /** A scroll position for the list to apply once, after a restore. */
  restore: number | null;
  /**
   * The tab went to the background and its rows were let go (UI-31); the
   * position is kept under the source's key and read again on return.
   */
  released: boolean;
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
  rows: NO_COMMITS,
  handle: null,
  done: true,
  loading: false,
  error: null,
  selected: null,
  marked: [],
  scrollTop: 0,
  restore: null,
  released: false,
  positions: {},
  offers: {},
  answered: {},
  offerErrors: {},
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The multi-selection of a single selected row. */
const only = (rowId: string | null) => (rowId === null || rowId === UNCOMMITTED_ID ? [] : [rowId]);

export function createHistoryStore(selection: SelectionStore) {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceHistory> }>({ byWorkspace: {} });
  /** The latest request of each workspace; older answers are dropped. */
  const requests = new Map<WorkspaceId, number>();
  /** The last `refs_version` heard of each repo, by workspace (OPS-7). */
  const refsVersions = new Map<WorkspaceId, Map<RepoId, number>>();
  /** The rows and scroll position a load is restoring, by workspace, until it is done. */
  const restoring = new Map<WorkspaceId, { rows: number; scrollTop: number }>();

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
      h.rows = new Commits([...h.rows.list, ...view.rows]);
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
      restoring.delete(id);
      return;
    }
    restoring.set(id, { rows, scrollTop });
    update(id, (h) => {
      h.rows = NO_COMMITS;
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
        h.rows = new Commits(view.rows);
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
      restoring.delete(id);
      update(id, (h) => (h.restore = scrollTop));
    }
  };

  /** Shows `source`, keeping where the user was in the previous one. */
  const show = (id: WorkspaceId, source: HistorySource | null) => untrack(() => showSource(id, source));

  const showSource = (id: WorkspaceId, source: HistorySource | null) => {
    const current = history(id);
    if (current.source?.key === source?.key && !current.released) {
      if (source !== null) {
        // The same history; only whether uncommitted changes apply may differ.
        update(id, (h) => (h.source = source));
      }
      return;
    }
    const saved = source === null ? undefined : current.positions[source.key];
    update(id, (h) => {
      // A released history kept its position when it let go of its rows.
      if (h.source !== null && !h.released) {
        h.positions[h.source.key] = {
          scrollTop: h.scrollTop,
          selected: h.selected,
          text: h.text,
          rows: h.rows.length,
        };
      }
      h.source = source;
      h.released = false;
      h.text = saved?.text ?? "";
      h.selected = saved?.selected ?? null;
      h.marked = only(h.selected);
      h.scrollTop = saved?.scrollTop ?? 0;
      h.rows = NO_COMMITS;
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

  /**
   * Reads the shown history again after its refs or HEAD changed (OPS-7),
   * keeping the scroll position and the selected commit while it still
   * exists. As many rows as were loaded are read again, and one page more
   * when the selected commit moved below them; the rows are replaced only
   * once all are read, so the list does not empty in between.
   */
  const reload = async (id: WorkspaceId) => {
    const [source, text, loaded, selected, released] = untrack(() => {
      const current = history(id);
      return [current.source, current.text, current.rows.length, current.selected, current.released] as const;
    });
    if (source === null || released) {
      return;
    }
    const request = begin(id);
    // A restore this supersedes still wants its rows and scroll position.
    const restore = restoring.get(id);
    const count = Math.max(loaded, restore?.rows ?? 0);
    try {
      const first = await historyGet(id, source.repo, source.target, text);
      if (first === null || !isLatest(id, request)) {
        return;
      }
      const rows = [...first.rows];
      let { handle, done } = first;
      const kept = () => selected === null || selected === UNCOMMITTED_ID || rows.some((row) => row.hash === selected);
      let extra = 1;
      while (!done && (rows.length < count || (!kept() && extra-- > 0))) {
        const page = await historyMore(id, handle);
        if (page === null || !isLatest(id, request)) {
          return;
        }
        rows.push(...page.rows);
        ({ handle, done } = page);
      }
      const gone = !kept();
      restoring.delete(id);
      update(id, (h) => {
        h.rows = new Commits(rows);
        h.handle = handle;
        h.done = done;
        h.loading = false;
        h.error = null;
        if (restore !== undefined) {
          h.restore = restore.scrollTop;
        }
        if (gone) {
          h.selected = null;
        }
        h.marked = h.marked.filter((hash) => rows.some((row) => row.hash === hash));
      });
      if (gone) {
        announce(id);
      }
    } catch (error) {
      if (isLatest(id, request)) {
        update(id, (h) => {
          h.error = message(error);
          h.loading = false;
        });
      }
    }
  };

  /**
   * Lets go of the loaded rows of workspace `id`, whose tab went to the
   * background (UI-31); the core has ended its walk. Where the user was is
   * kept, and `show` reads as many rows again and scrolls back.
   */
  const release = (id: WorkspaceId) => {
    begin(id);
    restoring.delete(id);
    update(id, (h) => {
      if (h.source !== null && !h.released) {
        h.positions[h.source.key] = {
          scrollTop: h.scrollTop,
          selected: h.selected,
          text: h.text,
          rows: h.rows.length,
        };
      }
      h.released = true;
      h.rows = NO_COMMITS;
      h.handle = null;
      h.done = true;
      h.loading = false;
      h.restore = null;
    });
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
    update(id, (h) => {
      h.selected = rowId;
      h.marked = only(rowId);
    });
    announce(id);
  };

  /** Selects row `rowId` with commits `hashes` selected for a cherry-pick (UI-13). */
  const mark = (id: WorkspaceId, rowId: string, hashes: string[]) => {
    update(id, (h) => {
      h.selected = rowId;
      h.marked = hashes;
    });
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

  /**
   * Reports failed commit-graph writes of workspace `id`, and reads the
   * shown history again when the core says its repo's refs changed (OPS-7).
   */
  const subscribe = async (id: WorkspaceId) => {
    const written = await core.events.commitGraphWritten.listen((event) => {
      const { workspace_id, repo_id, error, git_stderr } = event.payload;
      if (workspace_id === id && error !== null) {
        const detail = git_stderr ? `${error}\n${git_stderr}` : error;
        update(id, (h) => (h.offerErrors[repo_id] = `Cannot write the commit-graph file: ${detail}`));
      }
    });
    const summaries = await core.events.repoSummary.listen((event) => {
      const { workspace_id, repo_id, refreshing, refs_version } = event.payload;
      if (workspace_id !== id || refreshing) {
        return;
      }
      const versions = refsVersions.get(id) ?? new Map<RepoId, number>();
      refsVersions.set(id, versions);
      if (refs_version <= (versions.get(repo_id) ?? 0)) {
        return;
      }
      versions.set(repo_id, refs_version);
      if (untrack(() => history(id).source?.repo) === repo_id) {
        void reload(id);
      }
    });
    return () => {
      written();
      summaries();
    };
  };

  return {
    state,
    history,
    show,
    release,
    more,
    setText,
    select,
    mark,
    setScrollTop,
    writeCommitGraph,
    declineCommitGraph,
    dismissOfferError,
    subscribe,
  };
}

export type HistoryStore = ReturnType<typeof createHistoryStore>;
