// The history as rows on screen: the "Uncommitted changes" row (UI-10,
// UI-34), the loaded commits, then placeholders for the page that loads next (PERF-2).
// Rows are found by index, so nothing is built per row of a long history.
import type { HistoryRow } from "../ipc";
import { rowIdOf, UNCOMMITTED_ID, type WorkspaceHistory } from "../stores/history";

/** Placeholder rows past the loaded ones while more may follow: one page. */
export const PLACEHOLDER_ROWS = 500;

export type ListRow =
  /** `count` is of changed paths, or in a merged history of dirty repos (`repos`). */
  | { kind: "uncommitted"; id: string; count: number; repos: boolean }
  | { kind: "commit"; id: string; commit: HistoryRow }
  | { kind: "placeholder"; id: string };

/** What the list shows of `history`, with `changes` uncommitted paths, or dirty repos when merged. */
export interface HistoryRows {
  count: number;
  /** Rows before the first commit. */
  offset: number;
  /** Index of the first placeholder; `count` when there are none. */
  loaded: number;
  at: (index: number) => ListRow | undefined;
  indexOf: (id: string) => number;
}

/**
 * The rows of `history`. The uncommitted row leads when the source allows
 * it, the working copy has changes and no search is shown.
 */
export function historyRows(history: WorkspaceHistory, changes: number): HistoryRows {
  const uncommitted = history.source?.uncommitted === true && changes > 0 && history.text === "";
  const offset = uncommitted ? 1 : 0;
  const rows = history.rows.list;
  const loaded = offset + rows.length;
  const count = loaded + (history.done ? 0 : PLACEHOLDER_ROWS);
  const at = (index: number): ListRow | undefined => {
    if (index < 0 || index >= count) {
      return undefined;
    }
    if (index < offset) {
      return { kind: "uncommitted", id: UNCOMMITTED_ID, count: changes, repos: history.source?.repo === null };
    }
    const commit = rows[index - offset];
    return commit
      ? { kind: "commit", id: rowIdOf(history.source, commit), commit }
      : { kind: "placeholder", id: `placeholder:${index}` };
  };
  const indexOf = (id: string) => {
    if (id === UNCOMMITTED_ID) {
      return uncommitted ? 0 : -1;
    }
    const found = rows.findIndex((row) => rowIdOf(history.source, row) === id);
    return found < 0 ? -1 : found + offset;
  };
  return { count, offset, loaded, at, indexOf };
}

const pad = (value: number) => String(value).padStart(2, "0");

/** `seconds` as local time `YYYY-MM-DD HH:MM` (UI-9). */
export function formatDate(seconds: number): string {
  const date = new Date(seconds * 1000);
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** `seconds` in full, local time, for a tooltip. */
export function formatFullDate(seconds: number): string {
  return new Date(seconds * 1000).toString();
}
