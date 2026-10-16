// The tree pane (UI-1, UI-3, UI-7, UI-8): filter box, add menu (add a
// folder, or clone) and the tree, virtualized (PERF-1), with a context menu
// on each row that has operations and double-click to check out a local
// branch; a repo row's menu also opens the repo in the file manager or a
// terminal, a stash row's applies, pops or drops it, a tag row's pushes or
// deletes it, the Remotes node's adds a remote and a remote row's renames,
// edits or removes it. Group rows (UI-4) and their menus create, rename,
// delete and reorder groups, and repos are dragged between them (UI-7); every
// drag is also in a menu, and the context menu key opens a row's menu
// (UI-22). Repo rows are multi-selected with Ctrl/Cmd and Shift (UI-6).
// A name in a group's Branches node is checked out on the group by
// double-click (UI-5), and group rows' menus run the group operations.
// Everything shown comes from the stores, which take it from the
// core; this component only lays it out and forwards input.
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch, untrack } from "solid-js";
import {
  type GroupId,
  type RepoId,
  type TargetSet,
  repoOpenFileManager,
  repoOpenTerminal,
  type TreeItem,
  type TreeNode,
  type WorkspaceId,
  workspacePickAndAdd,
  workspaceRemoveRepo,
} from "../ipc";
import type { BranchStore } from "../stores/branch";
import type { CloneStore } from "../stores/clone";
import type { ShareStore } from "../stores/share";
import { DEFAULT_GROUP_OPS, type GroupNameDialog, type GroupOpStore } from "../stores/groupOps";
import type { StashStore } from "../stores/stash";
import type { RemoteStore } from "../stores/remote";
import type { TagStore } from "../stores/tag";
import { isBlocked, type SummaryStore } from "../stores/summaries";
import type { TreeStore } from "../stores/tree";
import type { WorkspaceStore } from "../stores/workspaces";
import GroupDialogs, { type GroupDialog } from "./GroupDialogs";
import GroupRow from "./GroupRow";
import {
  type Dragged,
  draggedOf,
  dropOp,
  type DropPlace,
  dropPlace,
  type GroupInfo,
  type GroupOp,
  groupsOf,
  moveGroup,
  moveMember,
  runGroupOp,
} from "./groups";
import { type MenuAction, type MenuEntry, menuFor } from "./menus";
import RepoRow, { AheadBehind } from "./RepoRow";
import { flatten, type Row } from "./rows";

/** Height of one row in CSS pixels; `.tree-row` in App.css sets it, as `.history-row` does. */
export const ROW_HEIGHT = 22;

/** Rows rendered beyond each edge of the visible ones. */
export const OVERSCAN = 8;

const INDENT = 14;

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The accessible name of each kind of row's menu. */
const MENU_NAMES: Partial<Record<TreeItem["kind"], string>> = {
  repo: "Repository",
  branch: "Branch",
  remote_branch: "Remote branch",
  tag: "Tag",
  stash: "Stash",
  remotes: "Remotes",
  remote: "Remote",
  group: "Group",
  ungrouped: "Ungrouped",
};

/** The group operation on a name X each menu action opens the dialog of (section 5). */
const NAME_DIALOGS = {
  group_checkout: "checkout",
  group_create: "create",
  group_delete_branch: "delete",
  group_merge: "merge",
  group_rebase: "rebase",
  group_tag: "tag",
} as const satisfies Partial<Record<MenuAction, GroupNameDialog["kind"]>>;

const asNode = (row: Row) => (row.kind === "node" ? row : undefined);
const asMessage = (row: Row) => (row.kind === "message" ? row : undefined);

