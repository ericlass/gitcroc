// Pane sizes, keyed by workspace id (ARCH-9, UI-29). Nothing here knows which
// workspace is shown; callers pass the id.
import { createStore } from "solid-js/store";
import type { WorkspaceId } from "../ipc";

/** Widths in CSS pixels of the two left panes; the changes pane takes the rest. */
export interface PaneSizes {
  tree: number;
  history: number;
}

export type ResizablePane = keyof PaneSizes;

export const MIN_PANE_WIDTH = 200;

export const DEFAULT_PANE_SIZES: Readonly<PaneSizes> = { tree: 280, history: 480 };

/**
 * `width` limited to at least MIN_PANE_WIDTH and at most `max`, when known.
 * The minimum wins when they conflict.
 */
export function clampPaneWidth(width: number, max?: number): number {
  const capped = max === undefined ? width : Math.min(width, max);
  return Math.max(MIN_PANE_WIDTH, Math.round(capped));
}

export function createLayoutStore() {
  const [sizes, setSizes] = createStore<Record<WorkspaceId, PaneSizes>>({});

  const paneSizes = (workspace: WorkspaceId): PaneSizes => sizes[workspace] ?? DEFAULT_PANE_SIZES;

  const setPaneWidth = (workspace: WorkspaceId, pane: ResizablePane, width: number, max?: number) => {
    setSizes(workspace, { ...paneSizes(workspace), [pane]: clampPaneWidth(width, max) });
  };

  const resetPane = (workspace: WorkspaceId, pane: ResizablePane) => {
    setSizes(workspace, { ...paneSizes(workspace), [pane]: DEFAULT_PANE_SIZES[pane] });
  };

  return { paneSizes, setPaneWidth, resetPane };
}

export type LayoutStore = ReturnType<typeof createLayoutStore>;
