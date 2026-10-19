import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { type Conflict, opIntegrate } from "../ipc";
import { fakeState, reset, setConflict } from "../ipc/fake";
import { textFile } from "../ipc/fakeChanges";
import { summary } from "../ipc/fakeTree";
import ChangesPane from "../changes/ChangesPane";
import { createChangesStore } from "../stores/changes";
import { createCommitStore } from "../stores/commit";
import { createConflictStore } from "../stores/conflicts";
import { createLayoutStore } from "../stores/layout";
import { createSettingsStore } from "../stores/settings";
import { createCommandStore } from "../palette/commands";

// jsdom lays nothing out; give the file list a height so rows are rendered,
// and text ranges the empty rectangles CodeMirror measures.
beforeEach(() => {
  Range.prototype.getClientRects = () => ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect();
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

/** Lets several rounds of fake calls and jobs finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

/** A text conflict of `path` with a hunk per entry of `hunks`, a resolved line around each. */
function conflictOf(path: string, hunks: [ours: string, base: string, theirs: string][], change: Partial<Conflict> = {}): Conflict {
  return {
    path,
    labels: { ours: "main", theirs: "feature" },
    content: {
      kind: "text",
      chunks: [
        { kind: "resolved", text: "top\n" },
        ...hunks.flatMap(([ours, base, theirs], index) => [
          { kind: "hunk" as const, ours, base, theirs },
          { kind: "resolved" as const, text: `after ${index + 1}\n` },
        ]),
      ],
    },
    edited_outside: false,
    on_disk: null,
    ...change,
  };
}

async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      { id: "r1", name: "api", path: "/src/api", remote: null },
      { id: "r2", name: "web", path: "/src/web", remote: null },
    ],
  };
  fakeState.summaries = [summary(id(), "r1", "main"), summary(id(), "r2", "main")];
  fakeState.workingCopies = {
    r1: {
      staged: [],
      unstaged: [textFile("notes.txt", [{ add: "x" }])],
      unmerged: [
        { path: "a.txt", conflict: "both_modified" },
        { path: "b.txt", conflict: "both_modified" },
      ],
      inConflict: true,
    },
    r2: { staged: [], unstaged: [], unmerged: [{ path: "c.txt", conflict: "both_modified" }], inConflict: true },
  };
  const settings = createSettingsStore();
  await settings.load();
  const changes = createChangesStore(settings.changes);
  const conflicts = createConflictStore((workspace) => void changes.reload(workspace));
  const stops = [changes.subscribe(id()), conflicts.subscribe(id())];
  render(() => (
    <ChangesPane
      workspace={id()}
      commands={createCommandStore()}
      changes={changes}
      commit={createCommitStore()}
      conflicts={conflicts}
      settings={settings}
      layout={createLayoutStore()}
      onSelectCommit={() => {}}
    />
  ));
  const show = async (repo: string) => {
    await changes.show(id(), { kind: "uncommitted", repo_id: repo });
    await settleAll();
  };
  return { changes, conflicts, show, stops };
}

const files = () => screen.getByRole("listbox", { name: "Changed files" });
const conflictPane = () => screen.getByRole("region", { name: "Conflict" });

/** The shown conflict editor. */
function editor(): EditorView {
  const element = document.querySelector<HTMLElement>(".conflict-editor .cm-editor");
  const view = element === null ? null : EditorView.findFromDOM(element);
  if (view === null) {
    throw new Error("no conflict editor is shown");
  }
  return view;
}

const text = () => editor().state.doc.toString();

/** Types `insert` at `at` as the user would. */
const type = (at: number, insert: string) =>
  editor().dispatch({ changes: { from: at, insert }, selection: { anchor: at + insert.length }, userEvent: "input.type" });

const selectFile = async (name: string) => {
  fireEvent.click(within(files()).getByText(name));
  await settleAll();
};

const rowOf = (name: string) =>
  within(files())
    .getAllByRole("option")
    .find((option) => option.textContent?.includes(name));

