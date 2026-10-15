import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { WorkspaceView } from "../ipc";
import { emitJobEnded, emitJobProgress, emptyLayout, fakeState, holdsViews, reset } from "../ipc/fake";
import { textFile } from "../ipc/fakeChanges";
import { fakeJob } from "../ipc/fakeJobs";
import { summary } from "../ipc/fakeTree";

/** Where each element was scrolled: jsdom lays nothing out and keeps no scroll position. */
let scrolled = new WeakMap<Element, number>();
const scrollTop = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop");

beforeEach(() => {
  scrolled = new WeakMap();
  Object.defineProperty(Element.prototype, "scrollTop", {
    configurable: true,
    get(this: Element) {
      return scrolled.get(this) ?? 0;
    },
    set(this: Element, value: number) {
      scrolled.set(this, value);
    },
  });
  // Give the lists a height so rows are rendered, and CodeMirror the rectangles it measures.
  Range.prototype.getClientRects = () =>
    ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect();
  const real = HTMLElement.prototype.getBoundingClientRect;
  const tall = ["tree-scroller", "history-scroller", "file-list"];
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return tall.some((name) => this.classList.contains(name)) ? new DOMRect(0, 0, 400, 400) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
  if (scrollTop) {
    Object.defineProperty(Element.prototype, "scrollTop", scrollTop);
  }
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const workspace = (id: string, name: string, repos: WorkspaceView["repos"] = []): WorkspaceView => ({
  id,
  name,
  repos,
  groups: [],
  layout: emptyLayout,
  read_only: false,
  warning: null,
});

const tabNames = () => screen.getAllByRole("tab").map((tab) => tab.textContent);
const activeTab = () => screen.getAllByRole("tab").find((tab) => tab.getAttribute("aria-selected") === "true");

const openPlusMenu = async () => {
  fireEvent.click(screen.getByRole("button", { name: "Open workspace" }));
  const menu = await screen.findByRole("menu", { name: "Open workspace" });
  await settleAll();
  return within(menu);
};

test("ui_28_plus_menu_lists_workspaces_and_offers_new_and_import", async () => {
  fakeState.others = [workspace("ws-java", "Java"), workspace("ws-archive", "archive")];
  render(() => <App />);
  await screen.findByRole("tab", { name: "My workspace" });

  // Every workspace file, sorted by name, the open one marked; Delete only for closed ones.
  let menu = await openPlusMenu();
  const items = menu.getAllByRole("menuitem");
  expect(items.map((item) => item.textContent)).toEqual([
    "archive",
    "Delete…",
    "Java",
    "Delete…",
    "My workspace (open)",
    "New workspace…",
    "Import workspace…",
  ]);
  expect(menu.queryByRole("menuitem", { name: "Delete My workspace…" })).toBeNull();
  expect((menu.getByRole("menuitem", { name: "Import workspace…" }) as HTMLButtonElement).disabled).toBe(true);
  expect(document.activeElement).toBe(items[0]);

  // Delete says that the repos on disk are not touched; nothing is deleted before Delete.
  fireEvent.click(menu.getByRole("menuitem", { name: "Delete archive…" }));
  const confirm = await screen.findByRole("alertdialog");
  expect(confirm.textContent).toContain("Delete the workspace archive?");
  expect(confirm.textContent).toContain("The repositories on disk are not touched.");
  expect(document.activeElement).toBe(within(confirm).getByRole("button", { name: "Cancel" }));
  expect(fakeState.others.map((other) => other.name)).toEqual(["Java", "archive"]);
  fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
  await settleAll();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(fakeState.others.map((other) => other.name)).toEqual(["Java"]);

  // Choosing a closed workspace opens it as the last tab, and active (UI-27).
  menu = await openPlusMenu();
  fireEvent.click(menu.getByRole("menuitem", { name: "Java" }));
  await settleAll();
  expect(screen.queryByRole("menu")).toBeNull();
  expect(tabNames()).toEqual(["My workspace", "Java"]);
  expect(activeTab()?.textContent).toBe("Java");

  // Choosing an open one switches to it; no tab is added.
  menu = await openPlusMenu();
  fireEvent.click(menu.getByRole("menuitem", { name: /^My workspace/ }));
  await settleAll();
  expect(tabNames()).toEqual(["My workspace", "Java"]);
  expect(activeTab()?.textContent).toBe("My workspace");

  // New workspace shows the core's refusal inline, then creates the active last tab.
  menu = await openPlusMenu();
  fireEvent.click(menu.getByRole("menuitem", { name: "New workspace…" }));
  const dialog = within(await screen.findByRole("dialog", { name: "New workspace" }));
  const name = dialog.getByRole("textbox", { name: "Name" });
  fireEvent.input(name, { target: { value: "  JAVA " } });
  fireEvent.click(dialog.getByRole("button", { name: "Create" }));
  await settleAll();
  expect(name.getAttribute("aria-invalid")).toBe("true");
  expect(dialog.getByText("a workspace named JAVA exists")).toBeDefined();
  fireEvent.input(name, { target: { value: "   " } });
  fireEvent.click(dialog.getByRole("button", { name: "Create" }));
  await settleAll();
  expect(dialog.getByText("a workspace name cannot be blank")).toBeDefined();
  fireEvent.input(name, { target: { value: "C#" } });
  fireEvent.click(dialog.getByRole("button", { name: "Create" }));
  await settleAll();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(tabNames()).toEqual(["My workspace", "Java", "C#"]);
  expect(activeTab()?.textContent).toBe("C#");

  // Escape closes the menu without doing anything.
  await openPlusMenu();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
});

test("ui_28_tabs_reorder_by_drag_and_keyboard_and_close", async () => {
  const main = fakeState.workspace.id;
  fakeState.others = [workspace("ws-java", "Java"), workspace("ws-rust", "Rust")];
  fakeState.tabs = [main, "ws-java", "ws-rust"];
  render(() => <App />);
  await screen.findByRole("tab", { name: "Rust" });
  const item = (name: string) => screen.getByRole("tab", { name }).parentElement as HTMLElement;
  const tabAction = async (name: string) =>
    within(await screen.findByRole("menu", { name: "Tab actions" })).getByRole("menuitem", { name });

  // Dragging Rust onto the first tab puts it first; the core keeps the order.
  fireEvent.dragStart(item("Rust"));
  fireEvent.dragOver(item("My workspace"));
  fireEvent.drop(item("My workspace"));
  await settleAll();
  expect(tabNames()).toEqual(["Rust", "My workspace", "Java"]);
  expect(fakeState.tabs).toEqual(["ws-rust", main, "ws-java"]);
  // Dropping it right of where it was moves it there.
  fireEvent.dragStart(item("Rust"));
  fireEvent.drop(item("Java"));
  await settleAll();
  expect(tabNames()).toEqual(["My workspace", "Java", "Rust"]);

  // Ctrl/Cmd+Shift+PageDown and PageUp move the active tab; past the end it stays (UI-22).
  expect(activeTab()?.textContent).toBe("My workspace");
  fireEvent.keyDown(window, { key: "PageDown", ctrlKey: true, shiftKey: true });
  await settleAll();
  expect(tabNames()).toEqual(["Java", "My workspace", "Rust"]);
  fireEvent.keyDown(window, { key: "PageDown", metaKey: true, shiftKey: true });
  fireEvent.keyDown(window, { key: "PageDown", ctrlKey: true, shiftKey: true });
  await settleAll();
  expect(tabNames()).toEqual(["Java", "Rust", "My workspace"]);
  fireEvent.keyDown(window, { key: "PageUp", ctrlKey: true, shiftKey: true });
  await settleAll();
  expect(tabNames()).toEqual(["Java", "My workspace", "Rust"]);
  expect(document.activeElement).toBe(screen.getByRole("tab", { name: "My workspace" }));

  // The arrow keys activate the next tab.
  fireEvent.keyDown(screen.getByRole("tab", { name: "My workspace" }), { key: "ArrowRight" });
  await settleAll();
  expect(activeTab()?.textContent).toBe("Rust");
  expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Rust" }));
  fireEvent.click(screen.getByRole("tab", { name: "Java" }));
  await settleAll();
  expect(activeTab()?.textContent).toBe("Java");

  // The context menu renames, with the core's refusal inline.
  fireEvent.contextMenu(screen.getByRole("tab", { name: "Rust" }));
  fireEvent.click(await tabAction("Rename…"));
  const dialog = within(await screen.findByRole("dialog", { name: "Rename Rust" }));
  const name = dialog.getByRole("textbox", { name: "Name" }) as HTMLInputElement;
  expect(name.value).toBe("Rust");
  fireEvent.input(name, { target: { value: "java" } });
  fireEvent.click(dialog.getByRole("button", { name: "Rename" }));
  await settleAll();
  expect(dialog.getByText("a workspace named java exists")).toBeDefined();
  fireEvent.input(name, { target: { value: "Kotlin" } });
  fireEvent.click(dialog.getByRole("button", { name: "Rename" }));
  await settleAll();
  expect(tabNames()).toEqual(["Java", "My workspace", "Kotlin"]);

  // Closing the active tab activates the one that takes its place; the file stays.
  fireEvent.click(screen.getByRole("button", { name: "Close Java" }));
  await settleAll();
  expect(tabNames()).toEqual(["My workspace", "Kotlin"]);
  expect(activeTab()?.textContent).toBe("My workspace");
  expect(fakeState.others.map((other) => other.name)).toEqual(["Java", "Kotlin"]);
  // The context menu's Close, and the keyboard's context menu key.
  fireEvent.keyDown(screen.getByRole("tab", { name: "Kotlin" }), { key: "F10", shiftKey: true });
  fireEvent.click(await tabAction("Close"));
  await settleAll();
  expect(tabNames()).toEqual(["My workspace"]);
});

/** Two workspaces of different repos; the first has a commit with files and a dirty repo. */
function twoWorkspaces() {
  const main = fakeState.workspace.id;
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      { id: "r1", name: "api", path: "/src/api", remote: null },
      { id: "r2", name: "web", path: "/src/web", remote: null },
    ],
  };
  fakeState.others = [workspace("ws-java", "Java", [{ id: "r3", name: "core", path: "/src/core", remote: null }])];
  fakeState.tabs = [main, "ws-java"];
  fakeState.summaries = [summary(main, "r1", "main"), summary(main, "r2", "main"), summary("ws-java", "r3", "main")];
}

