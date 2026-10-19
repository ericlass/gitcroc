import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { defaultDetails, keep, otherFile, PNG_1X1, textFile } from "../ipc/fakeChanges";
import { hashOf } from "../ipc/fakeHistory";
import { createChangesStore } from "../stores/changes";
import { createCommitStore } from "../stores/commit";
import { createConflictStore } from "../stores/conflicts";
import { createLayoutStore } from "../stores/layout";
import type { CommitSelection } from "../stores/selection";
import { createSettingsStore } from "../stores/settings";
import ChangesPane from "./ChangesPane";
import { createCommandStore } from "../palette/commands";

// jsdom lays nothing out; give the file list a height so rows are rendered,
// and stand in for the blob URLs it cannot make.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("file-list") ? new DOMRect(0, 0, 400, 400) : real.call(this);
  });
  let next = 0;
  URL.createObjectURL = vi.fn(() => `blob:fake/${next++}`);
  URL.revokeObjectURL = vi.fn();
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls finish. */
const settleAll = async () => {
  for (let round = 0; round < 5; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

const commit = (hash: string): CommitSelection => ({ kind: "commit", repo_id: "r1", hash });

async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  const settings = createSettingsStore();
  await settings.load();
  const layout = createLayoutStore();
  const changes = createChangesStore(settings.changes);
  const onSelectCommit = vi.fn();
  render(() => (
    <ChangesPane
      workspace={id()}
      commands={createCommandStore()}
      changes={changes}
      commit={createCommitStore()}
      conflicts={createConflictStore(() => {})}
      settings={settings}
      layout={layout}
      onSelectCommit={onSelectCommit}
    />
  ));
  return { settings, layout, changes, onSelectCommit };
}

/** The rendered file rows, top to bottom, without their buttons. */
const fileRowTexts = () =>
  [...document.querySelectorAll<HTMLElement>(".file-row")]
    .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index))
    .map((row) => {
      const copy = row.cloneNode(true) as HTMLElement;
      copy.querySelectorAll("button").forEach((button) => button.remove());
      return copy.textContent ?? "";
    });

const diffCalls = () => fakeState.changesCalls.filter((call) => call.command === "diff");

test("UI-14 commit header and file list", async () => {
  const { changes, onSelectCommit } = await setup();
  const hash = hashOf(3);
  await changes.show(id(), commit(hash));
  await settleAll();

  // Header: full hash, author and committer with dates, parents, the full message.
  const header = within(screen.getByRole("region", { name: "Commit" }));
  expect(header.getByText(hash)).toBeDefined();
  expect(header.getByText("Ada Lovelace")).toBeDefined();
  expect(header.getByText("Grace Hopper")).toBeDefined();
  expect(header.getAllByText(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/)).toHaveLength(2);
  const message = header.getByText(/A body with/);
  expect(message.textContent).toBe(defaultDetails(hash, []).message);
  // SEC-3: the message is text; its markup is not.
  expect(message.querySelector("b")).toBeNull();
  fireEvent.click(header.getByRole("button", { name: hashOf(4).slice(0, 10) }));
  expect(onSelectCommit).toHaveBeenCalledWith(hashOf(4));

  // The files, flat, in the core's order; the first is selected and its diff shown.
  expect(fileRowTexts().map((text) => text.replace(/\+.*/, ""))).toEqual([
    "MREADME.md",
    "Massets/logo.png",
    "Msrc/main.rs",
  ]);
  const files = screen.getByRole("listbox", { name: "Changed files" });
  expect(within(files).getByRole("option", { selected: true }).textContent).toContain("README.md");
  expect(diffCalls()).toEqual([
    {
      command: "diff",
      file: { list: "commit", id: 0 },
      request: { ignore_whitespace: false, context: "three", load_anyway: false, full_text: false },
    },
  ]);
  expect(document.querySelector(".diff-editor .cm-content")?.textContent).toContain("line five");

  // As a tree: folders first, the selection kept.
  fireEvent.click(screen.getByRole("button", { name: "Tree" }));
  await settleAll();
  expect(fakeState.settings.settings.changes.file_list).toBe("tree");
  expect(fileRowTexts().map((text) => text.replace(/\+.*/, ""))).toEqual([
    "▾assets",
    "Mlogo.png",
    "▾src",
    "Mmain.rs",
    "MREADME.md",
  ]);
  expect(within(files).getByRole("option", { selected: true }).textContent).toContain("README.md");

  // Another file: its diff.
  fireEvent.click(screen.getByText("main.rs"));
  await settleAll();
  expect(diffCalls().at(-1)?.file).toEqual({ list: "commit", id: 2 });
  expect(document.querySelector(".diff-editor .cm-content")?.textContent).toContain("fn added() {}");
});

