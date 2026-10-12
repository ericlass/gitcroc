import { afterEach, expect, test } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { PAGE_SIZE } from "../ipc/fakeHistory";
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
  expect(history.history(id()).rows).toHaveLength(PAGE_SIZE);
  expect(history.history(id()).done).toBe(false);
  expect(fakeState.historyCalls.map((call) => call.command)).toEqual(["get"]);

  await history.more(id());
  await history.more(id());
  expect(history.history(id()).rows).toHaveLength(1200);
  expect(history.history(id()).done).toBe(true);
  await history.more(id());
  expect(fakeState.historyCalls.map((call) => call.command)).toEqual(["get", "more", "more"]);
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
  expect(sourceOf("s", { kind: "stash", repo_id: "r1", index: 2, message: "m", seconds: 0 })?.target).toEqual({
    kind: "stash",
    index: 2,
  });
  expect(sourceOf("o", { kind: "remote_branch", repo_id: "r1", name: "origin/x" })?.target).toEqual({
    kind: "remote_branch",
    name: "origin/x",
  });
});

test("ARCH-9 history position is kept per tree selection", async () => {
  const { history, selection } = setup();
  history.show(id(), api);
  await settle();
  await history.more(id());
  const hash = history.history(id()).rows[700]?.hash ?? "";
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
  const subjects = history.history(id()).rows.map((row) => row.subject);
  expect(subjects.every((subject) => subject.includes("change 12"))).toBe(true);
  expect(subjects).toContain("api: change 12");
  expect(fakeState.historyCalls.filter((call) => call.command === "get").map((call) => call.text)).toEqual([
    "",
    "change 1",
    "change 12",
  ]);
});