const treeRow = (label: string) =>
  screen.getAllByRole("treeitem").find((row) => row.querySelector(".tree-label")?.textContent === label) ??
  screen.getAllByRole("treeitem").find((row) => row.textContent?.includes(label));

test("ui_29_switching_tabs_restores_selection_expansion_scroll_filter_and_sizes", async () => {
  twoWorkspaces();
  fakeState.histories = { r1: { length: 300, prefix: "api" } };
  render(() => <App />);
  await screen.findByRole("tab", { name: "Java" });
  await settleAll();

  // In the first tab: select a repo, expand it, select a commit and a file,
  // scroll each list, widen the tree and filter it.
  fireEvent.click(treeRow("api") as HTMLElement);
  await settleAll();
  fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowRight" });
  await settleAll();
  expect(treeRow("api")?.getAttribute("aria-expanded")).toBe("true");
  const history = within(screen.getByRole("region", { name: "History" }));
  const commit = history.getAllByRole("option")[2] as HTMLElement;
  const subject = commit.textContent;
  fireEvent.click(commit);
  await settleAll();
  const files = () => within(screen.getByRole("listbox", { name: "Changed files" }));
  const file = files().getAllByRole("option")[1] as HTMLElement;
  const fileName = file.textContent;
  fireEvent.click(file);
  await settleAll();

  const scroll = (selector: string, top: number) => {
    const element = document.querySelector(selector) as HTMLElement;
    element.scrollTop = top;
    fireEvent.scroll(element);
  };
  scroll(".tree-scroller", 30);
  scroll(".history-scroller", 220);
  scroll(".file-list", 15);
  const splitter = screen.getByRole("separator", { name: "Resize Tree" });
  const width = () => (screen.getByRole("region", { name: "Tree" }) as HTMLElement).style.width;
  const before = width();
  fireEvent.keyDown(splitter, { key: "ArrowRight" });
  const wider = width();
  expect(wider).not.toBe(before);
  const filter = () => screen.getByRole("searchbox", { name: "Filter repos and branches" }) as HTMLInputElement;
  fireEvent.input(filter(), { target: { value: "a" } });
  await settleAll();

  // The other tab shows its own state.
  fireEvent.click(screen.getByRole("tab", { name: "Java" }));
  await settleAll();
  expect(filter().value).toBe("");
  expect(treeRow("core")).toBeDefined();
  expect(screen.queryAllByRole("treeitem").some((row) => row.getAttribute("aria-selected") === "true")).toBe(false);
  expect(width()).toBe(before);
  expect((document.querySelector(".tree-scroller") as HTMLElement).scrollTop).toBe(0);

  // Back in the first tab, everything is as it was left.
  fireEvent.click(screen.getByRole("tab", { name: "My workspace" }));
  await settleAll();
  expect(filter().value).toBe("a");
  expect(width()).toBe(wider);
  expect((document.querySelector(".tree-scroller") as HTMLElement).scrollTop).toBe(30);
  expect((document.querySelector(".history-scroller") as HTMLElement).scrollTop).toBe(220);
  expect((document.querySelector(".file-list") as HTMLElement).scrollTop).toBe(15);
  const selectedCommit = within(screen.getByRole("region", { name: "History" }))
    .getAllByRole("option")
    .find((row) => row.getAttribute("aria-selected") === "true");
  expect(selectedCommit?.textContent).toBe(subject);
  expect(files().getByRole("option", { selected: true }).textContent).toBe(fileName);
  fireEvent.input(filter(), { target: { value: "" } });
  await settleAll();
  expect(treeRow("api")?.getAttribute("aria-selected")).toBe("true");
  expect(treeRow("api")?.getAttribute("aria-expanded")).toBe("true");
});

