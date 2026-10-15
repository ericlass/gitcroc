// The context menu of each tree row (UI-7): the operations valid for its
// kind, those that cannot apply disabled rather than hidden. A blocked repo
// (GRP-8) has every repo write disabled; the core refuses them as well. The
// group entries never touch a repo (GRP-3), so they stay enabled.
import type { GroupId, TreeItem } from "../ipc";
import type { GroupInfo } from "./groups";

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
  | "add_to_group"
  | "new_group"
  | "remove_from_group"
  | "move_up"
  | "move_down"
  | "group_rename"
  | "group_delete";

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
        entry("group_rename", "Rename…"),
        entry("group_delete", "Delete…"),
        entry("new_group", "New group…"),
        entry("move_up", "Move up", index <= 0),
        entry("move_down", "Move down", index < 0 || index === groups.length - 1),
      ];
    }
    case "ungrouped":
      return [entry("new_group", "New group…")];
    case "repo": {
      const others = groups.filter((group) => !group.repos.includes(item.repo_id));
      const add: MenuEntry = {
        ...entry("add_to_group", "Add to group"),
        submenu: [
          ...others.map((group) => ({ ...entry("add_to_group", group.name), group: group.id })),
          entry("new_group", "New group…"),
        ],
      };
      if (item.group_id === null) {
        return [add];
      }
      const members = groups.find((group) => group.id === item.group_id)?.repos ?? [];
      const index = members.indexOf(item.repo_id);
      return [
        add,
        entry("remove_from_group", "Remove from group"),
        entry("move_up", "Move up", index <= 0),
        entry("move_down", "Move down", index < 0 || index === members.length - 1),
      ];
    }
    default:
      return [];
  }
}

/**
 * What the menu of a row for `item` lists, with `groups` the workspace's
 * groups; empty when the row has none.
 */
export function menuFor(item: TreeItem, blocked: boolean, groups: GroupInfo[]): MenuEntry[] {
  const entry = (action: MenuAction, label: string, disabled = false): MenuEntry => ({
    action,
    label,
    disabled: blocked || disabled,
  });
  switch (item.kind) {
    case "repo":
      return [
        entry("create", "New branch…"),
        entry("tag_create", "New tag…"),
        ...groupEntries(item, groups),
        // Not writes: a blocked repo opens all the same.
        { action: "open_file_manager", label: "Open in file manager", disabled: false },
        { action: "open_terminal", label: "Open in terminal", disabled: false },
        { action: "remove_repo", label: "Remove from workspace", disabled: false },
      ];
    case "branch":
      return [
        entry("checkout", "Check out", item.current),
        entry("create", "New branch from here…"),
        entry("rename", "Rename…"),
        entry("delete", "Delete…", item.current),
        entry("set_upstream", "Set upstream…"),
        entry("unset_upstream", "Unset upstream", item.upstream === null),
      ];
    case "remotes":
      return [entry("remote_add", "Add remote…")];
    case "remote":
      return [
        entry("remote_rename", "Rename…"),
        entry("remote_url", "Edit URL…"),
        entry("remote_remove", "Remove…"),
      ];
    case "remote_branch":
      return [
        entry("checkout_remote", "Check out"),
        entry("create", "New branch from here…"),
        entry("delete_remote", "Delete on the remote…"),
      ];
    case "tag":
      return [
        entry("checkout_detached", "Check out (detached)"),
        entry("create", "New branch from here…"),
        entry("tag_push", "Push"),
        entry("tag_delete", "Delete…"),
      ];
    case "stash":
      return [entry("stash_apply", "Apply"), entry("stash_pop", "Pop"), entry("stash_drop", "Drop…")];
    default:
      return groupEntries(item, groups);
  }
}