/** What a branch, tag, stash or other non-repo row shows. */
function RefLabel(props: { node: TreeNode }) {
  const branch = () => {
    const item = props.node.item;
    return item.kind === "branch" ? item : undefined;
  };
  const stash = () => {
    const item = props.node.item;
    return item.kind === "stash" ? item : undefined;
  };
  const groupBranch = () => {
    const item = props.node.item;
    return item.kind === "group_branch" ? item : undefined;
  };
  return (
    <Show
      when={branch()}
      fallback={
        <>
          <span
            class="tree-label"
            title={stash() ? new Date((stash()?.seconds ?? 0) * 1000).toLocaleString() : undefined}
          >
            {props.node.label}
          </span>
          <Show when={groupBranch()}>
            {(name) => (
              <span class="tree-count" aria-label={`in ${name().present} of ${name().members} members`}>
                ({name().present}/{name().members})
              </span>
            )}
          </Show>
        </>
      }
    >
      {(b) => (
        <>
          <span class="tree-label" classList={{ current: b().current }} title={b().upstream ?? undefined}>
            {props.node.label}
          </span>
          <Show when={b().current}>
            <span class="tree-current" role="img" aria-label="Current branch" title="Current branch">
              ✓
            </span>
          </Show>
          <AheadBehind ahead={b().ahead} behind={b().behind} />
          <Show when={b().gone}>
            <span class="tree-gone" title="The upstream is gone">
              gone
            </span>
          </Show>
        </>
      )}
    </Show>
  );
}

function AddReportView(props: { workspaces: WorkspaceStore; workspace: WorkspaceId }) {
  const report = () => props.workspaces.state.reports[props.workspace];
  return (
    <Show when={report()}>
      {(report) => (
        <ul class="tree-report" aria-label="Add result">
          <Show when={report().added.length > 0}>
            <li>Added: {report().added.map((repo) => repo.name).join(", ")}</li>
          </Show>
          <Show when={report().already_present.length > 0}>
            <li>Already present: {report().already_present.map((repo) => repo.name).join(", ")}</li>
          </Show>
          <For each={report().refused}>
            {(refusal) => (
              <li class="tree-refused">
                Refused {refusal.path}: {refusal.reason}
              </li>
            )}
          </For>
          <Show when={report().added.length + report().already_present.length + report().refused.length === 0}>
            <li>No repos found.</li>
          </Show>
        </ul>
      )}
    </Show>
  );
}