/** Repo r1 of the first workspace in conflict state with one conflicted file. */
function inConflict() {
  twoWorkspaces();
  const main = fakeState.workspace.id;
  const dirty = summary(main, "r1", "main");
  fakeState.summaries[0] = {
    ...dirty,
    conflict: true,
    summary: dirty.summary && {
      ...dirty.summary,
      dirty: true,
      changes: 2,
      state: { operation: "merge", unmerged_paths: 1 },
    },
  };
  fakeState.workingCopies = {
    r1: {
      staged: [],
      unstaged: [textFile("notes.txt", [{ add: "x" }])],
      unmerged: [{ path: "a.txt", conflict: "both_modified" }],
      inConflict: true,
    },
  };
}

function editor(): EditorView {
  const element = document.querySelector<HTMLElement>(".conflict-editor .cm-editor");
  const view = element === null ? null : EditorView.findFromDOM(element);
  if (view === null) {
    throw new Error("no conflict editor is shown");
  }
  return view;
}

/** Opens the conflicted file of r1 in the conflict editor and types into it. */
async function editConflict() {
  fireEvent.click(treeRow("api") as HTMLElement);
  await settleAll();
  fireEvent.click(within(screen.getByRole("region", { name: "History" })).getAllByRole("option")[0] as HTMLElement);
  await settleAll();
  fireEvent.click(within(screen.getByRole("listbox", { name: "Changed files" })).getByText("a.txt"));
  await settleAll();
  editor().dispatch({ changes: { from: 0, insert: "mine " }, userEvent: "input.type" });
  return editor().state.doc.toString();
}

