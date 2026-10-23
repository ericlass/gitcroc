// The history's column headers (UI-9, UI-36), in the workspace's column
// order. The edge between two headers resizes the column on the side away
// from the subject, so the edge follows the pointer and the subject takes
// what is left. A header is dragged onto another to move its column, and
// opens a menu to move it from the keyboard or reset every column. The
// headers are one tab stop: the arrow keys, Home and End move between them.
import { createSignal, For, Show } from "solid-js";
import type { HistoryColumn, WorkspaceId } from "../ipc";
import Splitter from "../layout/Splitter";
import {
  type HistoryColumns,
  type LayoutStore,
  MIN_COLUMN_WIDTH,
  MIN_SUBJECT_WIDTH,
  type SizedColumn,
} from "../stores/layout";
import { strings } from "../strings/en";
import ColumnMenu, { type ColumnAction } from "./ColumnMenu";

/** Each column's header text, cell class and, but for the subject, width. */
export const COLUMNS: Record<HistoryColumn, { label: string; class: string; sized: SizedColumn | null }> = {
  graph: { label: strings.history.columns.graph, class: "history-graph", sized: "graph" },
  subject: { label: strings.history.columns.subject, class: "history-subject", sized: null },
  author: { label: strings.history.columns.author, class: "history-author", sized: "author" },
  date: { label: strings.history.columns.date, class: "history-date", sized: "date" },
  hash: { label: strings.history.columns.hash, class: "history-hash", sized: "hash" },
};

/**
 * A cell's width, as its own style: none for the subject, which takes the rest. Each cell has its width set, not
 * inherited from the pane, because WebKitGTK leaves the rows of the scrolled list drawn at their old widths when
 * only an inherited value changes (M5-24).
 */
export function cellStyle(widths: HistoryColumns, column: HistoryColumn): { width: string } | undefined {
  const sized = COLUMNS[column].sized;
  return sized === null ? undefined : { width: `${widths[sized]}px` };
}

