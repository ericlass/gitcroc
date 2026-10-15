// The tree of each workspace as the core reports it, and what the user did
// with it: expanded nodes, the selected node and the filter text, all keyed
// by workspace id (ARCH-9, UI-29). The core builds, folds and filters the
// tree (ARCH-6); this store only asks for it and keeps the answers.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  core,
  type RepoId,
  type RepoRef,
  refsGet,
  repoSelect,
  type TreeItem,
  type TreeNode,
  treeFilter,
  treeGet,
  type WorkspaceId,
} from "../ipc";

export interface WorkspaceTree {
  /** The skeleton from `tree_get`; `null` until the first read. */
  roots: TreeNode[] | null;
  /** Why the last `tree_get` failed. */
  error: string | null;
  /** Each repo's loaded ref nodes from `refs_get`. */
  refs: Record<RepoId, TreeNode[]>;
  /** Why a repo's refs could not be loaded. */
  refsErrors: Record<RepoId, string | undefined>;
  /** Repos whose refs are being loaded. */
  loading: Record<RepoId, boolean | undefined>;
  /** Expanded or collapsed nodes by id; others use their default. */
  expanded: Record<string, boolean>;
  /** The same while a filter is shown; cleared when the text changes. */
  filterExpanded: Record<string, boolean>;
  selected: string | null;
  /** What the selected node is; the history pane follows it. */
  selectedItem: TreeItem | null;
  /** A repo to select once the tree has its row, such as a clone that was just added. */
  pendingRepo: RepoId | null;
  filter: string;
  /** The core's answer to `filter`; `null` while the filter is empty. */
  filtered: TreeNode[] | null;
  /** Where the tree was scrolled, for when it is shown again (UI-29). */
  scrollTop: number;
}