test("OPS-3 conflicted files listed first", async () => {
  const { show } = await setup();
  await show("r1");
  const sections = [...document.querySelectorAll(".file-section")].map((section) => section.textContent);
  expect(sections).toEqual(["Unmerged (2)", "Staged (0)", "Unstaged (1)"]);
  const options = within(files()).getAllByRole("option");
  expect(options.map((option) => option.querySelector(".file-name")?.textContent)).toEqual([
    "a.txt (both modified)",
    "b.txt (both modified)",
    "notes.txt",
  ]);
  // The first conflicted file is selected and opened in the conflict editor.
  expect(within(files()).getByRole("option", { selected: true }).textContent).toContain("a.txt");
  expect(within(conflictPane()).getByRole("toolbar", { name: "Conflict" })).toBeDefined();
  // Unmerged files have no stage or discard controls; they are resolved here.
  expect(within(rowOf("a.txt") ?? document.body).queryAllByRole("button")).toHaveLength(0);
});

test("OPS-3 conflict hunks with take ours, theirs and both", async () => {
  fakeState.fileConflicts = {
    "a.txt": conflictOf("a.txt", [
      ["ours 1\n", "base 1\n", "theirs 1\n"],
      ["ours 2\n", "base 2\n", "theirs 2\n"],
      ["ours 3\n", "base 3\n", "theirs 3\n"],
      ["ours 4\n", "base 4\n", "theirs 4\n"],
    ]),
  };
  const { show } = await setup();
  await show("r1");
  const pane = within(conflictPane());
  // The headers name the sides with their labels (OPS-4), and the markers are in the text.
  expect(pane.getAllByText("Ours: main")).toHaveLength(4);
  expect(pane.getAllByText("Theirs: feature")).toHaveLength(4);
  expect(text()).toContain("<<<<<<< main\nours 1\n=======\ntheirs 1\n>>>>>>> feature\nafter 1\n");
  expect(pane.getByRole("status").textContent).toBe("4 conflicts left");

  // The base of a hunk on request, below ours.
  fireEvent.click(pane.getByRole("button", { name: "Show base of conflict 1" }));
  expect(document.querySelector(".cm-conflict-base")?.textContent).toBe("Basebase 1");
  fireEvent.click(pane.getByRole("button", { name: "Hide base of conflict 1" }));
  expect(document.querySelector(".cm-conflict-base")).toBeNull();

  fireEvent.click(pane.getByRole("button", { name: "Take ours in conflict 1" }));
  // The remaining hunks are numbered again.
  fireEvent.click(pane.getByRole("button", { name: "Take theirs in conflict 1" }));
  fireEvent.click(pane.getByRole("button", { name: "Take both in conflict 1" }));
  expect(text()).toBe(
    "top\nours 1\nafter 1\ntheirs 2\nafter 2\nours 3\ntheirs 3\nafter 3\n" +
      "<<<<<<< main\nours 4\n=======\ntheirs 4\n>>>>>>> feature\nafter 4\n",
  );
  expect(pane.getByRole("status").textContent).toBe("1 conflict left");

  // Undo brings the last taken hunk back with its buttons.
  const view = editor();
  view.focus();
  fireEvent.keyDown(view.contentDOM, { key: "z", ctrlKey: true });
  expect(text()).toContain("<<<<<<< main\nours 3\n=======\ntheirs 3\n>>>>>>> feature\n");
  expect(pane.getByRole("button", { name: "Take both in conflict 1" })).toBeDefined();

  // The text stays editable everywhere, inside a hunk too; a changed hunk
  // keeps its highlight and loses its buttons.
  type(0, "new ");
  expect(text().startsWith("new top\n")).toBe(true);
  const inside = text().indexOf("ours 3");
  type(inside, "edited ");
  expect(text()).toContain("<<<<<<< main\nedited ours 3\n");
  expect(pane.queryByRole("button", { name: "Take ours in conflict 1" })).toBeNull();
  expect(pane.getByRole("button", { name: "Take ours in conflict 2" })).toBeDefined();
  expect(document.querySelectorAll(".cm-conflict-changed").length).toBeGreaterThan(0);

  // The keyboard: next conflict and take, at the cursor.
  view.dispatch({ selection: { anchor: 0 } });
  fireEvent.keyDown(view.contentDOM, { key: "ArrowDown", altKey: true });
  fireEvent.keyDown(view.contentDOM, { key: "ArrowDown", altKey: true });
  fireEvent.keyDown(view.contentDOM, { key: "2", code: "Digit2", altKey: true });
  expect(text().endsWith("after 3\ntheirs 4\nafter 4\n")).toBe(true);
  // Nothing was sent: the buffer lives in the UI until Mark resolved (OPS-9).
  expect(fakeState.conflictRequests).toEqual([]);
});

