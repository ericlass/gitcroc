// The group changes the tree offers (UI-7), from its menus and by dragging:
// which core command a gesture sends. The groups and their members are read
// off the core's tree; the core checks and applies every change (GRP-2,
// GRP-3) and the tree follows its `workspace_changed`.
import {
  groupAddRepos,
  groupMoveRepoBetween,
  groupMoveRepos,
  groupRemoveRepos,
  groupReorder,
  type GroupId,
  type RepoId,
  type TreeItem,
  type TreeNode,
  type WorkspaceId,
} from "../ipc";

/** A group as the tree shows it: its members in the user's order. */
export interface GroupInfo {
  id: GroupId;
  name: string;
  repos: RepoId[];
}

/** The groups of the unfiltered tree `roots`, in the user's order. */
export function groupsOf(roots: TreeNode[]): GroupInfo[] {
  return roots.flatMap((node) =>
    node.item.kind === "group"
      ? [
          {
            id: node.item.group_id,
            name: node.label,
            repos: (node.children ?? []).flatMap((child) => (child.item.kind === "repo" ? [child.item.repo_id] : [])),
          },
        ]
      : [],
  );
}

export type GroupOp =
  | { kind: "add"; group: GroupId; repos: RepoId[]; at: number | null }
  | { kind: "remove"; group: GroupId; repo: RepoId }
  | { kind: "move_in"; group: GroupId; repo: RepoId; at: number }
  | { kind: "move_between"; repo: RepoId; from: GroupId; to: GroupId; at: number | null }
  | { kind: "reorder"; groups: GroupId[] };

/** Sends `op` to the core. */
export function runGroupOp(workspace: WorkspaceId, op: GroupOp): Promise<unknown> {
  switch (op.kind) {
    case "add":
      return groupAddRepos(workspace, op.group, op.repos, op.at);
    case "remove":
      return groupRemoveRepos(workspace, op.group, [op.repo]);
    case "move_in":
      return groupMoveRepos(workspace, op.group, [op.repo], op.at);
    case "move_between":
      return groupMoveRepoBetween(workspace, op.repo, op.from, op.to, op.at);
    case "reorder":
      return groupReorder(workspace, op.groups);
  }
}

/** `list` with the entry at `from` moved one step by `step`; `null` at an end. */
function stepped<T>(list: T[], from: number, step: 1 | -1): T[] | null {
  const to = from + step;
  if (from < 0 || to < 0 || to >= list.length) {
    return null;
  }
  const moved = [...list];
  [moved[from], moved[to]] = [moved[to] as T, moved[from] as T];
  return moved;
}

/** Moves group `group` one place up or down; `null` when it is first or last. */
export function moveGroup(groups: GroupInfo[], group: GroupId, step: 1 | -1): GroupOp | null {
  const ids = groups.map((known) => known.id);
  const order = stepped(ids, ids.indexOf(group), step);
  return order === null ? null : { kind: "reorder", groups: order };
}

/** Moves member `repo` of `group` one place up or down; `null` when it is first or last. */
export function moveMember(groups: GroupInfo[], group: GroupId, repo: RepoId, step: 1 | -1): GroupOp | null {
  const members = groups.find((known) => known.id === group)?.repos ?? [];
  const index = members.indexOf(repo);
  // The position counts among the other members.
  return stepped(members, index, step) === null ? null : { kind: "move_in", group, repo, at: index + step };
}

/** What is being dragged: a repo from the place it was dragged from, or a group. */
export type Dragged = { kind: "repo"; repo: RepoId; from: GroupId | null } | { kind: "group"; group: GroupId };

export function draggedOf(item: TreeItem): Dragged | null {
  if (item.kind === "repo") {
    return { kind: "repo", repo: item.repo_id, from: item.group_id };
  }
  return item.kind === "group" ? { kind: "group", group: item.group_id } : null;
}

/** Where on a row a drop lands: on it, or above or below it. */
export type DropPlace = "into" | "before" | "after";

/** Where on `item`'s row a drop at `fraction` of its height lands, for `dragged`; `null` when it cannot. */
export function dropPlace(dragged: Dragged, item: TreeItem, fraction: number): DropPlace | null {
  const half = fraction < 0.5 ? "before" : "after";
  if (dragged.kind === "group") {
    return item.kind === "group" && item.group_id !== dragged.group ? half : null;
  }
  switch (item.kind) {
    case "group":
    case "ungrouped":
      return "into";
    case "repo":
      // Between members of a group; under Ungrouped there is no order to drop into.
      if (item.group_id === null) {
        return "into";
      }
      return item.repo_id === dragged.repo && item.group_id === dragged.from ? null : half;
    default:
      return null;
  }
}

/**
 * What dropping `dragged` at `place` of `item`'s row does (UI-7). A repo
 * dropped on another group moves there from the group it was dragged from,
 * or is added there with `copy` (Ctrl, or Alt on macOS) or when it came
 * from Ungrouped. Dropped on Ungrouped, it leaves the group it came from.
 * Dropped between members, it goes there; within its own group that
 * reorders. A group dropped above or below another moves there.
 */
export function dropOp(
  groups: GroupInfo[],
  dragged: Dragged,
  item: TreeItem,
  place: DropPlace,
  copy: boolean,
): GroupOp | null {
  if (dragged.kind === "group") {
    if (item.kind !== "group" || item.group_id === dragged.group || place === "into") {
      return null;
    }
    const order = groups.map((group) => group.id).filter((id) => id !== dragged.group);
    const target = order.indexOf(item.group_id);
    if (target < 0) {
      return null;
    }
    order.splice(place === "before" ? target : target + 1, 0, dragged.group);
    const same = order.every((id, index) => groups[index]?.id === id);
    return same ? null : { kind: "reorder", groups: order };
  }
  const { repo, from } = dragged;
  const to = item.kind === "group" ? item.group_id : item.kind === "repo" ? item.group_id : null;
  if (item.kind === "ungrouped" || (item.kind === "repo" && to === null)) {
    return from === null ? null : { kind: "remove", group: from, repo };
  }
  if (to === null) {
    return null;
  }
  let at: number | null = null;
  if (item.kind === "repo" && place !== "into") {
    // Counted among the members other than the dragged repo.
    const others = (groups.find((group) => group.id === to)?.repos ?? []).filter((member) => member !== repo);
    const target = others.indexOf(item.repo_id);
    at = target < 0 ? null : place === "before" ? target : target + 1;
  }
  if (to === from) {
    if (at === null) {
      return null;
    }
    const members = groups.find((group) => group.id === to)?.repos ?? [];
    return members.indexOf(repo) === at ? null : { kind: "move_in", group: to, repo, at };
  }
  if (from === null || copy) {
    return { kind: "add", group: to, repos: [repo], at };
  }
  return { kind: "move_between", repo, from, to, at };
}