test("UI-16 merge commit parent selector", async () => {
  const merge = "aaaa000000000000000000000000000000000000";
  fakeState.commits = {
    [merge]: {
      details: defaultDetails(merge, [hashOf(1), "bbbb000000000000000000000000000000000000"]),
      files: [[textFile("side.txt", [{ add: "side" }], "added")], [textFile("main.txt", [{ add: "main" }], "added")]],
    },
  };
  const { changes } = await setup();
  await changes.show(id(), commit(merge));
  await settleAll();
  expect(fileRowTexts()[0]).toContain("side.txt");

  const parent = screen.getByRole("combobox", { name: "Compare with parent" }) as HTMLSelectElement;
  expect(parent.value).toBe("0");
  fireEvent.change(parent, { target: { value: "1" } });
  await settleAll();
  const reads = fakeState.changesCalls.filter((call) => call.command === "changes");
  expect(reads.map((call) => call.target)).toEqual([
    { kind: "commit", hash: merge, parent: 0 },
    { kind: "commit", hash: merge, parent: 1 },
  ]);
  expect(fileRowTexts()[0]).toContain("main.txt");
  expect((screen.getByRole("combobox", { name: "Compare with parent" }) as HTMLSelectElement).value).toBe("1");
  const shown = document.querySelector(".diff-editor .cm-content")?.textContent;
  expect(shown).toContain("main");
  expect(shown).not.toContain("side");
});

test("UI-17 diff modes and options are remembered", async () => {
  const { changes } = await setup();
  await changes.show(id(), commit(hashOf(3)));
  await settleAll();

  // Side by side asks the core for whole files too and shows two editors.
  fireEvent.click(screen.getByRole("button", { name: "Side by side" }));
  await settleAll();
  // In the app, an effect reads the diff again after a change (App.tsx).
  await changes.reloadDiff(id());
  await settleAll();
  expect(diffCalls().at(-1)?.request).toMatchObject({ context: "three", full_text: true });
  expect(document.querySelectorAll(".diff-editor .cm-mergeView .cm-editor")).toHaveLength(2);

  fireEvent.click(screen.getByRole("checkbox", { name: "Ignore whitespace" }));
  await settleAll();
  fireEvent.click(screen.getByRole("checkbox", { name: "Wrap lines" }));
  await settleAll();
  fireEvent.change(screen.getByRole("combobox", { name: "Context" }), { target: { value: "ten" } });
  await settleAll();
  fireEvent.click(screen.getByRole("button", { name: "Inline" }));
  await settleAll();
  await changes.reloadDiff(id());
  await settleAll();
  expect(diffCalls().at(-1)?.request).toEqual({
    ignore_whitespace: true,
    context: "ten",
    load_anyway: false,
    full_text: false,
  });
  expect(document.querySelector(".diff-editor .cm-lineWrapping")).not.toBeNull();

  // The core kept them: a fresh store reads them back.
  const expected = {
    diff_mode: "inline",
    file_list: "flat",
    ignore_whitespace: true,
    wrap_lines: true,
    context: "ten",
  };
  expect(fakeState.settings.settings.changes).toEqual(expected);
  const again = createSettingsStore();
  await again.load();
  expect(again.changes()).toEqual(expected);
});