test("OPS-9 unsaved edits survive switching files and repos", async () => {
  const { show } = await setup();
  await show("r1");
  expect(rowOf("a.txt")?.querySelector(".file-unsaved")).toBeNull();
  type(0, "mine ");
  editor().dispatch({ selection: { anchor: 2 } });
  const edited = text();
  expect(rowOf("a.txt")?.querySelector(".file-unsaved")).not.toBeNull();

  await selectFile("b.txt");
  expect(text().startsWith("first\n")).toBe(true);
  type(0, "other ");
  await selectFile("a.txt");
  expect(text()).toBe(edited);
  expect(editor().state.selection.main.head).toBe(2);
  // Undo still knows the edits.
  fireEvent.keyDown(editor().contentDOM, { key: "z", ctrlKey: true });
  expect(text()).not.toBe(edited);
  type(0, "mine ");

  await show("r2");
  expect(within(files()).getByRole("option", { selected: true }).textContent).toContain("c.txt");
  expect(text().startsWith("first\n")).toBe(true);
  await show("r1");
  expect(text()).toBe(edited);
  expect(rowOf("b.txt")?.querySelector(".file-unsaved")).not.toBeNull();
  // A file read again after its conflict was read keeps its buffer: one read each.
  expect(fakeState.conflictRequests).toEqual([]);
});

test("OPS-9 unsaved edits are dropped on abort", async () => {
  const { show } = await setup();
  fakeState.refModels = { r1: { local: ["main", "feature"], current: "main" } };
  const merging = { operation: "merge" as const, applying: "feature", onto: "main", step: null, unmerged_paths: 2 };
  setConflict("r1", merging);
  await show("r1");
  type(0, "mine ");
  await selectFile("b.txt");
  type(0, "other ");
  await show("r2");
  type(0, "kept ");

  await opIntegrate(id(), { repo_id: "r1", op: { kind: "abort" }, confirmed: false });
  await settleAll();

  // The same files conflict again, as after merging once more.
  fakeState.workingCopies.r1 = {
    ...fakeState.workingCopies.r1,
    staged: [],
    unstaged: [],
    unmerged: [
      { path: "a.txt", conflict: "both_modified" },
      { path: "b.txt", conflict: "both_modified" },
    ],
  };
  setConflict("r1", merging);
  await show("r1");
  expect(text().startsWith("first\n")).toBe(true);
  expect(document.querySelectorAll(".file-unsaved")).toHaveLength(0);
  await selectFile("b.txt");
  expect(text().startsWith("first\n")).toBe(true);
  // The other repo is still in conflict state: its edits stay.
  await show("r2");
  expect(text().startsWith("kept first\n")).toBe(true);
});

test("OPS-5 mark resolved anyway", async () => {
  const { show } = await setup();
  await show("r1");
  const pane = within(conflictPane());
  fireEvent.click(pane.getByRole("button", { name: "Mark resolved" }));
  await settleAll();
  // The hunk was left as it is: its first marker is line 2.
  expect(pane.getByRole("alert").textContent).toContain("Line 2 is still a conflict marker line");
  expect(rowOf("a.txt")).toBeDefined();

  fireEvent.click(pane.getByRole("button", { name: "Mark resolved anyway" }));
  await settleAll();
  expect(fakeState.conflictRequests.map((request) => "override_markers" in request && request.override_markers)).toEqual([
    false,
    true,
  ]);
  expect(fakeState.conflictRequests.at(-1)).toMatchObject({ text: expect.stringContaining("<<<<<<< main\n") });
  // The file left the unmerged list; the next one is shown.
  expect(rowOf("a.txt")).toBeUndefined();
  expect(within(files()).getByRole("option", { selected: true }).textContent).toContain("b.txt");

  // Resolved text goes through without the override, as the buffer reads.
  fireEvent.click(within(conflictPane()).getByRole("button", { name: "Take theirs in conflict 1" }));
  fireEvent.click(within(conflictPane()).getByRole("button", { name: "Mark resolved" }));
  await settleAll();
  expect(fakeState.conflictRequests.at(-1)).toEqual({ handle: expect.any(Number), file: 0, text: "first\ntheirs\nlast\n", override_markers: false });
  expect(rowOf("b.txt")).toBeUndefined();
});

