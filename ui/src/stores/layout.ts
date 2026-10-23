// Pane sizes, where the changes pane is docked and the history's column
// order and widths, keyed by workspace id (ARCH-9, UI-19, UI-29, UI-36).
// Nothing here knows which workspace is shown; callers pass the id. They are
// kept in the workspace file by the core (section 10); this store starts
// from what the core read and sends it each change once a resize ends.
import { createStore } from "solid-js/store";
import { type ColumnLayout, type Dock, type HistoryColumn, type PaneLayout, type WorkspaceId, workspaceSetLayout } from "../ipc";

/**
 * Pane sizes in CSS pixels: the widths of the tree and of the history while
 * the changes pane is docked right, and the changes pane's height while it
 * is docked at the bottom. The remaining pane takes the rest.
 */
export interface PaneSizes {
  tree: number;
  history: number;
  changes: number;
}

export type ResizablePane = keyof PaneSizes;

export const MIN_PANE_WIDTH = 200;

/** The smallest height of a pane stacked above or below another. */
export const MIN_PANE_HEIGHT = 120;

export const DEFAULT_PANE_SIZES: Readonly<PaneSizes> = { tree: 280, history: 480, changes: 320 };

/** Where the changes pane sits until the user moves it (UI-19). */
export const DEFAULT_DOCK: Dock = "right";

/**
 * `width` limited to at least MIN_PANE_WIDTH and at most `max`, when known.
 * The minimum wins when they conflict.
 */
export function clampPaneWidth(width: number, max?: number, min = MIN_PANE_WIDTH): number {
  const capped = max === undefined ? width : Math.min(width, max);
  return Math.max(min, Math.round(capped));
}

/** Widths in CSS pixels of the history columns (UI-9); the subject takes the rest. */
export interface HistoryColumns {
  graph: number;
  author: number;
  date: number;
  hash: number;
}

/** A history column with a width of its own: every one but the subject. */
export type SizedColumn = keyof HistoryColumns;

export const MIN_COLUMN_WIDTH = 32;

/** What a resize leaves the subject at least (UI-36). */
export const MIN_SUBJECT_WIDTH = 80;

export const DEFAULT_HISTORY_COLUMNS: Readonly<HistoryColumns> = { graph: 80, author: 140, date: 128, hash: 72 };

/** The history's columns left to right, until the user moves them (UI-9). */
export const DEFAULT_COLUMN_ORDER: readonly HistoryColumn[] = ["graph", "subject", "author", "date", "hash"];

