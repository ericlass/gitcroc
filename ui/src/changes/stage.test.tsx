import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { keep, textFile } from "../ipc/fakeChanges";
import { changesGet } from "../ipc";
import { createChangesStore, FILE_CHANGED, LIST_CHANGED } from "../stores/changes";
import { createCommitStore } from "../stores/commit";
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

/** A file of two hunks, far enough apart. */
const twoHunks = (path: string) =>
  textFile(path, [...keep(1, 2), { remove: "line 3" }, { add: "line three" }, ...keep(4, 20), { add: "added" }, ...keep(24, 2)]);

async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.workingCopies = {
    r1: {
      staged: [textFile("b.txt", [{ add: "staged" }], "added")],
      unstaged: [twoHunks("a.txt"), textFile("c.txt", [{ add: "c" }]), textFile("new.txt", [{ add: "x" }], "untracked")],
      unmerged: [{ path: "z.txt", conflict: "both_modified" }],
    },
  };
  const settings = createSettingsStore();
  await settings.load();
  const changes = createChangesStore(settings.changes);
  render(() => (
    <ChangesPane
      workspace={id()}
      changes={changes}
      commit={createCommitStore()}
      conflicts={createConflictStore(() => {})}
      settings={settings}
      layout={createLayoutStore()}
      onSelectCommit={() => undefined}
    />
  ));
  const unsubscribe = await changes.subscribe(id());
  await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
  await settleAll();
  return { settings, changes, unsubscribe };
}

const files = () => screen.getByRole("listbox", { name: "Changed files" });
const row = (path: string) => {
  const found = within(files())
    .getAllByRole("option")
    .find((option) => option.querySelector(".file-name")?.firstChild?.textContent === path);
  if (!found) {
    throw new Error(`no row ${path}`);
  }
  return found;
};
const listOf = (list: "staged" | "unstaged") =>
  (fakeState.workingCopies.r1?.[list] ?? []).map((file) => file.file.path).sort();
const lastRequest = () => fakeState.changeRequests.at(-1);