const empty = (): WorkspaceTree => ({
  roots: null,
  error: null,
  refs: {},
  refsErrors: {},
  loading: {},
  expanded: {},
  filterExpanded: {},
  selected: null,
  selectedItem: null,
  pendingRepo: null,
  filter: "",
  filtered: null,
  scrollTop: 0,
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The repo a node belongs to, if any. */
export function repoOf(node: TreeNode): RepoId | null {
  return node.item.kind === "ungrouped" ? null : node.item.repo_id;
}

/** The row of `repo` in `nodes` or below. */
function findRepo(nodes: TreeNode[], repo: RepoId): TreeNode | undefined {
  for (const node of nodes) {
    if (node.item.kind === "repo" && node.item.repo_id === repo) {
      return node;
    }
    const below = findRepo(node.children ?? [], repo);
    if (below !== undefined) {
      return below;
    }
  }
  return undefined;
}

/** A repo's Local, Remotes, Tags and Stashes node. */
export function isRefsNode(node: TreeNode): boolean {
  const kind = node.item.kind;
  return kind === "local" || kind === "remotes" || kind === "tags" || kind === "stashes";
}

export function createTreeStore() {
  const [state, setState] = createStore<{ byWorkspace: Record<WorkspaceId, WorkspaceTree> }>({ byWorkspace: {} });

  const tree = (id: WorkspaceId): WorkspaceTree => state.byWorkspace[id] ?? empty();

  const update = (id: WorkspaceId, change: (tree: WorkspaceTree) => void) => {
    setState(
      produce((draft) => {
        const current = (draft.byWorkspace[id] ??= empty());
        change(current);
      }),
    );
  };

  /** Asks the core again for the current filter text's tree. */
  const refilter = async (id: WorkspaceId) => {
    const text = tree(id).filter;
    if (text === "") {
      update(id, (t) => (t.filtered = null));
      return;
    }
    try {
      const result = await treeFilter(id, text);
      // `null`: a newer request superseded this one. A changed text means
      // a newer request is on its way.
      if (result !== null && tree(id).filter === text) {
        update(id, (t) => (t.filtered = result));
      }
    } catch (error) {
      if (tree(id).filter === text) {
        update(id, (t) => (t.error = message(error)));
      }
    }
  };

  const reload = async (id: WorkspaceId) => {
    try {
      const roots = await treeGet(id);
      update(id, (t) => {
        t.roots = roots;
        t.error = null;
      });
      selectPending(id);
    } catch (error) {
      update(id, (t) => (t.error = message(error)));
    }
    if (tree(id).filter !== "") {
      await refilter(id);
    }
  };

  const loadRefs = async (id: WorkspaceId, repo: RepoId) => {
    update(id, (t) => (t.loading[repo] = true));
    try {
      const refs = await refsGet(id, repo);
      update(id, (t) => {
        t.refs[repo] = refs;
        t.refsErrors[repo] = undefined;
      });
    } catch (error) {
      update(id, (t) => (t.refsErrors[repo] = message(error)));
    } finally {
      update(id, (t) => (t.loading[repo] = false));
    }
  };

  /**
   * Reads the tree of `id` and keeps it current: again after each
   * `workspace_changed`, and a repo's loaded refs again after its
   * `repo_refs_changed`. Resolves to the unsubscribe function.
   */
  const subscribe = async (id: WorkspaceId) => {
    const onWorkspaceChanged = (event: { payload: { id: WorkspaceId } }) => {
      if (event.payload.id === id) {
        void reload(id);
      }
    };
    const onRefsChanged = (event: { payload: RepoRef }) => {
      const { workspace_id, repo_id } = event.payload;
      if (workspace_id === id && untrack(() => tree(id).refs[repo_id])) {
        void loadRefs(id, repo_id).then(() => refilter(id));
      }
    };
    const stopChanged = await core.events.workspaceChanged.listen(onWorkspaceChanged);
    const stopRefs = await core.events.repoRefsChanged.listen(onRefsChanged);
    await reload(id);
    return () => {
      stopChanged();
      stopRefs();
    };
  };

  /**
   * Expands or collapses `node`. Expanding one of a repo's ref nodes the
   * first time loads that repo's refs, and with them ahead and behind.
   */
  const setExpanded = (id: WorkspaceId, node: TreeNode, expanded: boolean) => {
    update(id, (t) => {
      (t.filtered === null ? t.expanded : t.filterExpanded)[node.id] = expanded;
    });
    const repo = repoOf(node);
    const current = tree(id);
    if (expanded && repo !== null && isRefsNode(node) && !current.refs[repo] && !current.loading[repo]) {
      void loadRefs(id, repo);
    }
  };

  /** Selects `node` and tells the core which repo it is in. */
  const select = (id: WorkspaceId, node: TreeNode | null) => {
    const nodeId = node?.id ?? null;
    if (tree(id).selected === nodeId) {
      return;
    }
    update(id, (t) => {
      t.selected = nodeId;
      t.selectedItem = node?.item ?? null;
    });
    void repoSelect(id, node === null ? null : repoOf(node)).catch(() => {
      // Only the refresh order depends on it.
    });
  };

  /** Selects the row of the pending repo, once the tree has it. */
  const selectPending = (id: WorkspaceId) => {
    const repo = tree(id).pendingRepo;
    const node = repo === null ? undefined : findRepo(tree(id).roots ?? [], repo);
    if (node !== undefined) {
      update(id, (t) => (t.pendingRepo = null));
      select(id, node);
    }
  };

  /** Selects the row of `repo`, now or when the tree next has it. */
  const selectRepo = (id: WorkspaceId, repo: RepoId) => {
    update(id, (t) => (t.pendingRepo = repo));
    selectPending(id);
  };

  /** Filters the tree; each change supersedes the previous one (UI-8). */
  const setFilter = (id: WorkspaceId, text: string) => {
    update(id, (t) => {
      t.filter = text;
      t.filterExpanded = {};
    });
    return refilter(id);
  };

  const setScrollTop = (id: WorkspaceId, scrollTop: number) => update(id, (t) => (t.scrollTop = scrollTop));

  return { state, tree, subscribe, setExpanded, select, selectRepo, setFilter, setScrollTop };
}

export type TreeStore = ReturnType<typeof createTreeStore>;
