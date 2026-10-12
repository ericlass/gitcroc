// Histories for the fake core (fake.ts): a synthetic history of any length
// with lanes like the core's, paged and filtered like the core does it.
// Test support only; the real history and its lanes come from the core.
import type { CommitRow, Edge, GraphRow, HistoryTarget, RefLabel } from "./bindings";

/** Commits on one page (ARCH-8). */
export const PAGE_SIZE = 500;

/** A generated history. */
export interface FakeHistory {
  /** How many commits it has. */
  length: number;
  /** Starts each subject, so tests can tell histories apart. */
  prefix: string;
  /** Labels by row index. */
  refs?: Record<number, RefLabel[]>;
}

const AUTHORS = [
  ["Ada Lovelace", "ada@example.com"],
  ["Grace Hopper", "grace@example.com"],
  ["Alan Turing", "alan@example.com"],
] as const;

/** 2024-01-01 00:00 UTC; each older commit is an hour earlier. */
const NEWEST = 1_704_067_200;

const hashOf = (index: number) => index.toString(16).padStart(8, "0").padEnd(40, "c");

const edge = (from: number, to: number, color: number): Edge => ({ from, to, color });

/**
 * The graph of row `index`: a main line on lane 0 and, in every block of
 * ten rows, a side branch of three commits on lane 1 merged at the block's
 * first row.
 */
function graphOf(index: number, length: number): GraphRow {
  const block = Math.floor(index / 10);
  const at = index % 10;
  const side = 1 + (block % 7);
  const last = index === length - 1;
  const up: Edge[] = index === 0 ? [] : [edge(0, 0, 0)];
  const down: Edge[] = last ? [] : [edge(0, 0, 0)];
  if (at === 0 && index + 4 < length) {
    down.push(edge(0, 1, side));
  } else if (at >= 1 && at <= 3 && Math.floor((index - at) / 10) * 10 + 4 < length) {
    up.push(edge(1, 1, side));
    down.push(at === 3 ? edge(1, 0, side) : edge(1, 1, side));
    return { lane: 1, color: side, up, down };
  }
  return { lane: 0, color: 0, up, down };
}

export function commitAt(history: FakeHistory, index: number): CommitRow {
  const [name, email] = AUTHORS[index % AUTHORS.length] ?? AUTHORS[0];
  const hash = hashOf(index);
  const seconds = NEWEST - index * 3600;
  return {
    hash,
    short_hash: hash.slice(0, 7),
    subject: `${history.prefix}: change ${history.length - index}`,
    author_name: name,
    author_email: email,
    author_time: { seconds, offset: 0 },
    commit_time: { seconds: seconds + 60, offset: 0 },
    parents: index + 1 < history.length ? [hashOf(index + 1)] : [],
    refs: history.refs?.[index] ?? [],
    graph: graphOf(index, history.length),
  };
}

/** Like the core's filter: subject, author name or email, or hash prefix. */
function matches(row: CommitRow, text: string): boolean {
  const lower = text.toLowerCase();
  return (
    row.subject.toLowerCase().includes(lower) ||
    row.author_name.toLowerCase().includes(lower) ||
    row.author_email.toLowerCase().includes(lower) ||
    row.hash.startsWith(lower)
  );
}

/** Where a fake walk continues. */
export interface FakeWalk {
  history: FakeHistory;
  text: string;
  next: number;
}

/** The next page of `walk`, which it advances; filtered rows have no graph. */
export function nextPage(walk: FakeWalk): { rows: CommitRow[]; done: boolean } {
  const rows: CommitRow[] = [];
  while (rows.length < PAGE_SIZE && walk.next < walk.history.length) {
    const row = commitAt(walk.history, walk.next++);
    if (walk.text === "") {
      rows.push(row);
    } else if (matches(row, walk.text)) {
      rows.push({ ...row, graph: null });
    }
  }
  return { rows, done: walk.next >= walk.history.length };
}

/** What a target shows when the fake state has nothing for it. */
export function defaultHistory(repoName: string, target: HistoryTarget): FakeHistory {
  switch (target.kind) {
    case "repo":
      return { length: 40, prefix: repoName };
    case "stash":
      return { length: 1, prefix: `stash@{${target.index}}` };
    default:
      return { length: 20, prefix: target.name };
  }
}

/** The key of a target in `fakeState.histories`. */
export function historyKey(repoId: string, target: HistoryTarget): string {
  switch (target.kind) {
    case "repo":
      return repoId;
    case "stash":
      return `${repoId}:stash:${target.index}`;
    default:
      return `${repoId}:${target.kind}:${target.name}`;
  }
}
