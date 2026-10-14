// The context menu of a commit row (UI-13). Entries that cannot apply are
// disabled, not hidden: with several commits selected only Cherry-pick
// applies, and a blocked repo (GRP-8) has every write disabled; the core
// refuses them as well.
import { For, onCleanup } from "solid-js";

export type CommitAction = "checkout" | "create_branch" | "create_tag" | "cherry_pick" | "revert" | "reset" | "copy_hash";

export interface CommitMenuEntry {
  action: CommitAction;
  label: string;
  disabled: boolean;
}

/** What the menu lists for `count` selected commits of a repo that is `blocked` or not. */
export function commitMenu(count: number, blocked: boolean): CommitMenuEntry[] {
  const one = count === 1;
  const write = (action: CommitAction, label: string, applies = one): CommitMenuEntry => ({
    action,
    label,
    disabled: blocked || !applies,
  });
  return [
    write("checkout", "Check out (detached)"),
    write("create_branch", "Create branch here…"),
    write("create_tag", "Create tag here…"),
    write("cherry_pick", one ? "Cherry-pick" : `Cherry-pick ${count} commits`, count > 0),
    write("revert", "Revert"),
    write("reset", "Reset current branch to here…"),
    { action: "copy_hash", label: "Copy hash", disabled: !one },
  ];
}

export default function CommitMenu(props: {
  x: number;
  y: number;
  entries: CommitMenuEntry[];
  onAction: (action: CommitAction) => void;
  onClose: () => void;
}) {
  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      props.onClose();
    }
  };
  document.addEventListener("keydown", closeOnEscape);
  onCleanup(() => document.removeEventListener("keydown", closeOnEscape));
  return (
    <div class="menu-backdrop" onClick={() => props.onClose()} onContextMenu={(event) => event.preventDefault()}>
      <div
        class="menu"
        role="menu"
        aria-label="Commit actions"
        style={{ left: `${props.x}px`, top: `${props.y}px` }}
        ref={(element) =>
          queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus())
        }
        onClick={(event) => event.stopPropagation()}
      >
        <For each={props.entries}>
          {(entry) => (
            <button
              type="button"
              role="menuitem"
              disabled={entry.disabled}
              onClick={() => props.onAction(entry.action)}
            >
              {entry.label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
