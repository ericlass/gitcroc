// The tree of each workspace as the core reports it, and what the user did
// with it: expanded nodes, the selected node or repo rows (UI-6) and the
// filter text, all keyed by workspace id (ARCH-9, UI-29). The core builds, folds and filters the
// tree (ARCH-6); this store only asks for it and keeps the answers.
import { untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import {
  core,
  groupBranches,
  type GroupId,
  type GroupSummary,
  type RepoId,
  type RepoRef,
  refsGet,
  type TargetSet,
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
  /**
   * Loaded children by {@link loadKey}: a repo's ref nodes from `refs_get`
   * for each place it appears in, and a group's Branches node's children
   * from `group_branches`.
   */
  refs: Record<string, TreeNode[]>;
  /** What each entry of `refs` was loaded for, to load it again. */
  loadedFor: Record<string, TreeItem>;
  /** Why loading failed, by key. */
  refsErrors: Record<string, string | undefined>;
  /** Keys being loaded. */
  loading: Record<string, boolean | undefined>;
  /** Expanded or collapsed nodes by id; others use their default. */
  expanded: Record<string, boolean>;
  /** The same while a filter is shown; cleared when the text changes. */
  filterExpanded: Record<string, boolean>;
  /** The selected node; while repo rows are multi-selected, the row the keyboard is on. */
  selected: string | null;
  /** What the selected node is. */
  selectedItem: TreeItem | null;
  /**
   * The multi-selected repo rows (UI-6), each repo once; `null` for a
   * single selection. One row of it is like a single selection.
   */
  multi: SelectedRow[] | null;
  /** Where a Shift range starts. */
  anchor: string | null;
  /** A repo to select once the tree has its row, such as a clone that was just added. */
  pendingRepo: RepoId | null;
  filter: string;
  /** The core's answer to `filter`; `null` while the filter is empty. */
  filtered: TreeNode[] | null;
  /** Where the tree was scrolled, for when it is shown again (UI-29). */
  scrollTop: number;
  /**
   * Group rows the core sent since the last tree read, by group (UI-4);
   * newer than the summaries in the tree's group nodes.
   */
  groups: Record<GroupId, GroupSummary>;
}

/** A multi-selected repo row. */
export interface SelectedRow {
  row: string;
  item: Extract<TreeItem, { kind: "repo" }>;
}

const empty = (): WorkspaceTree => ({
  roots: null,
  error: null,
  refs: {},
  loadedFor: {},
  refsErrors: {},
  loading: {},
  expanded: {},
  filterExpanded: {},
  selected: null,
  selectedItem: null,
  multi: null,
  anchor: null,
  pendingRepo: null,
  filter: "",
  filtered: null,
  scrollTop: 0,
  groups: {},
});

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The repo an item belongs to; `null` for Ungrouped and a group's nodes. */
export function repoOfItem(item: TreeItem): RepoId | null {
  return "repo_id" in item ? item.repo_id : null;
}

/** The repo a node belongs to, if any. */
export function repoOf(node: TreeNode): RepoId | null {
  return repoOfItem(node.item);
}

/**
 * Where the loaded children of a node not loaded are kept: a repo's ref
 * nodes for the place the repo appears in (UI-2), or a group's Branches
 * node by its group. `null` for nodes that are never loaded.
 */
export function loadKey(item: TreeItem): string | null {
  switch (item.kind) {
    case "local":
    case "remotes":
    case "tags":
    case "stashes":
      return `refs:${item.group_id ?? ""}:${item.repo_id}`;
    case "group_branches":
      return `branches:${item.group_id}`;
    default:
      return null;
  }
}

const asRepo = (node: { id: string; item: TreeItem }): SelectedRow | null =>
  node.item.kind === "repo" ? { row: node.id, item: node.item } : null;

/** The node with id `id` in `nodes` or below. */
function findNode(nodes: TreeNode[], id: string): TreeNode | undefined {
  for (const node of nodes) {
    if (node.id === id) {
      return node;
    }
    const below = findNode(node.children ?? [], id);
    if (below !== undefined) {
      return below;
    }
  }
  return undefined;
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
        t.groups = {};
      });
      selectPending(id);
    } catch (error) {
      update(id, (t) => (t.error = message(error)));
    }
    if (tree(id).filter !== "") {
      await refilter(id);
    }
  };

  /** Loads the children kept under `item`'s {@link loadKey}. */
  const load = async (id: WorkspaceId, item: TreeItem) => {
    const key = loadKey(item);
    if (key === null) {
      return;
    }
    update(id, (t) => (t.loading[key] = true));
    try {
      const nodes =
        item.kind === "group_branches"
          ? await groupBranches(id, item.group_id)
          : await refsGet(id, repoOfItem(item) ?? "", "group_id" in item ? item.group_id : null);
      update(id, (t) => {
        t.refs[key] = nodes;
        t.loadedFor[key] = item;
        t.refsErrors[key] = undefined;
      });
    } catch (error) {
      update(id, (t) => (t.refsErrors[key] = message(error)));
    } finally {
      update(id, (t) => (t.loading[key] = false));
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
    // A repo's refs changed: its loaded places, and the loaded Branches
    // nodes of groups, which may count it, are read again.
    const onRefsChanged = (event: { payload: RepoRef }) => {
      const { workspace_id, repo_id } = event.payload;
      if (workspace_id !== id) {
        return;
      }
      const stale = untrack(() => Object.values(tree(id).loadedFor)).filter(
        (item) => item.kind === "group_branches" || repoOfItem(item) === repo_id,
      );
      if (stale.length > 0) {
        void Promise.all(stale.map((item) => load(id, item))).then(() => refilter(id));
      }
    };
    // A member's summary changed: the group's row follows (GRP-4).
    const onGroupSummary = (event: { payload: GroupSummary }) => {
      if (event.payload.workspace_id === id) {
        update(id, (t) => (t.groups[event.payload.group_id] = event.payload));
      }
    };
    const stopChanged = await core.events.workspaceChanged.listen(onWorkspaceChanged);
    const stopRefs = await core.events.repoRefsChanged.listen(onRefsChanged);
    const stopGroups = await core.events.groupSummary.listen(onGroupSummary);
    await reload(id);
    return () => {
      stopChanged();
      stopRefs();
      stopGroups();
    };
  };

  /**
   * Expands or collapses `node`. Expanding one of a repo's ref nodes the
   * first time loads that repo's refs, and with them ahead and behind;
   * expanding a group's Branches node loads its branch names (UI-5).
   */
  const setExpanded = (id: WorkspaceId, node: TreeNode, expanded: boolean) => {
    update(id, (t) => {
      (t.filtered === null ? t.expanded : t.filterExpanded)[node.id] = expanded;
    });
    const key = loadKey(node.item);
    const current = tree(id);
    if (expanded && key !== null && !current.refs[key] && !current.loading[key]) {
      void load(id, node.item);
    }
  };

  /** Puts the keyboard on `node`, with `multi` selected, and tells the core which repo it is in. */
  const place = (id: WorkspaceId, node: TreeNode | null, multi: SelectedRow[] | null, anchor: string | null) => {
    const nodeId = node?.id ?? null;
    const moved = tree(id).selected !== nodeId;
    update(id, (t) => {
      t.selected = nodeId;
      t.selectedItem = node?.item ?? null;
      t.multi = multi;
      t.anchor = anchor;
    });
    if (moved) {
      void repoSelect(id, node === null ? null : repoOf(node)).catch(() => {
        // Only the refresh order depends on it.
      });
    }
  };

  /** Selects `node` alone, replacing a multi-selection. */
  const select = (id: WorkspaceId, node: TreeNode | null) => {
    const current = tree(id);
    if (current.selected === (node?.id ?? null) && current.multi === null) {
      return;
    }
    place(id, node, null, node?.id ?? null);
  };

  /** The selected repo rows: the multi-selection, or the single selection when it is a repo row. */
  const pickedRows = (id: WorkspaceId): SelectedRow[] => {
    const current = tree(id);
    if (current.multi !== null) {
      return current.multi;
    }
    const single =
      current.selected === null || current.selectedItem === null
        ? null
        : asRepo({ id: current.selected, item: current.selectedItem });
    return single === null ? [] : [single];
  };

  /**
   * Ctrl/Cmd-click or Ctrl/Cmd+Space (UI-6): a repo row joins the
   * selection, or leaves it with every row of its repo, which counts once.
   * Any other row is selected alone.
   */
  const toggle = (id: WorkspaceId, node: TreeNode) => {
    const picked = asRepo(node);
    if (picked === null) {
      select(id, node);
      return;
    }
    const rows = pickedRows(id);
    const known = rows.some((row) => row.item.repo_id === picked.item.repo_id);
    const multi = known ? rows.filter((row) => row.item.repo_id !== picked.item.repo_id) : [...rows, picked];
    place(id, node, multi, node.id);
  };

  /**
   * Shift-click or Shift+Up/Down (UI-6): the repo rows of `visible` from the
   * anchor to `node`, each repo once. Any other row is selected alone.
   */
  const extend = (id: WorkspaceId, visible: { id: string; item?: TreeItem }[], node: TreeNode) => {
    if (asRepo(node) === null) {
      select(id, node);
      return;
    }
    const anchor = tree(id).anchor ?? tree(id).selected ?? node.id;
    const from = visible.findIndex((row) => row.id === anchor);
    const to = visible.findIndex((row) => row.id === node.id);
    if (from < 0 || to < 0) {
      // The anchor is no longer shown: the row is selected alone.
      place(id, node, null, node.id);
      return;
    }
    const multi: SelectedRow[] = [];
    for (const row of visible.slice(Math.min(from, to), Math.max(from, to) + 1)) {
      const picked = row.item === undefined ? null : asRepo({ id: row.id, item: row.item });
      if (picked !== null && !multi.some((known) => known.item.repo_id === picked.item.repo_id)) {
        multi.push(picked);
      }
    }
    place(id, node, multi, anchor);
  };

  /** Moves the keyboard to `node`, keeping the selected repo rows (Ctrl/Cmd+Up/Down). */
  const moveCursor = (id: WorkspaceId, node: TreeNode) => {
    place(id, node, pickedRows(id), tree(id).anchor);
  };

  /** Whether the row with id `row` is selected. */
  const isSelected = (id: WorkspaceId, row: string) => {
    const current = tree(id);
    return current.multi === null ? current.selected === row : current.multi.some((known) => known.row === row);
  };

  /** The one selected row: the single selection, or a multi-selection of one. */
  const single = (id: WorkspaceId): { row: string; item: TreeItem } | null => {
    const current = tree(id);
    if (current.multi !== null) {
      return current.multi.length === 1 ? (current.multi[0] ?? null) : null;
    }
    return current.selected === null || current.selectedItem === null
      ? null
      : { row: current.selected, item: current.selectedItem };
  };

  /**
   * What the toolbar acts on (GRP-5): the selected row's repo, the selected
   * group, or the multi-selected repos; `null` for anything else.
   */
  const target = (id: WorkspaceId): TargetSet | null => {
    const current = tree(id);
    if (current.multi !== null && current.multi.length > 1) {
      return { kind: "repos", repo_ids: current.multi.map((row) => row.item.repo_id) };
    }
    const item = single(id)?.item;
    if (item?.kind === "group") {
      return { kind: "group", group_id: item.group_id };
    }
    const repo = item === undefined ? null : repoOfItem(item);
    return repo === null ? null : { kind: "repo", repo_id: repo };
  };

  /** Selects the row with id `row` alone, when the tree has it. */
  const selectRow = (id: WorkspaceId, row: string) => {
    const node = findNode(tree(id).roots ?? [], row);
    if (node !== undefined) {
      select(id, node);
    }
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

  /** What the row of a group node with `summary` shows now. */
  const groupSummary = (id: WorkspaceId, summary: GroupSummary): GroupSummary =>
    tree(id).groups[summary.group_id] ?? summary;

  return {
    state,
    tree,
    subscribe,
    setExpanded,
    select,
    toggle,
    extend,
    moveCursor,
    isSelected,
    single,
    target,
    selectRow,
    selectRepo,
    setFilter,
    setScrollTop,
    groupSummary,
  };
}

export type TreeStore = ReturnType<typeof createTreeStore>;