test("UI-15 a file's controls stage, unstage and discard it", async () => {
  await setup();
  const handle = () => lastRequest()?.handle;

  fireEvent.click(screen.getByRole("button", { name: "Stage c.txt" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({
    action: "stage",
    target: { kind: "files", files: [{ list: "unstaged", id: 1 }] },
    confirmed: false,
  });
  expect(handle()).toBeTypeOf("number");
  // The fake moved it, and the job's end read the lists again.
  expect(listOf("staged")).toEqual(["b.txt", "c.txt"]);
  expect(row("c.txt").closest(".file-row")).not.toBeNull();
  expect(within(files()).getAllByRole("option").map((option) => option.textContent)).toContain("Mc.txt+1 −0−");

  fireEvent.click(screen.getByRole("button", { name: "Unstage b.txt" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({
    action: "unstage",
    target: { kind: "files", files: [{ list: "staged", id: 0 }] },
  });
  expect(listOf("unstaged")).toContain("b.txt");

  // Unmerged files have no controls until the conflict editor (M2-10).
  expect(within(row("z.txt")).queryAllByRole("button")).toHaveLength(0);
});

test("UI-15 the lists' controls act on the whole list, one request each", async () => {
  await setup();
  fireEvent.click(screen.getByRole("button", { name: "Stage all" }));
  await settleAll();
  expect(fakeState.changeRequests).toHaveLength(1);
  expect(lastRequest()?.target).toEqual({
    kind: "files",
    files: [
      { list: "unstaged", id: 0 },
      { list: "unstaged", id: 1 },
      { list: "unstaged", id: 2 },
    ],
  });
  expect(listOf("unstaged")).toEqual([]);

  fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));
  await settleAll();
  expect(fakeState.changeRequests).toHaveLength(2);
  expect(lastRequest()?.action).toBe("unstage");
  expect(listOf("staged")).toEqual([]);
});

test("UI-15 several files: Ctrl and Shift select, a row's control acts on all of them", async () => {
  await setup();
  fireEvent.click(row("a.txt"));
  fireEvent.click(row("new.txt"), { shiftKey: true });
  await settleAll();
  expect(within(files()).getAllByRole("option", { selected: true })).toHaveLength(3);
  // Ctrl takes one out again.
  fireEvent.click(row("c.txt"), { ctrlKey: true });
  await settleAll();
  expect(within(files()).getAllByRole("option", { selected: true })).toHaveLength(2);
  // A staged file does not join an unstaged selection.
  fireEvent.click(row("b.txt"), { ctrlKey: true });
  await settleAll();
  expect(within(files()).getAllByRole("option", { selected: true })).toHaveLength(1);

  fireEvent.click(row("a.txt"));
  fireEvent.click(row("c.txt"), { ctrlKey: true });
  await settleAll();
  fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));
  await settleAll();
  expect(lastRequest()?.target).toEqual({
    kind: "files",
    files: [
      { list: "unstaged", id: 0 },
      { list: "unstaged", id: 1 },
    ],
  });
});

test("UI-15 double-click moves a row to the other list and the selection stays on it", async () => {
  await setup();
  fireEvent.click(row("c.txt"));
  await settleAll();
  fireEvent.dblClick(row("c.txt"));
  await settleAll();
  expect(lastRequest()).toMatchObject({ action: "stage", target: { kind: "files", files: [{ list: "unstaged", id: 1 }] } });
  // Read again: c.txt is staged now, and still selected.
  const selected = within(files()).getByRole("option", { selected: true });
  expect(selected.textContent).toContain("c.txt");
  const diffs = fakeState.changesCalls.filter((call) => call.command === "diff");
  expect(diffs.at(-1)).toMatchObject({ file: { list: "staged", id: 1 } });

  fireEvent.dblClick(row("c.txt"));
  await settleAll();
  expect(lastRequest()?.action).toBe("unstage");
});

test("UI-15 a hunk's controls send its index", async () => {
  await setup();
  fireEvent.click(row("a.txt"));
  await settleAll();
  const diff = screen.getByRole("region", { name: "Diff" });
  expect(within(diff).getAllByRole("button", { name: /^Stage hunk/ })).toHaveLength(2);
  fireEvent.click(within(diff).getByRole("button", { name: "Stage hunk 2" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({
    action: "stage",
    target: { kind: "hunk", file: { list: "unstaged", id: 0 }, hunk: 1 },
    confirmed: false,
  });
  // A file with hunks left stays unstaged too.
  expect(listOf("unstaged")).toContain("a.txt");
  expect(listOf("staged")).toContain("a.txt");

  // In the staged diff, a hunk is unstaged.
  fireEvent.click(row("b.txt"));
  await settleAll();
  fireEvent.click(within(screen.getByRole("region", { name: "Diff" })).getByRole("button", { name: "Unstage hunk 1" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({ action: "unstage", target: { kind: "hunk", hunk: 0 } });
});

test("UI-15 side by side has the same hunk controls", async () => {
  const { settings, changes } = await setup();
  await settings.setChanges({ diff_mode: "side_by_side" });
  await changes.reloadDiff(id());
  fireEvent.click(row("a.txt"));
  await settleAll();
  expect(document.querySelectorAll(".diff-editor .cm-mergeView .cm-editor")).toHaveLength(2);
  const diff = screen.getByRole("region", { name: "Diff" });
  fireEvent.click(within(diff).getByRole("button", { name: "Stage hunk 1" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({ action: "stage", target: { kind: "hunk", hunk: 0 } });
});

test("UI-15 no hunk controls while whitespace is ignored", async () => {
  const { settings, changes } = await setup();
  await settings.setChanges({ ignore_whitespace: true });
  fireEvent.click(row("a.txt"));
  await settleAll();
  await changes.reloadDiff(id());
  await settleAll();
  const diff = screen.getByRole("region", { name: "Diff" });
  expect(within(diff).queryAllByRole("button", { name: /hunk/ })).toHaveLength(0);
  expect(diff.textContent).toContain('Hunks can be staged with "Ignore whitespace" off.');
  // The file's controls stay.
  expect(screen.getByRole("button", { name: "Stage a.txt" })).toBeDefined();
});

test("GRP-9 discard shows the confirmation first", async () => {
  await setup();
  fireEvent.click(screen.getByRole("button", { name: "Discard c.txt" }));
  await settleAll();
  expect(lastRequest()).toMatchObject({ action: "discard", confirmed: false });
  const dialog = screen.getByRole("alertdialog");
  expect(dialog.textContent).toContain("Discard changes in api?");
  expect(within(dialog).getByText("c.txt")).toBeDefined();
  // Nothing was discarded yet; Cancel has the focus and keeps the changes.
  expect(listOf("unstaged")).toContain("c.txt");
  expect(document.activeElement?.textContent).toBe("Cancel");
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await settleAll();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeState.changeRequests).toHaveLength(1);

  // Confirmed, the same request is sent again with `confirmed`.
  fireEvent.click(screen.getByRole("button", { name: "Discard c.txt" }));
  await settleAll();
  fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Discard" }));
  await settleAll();
  expect(fakeState.changeRequests).toHaveLength(3);
  expect(lastRequest()).toMatchObject({
    action: "discard",
    target: { kind: "files", files: [{ list: "unstaged", id: 1 }] },
    confirmed: true,
  });
  expect(listOf("unstaged")).not.toContain("c.txt");

  // A hunk too, and Escape cancels.
  fireEvent.click(row("a.txt"));
  await settleAll();
  fireEvent.click(within(screen.getByRole("region", { name: "Diff" })).getByRole("button", { name: "Discard hunk 1" }));
  await settleAll();
  const hunk = screen.getByRole("alertdialog");
  expect(hunk.textContent).toContain("This hunk");
  fireEvent.keyDown(hunk, { key: "Escape" });
  await settleAll();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(lastRequest()?.confirmed).toBe(false);
});

test("UI-15 a hunk of a file changed since its diff was read is shown again", async () => {
  await setup();
  fireEvent.click(row("a.txt"));
  await settleAll();
  fakeState.editedFiles = ["a.txt"];
  const views = fakeState.changesCalls.filter((call) => call.command === "changes").length;
  fireEvent.click(within(screen.getByRole("region", { name: "Diff" })).getByRole("button", { name: "Stage hunk 1" }));
  await settleAll();
  expect(screen.getByRole("status").textContent).toBe(FILE_CHANGED);
  // The view and the diff were read again, the same file selected.
  expect(fakeState.changesCalls.filter((call) => call.command === "changes").length).toBe(views + 1);
  expect(within(files()).getByRole("option", { selected: true }).textContent).toContain("a.txt");
  expect(listOf("staged")).toEqual(["b.txt"]);
});

test("UI-15 a click on a list the core has replaced is not lost silently", async () => {
  await setup();
  // A refresh read the view again; its answer is not in yet, so the list
  // shown still has the old handle.
  await changesGet(id(), "r1", { kind: "uncommitted" });
  const views = fakeState.changesCalls.filter((call) => call.command === "changes").length;
  fireEvent.click(within(files()).getByRole("button", { name: "Stage a.txt" }));
  await settleAll();
  expect(fakeState.changeRequests).toHaveLength(1);
  expect(screen.getByRole("status").textContent).toBe(LIST_CHANGED);
  // The list was read again, so the next click reaches the core.
  expect(fakeState.changesCalls.filter((call) => call.command === "changes").length).toBe(views + 1);
  fireEvent.click(within(files()).getByRole("button", { name: "Stage a.txt" }));
  await settleAll();
  expect(listOf("staged")).toContain("a.txt");
});
