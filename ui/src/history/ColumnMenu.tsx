// The context menu of a history column's header (UI-36): move the column a
// place left or right, or restore the default order and widths. A move that
// cannot apply is disabled, not hidden.
import { For, onCleanup } from "solid-js";
import { onMenuKey } from "../keys";
import { strings } from "../strings/en";

export type ColumnAction = "move_left" | "move_right" | "reset";

export default function ColumnMenu(props: {
  x: number;
  y: number;
  /** The column's name, as its header shows it. */
  column: string;
  first: boolean;
  last: boolean;
  onAction: (action: ColumnAction) => void;
  onClose: () => void;
}) {
  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      props.onClose();
    }
  };
  document.addEventListener("keydown", closeOnEscape);
  onCleanup(() => document.removeEventListener("keydown", closeOnEscape));
  const entries = (): { action: ColumnAction; label: string; disabled: boolean }[] => [
    { action: "move_left", label: strings.history.moveColumnLeft, disabled: props.first },
    { action: "move_right", label: strings.history.moveColumnRight, disabled: props.last },
    { action: "reset", label: strings.history.resetColumns, disabled: false },
  ];
  return (
    <div class="menu-backdrop" onClick={() => props.onClose()} onContextMenu={(event) => event.preventDefault()}>
      <div
        class="menu"
        role="menu"
        aria-label={strings.history.columnMenu(props.column)}
        style={{ left: `${props.x}px`, top: `${props.y}px` }}
        ref={(element) =>
          queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus())
        }
        onClick={(event) => event.stopPropagation()}
        onKeyDown={onMenuKey}
      >
        <For each={entries()}>
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
