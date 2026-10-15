import { unwrap } from "solid-js/store";
import { afterEach, expect, test } from "vitest";
import { emitRepoSummary, fakeState, reset } from "../ipc/fake";
import { hashOf, PAGE_SIZE } from "../ipc/fakeHistory";
import { summary } from "../ipc/fakeTree";
import { createHistoryStore, type HistorySource, sourceOf } from "./history";
import { createSelectionStore } from "./selection";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const id = () => fakeState.workspace.id;

function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      { id: "r1", name: "api", path: "/src/api", remote: null },
      { id: "r2", name: "web", path: "/src/web", remote: null },
    ],
  };
  fakeState.histories = { r1: { length: 1200, prefix: "api" } };
  const selection = createSelectionStore();
  return { history: createHistoryStore(selection), selection };
}

const api = sourceOf("repo:r1", { kind: "repo", repo_id: "r1" }) as HistorySource;
const web = sourceOf("repo:r2", { kind: "repo", repo_id: "r2" }) as HistorySource;

test("PERF-2 pages load on demand, never ahead", async () => {
  const { history } = setup();
  history.show(id(), api);
  await settle();
  expect(history.history(id()).rows.list).toHaveLength(PAGE_SIZE);
  expect(history.history(id()).done).toBe(false);
  expect(fakeState.historyCalls.map((call) => call.command)).toEqual(["get"]);

  await history.more(id());
  await history.more(id());
  expect(history.history(id()).rows.list).toHaveLength(1200);
  expect(history.history(id()).done).toBe(true);
  await history.more(id());
  expect(fakeState.historyCalls.map((call) => call.command)).toEqual(["get", "more", "more"]);
});

test("PERF-2 loaded commits are kept as read, not wrapped by the store", async () => {
  const { history } = setup();
  history.show(id(), api);
  await settle();
  await history.more(id());
  // A wrapped row would keep a proxy and signals for as long as the
  // history is loaded (M1-14).
  const rows = history.history(id()).rows.list;
  expect(rows).toHaveLength(2 * PAGE_SIZE);
  for (const row of [rows[0], rows[PAGE_SIZE]]) {
    expect(row).toBeDefined();
    expect(unwrap(row)).toBe(row);
    expect(unwrap(row?.graph)).toBe(row?.graph);
  }
});

test("section 8 tree selections map to what the history shows", () => {
  expect(sourceOf("ungrouped", { kind: "ungrouped" })).toBeNull();
  expect(sourceOf(null, null)).toBeNull();
  // Nodes that only group a repo's refs show the repo, under its key.
  expect(sourceOf("repo:r1:local", { kind: "local", repo_id: "r1" })).toEqual(api);
  const branch = { repo_id: "r1", name: "main", upstream: null, gone: false, ahead: null, behind: null };
  expect(sourceOf("b", { kind: "branch", current: true, ...branch })).toEqual({
    key: "b",
    repo: "r1",
    target: { kind: "branch", name: "main" },
    uncommitted: true,
  });
  expect(sourceOf("b", { kind: "branch", current: false, ...branch })?.uncommitted).toBe(false);
  expect(sourceOf("s", { kind: "stash", repo_id: "r1", index: 2, hash: "h", message: "m", seconds: 0 })?.target).toEqual({
    kind: "stash",
    index: 2,
  });
  expect(sourceOf("o", { kind: "remote_branch", repo_id: "r1", name: "origin/x", remote: "origin", branch: "x" })?.target).toEqual({
    kind: "remote_branch",
    name: "origin/x",
  });
});

test("ARCH-9 history position is kept per tree selection", async () => {
  const { history, selection } = setup();
  history.show(id(), api);
  await settle();
  await history.more(id());
  const hash = history.history(id()).rows.list[700]?.hash ?? "";
  history.select(id(), hash);
  history.setScrollTop(id(), 700 * 24);
  expect(selection.selected(id())).toEqual({ kind: "commit", repo_id: "r1", hash });

  history.show(id(), web);
  await settle();
  expect(history.history(id()).selected).toBeNull();
  expect(selection.selected(id())).toBeNull();

  // Back: as many rows as before, the same commit selected, the same place.
  history.show(id(), api);
  expect(history.history(id()).selected).toBe(hash);
  expect(selection.selected(id())).toEqual({ kind: "commit", repo_id: "r1", hash });
  await settle();
  await settle();
  await settle();
  expect(history.history(id()).rows.length).toBeGreaterThanOrEqual(1000);
  expect(history.history(id()).restore).toBe(700 * 24);

  // Another workspace has its own history.
  expect(history.history("elsewhere").source).toBeNull();
});

