// Pane sizes, where the changes pane is docked and history column widths,
// keyed by workspace id (ARCH-9, UI-19, UI-29). Nothing here knows which
// workspace is shown; callers pass the id. The dock position and pane sizes
// are kept in the workspace file by the core (section 10); this store starts
// from what the core read and sends it each change once a resize ends.
import { createStore } from "solid-js/store";
import { type Dock, type PaneLayout, type WorkspaceId, workspaceSetLayout } from "../ipc";

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

export type HistoryColumn = keyof HistoryColumns;

export const MIN_COLUMN_WIDTH = 32;

export const DEFAULT_HISTORY_COLUMNS: Readonly<HistoryColumns> = { graph: 80, author: 140, date: 128, hash: 72 };

export function createLayoutStore() {
  const [sizes, setSizes] = createStore<Record<WorkspaceId, PaneSizes>>({});
  const [docks, setDocks] = createStore<Record<WorkspaceId, Dock>>({});
  const [columns, setColumns] = createStore<Record<WorkspaceId, HistoryColumns>>({});
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
  };

  /**
   * Sends the workspace's layout to the core, which keeps it in the
   * workspace file. A read-only workspace refuses it: the layout then lasts
   * this session only, and the workspace's banner already says why.
   */
  const save = (workspace: WorkspaceId) => {
    const current = paneSizes(workspace);
    const layout: PaneLayout = { dock: dock(workspace), ...current };
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

  const setColumnWidth = (workspace: WorkspaceId, column: HistoryColumn, width: number) => {
    setColumns(workspace, { ...historyColumns(workspace), [column]: Math.max(MIN_COLUMN_WIDTH, Math.round(width)) });
  };

  const resetColumn = (workspace: WorkspaceId, column: HistoryColumn) => {
    setColumns(workspace, { ...historyColumns(workspace), [column]: DEFAULT_HISTORY_COLUMNS[column] });
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
  };
}

export type LayoutStore = ReturnType<typeof createLayoutStore>;
