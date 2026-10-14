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
      return [entry("create", "New branch…"), { action: "remove_repo", label: "Remove from workspace", disabled: false }];
    case "branch":
      return [
        entry("checkout", "Check out", item.current),
        entry("create", "New branch from here…"),
        entry("rename", "Rename…"),
        entry("delete", "Delete…", item.current),
        entry("set_upstream", "Set upstream…"),
        entry("unset_upstream", "Unset upstream", item.upstream === null),
      ];
    case "remote_branch":
      return [
        entry("checkout_remote", "Check out"),
        entry("create", "New branch from here…"),
        entry("delete_remote", "Delete on the remote…"),
      ];
    case "tag":
      return [entry("checkout_detached", "Check out (detached)"), entry("create", "New branch from here…")];
    default:
      return [];
  }
}