export function createLayoutStore() {
  const [sizes, setSizes] = createStore<Record<WorkspaceId, PaneSizes>>({});
  const [docks, setDocks] = createStore<Record<WorkspaceId, Dock>>({});
  // A workspace has an entry once its columns differ from the defaults; until then the file has none.
  const [columns, setColumns] = createStore<Record<WorkspaceId, HistoryColumns | undefined>>({});
  const [orders, setOrders] = createStore<Record<WorkspaceId, HistoryColumn[] | undefined>>({});
  /** Workspaces whose saved layout was applied. */
  const loaded = new Set<WorkspaceId>();

  const paneSizes = (workspace: WorkspaceId): PaneSizes => sizes[workspace] ?? DEFAULT_PANE_SIZES;

  const dock = (workspace: WorkspaceId): Dock => docks[workspace] ?? DEFAULT_DOCK;

  /** Applies the layout the core read from the workspace file, once per workspace. */
  const load = (workspace: WorkspaceId, layout: PaneLayout) => {
    if (loaded.has(workspace)) {
      return;
    }
    loaded.add(workspace);
    setSizes(workspace, {
      tree: layout.tree ?? DEFAULT_PANE_SIZES.tree,
      history: layout.history ?? DEFAULT_PANE_SIZES.history,
      changes: layout.changes ?? DEFAULT_PANE_SIZES.changes,
    });
    if (layout.dock !== null) {
      setDocks(workspace, layout.dock);
    }
    const saved = layout.columns;
    if (saved !== null) {
      const width = (column: SizedColumn) => Math.max(MIN_COLUMN_WIDTH, saved[column] ?? DEFAULT_HISTORY_COLUMNS[column]);
      setColumns(workspace, { graph: width("graph"), author: width("author"), date: width("date"), hash: width("hash") });
      if (saved.order !== null) {
        setOrders(workspace, saved.order);
      }
    }
  };

  /** The workspace's columns for its file: none while they are the defaults. */
  const columnLayout = (workspace: WorkspaceId): ColumnLayout | null => {
    if (columns[workspace] === undefined && orders[workspace] === undefined) {
      return null;
    }
    const order = orders[workspace];
    return { order: order === undefined ? null : [...order], ...historyColumns(workspace) };
  };

  /**
   * Sends the workspace's layout to the core, which keeps it in the
   * workspace file. A read-only workspace refuses it: the layout then lasts
   * this session only, and the workspace's banner already says why.
   */
  const save = (workspace: WorkspaceId) => {
    const current = paneSizes(workspace);
    const layout: PaneLayout = { dock: dock(workspace), ...current, columns: columnLayout(workspace) };
    workspaceSetLayout(workspace, layout).catch(() => {});
  };

  const setPaneWidth = (workspace: WorkspaceId, pane: ResizablePane, width: number, max?: number) => {
    const min = pane === "changes" ? MIN_PANE_HEIGHT : MIN_PANE_WIDTH;
    setSizes(workspace, { ...paneSizes(workspace), [pane]: clampPaneWidth(width, max, min) });
  };

  const resetPane = (workspace: WorkspaceId, pane: ResizablePane) => {
    setSizes(workspace, { ...paneSizes(workspace), [pane]: DEFAULT_PANE_SIZES[pane] });
  };

  /** Docks the changes pane right or at the bottom (UI-19), and saves it. */
  const setDock = (workspace: WorkspaceId, next: Dock) => {
    setDocks(workspace, next);
    save(workspace);
  };

  const historyColumns = (workspace: WorkspaceId): HistoryColumns => columns[workspace] ?? DEFAULT_HISTORY_COLUMNS;

  /** Sets a column's width, at most `max` when known: what the subject can give up (UI-36). */
  const setColumnWidth = (workspace: WorkspaceId, column: SizedColumn, width: number, max?: number) => {
    setColumns(workspace, { ...historyColumns(workspace), [column]: clampPaneWidth(width, max, MIN_COLUMN_WIDTH) });
  };

  const resetColumn = (workspace: WorkspaceId, column: SizedColumn) => {
    setColumns(workspace, { ...historyColumns(workspace), [column]: DEFAULT_HISTORY_COLUMNS[column] });
  };

  const columnOrder = (workspace: WorkspaceId): readonly HistoryColumn[] =>
    orders[workspace] ?? DEFAULT_COLUMN_ORDER;

  /** Moves `column` to place `to` of the order (UI-36), and saves. */
  const moveColumn = (workspace: WorkspaceId, column: HistoryColumn, to: number) => {
    const order = columnOrder(workspace).filter((other) => other !== column);
    order.splice(Math.max(0, Math.min(order.length, to)), 0, column);
    setOrders(workspace, order);
    save(workspace);
  };

  /** The default order and widths (UI-36), saved: the file has no columns again. */
  const resetColumns = (workspace: WorkspaceId) => {
    setColumns(workspace, undefined);
    setOrders(workspace, undefined);
    save(workspace);
  };

  return {
    paneSizes,
    dock,
    load,
    save,
    setPaneWidth,
    resetPane,
    setDock,
    historyColumns,
    setColumnWidth,
    resetColumn,
    columnOrder,
    moveColumn,
    resetColumns,
  };
}

export type LayoutStore = ReturnType<typeof createLayoutStore>;