/** The rows, virtualized (PERF-1), with mouse and keyboard selection. */
function TreeList(props: {
  workspace: WorkspaceId;
  tree: TreeStore;
  summaries: SummaryStore;
  /** The workspace's groups, from the unfiltered tree. */
  groups: GroupInfo[];
  onMenu: (item: TreeItem, x: number, y: number) => void;
  onCheckout: (repo: RepoId, name: string) => void;
  /** A double-click on branch `name` of group `group`'s Branches node (UI-5). */
  onGroupCheckout: (group: GroupId, name: string) => void;
  /** A drop changed the groups (UI-7). */
  onGroupOp: (op: GroupOp) => void;
  ref: (element: HTMLDivElement) => void;
}) {
  let scroller: HTMLDivElement | undefined;
  const [dragged, setDragged] = createSignal<{ row: string; what: Dragged } | null>(null);
  /** The row a drop would land on, and where on it. */
  const [target, setTarget] = createSignal<{ row: string; place: DropPlace } | null>(null);
  const tree = () => props.tree.tree(props.workspace);
  const rows = createMemo(() => flatten(tree()));
  const selectedIndex = () => rows().findIndex((row) => row.id === tree().selected);

  const virtualizer = createVirtualizer({
    get count() {
      return rows().length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });

  const select = (index: number) => {
    const row = rows()[index];
    if (row?.kind !== "node") {
      return;
    }
    props.tree.select(props.workspace, row.node);
    virtualizer.scrollToIndex(index, { align: "auto" });
  };

  /** The visible rows as the store ranges over them. */
  const visible = () => rows().map((row) => ({ id: row.id, item: row.kind === "node" ? row.node.item : undefined }));

  /** UI-6: a click selects the row; with Ctrl/Cmd it toggles a repo row, with Shift it selects a range. */
  const clickRow = (index: number, event: MouseEvent) => {
    const row = rows()[index];
    if (row?.kind !== "node") {
      return;
    }
    if (event.ctrlKey || event.metaKey) {
      props.tree.toggle(props.workspace, row.node);
    } else if (event.shiftKey) {
      props.tree.extend(props.workspace, visible(), row.node);
    } else {
      props.tree.select(props.workspace, row.node);
    }
  };

  // The selection moved from outside, as from the summary strip: its row in view.
  createEffect(
    on(
      selectedIndex,
      (index) => {
        if (index >= 0) {
          virtualizer.scrollToIndex(index, { align: "auto" });
        }
      },
      { defer: true },
    ),
  );

  // Shown again, as when its tab is active again: where it was (UI-29).
  onMount(() => {
    if (scroller) {
      scroller.scrollTop = untrack(() => tree().scrollTop);
    }
  });

  const toggle = (row: Row) => {
    if (row.kind === "node" && row.expandable) {
      props.tree.setExpanded(props.workspace, row.node, !row.expanded);
    }
  };

  /** The next node row from `from` in direction `step`; with `repos`, the next repo row. */
  const nextNode = (from: number, step: 1 | -1, repos = false) => {
    for (let index = from + step; index >= 0 && index < rows().length; index += step) {
      const row = rows()[index];
      if (row?.kind === "node" && (!repos || row.node.item.kind === "repo")) {
        return index;
      }
    }
    return -1;
  };

  /** Arrow keys: Shift extends over repo rows, Ctrl/Cmd moves without changing the selection (UI-6). */
  const step = (event: KeyboardEvent, direction: 1 | -1) => {
    const index = selectedIndex();
    const next = index < 0 ? nextNode(-1, 1, event.shiftKey) : nextNode(index, direction, event.shiftKey);
    const row = rows()[next];
    if (row?.kind !== "node") {
      return;
    }
    if (event.shiftKey) {
      props.tree.extend(props.workspace, visible(), row.node);
    } else if (event.ctrlKey || event.metaKey) {
      props.tree.moveCursor(props.workspace, row.node);
    } else {
      props.tree.select(props.workspace, row.node);
    }
    virtualizer.scrollToIndex(next, { align: "auto" });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const index = selectedIndex();
    const row = rows()[index];
    switch (event.key) {
      case "ArrowDown":
        step(event, 1);
        break;
      case "ArrowUp":
        step(event, -1);
        break;
      case " ":
        if (!(event.ctrlKey || event.metaKey) || row?.kind !== "node") {
          return;
        }
        props.tree.toggle(props.workspace, row.node);
        break;
      case "ArrowRight":
        if (row?.kind === "node" && row.expandable && !row.expanded) {
          toggle(row);
        } else if (row?.kind === "node" && row.expanded) {
          const child = nextNode(index, 1);
          if (child >= 0 && rows()[child]?.depth === row.depth + 1) {
            select(child);
          }
        }
        break;
      case "ArrowLeft":
        if (row?.kind === "node" && row.expanded) {
          toggle(row);
        } else if (row?.parent) {
          const parent = row.parent;
          select(rows().findIndex((candidate) => candidate.id === parent));
        }
        break;
      case "Home":
        select(nextNode(-1, 1));
        break;
      case "End":
        select(nextNode(rows().length, -1));
        break;
      case "ContextMenu":
      case "F10": {
        if (event.key === "F10" && !event.shiftKey) {
          return;
        }
        // UI-22: the menu of the selected row, below it.
        const item = row?.kind === "node" ? row.node.item : undefined;
        if (item !== undefined && menuFor(item, false, props.groups).length > 0) {
          const box = document.getElementById(`tree-row-${index}`)?.getBoundingClientRect();
          props.onMenu(item, box?.left ?? 0, box?.bottom ?? 0);
        }
        break;
      }
      default:
        return;
    }
    event.preventDefault();
  };

  /** Where on its row `event` would drop what is dragged; `null` when it cannot. */
  const placeOf = (event: DragEvent, item: TreeItem) => {
    const what = dragged()?.what;
    if (what === undefined) {
      return null;
    }
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return dropPlace(what, item, box.height > 0 ? (event.clientY - box.top) / box.height : 0.5);
  };

  /** UI-7: Ctrl adds instead of moving, or Alt as on macOS. */
  const adds = (event: DragEvent) => event.ctrlKey || event.altKey;

  const endDrag = () => {
    setDragged(null);
    setTarget(null);
  };

  const summary = (node: TreeNode) => {
    const repo = node.item.kind === "repo" ? node.item.repo_id : null;
    return repo === null ? undefined : props.summaries.state.byWorkspace[props.workspace]?.[repo];
  };

  /** What a group node's row shows now (UI-4). */
  const groupOf = (node: TreeNode) =>
    node.item.kind === "group" ? props.tree.groupSummary(props.workspace, node.item.summary) : undefined;

  return (
    <div
      class="tree-scroller"
      ref={(element) => {
        scroller = element;
        props.ref(element);
      }}
      role="tree"
      aria-label="Repositories"
      aria-multiselectable="true"
      tabindex="0"
      aria-activedescendant={selectedIndex() >= 0 ? `tree-row-${selectedIndex()}` : undefined}
      onKeyDown={onKeyDown}
      onScroll={(event) => props.tree.setScrollTop(props.workspace, event.currentTarget.scrollTop)}
      onDragLeave={(event) => {
        if (!(event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget))) {
          setTarget(null);
        }
      }}
    >
      <div class="tree-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <For each={virtualizer.getVirtualItems()}>
          {(item) => (
            <Show when={rows()[item.index]}>
              {(row) => {
                const node = () => asNode(row());
                const what = () => {
                  const shown = node();
                  return shown === undefined ? null : draggedOf(shown.node.item);
                };
                const place = () => {
                  const current = target();
                  return current?.row === row().id ? current.place : null;
                };
                return (
                  <div
                    id={`tree-row-${item.index}`}
                    class="tree-row"
                    classList={{
                      selected: props.tree.isSelected(props.workspace, row().id),
                      cursor: tree().multi !== null && row().id === tree().selected,
                      dragged: dragged()?.row === row().id,
                      "drop-into": place() === "into",
                      "drop-before": place() === "before",
                      "drop-after": place() === "after",
                    }}
                    role={node() ? "treeitem" : "none"}
                    aria-level={row().depth + 1}
                    aria-expanded={node()?.expandable ? node()?.expanded : undefined}
                    aria-selected={node() ? props.tree.isSelected(props.workspace, row().id) : undefined}
                    style={{
                      transform: `translateY(${item.start}px)`,
                      "padding-left": `${4 + row().depth * INDENT}px`,
                    }}
                    onClick={(event) => clickRow(item.index, event)}
                    onDblClick={() => {
                      // UI-7: a local branch is checked out, UI-5: a group's branch name is checked
                      // out on the group; other rows open or close.
                      const target = node()?.node.item;
                      if (target?.kind === "branch") {
                        props.onCheckout(target.repo_id, target.name);
                      } else if (target?.kind === "group_branch") {
                        props.onGroupCheckout(target.group_id, target.name);
                      } else {
                        toggle(row());
                      }
                    }}
                    onContextMenu={(event) => {
                      const target = node()?.node.item;
                      if (target !== undefined && menuFor(target, false, props.groups).length > 0) {
                        event.preventDefault();
                        select(item.index);
                        props.onMenu(target, event.clientX, event.clientY);
                      }
                    }}
                    draggable={what() !== null}
                    onDragStart={(event) => {
                      const dragging = what();
                      if (dragging === null) {
                        return;
                      }
                      setDragged({ row: row().id, what: dragging });
                      if (event.dataTransfer) {
                        event.dataTransfer.effectAllowed = "copyMove";
                        event.dataTransfer.setData("text/plain", node()?.node.label ?? "");
                      }
                    }}
                    onDragOver={(event) => {
                      const over = node()?.node.item;
                      const at = over === undefined ? null : placeOf(event, over);
                      if (at === null) {
                        setTarget(null);
                        return;
                      }
                      event.preventDefault();
                      if (event.dataTransfer) {
                        event.dataTransfer.dropEffect = adds(event) ? "copy" : "move";
                      }
                      setTarget({ row: row().id, place: at });
                    }}
                    onDrop={(event) => {
                      event.preventDefault();
                      const over = node()?.node.item;
                      const at = over === undefined ? null : placeOf(event, over);
                      const dropped = dragged()?.what;
                      endDrag();
                      const op =
                        over === undefined || at === null || dropped === undefined
                          ? null
                          : dropOp(props.groups, dropped, over, at, adds(event));
                      if (op !== null) {
                        props.onGroupOp(op);
                      }
                    }}
                    onDragEnd={endDrag}
                  >
                    <Show
                      when={node()}
                      fallback={
                        <span class="tree-message" classList={{ error: asMessage(row())?.error }}>
                          {asMessage(row())?.text}
                        </span>
                      }
                    >
                      {(shown) => (
                        <>
                          <span
                            class="tree-chevron"
                            aria-hidden="true"
                            onClick={(event) => {
                              event.stopPropagation();
                              select(item.index);
                              toggle(shown());
                            }}
                          >
                            {shown().expandable ? (shown().expanded ? "▾" : "▸") : ""}
                          </span>
                          <Switch fallback={<RefLabel node={shown().node} />}>
                            <Match when={shown().node.item.kind === "repo"}>
                              <RepoRow name={shown().node.label} view={summary(shown().node)} />
                            </Match>
                            <Match when={groupOf(shown().node)}>
                              {(summary) => <GroupRow name={shown().node.label} summary={summary()} />}
                            </Match>
                          </Switch>
                        </>
                      )}
                    </Show>
                  </div>
                );
              }}
            </Show>
          )}
        </For>
      </div>
    </div>
  );
}