test("ui_29_unsaved_conflict_edits_survive_a_tab_switch", async () => {
  inConflict();
  render(() => <App />);
  await screen.findByRole("tab", { name: "Java" });
  await settleAll();
  const edited = await editConflict();

  fireEvent.click(screen.getByRole("tab", { name: "Java" }));
  await settleAll();
  expect(document.querySelector(".conflict-editor")).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: "My workspace" }));
  await settleAll();
  expect(editor().state.doc.toString()).toBe(edited);
  // Nothing was written: the buffer lives in the UI until Mark resolved (OPS-9).
  expect(fakeState.conflictRequests).toEqual([]);
});

test("ui_29 closing a tab with unsaved conflict edits asks first, naming the files", async () => {
  inConflict();
  render(() => <App />);
  await screen.findByRole("tab", { name: "Java" });
  await settleAll();
  await editConflict();

  // A tab without unsaved edits closes at once.
  fireEvent.click(screen.getByRole("button", { name: "Close Java" }));
  await settleAll();
  expect(tabNames()).toEqual(["My workspace"]);

  fireEvent.click(screen.getByRole("button", { name: "Close My workspace" }));
  const confirm = within(await screen.findByRole("alertdialog"));
  expect(confirm.getByRole("heading").textContent).toBe("Close My workspace?");
  expect(confirm.getByRole("listitem").textContent).toBe("api: a.txt");
  fireEvent.click(confirm.getByRole("button", { name: "Cancel" }));
  expect(tabNames()).toEqual(["My workspace"]);
  expect(editor().state.doc.toString().startsWith("mine ")).toBe(true);

  fireEvent.click(screen.getByRole("button", { name: "Close My workspace" }));
  fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Close tab" }));
  await settleAll();
  expect(screen.queryAllByRole("tab")).toEqual([]);
  expect(fakeState.conflictRequests).toEqual([]);

  // Reopened, the edits are gone with the tab.
  fireEvent.click(await screen.findByRole("button", { name: /^My workspace/ }));
  await settleAll();
  await editConflict();
  expect(editor().state.doc.toString().startsWith("mine mine ")).toBe(false);
});

