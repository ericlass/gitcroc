// The context menu of each tree row (UI-7): the operations valid for its
// kind, those that cannot apply disabled rather than hidden. A blocked repo
// (GRP-8) has every repo write disabled; the core refuses them as well.
import type { TreeItem } from "../ipc";

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
  | "remove_repo";

export interface MenuEntry {
  action: MenuAction;
  label: string;
  disabled: boolean;
}

/** What the menu of a row for `item` lists; empty when the row has none. */
export function menuFor(item: TreeItem, blocked: boolean): MenuEntry[] {
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
      return [];
  }
}
