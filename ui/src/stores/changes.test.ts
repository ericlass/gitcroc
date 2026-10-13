import { afterEach, expect, test } from "vitest";
import { emitRepoSummary, fakeState, reset } from "../ipc/fake";
import { textFile } from "../ipc/fakeChanges";
import { hashOf } from "../ipc/fakeHistory";
import { summary } from "../ipc/fakeTree";
import { createChangesStore, diffRequest } from "./changes";
import { DEFAULT_CHANGES_SETTINGS } from "./settings";

afterEach(reset);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const id = () => fakeState.workspace.id;

function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  return createChangesStore(() => DEFAULT_CHANGES_SETTINGS);
}

test("UI-17 side-by-side asks the core for whole files", () => {
  expect(diffRequest(DEFAULT_CHANGES_SETTINGS, false)).toEqual({
    ignore_whitespace: false,
    context: "three",
    load_anyway: false,
  });
  const sideBySide = { ...DEFAULT_CHANGES_SETTINGS, diff_mode: "side_by_side" as const, ignore_whitespace: true };
  expect(diffRequest(sideBySide, true)).toEqual({ ignore_whitespace: true, context: "whole_file", load_anyway: true });
});

test("ARCH-8 an older answer never replaces a newer selection", async () => {
  fakeState.changesDelay = 5;
  const changes = setup();
  const older = changes.show(id(), { kind: "commit", repo_id: "r1", hash: hashOf(1) });
  const newer = changes.show(id(), { kind: "commit", repo_id: "r1", hash: hashOf(2) });
  await Promise.all([older, newer]);
  const view = changes.changes(id()).view;
  expect(view?.content.kind === "commit" && view.content.details.hash).toBe(hashOf(2));
  expect(changes.changes(id()).diff?.kind).toBe("text");
  // The same selection again reads nothing.
  const calls = fakeState.changesCalls.length;
  await changes.show(id(), { kind: "commit", repo_id: "r1", hash: hashOf(2) });
  expect(fakeState.changesCalls).toHaveLength(calls);
  // Another workspace has its own.
  expect(changes.changes("elsewhere").selection).toBeNull();
});

test("ARCH-6 uncommitted changes are read again when the repo changes, keeping the file", async () => {
  fakeState.workingCopies = {
    r1: { staged: [], unstaged: [textFile("a.txt", [{ add: "a" }]), textFile("b.txt", [{ add: "b" }])], unmerged: [] },
  };
  const changes = setup();
  const unsubscribe = await changes.subscribe(id());
  await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
  await changes.selectFile(id(), { list: "unstaged", id: 1 });

  // A file before it appears: b.txt has another id now, and stays selected.
  fakeState.workingCopies = {
    r1: {
      staged: [],
      unstaged: [
        textFile("0.txt", [{ add: "0" }]),
        textFile("a.txt", [{ add: "a" }]),
        textFile("b.txt", [{ add: "b" }]),
      ],
      unmerged: [],
    },
  };
  emitRepoSummary(summary(id(), "r1", "main"));
  for (let round = 0; round < 5; round++) {
    await settle();
  }
  expect(changes.changes(id()).file).toEqual({ list: "unstaged", id: 2 });
  unsubscribe();
});

test("OPS-7 a repo queued for a refresh is read again only once its summary arrives", async () => {
  const changes = setup();
  const unsubscribe = await changes.subscribe(id());
  await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
  const calls = fakeState.changesCalls.length;

  emitRepoSummary(summary(id(), "r1", "main", { refreshing: true }));
  await settle();
  expect(fakeState.changesCalls).toHaveLength(calls);

  emitRepoSummary(summary(id(), "r1", "main"));
  await settle();
  expect(fakeState.changesCalls.length).toBeGreaterThan(calls);
  unsubscribe();
});
