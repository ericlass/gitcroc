// The context menu of a commit row (UI-13). Entries that cannot apply are
// disabled, not hidden: with several commits selected only Cherry-pick
// applies, and only to commits of one repo, and a blocked repo (GRP-8) has
// every write disabled; the core refuses them as well.
import { For, onCleanup } from "solid-js";
import { onMenuKey } from "../keys";
import { strings } from "../strings/en";

export type CommitAction = "checkout" | "create_branch" | "create_tag" | "cherry_pick" | "revert" | "reset" | "copy_hash";

export interface CommitMenuEntry {
  action: CommitAction;
  label: string;
  disabled: boolean;
  /** Why it is disabled, when that is not plain. */
  title?: string;
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
    write("checkout", strings.history.checkOutDetached),
    write("create_branch", strings.history.createBranch),
    write("create_tag", strings.history.createTag),
    write("cherry_pick", one ? strings.history.cherryPick : strings.history.cherryPickCommits(count), count > 0),
    write("revert", strings.history.revert),
    write("reset", strings.history.reset),
    { action: "copy_hash", label: strings.history.copyHash, disabled: !one },
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
        aria-label={strings.history.menu}
        style={{ left: `${props.x}px`, top: `${props.y}px` }}
        ref={(element) =>
          queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus())
        }
        onClick={(event) => event.stopPropagation()}
        onKeyDown={onMenuKey}
      >
        <For each={props.entries}>
          {(entry) => (
            <button
              type="button"
              role="menuitem"
              disabled={entry.disabled}
              title={entry.title}
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