test("OPS-6 choice conflict shows both versions", async () => {
  fakeState.fileConflicts = {
    "a.txt": {
      path: "a.txt",
      labels: { ours: "main", theirs: "abc1234 Add the logo (feature)" },
      content: {
        kind: "choice",
        reason: "deleted_by_them",
        ours: { kind: "file", size: 1234, executable: true },
        theirs: { kind: "deleted" },
      },
      edited_outside: false,
      on_disk: null,
    },
    "b.txt": {
      path: "b.txt",
      labels: { ours: "main", theirs: "feature" },
      content: {
        kind: "choice",
        reason: "submodule",
        ours: { kind: "submodule", commit: "0123456789abcdef0123456789abcdef01234567" },
        theirs: { kind: "symlink", size: 9 },
      },
      edited_outside: false,
      on_disk: null,
    },
  };
  const { show } = await setup();
  await show("r1");
  const pane = within(conflictPane());
  expect(pane.getByText(/Their side deleted the file/)).toBeDefined();
  const ours = within(pane.getByRole("region", { name: "Ours: main" }));
  expect(ours.getByText("File, 1,234 bytes, executable")).toBeDefined();
  const theirs = within(pane.getByRole("region", { name: "Theirs: abc1234 Add the logo (feature)" }));
  expect(theirs.getByText("Deleted")).toBeDefined();
  expect(document.querySelector(".conflict-editor")).toBeNull();

  fireEvent.click(theirs.getByRole("button", { name: "Use this version" }));
  await settleAll();
  expect(fakeState.conflictRequests).toEqual([{ handle: expect.any(Number), file: 0, side: "theirs" }]);
  expect(rowOf("a.txt")).toBeUndefined();

  const next = within(conflictPane());
  expect(within(next.getByRole("region", { name: "Ours: main" })).getByText("Submodule at commit 0123456789ab")).toBeDefined();
  expect(within(next.getByRole("region", { name: "Theirs: feature" })).getByText("Symbolic link, 9 bytes")).toBeDefined();
});

test("OPS-8 edited outside asks first", async () => {
  const onDisk = "top\n<<<<<<< HEAD\nmine, edited\n=======\ntheirs\n>>>>>>> feature\n";
  fakeState.fileConflicts = {
    "a.txt": conflictOf("a.txt", [["ours\n", "base\n", "theirs\n"]], { edited_outside: true, on_disk: onDisk }),
    "b.txt": conflictOf("b.txt", [["ours\n", "base\n", "theirs\n"]], { edited_outside: true, on_disk: "edited\n" }),
  };
  const { show } = await setup();
  await show("r1");
  const ask = within(screen.getByRole("group", { name: "Edited outside" }));
  expect(ask.getByText(/a\.txt was changed outside gitcroc/)).toBeDefined();
  expect(document.querySelector(".conflict-editor")).toBeNull();

  // Keep it: the disk text, plain, without hunk buttons.
  fireEvent.click(ask.getByRole("button", { name: "Keep the file as it is" }));
  expect(text()).toBe(onDisk);
  expect(within(conflictPane()).queryByRole("button", { name: /^Take/ })).toBeNull();
  expect(within(conflictPane()).getByRole("status").textContent).toBe("No conflict left");

  // Start over: the conflict's hunks. The answer is kept with the file.
  await selectFile("b.txt");
  fireEvent.click(within(screen.getByRole("group", { name: "Edited outside" })).getByRole("button", { name: "Start over from the conflict" }));
  expect(text()).toContain("<<<<<<< main\nours\n");
  expect(within(conflictPane()).getByRole("button", { name: "Take ours in conflict 1" })).toBeDefined();
  await selectFile("a.txt");
  expect(text()).toBe(onDisk);
});