test("UI-12 an older search answer never replaces a newer one", async () => {
  fakeState.historyDelay = 5;
  const { history } = setup();
  history.show(id(), api);
  await new Promise((resolve) => setTimeout(resolve, 10));
  const older = history.setText(id(), "change 1");
  const newer = history.setText(id(), "change 12");
  await Promise.all([older, newer]);
  expect(history.history(id()).text).toBe("change 12");
  const subjects = history.history(id()).rows.list.map((row) => row.subject);
  expect(subjects.every((subject) => subject.includes("change 12"))).toBe(true);
  expect(subjects).toContain("api: change 12");
  expect(fakeState.historyCalls.filter((call) => call.command === "get").map((call) => call.text)).toEqual([
    "",
    "change 1",
    "change 12",
  ]);
});

/** Settles every pending page read. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

test("OPS-7 the history reads again when its refs change, keeping the position and the selection", async () => {
  const { history, selection } = setup();
  const unsubscribe = await history.subscribe(id());
  history.show(id(), api);
  await settle();
  await history.more(id());
  const hash = hashOf(700);
  history.select(id(), hash);
  history.setScrollTop(id(), 700 * 24);
  const calls = fakeState.historyCalls.length;

  // Only the working tree changed: nothing to read.
  emitRepoSummary(summary(id(), "r1", "main"));
  // Another repo's refs changed: not the one shown.
  emitRepoSummary(summary(id(), "r2", "main", { refs_version: 1 }));
  // Queued, not read yet.
  emitRepoSummary(summary(id(), "r1", "main", { refs_version: 1, refreshing: true }));
  await settleAll();
  expect(fakeState.historyCalls).toHaveLength(calls);

  // A commit made elsewhere: one more commit on top.
  fakeState.histories = { r1: { length: 1201, prefix: "api" } };
  emitRepoSummary(summary(id(), "r1", "main", { refs_version: 1 }));
  await settleAll();
  const shown = history.history(id());
  expect(shown.rows.list[0]?.subject).toBe("api: change 1201");
  expect(shown.rows.length).toBeGreaterThanOrEqual(1000);
  expect(shown.selected).toBe(hash);
  expect(shown.scrollTop).toBe(700 * 24);
  expect(shown.loading).toBe(false);
  expect(selection.selected(id())).toEqual({ kind: "commit", repo_id: "r1", hash });

  // The same version again is not news.
  const after = fakeState.historyCalls.length;
  emitRepoSummary(summary(id(), "r1", "main", { refs_version: 1 }));
  await settleAll();
  expect(fakeState.historyCalls).toHaveLength(after);
  unsubscribe();
});

test("OPS-7 a selected commit that no longer exists is deselected", async () => {
  const { history, selection } = setup();
  const unsubscribe = await history.subscribe(id());
  history.show(id(), api);
  await settle();
  await history.more(id());
  await history.more(id());
  history.select(id(), hashOf(1100));

  // A reset elsewhere dropped the older half of the branch.
  fakeState.histories = { r1: { length: 600, prefix: "api" } };
  emitRepoSummary(summary(id(), "r1", "main", { refs_version: 1 }));
  await settleAll();
  expect(history.history(id()).rows.list).toHaveLength(600);
  expect(history.history(id()).selected).toBeNull();
  expect(selection.selected(id())).toBeNull();
  unsubscribe();
});

test("UI-31 a released history reads its rows again and scrolls back, also when a refresh comes meanwhile", async () => {
  const { history } = setup();
  const stop = await history.subscribe(id());
  history.show(id(), api);
  await settle();
  await history.more(id());
  history.select(id(), hashOf(700));
  history.setScrollTop(id(), 690 * 24);

  history.release(id());
  expect(history.history(id()).rows.list).toHaveLength(0);
  expect(history.history(id()).handle).toBeNull();

  // Shown again, a refresh of the repo lands before the rows are read.
  fakeState.historyCalls = [];
  history.show(id(), api);
  emitRepoSummary({ ...summary(id(), "r1", "main"), refs_version: 1 });
  for (let round = 0; round < 8; round++) {
    await settle();
  }
  const shown = history.history(id());
  expect(shown.rows.list).toHaveLength(2 * PAGE_SIZE);
  expect(shown.selected).toBe(hashOf(700));
  expect(shown.restore).toBe(690 * 24);
  expect(shown.released).toBe(false);
  stop();
});