test("ui_31_a_background_workspace_releases_history_and_diffs", async () => {
  twoWorkspaces();
  const main = fakeState.workspace.id;
  fakeState.histories = { r1: { length: 1200, prefix: "api" } };
  render(() => <App />);
  await screen.findByRole("tab", { name: "Java" });
  await settleAll();

  // A commit and a file selected, and the history scrolled into its second page.
  fireEvent.click(treeRow("api") as HTMLElement);
  await settleAll();
  const history = () => within(screen.getByRole("region", { name: "History" }));
  fireEvent.click(history().getAllByRole("option")[1] as HTMLElement);
  await settleAll();
  const files = () => within(screen.getByRole("listbox", { name: "Changed files" }));
  fireEvent.click(files().getAllByRole("option")[1] as HTMLElement);
  await settleAll();
  const fileName = files().getByRole("option", { selected: true }).textContent;
  const scroller = () => document.querySelector(".history-scroller") as HTMLElement;
  scroller().scrollTop = 495 * 24;
  fireEvent.scroll(scroller());
  await settleAll();
  const shown = () => history().queryAllByText(/^api: change 70[0-9]$/).length;
  expect(fakeState.historyCalls.filter((call) => call.command === "more")).toHaveLength(1);
  expect(shown()).toBeGreaterThan(0);
  expect(holdsViews(main)).toEqual({ walk: true, changes: true });

  // In the background, the core holds no walk or view for it, and the UI no rows.
  fakeState.historyCalls = [];
  fireEvent.click(screen.getByRole("tab", { name: "Java" }));
  await settleAll();
  expect(holdsViews(main)).toEqual({ walk: false, changes: false });

  // Back, the same rows are read again, and the list scrolls back to them.
  fireEvent.click(screen.getByRole("tab", { name: "My workspace" }));
  await settleAll();
  expect(fakeState.historyCalls.map((call) => call.command)).toEqual(["get", "more"]);
  expect(scroller().scrollTop).toBe(495 * 24);
  // A browser sends the scroll event the restore causes; jsdom does not.
  fireEvent.scroll(scroller());
  await settleAll();
  expect(shown()).toBeGreaterThan(0);
  scroller().scrollTop = 0;
  fireEvent.scroll(scroller());
  await settleAll();
  expect(history().getAllByRole("option")[1]?.getAttribute("aria-selected")).toBe("true");
  expect(files().getByRole("option", { selected: true }).textContent).toBe(fileName);
  expect(holdsViews(main)).toEqual({ walk: true, changes: true });
});

test("ui_32_tab_badge_counts_jobs_and_marks_unseen_failures", async () => {
  twoWorkspaces();
  const java = { id: "ws-java", name: "Java" };
  render(() => <App />);
  await screen.findByRole("tab", { name: "Java" });
  await settleAll();
  const tab = (name: string) => screen.getByRole("tab", { name: new RegExp(`^${name}`) });
  const badge = (name: string) => tab(name).querySelector(".tab-badge")?.textContent ?? null;
  const marked = (name: string) => tab(name).querySelector(".tab-marker") !== null;
  const job = (job_id: number, state: "queued" | "running") =>
    fakeJob(java, { job_id, label: `Job ${job_id}`, state, outcome: null, duration_ms: null });
  const ended = (job_id: number, kind: "succeeded" | "failed" | "conflict") =>
    fakeJob(java, {
      job_id,
      label: `Job ${job_id}`,
      state: "ended",
      outcome: { kind },
    });

  // Running and queued jobs of the background tab are counted on it.
  expect(badge("Java")).toBeNull();
  emitJobProgress(job(41, "running"));
  emitJobProgress(job(42, "queued"));
  await settle();
  expect(badge("Java")).toBe("2");
  expect(badge("My workspace")).toBeNull();

  // A success ends quietly; a failure the user has not seen marks the tab.
  emitJobEnded(ended(42, "succeeded"));
  await settle();
  expect(badge("Java")).toBe("1");
  expect(marked("Java")).toBe(false);
  emitJobEnded(ended(41, "failed"));
  await settle();
  expect(badge("Java")).toBeNull();
  expect(marked("Java")).toBe(true);

  // Activating the tab shows the report in the status bar: it is seen.
  fireEvent.click(tab("Java"));
  await settleAll();
  expect(screen.getByRole("status").textContent).toContain("Job 41");
  fireEvent.click(tab("My workspace"));
  await settleAll();
  expect(marked("Java")).toBe(false);

  // A conflict is marked too, until the job list has shown it.
  emitJobEnded(ended(43, "conflict"));
  await settle();
  expect(marked("Java")).toBe(true);
  fireEvent.click(screen.getByRole("button", { description: "Show the job list" }));
  await settleAll();
  expect(marked("Java")).toBe(false);
});
