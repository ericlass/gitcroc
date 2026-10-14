import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fakeState, finishCommit, reset } from "../ipc/fake";
import { type FakeWorkingCopy, textFile } from "../ipc/fakeChanges";
import { createChangesStore } from "../stores/changes";
import { COMMIT_FAILED, createCommitStore } from "../stores/commit";
import { createConflictStore } from "../stores/conflicts";
import { createLayoutStore } from "../stores/layout";
import { createSettingsStore } from "../stores/settings";
import ChangesPane from "./ChangesPane";

// jsdom lays nothing out; give the file list a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("file-list") ? new DOMRect(0, 0, 400, 400) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls and events finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      { id: "r1", name: "api", path: "/src/api", remote: null },
      { id: "r2", name: "web", path: "/src/web", remote: null },
    ],
  };
  fakeState.workingCopies = {
    r1: {
      staged: [textFile("b.txt", [{ add: "staged" }], "added")],
      unstaged: [textFile("a.txt", [{ add: "a" }])],
      unmerged: [],
    },
    r2: { staged: [], unstaged: [textFile("c.txt", [{ add: "c" }])], unmerged: [] },
  };
  const settings = createSettingsStore();
  await settings.load();
  const changes = createChangesStore(settings.changes);
  const commit = createCommitStore();
  render(() => (
    <ChangesPane
      workspace={id()}
      changes={changes}
      commit={commit}
      conflicts={createConflictStore(() => {})}
      settings={settings}
      layout={createLayoutStore()}
      onSelectCommit={() => undefined}
    />
  ));
  const stops = await Promise.all([changes.subscribe(id()), commit.subscribe()]);
  const show = async (repo: string) => {
    fakeState.selected[id()] = repo;
    await changes.show(id(), { kind: "uncommitted", repo_id: repo });
    await settleAll();
  };
  await show("r1");
  return { show, stops };
}

/** Changes the fake working copy of `repo`, as an edit outside the app would. */
const changeWorkingCopy = (repo: string, change: Partial<FakeWorkingCopy>) => {
  const now = fakeState.workingCopies[repo];
  if (now === undefined) {
    throw new Error(`no working copy of ${repo}`);
  }
  fakeState.workingCopies[repo] = { ...now, ...change };
};

const box = () => screen.getByRole("region", { name: "Commit message" });
const subject = () => within(box()).getByRole<HTMLInputElement>("textbox", { name: "Subject" });
const body = () => within(box()).getByRole<HTMLTextAreaElement>("textbox", { name: "Body" });
const commitButton = () => within(box()).getByRole<HTMLButtonElement>("button", { name: "Commit" });
const amendButton = () => within(box()).getByRole<HTMLButtonElement>("button", { name: "Amend" });

const type = (field: HTMLInputElement | HTMLTextAreaElement, text: string) => fireEvent.input(field, { target: { value: text } });

test("UI-15 Commit is disabled without a subject or staged files", async () => {
  const { show } = await setup();
  expect(commitButton().disabled).toBe(true);
  type(body(), "A body alone");
  expect(commitButton().disabled).toBe(true);
  type(subject(), "   ");
  expect(commitButton().disabled).toBe(true);
  type(subject(), "Add b.txt");
  expect(commitButton().disabled).toBe(false);

  // Nothing staged in r2: a subject is not enough.
  await show("r2");
  type(subject(), "Change c.txt");
  expect(commitButton().disabled).toBe(true);
  expect(amendButton().disabled).toBe(false);
  expect(fakeState.commitRequests).toEqual([]);
});

test("UI-15 the message survives a repo switch", async () => {
  const { show } = await setup();
  type(subject(), "Add b.txt");
  type(body(), "Because.");

  await show("r2");
  expect(subject().value).toBe("");
  expect(body().value).toBe("");
  type(subject(), "Something for web");

  await show("r1");
  expect(subject().value).toBe("Add b.txt");
  expect(body().value).toBe("Because.");
  await show("r2");
  expect(subject().value).toBe("Something for web");
});

test("UI-15 a commit sends subject and body, then clears the box", async () => {
  await setup();
  fakeState.holdCommits = true;
  type(subject(), "Add b.txt");
  type(body(), "Because.");
  fireEvent.click(commitButton());
  await settleAll();
  expect(fakeState.commitRequests).toEqual([
    { handle: expect.any(Number) as number, message: { subject: "Add b.txt", body: "Because." }, amend: false },
  ]);
  // While the job runs, nothing else is sent.
  expect(within(box()).getByRole<HTMLButtonElement>("button", { name: "Committing…" }).disabled).toBe(true);
  expect(amendButton().disabled).toBe(true);

  finishCommit();
  await settleAll();
  expect(subject().value).toBe("");
  expect(body().value).toBe("");
  expect(fakeState.workingCopies.r1?.staged).toEqual([]);
  // The view was read again: nothing is staged, so Commit stays off.
  type(subject(), "Next");
  expect(commitButton().disabled).toBe(true);
});

test("UI-15 a failed commit keeps the message", async () => {
  await setup();
  fakeState.commitRejection = "pre-commit: tests failed";
  type(subject(), "Add b.txt");
  type(body(), "Because.");
  fireEvent.click(commitButton());
  await settleAll();

  expect(subject().value).toBe("Add b.txt");
  expect(body().value).toBe("Because.");
  expect(within(box()).getByRole("alert").textContent).toBe(COMMIT_FAILED);
  expect(fakeState.workingCopies.r1?.staged.map((file) => file.file.path)).toEqual(["b.txt"]);
  expect(commitButton().disabled).toBe(false);
});

test("UI-15 a refused commit shows the core's reason and keeps the message", async () => {
  await setup();
  changeWorkingCopy("r1", { staged: [] });
  type(subject(), "Add b.txt");
  // The view still lists b.txt as staged; the core knows better.
  fireEvent.click(commitButton());
  await settleAll();
  expect(within(box()).getByRole("alert").textContent).toBe("nothing is staged");
  expect(subject().value).toBe("Add b.txt");
});

test("UI-15 Amend fills an empty box with the last message, then amends", async () => {
  await setup();
  fireEvent.click(amendButton());
  await settleAll();
  // The box was empty: only filled, nothing sent.
  expect(fakeState.commitRequests).toEqual([]);
  expect(subject().value).toBe("Last commit");
  expect(body().value).toBe("Why it was made.");

  type(subject(), "Last commit, amended");
  fireEvent.click(amendButton());
  await settleAll();
  expect(fakeState.commitRequests).toEqual([
    {
      handle: expect.any(Number) as number,
      message: { subject: "Last commit, amended", body: "Why it was made." },
      amend: true,
    },
  ]);
  expect(subject().value).toBe("");
});

test("UI-15 Amend is disabled on a branch with no commit", async () => {
  const { show } = await setup();
  changeWorkingCopy("r2", { lastMessage: null });
  await show("r2");
  expect(amendButton().disabled).toBe(true);
});

test("UI-15 no commit box in conflict state", async () => {
  const { show } = await setup();
  changeWorkingCopy("r2", { inConflict: true });
  await show("r2");
  expect(screen.queryByRole("region", { name: "Commit message" })).toBeNull();
  await show("r1");
  expect(box()).toBeTruthy();
});
