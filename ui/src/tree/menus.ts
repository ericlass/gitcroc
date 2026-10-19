// The context menu of each tree row (UI-7): the operations valid for its
// kind, those that cannot apply disabled rather than hidden. A blocked repo
// (GRP-8) has every repo write disabled; the core refuses them as well. The
// group entries never touch a repo (GRP-3), so they stay enabled; Fetch,
// Pull, Push and the branch operations of a group, and those of a name in
// its Branches node (UI-5), go through its preflight, which skips or blocks
// each repo that cannot take them (GRP-8). A repo whose folder is missing
// offers Clone from remote, which opens the missing repos dialog (DATA-2).
import type { GroupId, TreeItem } from "../ipc";
import type { GroupInfo } from "./groups";
import { strings } from "../strings/en";

export type MenuAction =
  | "checkout"
  | "checkout_remote"
  | "checkout_detached"
  | "create"
  | "rename"
  | "delete"
  | "delete_remote"
  | "set_upstream"
  | "unset_upstream"
  | "stash_apply"
  | "stash_pop"
  | "stash_drop"
  | "tag_create"
  | "tag_push"
  | "tag_delete"
  | "remote_add"
  | "remote_rename"
  | "remote_url"
  | "remote_remove"
  | "open_file_manager"
  | "open_terminal"
  | "remove_repo"
  | "clone_missing"
  | "add_to_group"
  | "new_group"
  | "remove_from_group"
  | "move_up"
  | "move_down"
  | "group_rename"
  | "group_delete"
  | "group_fetch"
  | "group_pull"
  | "group_push"
  | "group_checkout"
  | "group_create"
  | "group_delete_branch"
  | "group_merge"
  | "group_rebase"
  | "group_tag";

export interface MenuEntry {
  action: MenuAction;
  label: string;
  disabled: boolean;
  /** The group `add_to_group` adds to. */
  group?: GroupId;
  /** Entries shown beside this one when it is chosen. */
  submenu?: MenuEntry[];
}

/** The entries of `item`'s row that change groups (UI-7). */
function groupEntries(item: TreeItem, groups: GroupInfo[]): MenuEntry[] {
  const entry = (action: MenuAction, label: string, disabled = false): MenuEntry => ({ action, label, disabled });
  switch (item.kind) {
    case "group": {
      const index = groups.findIndex((group) => group.id === item.group_id);
      return [
        entry("group_fetch", strings.treeMenu.fetch),
        entry("group_pull", strings.treeMenu.pull),
        entry("group_push", strings.treeMenu.push),
        entry("group_checkout", strings.treeMenu.checkOutBranch),
        entry("group_create", strings.treeMenu.newBranch),
        entry("group_delete_branch", strings.treeMenu.deleteBranch),
        entry("group_merge", strings.treeMenu.merge),
        entry("group_rebase", strings.treeMenu.rebase),
        entry("group_tag", strings.treeMenu.createTag),
        entry("group_rename", strings.treeMenu.rename),
        entry("group_delete", strings.treeMenu.delete),
        entry("new_group", strings.treeMenu.newGroup),
        entry("move_up", strings.treeMenu.moveUp, index <= 0),
        entry("move_down", strings.treeMenu.moveDown, index < 0 || index === groups.length - 1),
      ];
    }
    case "group_branch":
      return [
        entry("group_checkout", strings.treeMenu.checkOut),
        entry("group_delete_branch", strings.treeMenu.delete),
      ];
    case "ungrouped":
      return [entry("new_group", strings.treeMenu.newGroup)];
    case "repo": {
      const others = groups.filter((group) => !group.repos.includes(item.repo_id));
      const add: MenuEntry = {
        ...entry("add_to_group", strings.treeMenu.addToGroup),
        submenu: [
          ...others.map((group) => ({ ...entry("add_to_group", group.name), group: group.id })),
          entry("new_group", strings.treeMenu.newGroup),
        ],
      };
      if (item.group_id === null) {
        return [add];
      }
      const members = groups.find((group) => group.id === item.group_id)?.repos ?? [];
      const index = members.indexOf(item.repo_id);
      return [
        add,
        entry("remove_from_group", strings.treeMenu.removeFromGroup),
        entry("move_up", strings.treeMenu.moveUp, index <= 0),
        entry("move_down", strings.treeMenu.moveDown, index < 0 || index === members.length - 1),
      ];
    }
    default:
      return [];
  }
}

/**
 * What the menu of a row for `item` lists, with `groups` the workspace's
 * groups and `missing` when a repo's folder is missing; empty when the row
 * has none.
 */
export function menuFor(item: TreeItem, blocked: boolean, groups: GroupInfo[], missing = false): MenuEntry[] {
  const entry = (action: MenuAction, label: string, disabled = false): MenuEntry => ({
    action,
    label,
    disabled: blocked || disabled,
  });
  switch (item.kind) {
    case "repo":
      return [
        ...(missing
          ? [{ action: "clone_missing" as const, label: strings.treeMenu.cloneFromRemote, disabled: false }]
          : []),
        entry("create", strings.treeMenu.newBranch),
        entry("tag_create", strings.treeMenu.newTag),
        ...groupEntries(item, groups),
        // Not writes: a blocked repo opens all the same.
        { action: "open_file_manager", label: strings.treeMenu.openInFileManager, disabled: false },
        { action: "open_terminal", label: strings.treeMenu.openInTerminal, disabled: false },
        { action: "remove_repo", label: strings.treeMenu.removeFromWorkspace, disabled: false },
      ];
    case "branch":
      return [
        entry("checkout", strings.treeMenu.checkOut, item.current),
        entry("create", strings.treeMenu.newBranchFromHere),
        entry("rename", strings.treeMenu.rename),
        entry("delete", strings.treeMenu.delete, item.current),
        entry("set_upstream", strings.treeMenu.setUpstream),
        entry("unset_upstream", strings.treeMenu.unsetUpstream, item.upstream === null),
      ];
    case "remotes":
      return [entry("remote_add", strings.treeMenu.addRemote)];
    case "remote":
      return [
        entry("remote_rename", strings.treeMenu.rename),
        entry("remote_url", strings.treeMenu.editUrl),
        entry("remote_remove", strings.treeMenu.remove),
      ];
    case "remote_branch":
      return [
        entry("checkout_remote", strings.treeMenu.checkOut),
        entry("create", strings.treeMenu.newBranchFromHere),
        entry("delete_remote", strings.treeMenu.deleteOnRemote),
      ];
    case "tag":
      return [
        entry("checkout_detached", strings.treeMenu.checkOutDetached),
        entry("create", strings.treeMenu.newBranchFromHere),
        entry("tag_push", strings.treeMenu.push),
        entry("tag_delete", strings.treeMenu.delete),
      ];
    case "stash":
      return [
        entry("stash_apply", strings.treeMenu.apply),
        entry("stash_pop", strings.treeMenu.pop),
        entry("stash_drop", strings.treeMenu.drop),
      ];
    default:
      return groupEntries(item, groups);
  }
}