test("UI-18 special diffs", async () => {
  const hash = "cccc000000000000000000000000000000000000";
  fakeState.commits = {
    [hash]: {
      details: defaultDetails(hash, []),
      files: [
        [
          otherFile("a.bin", { kind: "binary", old_size: 2048, new_size: null }),
          otherFile("b.png", { kind: "image", old: null, new: { format: "png", size: 68 } }, { new: PNG_1X1 }),
          { ...textFile("c.txt", [...keep(1, 2), { add: "loaded at last" }]), tooLarge: true },
          otherFile("d", { kind: "submodule", old: "1".repeat(40), new: "2".repeat(40) }),
        ],
      ],
    },
  };
  const { changes } = await setup();
  await changes.show(id(), commit(hash));
  await settleAll();
  const diff = () => screen.getByRole("region", { name: "Diff" });

  expect(diff().textContent).toContain("Binary file changed. Before: 2.0 KB, after: none.");

  fireEvent.click(screen.getByText("b.png"));
  await settleAll();
  expect(within(diff()).getByText("Before: none")).toBeDefined();
  expect(within(diff()).getByText("After: PNG, 68 B")).toBeDefined();
  const image = within(diff()).getByRole("img", { name: "After image" });
  expect(image.getAttribute("src")).toMatch(/^blob:/);
  expect(URL.createObjectURL).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByText("c.txt"));
  await settleAll();
  expect(URL.revokeObjectURL).toHaveBeenCalled();
  expect(diff().textContent).toContain("1 lines added and 0 removed");
  fireEvent.click(within(diff()).getByRole("button", { name: "Load diff" }));
  await settleAll();
  expect(diffCalls().at(-1)?.request.load_anyway).toBe(true);
  expect(document.querySelector(".diff-editor .cm-content")?.textContent).toContain("loaded at last");

  fireEvent.click(screen.getByText("d"));
  await settleAll();
  expect(diff().textContent).toContain("1".repeat(40));
  expect(diff().textContent).toContain("2".repeat(40));
});

test("UI-15 uncommitted changes: unmerged, staged and unstaged, marked", async () => {
  fakeState.workingCopies = {
    r1: {
      staged: [textFile("b.txt", [{ add: "staged" }])],
      unstaged: [textFile("a.txt", [{ add: "unstaged" }]), textFile("new.txt", [{ add: "x" }], "untracked")],
      unmerged: [{ path: "z.txt", conflict: "both_modified" }],
    },
  };
  const { changes } = await setup();
  await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
  await settleAll();
  expect(screen.getByRole("heading", { name: "Uncommitted changes" })).toBeDefined();
  expect(fileRowTexts().map((text) => text.replace(/\+.*/, ""))).toEqual([
    "Unmerged (1)",
    "!z.txt (both modified)",
    "Staged (1)",
    "Mb.txt",
    "Unstaged (2)",
    "Ma.txt",
    "Unew.txt",
  ]);
  // The unmerged file is selected first; it has a conflict editor, no diff.
  expect(diffCalls()).toHaveLength(0);
  expect(screen.getByRole("region", { name: "Conflict" })).toBeDefined();

  fireEvent.click(screen.getByText("b.txt"));
  await settleAll();
  expect(diffCalls().at(-1)?.file).toEqual({ list: "staged", id: 0 });
  fireEvent.click(screen.getByText("new.txt"));
  await settleAll();
  expect(diffCalls().at(-1)?.file).toEqual({ list: "unstaged", id: 1 });
});

test("UI-19 the dock button moves the pane and is kept", async () => {
  const { layout } = await setup();
  expect(layout.dock(id())).toBe("right");
  fireEvent.click(screen.getByRole("button", { name: "Dock at bottom" }));
  expect(layout.dock(id())).toBe("bottom");
  expect(document.querySelector(".changes")?.getAttribute("data-dock")).toBe("bottom");
  await settle();
  expect(fakeState.layouts.at(-1)?.dock).toBe("bottom");
  fireEvent.click(screen.getByRole("button", { name: "Dock right" }));
  expect(layout.dock(id())).toBe("right");
});
