// The UI-side measurements of `just bench-ui` (PERF-8, M1-12), in
// `vite build --mode perf` only. bench/bench-ui.ts drives the app through
// WebDriver and calls `window.__gitcrocPerf`. This records the time from a
// user action to the frame after its result is rendered, and the frame times
// while scrolling, and writes them through `perf_trace_write`. It measures;
// it changes nothing the app does.
import { type PerfSample, perfTraceWrite } from "../ipc";
import { startMarks } from "./marks";

/** Must match ROW_HEIGHT in src/history/HistoryPane.tsx. */
const ROW_HEIGHT = 24;

/** How long a measured action may take before it counts as stuck. */
const GIVE_UP_MS = 30_000;

const frame = () => new Promise<number>((resolve) => requestAnimationFrame(resolve));

const historyList = () => document.querySelector<HTMLElement>('[role="listbox"][aria-label="History"]');

/** The rendered history rows with content, top first. */
function historyRows(): HTMLElement[] {
  const list = historyList();
  if (list === null) {
    return [];
  }
  return [...list.querySelectorAll<HTMLElement>(".history-row:not(.placeholder)")].sort(
    (a, b) => Number(a.dataset.index) - Number(b.dataset.index),
  );
}

/** What identifies the top rendered history row: its hash, or its text for the uncommitted row. */
function firstRow(): string | null {
  const row = historyRows()[0];
  if (row === undefined) {
    return null;
  }
  return row.querySelector(".history-hash")?.textContent ?? row.textContent;
}

/** A placeholder row is on screen in the history. */
function blankVisible(list: HTMLElement): boolean {
  const view = list.getBoundingClientRect();
  return [...list.querySelectorAll(".history-row.placeholder")].some((row) => {
    const box = row.getBoundingClientRect();
    return box.bottom > view.top && box.top < view.bottom;
  });
}

/** What a measured action waits for, checked once a frame. */
const conditions = {
  /** The tree shows repos with their cached summaries. */
  tree: () => document.querySelector('[role="tree"] .tree-repo .tree-head') !== null,
  /** The history shows a top row other than `before`. */
  history: (before: string) => {
    const first = firstRow();
    return first !== null && first !== before;
  },
  /** The changes pane shows commit `hash`, its file list and its first file's diff. */
  commit: (hash: string) =>
    document.querySelector(".commit-header .commit-hash")?.textContent === hash &&
    document.querySelector(".file-list [role='option']") !== null &&
    document.querySelector(".changes-diff .cm-line, .changes-diff .diff-special") !== null,
  /** The focused input holds `value`. */
  input: (value: string) => (document.activeElement as HTMLInputElement | null)?.value === value,
};

type Condition = keyof typeof conditions;

interface Result {
  ms: number;
  /** What the scenario waits for was running when the action started: see {@link busyWhen}. */
  busy: boolean;
}

/**
 * What makes an action busy: a refresh (a repo row shows its spinner), or a
 * job (the status bar shows one running, M2-15).
 */
const busyWhen = {
  refresh: () => document.querySelector(".tree-pane .spinner") !== null,
  job: () => document.querySelector(".status-bar .job-running") !== null,
};

type Busy = keyof typeof busyWhen;

/** What the panes show, for an error message. */
function snapshot(): string {
  const text = (selector: string) => document.querySelector(selector)?.textContent?.slice(0, 120) ?? null;
  return JSON.stringify({
    historyTop: firstRow(),
    historyScroll: historyList()?.scrollTop ?? null,
    selected: document.querySelector(".history-row[aria-selected='true'] .history-hash")?.textContent ?? null,
    active: document.activeElement?.className ?? null,
    commit: text(".commit-header .commit-hash"),
    files: document.querySelectorAll(".file-list [role='option']").length,
    diff: text(".changes-diff"),
  });
}

const samples: PerfSample[] = [];
let treeVisibleUnixMs: number | null = null;
let treeVisible: Promise<void> | null = null;
const pending = new Map<string, Promise<Result>>();

/** Resolves on the frame after `check` first holds, with that time. */
async function until(check: () => boolean, since: number): Promise<number> {
  while (!check()) {
    await frame();
    if (performance.now() - since > GIVE_UP_MS) {
      throw new Error(`gave up waiting; the page shows ${snapshot()}`);
    }
  }
  // The frame that renders the result, then the time the next one starts.
  await frame();
  return performance.now();
}

/**
 * Times the next user action: from its first pointer or key event to the
 * frame after `condition` holds. The driver arms, acts through WebDriver,
 * then awaits `result(scenario)`.
 */
function arm(scenario: string, condition: Condition, argument: string, busy: Busy = "refresh") {
  const events = ["pointerdown", "mousedown", "keydown"] as const;
  const check = () => conditions[condition](argument);
  const result = new Promise<Result>((resolve, reject) => {
    const armed = performance.now();
    const timer = setTimeout(() => {
      stop();
      reject(new Error(`${scenario}: no action within ${GIVE_UP_MS} ms`));
    }, GIVE_UP_MS);
    const onAction = (event: Event) => {
      stop();
      const start = event.timeStamp;
      const running = busyWhen[busy]();
      until(check, armed).then((end) => {
        const ms = end - start;
        samples.push({ scenario: running || condition !== "input" ? scenario : `${scenario}-idle`, ms });
        resolve({ ms, busy: running });
      }, reject);
    };
    const stop = () => {
      clearTimeout(timer);
      for (const name of events) {
        window.removeEventListener(name, onAction, true);
      }
    };
    for (const name of events) {
      window.addEventListener(name, onAction, true);
    }
  });
  // Rejections surface through `result`.
  result.catch(() => {});
  pending.set(scenario, result);
}