export default function TreePane(props: {
  workspace: WorkspaceId;
  tree: TreeStore;
  summaries: SummaryStore;
  workspaces: WorkspaceStore;
  branch: BranchStore;
  stash: StashStore;
  tag: TagStore;
  remote: RemoteStore;
  clone: CloneStore;
  share: ShareStore;
  groupOps: GroupOpStore;
}) {
  const [actionError, setActionError] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ item: TreeItem; x: number; y: number } | null>(null);
  /** The entries beside the menu entry they belong to, such as Add to group's. */
  const [submenu, setSubmenu] = createSignal<{
    entry: MenuEntry;
    x: number;
    y: number;
    from: HTMLElement;
  } | null>(null);
  const [groupDialog, setGroupDialog] = createSignal<GroupDialog | null>(null);
  let treeElement: HTMLDivElement | undefined;
  /** Where the add menu opened, below its button. */
  const [addMenu, setAddMenu] = createSignal<{ x: number; y: number } | null>(null);

  const tree = () => props.tree.tree(props.workspace);
  /** Loaded, unfiltered and without repos. */
  const empty = () =>
    tree().filtered === null && tree().roots?.length === 1 && tree().roots?.[0]?.children?.length === 0;
  const groups = createMemo(() => groupsOf(tree().roots ?? []));

  const closeMenu = () => {
    setSubmenu(null);
    if (menu() !== null) {
      setMenu(null);
      treeElement?.focus();
    }
  };

  const sendGroupOp = (op: GroupOp) => {
    setActionError(null);
    runGroupOp(props.workspace, op).catch((error) => setActionError(message(error)));
  };

  const add = () => {
    setAddMenu(null);
    setActionError(null);
    workspacePickAndAdd(props.workspace).catch((error) => setActionError(message(error)));
  };

  const clone = () => {
    setAddMenu(null);
    setActionError(null);
    props.clone.open(props.workspace);
  };

  const openAddMenu = (event: MouseEvent) => {
    const button = (event.currentTarget as HTMLElement).getBoundingClientRect();
    setAddMenu({ x: button.left, y: button.bottom });
  };

  const AddButton = () => (
    <button type="button" aria-haspopup="menu" aria-expanded={addMenu() !== null} onClick={openAddMenu}>
      Add repository
    </button>
  );

  const openRepo = (how: "file_manager" | "terminal", repo: RepoId) => {
    setActionError(null);
    const opened = how === "file_manager" ? repoOpenFileManager : repoOpenTerminal;
    opened(props.workspace, repo).catch((error) => setActionError(message(error)));
  };

  const remove = (repo: RepoId) => {
    setMenu(null);
    setActionError(null);
    workspaceRemoveRepo(props.workspace, repo).catch((error) => setActionError(message(error)));
  };

  const blocked = (repo: RepoId) => isBlocked(props.summaries.view(props.workspace, repo));
  const itemBlocked = (item: TreeItem) => "repo_id" in item && blocked(item.repo_id);
  const itemMissing = (item: TreeItem) =>
    item.kind === "repo" && props.summaries.view(props.workspace, item.repo_id)?.missing === true;

  const run = (repo: RepoId, op: Parameters<BranchStore["run"]>[2]) => void props.branch.run(props.workspace, repo, op);

  /** UI-5: a group checkout of branch `name` on group `group`'s members, through the preflight. */
  const groupCheckout = (group: GroupId, name: string) => {
    const target: TargetSet = { kind: "group", group_id: group };
    void props.groupOps.run(props.workspace, target, { kind: "checkout", name, missing: "skip" });
  };

  /** Does what a group `entry` of the menu of `item` says; `false` for other entries. */
  const actOnGroups = (entry: MenuEntry, item: TreeItem): boolean => {
    const step = entry.action === "move_up" ? -1 : 1;
    const named = (group: string) => groups().find((known) => known.id === group)?.name ?? "";
    switch (entry.action) {
      case "new_group":
        setGroupDialog({ kind: "new", repos: item.kind === "repo" ? [item.repo_id] : [] });
        return true;
      case "group_fetch":
      case "group_pull":
      case "group_push":
        if (item.kind === "group") {
          const kind = entry.action === "group_fetch" ? "fetch" : entry.action === "group_pull" ? "pull" : "push";
          const target: TargetSet = { kind: "group", group_id: item.group_id };
          void props.groupOps.run(props.workspace, target, DEFAULT_GROUP_OPS[kind]);
        }
        return true;
      case "group_checkout":
      case "group_create":
      case "group_delete_branch":
      case "group_merge":
      case "group_rebase":
      case "group_tag":
        if (item.kind === "group" || item.kind === "group_branch") {
          const kind = NAME_DIALOGS[entry.action];
          if (kind === "checkout" && item.kind === "group_branch") {
            groupCheckout(item.group_id, item.name);
            return true;
          }
          props.groupOps.openName({
            workspaceId: props.workspace,
            target: { kind: "group", group_id: item.group_id },
            targetName: named(item.group_id),
            kind,
            name: item.kind === "group_branch" ? item.name : "",
          });
        }
        return true;
      case "group_rename":
      case "group_delete":
        if (item.kind === "group") {
          const kind = entry.action === "group_rename" ? "rename" : "delete";
          setGroupDialog({ kind, group: item.group_id, name: named(item.group_id) });
        }
        return true;
      case "add_to_group":
        if (item.kind === "repo" && entry.group !== undefined) {
          sendGroupOp({ kind: "add", group: entry.group, repo: item.repo_id, at: null });
        }
        return true;
      case "remove_from_group":
        if (item.kind === "repo" && item.group_id !== null) {
          sendGroupOp({ kind: "remove", group: item.group_id, repo: item.repo_id });
        }
        return true;
      case "move_up":
      case "move_down": {
        const op =
          item.kind === "group"
            ? moveGroup(groups(), item.group_id, step)
            : item.kind === "repo" && item.group_id !== null
              ? moveMember(groups(), item.group_id, item.repo_id, step)
              : null;
        if (op !== null) {
          sendGroupOp(op);
        }
        return true;
      }
      default:
        return false;
    }
  };

  /** Does what `entry` of the menu of `item` says. */
  const act = (entry: MenuEntry, item: TreeItem) => {
    const action = entry.action;
    closeMenu();
    if (actOnGroups(entry, item) || !("repo_id" in item)) {
      return;
    }
    const repo = item.repo_id;
    const open = props.branch.open.bind(null, props.workspace);
    switch (action) {
      case "remove_repo":
        remove(repo);
        return;
      case "clone_missing":
        props.share.openMissing(props.workspace, repo);
        return;
      case "open_file_manager":
        openRepo("file_manager", repo);
        return;
      case "open_terminal":
        openRepo("terminal", repo);
        return;
      case "create":
        open({
          kind: "create",
          repo,
          start:
            item.kind === "branch" || item.kind === "remote_branch" || item.kind === "tag"
              ? { kind: "revision", revision: item.name }
              : { kind: "head" },
        });
        return;
      case "checkout":
        if (item.kind === "branch") {
          run(repo, { kind: "checkout", name: item.name });
        }
        return;
      case "checkout_remote":
        if (item.kind === "remote_branch") {
          run(repo, { kind: "checkout_remote", branch: { remote: item.remote, branch: item.branch } });
        }
        return;
      case "checkout_detached":
        if (item.kind === "tag") {
          run(repo, { kind: "checkout_detached", revision: item.name });
        }
        return;
      case "delete_remote":
        if (item.kind === "remote_branch") {
          run(repo, { kind: "delete_remote", branch: { remote: item.remote, branch: item.branch } });
        }
        return;
      case "stash_apply":
      case "stash_pop":
      case "stash_drop":
        if (item.kind === "stash") {
          // Named by the hash read with the tree: the core refuses it if the list changed since.
          const stash = { index: item.index, hash: item.hash };
          const kind = action === "stash_apply" ? "apply" : action === "stash_pop" ? "pop" : "drop";
          void props.stash.run(props.workspace, repo, { kind, stash }, false, item.message);
        }
        return;
      case "tag_create":
        props.tag.open(props.workspace, { kind: "create", repo, commit: null, at: "HEAD" });
        return;
      case "tag_push":
        if (item.kind === "tag") {
          void props.tag.run(props.workspace, repo, { kind: "push", name: item.name, remote: null });
        }
        return;
      case "tag_delete":
        if (item.kind === "tag") {
          props.tag.open(props.workspace, { kind: "delete", repo, name: item.name });
        }
        return;
      case "remote_add":
        props.remote.open(props.workspace, { kind: "add", repo });
        return;
      case "remote_rename":
        if (item.kind === "remote") {
          props.remote.open(props.workspace, { kind: "rename", repo, name: item.remote });
        }
        return;
      case "remote_url":
        if (item.kind === "remote") {
          void props.remote.openUrl(props.workspace, repo, item.remote);
        }
        return;
      case "remote_remove":
        if (item.kind === "remote") {
          void props.remote.run(props.workspace, repo, { kind: "remove", name: item.remote });
        }
        return;
      case "rename":
      case "delete":
      case "set_upstream":
      case "unset_upstream":
        if (item.kind !== "branch") {
          return;
        }
        if (action === "rename") {
          open({ kind: "rename", repo, name: item.name });
        } else if (action === "delete") {
          open({ kind: "delete", repo, name: item.name, tracked: item.upstream !== null && !item.gone });
        } else if (action === "set_upstream") {
          open({ kind: "upstream", repo, name: item.name, upstream: item.upstream });
        } else {
          run(repo, { kind: "set_upstream", name: item.name, upstream: null });
        }
    }
  };

  /** Opens `entry`'s submenu beside it, its first entry focused. */
  const openSubmenu = (entry: MenuEntry, button: HTMLElement) => {
    const box = button.getBoundingClientRect();
    setSubmenu({ entry, x: box.right, y: box.top, from: button });
  };

  /** Escape closes the submenu, then the menus. */
  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key !== "Escape") {
      return;
    }
    const open = submenu();
    if (open !== null) {
      setSubmenu(null);
      open.from.focus();
      return;
    }
    closeMenu();
    setAddMenu(null);
  };
  document.addEventListener("keydown", closeOnEscape);
  onCleanup(() => document.removeEventListener("keydown", closeOnEscape));

  return (
    <div class="tree-pane">
      <div class="tree-header">
        <input
          class="tree-filter"
          type="search"
          placeholder="Filter repos and branches"
          aria-label="Filter repos and branches"
          value={tree().filter}
          onInput={(event) => void props.tree.setFilter(props.workspace, event.currentTarget.value)}
        />
        <AddButton />
      </div>
      <Show when={actionError()}>{(error) => <p role="alert">{error()}</p>}</Show>
      <Show when={props.stash.dialog(props.workspace) === null && props.stash.error(props.workspace)}>
        {(error) => (
          <div role="alert" class="tree-refusal">
            <span>{error()}</span>
            <button type="button" onClick={() => props.stash.dismissError(props.workspace)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <Show when={props.tag.dialog(props.workspace) === null && props.tag.error(props.workspace)}>
        {(error) => (
          <div role="alert" class="tree-refusal">
            <span>{error()}</span>
            <button type="button" onClick={() => props.tag.dismissError(props.workspace)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <Show when={props.remote.dialog(props.workspace) === null && props.remote.error(props.workspace)}>
        {(error) => (
          <div role="alert" class="tree-refusal">
            <span>{error()}</span>
            <button type="button" onClick={() => props.remote.dismissError(props.workspace)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <Show when={props.branch.dialog(props.workspace) === null && props.branch.error(props.workspace)}>
        {(error) => (
          <div role="alert" class="tree-refusal">
            <span>{error()}</span>
            <button type="button" onClick={() => props.branch.dismissError(props.workspace)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <AddReportView workspaces={props.workspaces} workspace={props.workspace} />
      <Show when={tree().error}>{(error) => <p role="alert">Cannot read the tree: {error()}</p>}</Show>
      <Show
        when={!empty()}
        fallback={
          <div class="tree-empty">
            <p class="placeholder">This workspace has no repositories yet.</p>
            <AddButton />
          </div>
        }
      >
        <TreeList
          workspace={props.workspace}
          tree={props.tree}
          summaries={props.summaries}
          groups={groups()}
          onMenu={(item, x, y) => {
            setSubmenu(null);
            setMenu({ item, x, y });
          }}
          onGroupOp={sendGroupOp}
          ref={(element) => (treeElement = element)}
          onCheckout={(repo, name) => {
            if (!blocked(repo)) {
              run(repo, { kind: "checkout", name });
            }
          }}
          onGroupCheckout={groupCheckout}
        />
      </Show>
      <Show when={addMenu()}>
        {(open) => (
          <div class="menu-backdrop" onClick={() => setAddMenu(null)} onContextMenu={(event) => event.preventDefault()}>
            <div
              class="menu"
              role="menu"
              aria-label="Add repository"
              style={{ left: `${open().x}px`, top: `${open().y}px` }}
              ref={(element) => queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]")?.focus())}
              onClick={(event) => event.stopPropagation()}
            >
              <button type="button" role="menuitem" onClick={add}>
                Add folder…
              </button>
              <button type="button" role="menuitem" onClick={clone}>
                Clone…
              </button>
            </div>
          </div>
        )}
      </Show>
      <Show when={menu()}>
        {(open) => (
          <div class="menu-backdrop" onClick={closeMenu} onContextMenu={(event) => event.preventDefault()}>
            <div
              class="menu"
              role="menu"
              aria-label={MENU_NAMES[open().item.kind] ?? "Actions"}
              style={{ left: `${open().x}px`, top: `${open().y}px` }}
              ref={(element) =>
                queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus())
              }
              onClick={(event) => event.stopPropagation()}
            >
              <For each={menuFor(open().item, itemBlocked(open().item), groups(), itemMissing(open().item))}>
                {(entry) => (
                  <Show
                    when={entry.submenu}
                    fallback={
                      <button
                        type="button"
                        role="menuitem"
                        disabled={entry.disabled}
                        onClick={() => act(entry, open().item)}
                      >
                        {entry.label}
                      </button>
                    }
                  >
                    <button
                      type="button"
                      role="menuitem"
                      aria-haspopup="menu"
                      aria-expanded={submenu()?.entry.label === entry.label}
                      disabled={entry.disabled}
                      onClick={(event) => openSubmenu(entry, event.currentTarget)}
                      onKeyDown={(event) => {
                        if (event.key === "ArrowRight") {
                          event.preventDefault();
                          openSubmenu(entry, event.currentTarget);
                        }
                      }}
                    >
                      {entry.label} ▸
                    </button>
                  </Show>
                )}
              </For>
            </div>
            <Show when={submenu()}>
              {(beside) => (
                <div
                  class="menu"
                  role="menu"
                  aria-label={beside().entry.label}
                  style={{ left: `${beside().x}px`, top: `${beside().y}px` }}
                  ref={(element) =>
                    queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]")?.focus())
                  }
                  onClick={(event) => event.stopPropagation()}
                  onKeyDown={(event) => {
                    if (event.key === "ArrowLeft") {
                      event.preventDefault();
                      setSubmenu(null);
                      beside().from.focus();
                    }
                  }}
                >
                  <For each={beside().entry.submenu}>
                    {(entry) => (
                      <button
                        type="button"
                        role="menuitem"
                        disabled={entry.disabled}
                        onClick={() => act(entry, open().item)}
                      >
                        {entry.label}
                      </button>
                    )}
                  </For>
                </div>
              )}
            </Show>
          </div>
        )}
      </Show>
      <GroupDialogs workspace={props.workspace} dialog={groupDialog()} onClose={() => setGroupDialog(null)} />
    </div>
  );
}