export default function ColumnHeaders(props: {
  workspace: WorkspaceId;
  layout: LayoutStore;
  /** A merged history, whose graph column shows the repo. */
  merged: boolean;
}) {
  let bar: HTMLDivElement | undefined;
  const order = () => props.layout.columnOrder(props.workspace);
  const widths = () => props.layout.historyColumns(props.workspace);
  const label = (column: HistoryColumn) =>
    column === "graph" && props.merged ? strings.history.repository : COLUMNS[column].label;

  /** The header in the tab order: the one last focused. */
  const [current, setCurrent] = createSignal<HistoryColumn | null>(null);
  const tabbable = () => current() ?? order()[0] ?? null;
  const focus = (column: HistoryColumn) => {
    setCurrent(column);
    bar?.querySelector<HTMLElement>(`[data-column="${column}"]`)?.focus();
  };

  /** How wide `column` can get: what the subject can give up besides its own minimum. */
  const maxWidth = (column: SizedColumn) => {
    const subject = bar?.querySelector(".history-subject")?.getBoundingClientRect().width;
    return subject === undefined ? undefined : widths()[column] + Math.max(0, Math.round(subject) - MIN_SUBJECT_WIDTH);
  };

  const [menu, setMenu] = createSignal<{ column: HistoryColumn; x: number; y: number } | null>(null);
  const openMenu = (column: HistoryColumn, element: HTMLElement, x?: number, y?: number) => {
    const box = element.getBoundingClientRect();
    setMenu({ column, x: x ?? box.left, y: y ?? box.bottom });
  };
  /** Closes the menu; the focus goes back to the column's header. */
  const closeMenu = () => {
    const open = menu();
    setMenu(null);
    if (open !== null) {
      focus(open.column);
    }
  };
  const act = (column: HistoryColumn, action: ColumnAction) => {
    const at = order().indexOf(column);
    if (action === "reset") {
      props.layout.resetColumns(props.workspace);
    } else {
      props.layout.moveColumn(props.workspace, column, at + (action === "move_left" ? -1 : 1));
    }
    closeMenu();
  };

  const onKeyDown = (event: KeyboardEvent & { currentTarget: HTMLElement }, column: HistoryColumn) => {
    const at = order().indexOf(column);
    let next: HistoryColumn | undefined;
    switch (event.key) {
      case "ArrowLeft":
        next = order()[at - 1];
        break;
      case "ArrowRight":
        next = order()[at + 1];
        break;
      case "Home":
        next = order()[0];
        break;
      case "End":
        next = order()[order().length - 1];
        break;
      case "F10":
      case "ContextMenu":
        if (event.key === "F10" && !event.shiftKey) {
          return;
        }
        event.preventDefault();
        openMenu(column, event.currentTarget);
        return;
      default:
        return;
    }
    event.preventDefault();
    if (next !== undefined) {
      focus(next);
    }
  };

  /** The column being dragged, and where it would drop: before or after another column. */
  const [dragged, setDragged] = createSignal<HistoryColumn | null>(null);
  const [target, setTarget] = createSignal<{ column: HistoryColumn; after: boolean } | null>(null);
  const endDrag = () => {
    setDragged(null);
    setTarget(null);
  };
  const drop = () => {
    const moving = dragged();
    const at = target();
    endDrag();
    if (moving === null || at === null || moving === at.column) {
      return;
    }
    const rest = order().filter((column) => column !== moving);
    props.layout.moveColumn(props.workspace, moving, rest.indexOf(at.column) + (at.after ? 1 : 0));
  };

  return (
    <>
      <div
        class="history-columns"
        role="toolbar"
        aria-label={strings.history.columnHeaders}
        ref={(element) => (bar = element)}
      >
        <For each={order()}>
          {(column) => (
            <span
              class={`history-cell ${COLUMNS[column].class}`}
              style={cellStyle(widths(), column)}
              classList={{
                "column-dragged": dragged() === column,
                "drop-before": dragged() !== column && target()?.column === column && !target()?.after,
                "drop-after": dragged() !== column && target()?.column === column && target()?.after === true,
              }}
              onDragOver={(event) => {
                if (dragged() === null) {
                  return;
                }
                event.preventDefault();
                const box = event.currentTarget.getBoundingClientRect();
                setTarget({ column, after: event.clientX > box.left + box.width / 2 });
              }}
              onDrop={(event) => {
                event.preventDefault();
                drop();
              }}
            >
              <button
                type="button"
                class="history-column-label"
                data-column={column}
                tabindex={tabbable() === column ? 0 : -1}
                aria-haspopup="menu"
                draggable="true"
                onFocus={() => setCurrent(column)}
                onClick={(event) => openMenu(column, event.currentTarget)}
                onContextMenu={(event) => {
                  event.preventDefault();
                  openMenu(column, event.currentTarget, event.clientX, event.clientY);
                }}
                onKeyDown={(event) => onKeyDown(event, column)}
                onDragStart={(event) => {
                  setDragged(column);
                  if (event.dataTransfer) {
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData("text/plain", label(column));
                  }
                }}
                onDragEnd={endDrag}
              >
                {label(column)}
              </button>
              <Show when={COLUMNS[column].sized}>
                {(sized) => (
                  <Splitter
                    pane={sized()}
                    class="column-splitter"
                    before={order().indexOf(column) > order().indexOf("subject")}
                    label={strings.history.resizeColumn(label(column))}
                    width={widths()[sized()]}
                    min={MIN_COLUMN_WIDTH}
                    max={() => maxWidth(sized())}
                    onResize={(width, max) => props.layout.setColumnWidth(props.workspace, sized(), width, max)}
                    onReset={() => props.layout.resetColumn(props.workspace, sized())}
                    onCommit={() => props.layout.save(props.workspace)}
                  />
                )}
              </Show>
            </span>
          )}
        </For>
      </div>
      <Show when={menu()}>
        {(open) => (
          <ColumnMenu
            x={open().x}
            y={open().y}
            column={label(open().column)}
            first={order()[0] === open().column}
            last={order()[order().length - 1] === open().column}
            onAction={(action) => act(open().column, action)}
            onClose={closeMenu}
          />
        )}
      </Show>
    </>
  );
}