function result(scenario: string): Promise<Result> {
  const measured = pending.get(scenario);
  pending.delete(scenario);
  return measured ?? Promise.reject(new Error(`${scenario} was not armed`));
}

/**
 * Scrolls the history down `pxPerFrame` each frame, from the top, until the
 * end shows with no blank row or `maxFrames` pass. Records each frame's time
 * as `scroll-frame` and the longest time a blank row stayed on screen as
 * `scroll-blank`.
 */
async function scroll(pxPerFrame: number, maxFrames: number) {
  const list = historyList();
  if (list === null) {
    throw new Error("no history is shown");
  }
  list.scrollTop = 0;
  await frame();
  let last = await frame();
  let blankSince: number | null = null;
  let longestBlank = 0;
  let frames = 0;
  let atEnd = false;
  const times: number[] = [];
  while (frames < maxFrames) {
    list.scrollTop += pxPerFrame;
    const now = await frame();
    times.push(now - last);
    last = now;
    frames++;
    if (blankVisible(list)) {
      blankSince ??= now;
    } else if (blankSince !== null) {
      longestBlank = Math.max(longestBlank, now - blankSince);
      blankSince = null;
    }
    atEnd = list.scrollTop + list.clientHeight >= list.scrollHeight - 1;
    if (atEnd && blankSince === null) {
      break;
    }
  }
  if (blankSince !== null) {
    longestBlank = Math.max(longestBlank, last - blankSince);
  }
  for (const ms of times) {
    samples.push({ scenario: "scroll-frame", ms });
  }
  samples.push({ scenario: "scroll-blank", ms: longestBlank });
  return { frames, atEnd, longestBlank, rows: Math.round(list.scrollHeight / ROW_HEIGHT) };
}

/** The tree row of the repo named `name`. */
function repoRow(name: string): Element | null {
  return (
    [...document.querySelectorAll('[role="tree"] [role="treeitem"]')].find(
      (row) => row.querySelector(".tree-repo .tree-label")?.textContent === name,
    ) ?? null
  );
}

/**
 * Scrolls history row `index` to the middle and returns it once its commit
 * `hash` shows. The list re-creates rows as it scrolls, so the row is looked
 * up again after a few quiet frames, when it will stay.
 */
async function historyRow(index: number, hash: string): Promise<Element> {
  const list = historyList();
  if (list === null) {
    throw new Error("no history is shown");
  }
  list.scrollTop = Math.max(0, index * ROW_HEIGHT - list.clientHeight / 2);
  const find = () =>
    historyRows().find((row) => {
      const short = row.querySelector(".history-hash")?.textContent;
      return !!short && hash.startsWith(short);
    });
  const since = performance.now();
  while (find() === undefined) {
    await frame();
    if (performance.now() - since > GIVE_UP_MS) {
      throw new Error(`commit ${hash} is not at row ${index}`);
    }
  }
  for (let quiet = 0; quiet < 3; quiet++) {
    await frame();
  }
  const row = find();
  if (row === undefined) {
    throw new Error(`commit ${hash} left the screen`);
  }
  return row;
}

/** Resolves once no repo is refreshing and none has for `quietMs`. */
async function idle(quietMs: number) {
  let quietSince = performance.now();
  for (;;) {
    const now = await frame();
    if (document.querySelector(".tree-pane .spinner") !== null) {
      quietSince = now;
    } else if (now - quietSince >= quietMs) {
      return;
    }
  }
}

/** Writes everything measured so far through `perf_trace_write`. */
function flush() {
  return perfTraceWrite({ tree_visible_unix_ms: treeVisibleUnixMs, samples, marks: startMarks() });
}

const recorder = {
  ready: () => treeVisible,
  arm,
  result,
  scroll,
  repoRow,
  historyRow,
  firstRow,
  idle,
  flush,
};

declare global {
  interface Window {
    __gitcrocPerf?: typeof recorder;
  }
}

/** Starts recording; the cold start ends on the frame after the tree shows cached repos. */
export function install() {
  treeVisible = until(conditions.tree, performance.now()).then((end) => {
    treeVisibleUnixMs = performance.timeOrigin + end;
    performance.mark("tree visible", { startTime: end });
    // For a start without WebDriver (`bench-ui --direct`), which reads the
    // trace and ends the app. Later flushes write it again.
    void flush().catch(() => {});
  });
  // The app replaces "Looking for git…" once the core reports git (M1-15).
  void until(() => document.querySelector("main.message-screen") === null, performance.now()).then((end) =>
    performance.mark("past the git check", { startTime: end }),
  );
  window.__gitcrocPerf = recorder;
}
