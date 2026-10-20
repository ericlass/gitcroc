import { expect, type Locator, type Page, test } from "@playwright/test";
import type { PaletteOp } from "../src/ipc";

// UI-22: every operation of sections 5 and 6 is reachable without a mouse from where it lives in the window. These
// tests use the keyboard only: no click and no other mouse event.

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const history = (page: Page) => page.getByRole("listbox", { name: "History" });
const files = (page: Page) => page.getByRole("treegrid", { name: "Changed files" });
const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });
const palette = (page: Page) => page.getByRole("dialog", { name: "Command palette" });
const box = (page: Page) => palette(page).getByRole("combobox", { name: "Command palette" });
const options = (page: Page) => palette(page).getByRole("listbox", { name: "Matches" }).getByRole("option");
const diffText = (page: Page) => page.locator(".diff-editor");
const button = (page: Page, name: string) => page.getByRole("button", { name, exact: true });
const banner = (page: Page) => page.getByRole("region", { name: "Conflict state" });

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Whether the focus is in the element `selector` names. */
const focusedIn = (page: Page, selector: string) =>
  page.evaluate((within) => document.activeElement?.closest(within) != null, selector);

/** Presses F6 until the focus is in the region `selector` names. */
async function toRegion(page: Page, selector: string) {
  for (let presses = 0; presses < 8 && !(await focusedIn(page, selector)); presses++) {
    await page.keyboard.press("F6");
  }
  expect(await focusedIn(page, selector), `F6 reaches ${selector}`).toBe(true);
}

/** Presses `key` until `target` has the focus. */
async function tabTo(page: Page, target: Locator, key = "Tab") {
  await expect(target).toBeVisible();
  for (let presses = 0; presses < 150; presses++) {
    if (await target.evaluate((element) => element === document.activeElement)) {
      return;
    }
    await page.keyboard.press(key);
  }
  await expect(target).toBeFocused();
}

/** Tabs to `target` and presses Enter on it. */
async function press(page: Page, target: Locator) {
  await tabTo(page, target);
  await page.keyboard.press("Enter");
}

/** The level, text and expanded state of the tree's selected row. */
const treeRow = (page: Page) =>
  page.evaluate(() => {
    const id = document.querySelector("[role=tree]")?.getAttribute("aria-activedescendant");
    const row = id ? document.getElementById(id) : null;
    return row === null
      ? null
      : {
          level: Number(row.getAttribute("aria-level")),
          text: row.textContent ?? "",
          expanded: row.getAttribute("aria-expanded"),
        };
  });

/** The tree row `name` at `level`, by the start of its text after the chevron. */
const row = (page: Page, level: number, name: string) =>
  tree(page)
    .locator(`[role=treeitem][aria-level="${level}"]`)
    .filter({ hasText: new RegExp(`^[\\s▸▾]*${escape(name)}`) })
    .first();

/** Selects the tree row of `path`, its first name at `level`, with the arrow keys, opening the rows above it. */
async function treeTo(page: Page, path: string[], level = 2) {
  await toRegion(page, ".pane-tree");
  await expect(row(page, level, path[0] ?? "")).toBeVisible();
  await page.keyboard.press("Home");
  for (const [at, name] of path.entries()) {
    const want = new RegExp(`^[\\s▸▾]*${escape(name)}`);
    for (let presses = 0; ; presses++) {
      const shown = await treeRow(page);
      if (shown !== null && shown.level === level + at && want.test(shown.text)) {
        break;
      }
      expect(presses, `${name} at level ${level + at}`).toBeLessThan(80);
      await page.keyboard.press("ArrowDown");
    }
    const next = path[at + 1];
    if (next !== undefined) {
      if ((await treeRow(page))?.expanded === "false") {
        await page.keyboard.press("ArrowRight");
      }
      await expect(row(page, level + at + 1, next)).toBeVisible();
    }
  }
}

/** Opens the menu of the focused row by Shift+F10 and chooses `entry` with the arrow keys. */
async function menu(page: Page, entry: string) {
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu").first()).toBeVisible();
  await chooseInMenu(page, entry);
}

/** Moves the focus in an open menu to `entry` with the arrow keys and presses Enter. */
async function chooseInMenu(page: Page, entry: string) {
  const name = () => page.evaluate(() => document.activeElement?.textContent?.replace(/\s*▸$/, "").trim() ?? "");
  await expect.poll(() => page.evaluate(() => document.activeElement?.getAttribute("role"))).toMatch(/^menuitem/);
  for (let presses = 0; (await name()) !== entry; presses++) {
    expect(presses, `menu entry ${entry}`).toBeLessThan(30);
    await page.keyboard.press("ArrowDown");
  }
  await page.keyboard.press("Enter");
}

/** Selects the history row whose text matches `text`, or the `text`th row, with the arrow keys. */
async function historyTo(page: Page, text: RegExp | number) {
  await toRegion(page, ".pane-history");
  await expect(history(page).getByRole("option").first()).toBeVisible();
  await page.keyboard.press("Home");
  const target = typeof text === "number" ? history(page).getByRole("option").nth(text) : null;
  for (let presses = 0; ; presses++) {
    const selected = history(page).getByRole("option", { selected: true });
    const reached =
      target !== null
        ? (await target.getAttribute("aria-selected")) === "true"
        : (await selected.filter({ hasText: text as RegExp }).count()) > 0;
    if (reached) {
      return;
    }
    expect(presses, `history row ${String(text)}`).toBeLessThan(40);
    await page.keyboard.press("ArrowDown");
  }
}

/** Selects the changed file `path` with the arrow keys; `nth` picks among rows of the same path. */
async function fileTo(page: Page, path: string, nth = 0) {
  await expect(files(page)).toBeVisible();
  await toRegion(page, ".pane-changes");
  await expect(files(page)).toBeFocused();
  await page.keyboard.press("Home");
  const want = files(page).getByRole("row", { name: new RegExp(escape(path)) }).nth(nth);
  await expect(want).toBeVisible();
  for (let presses = 0; (await want.getAttribute("aria-selected")) !== "true"; presses++) {
    expect(presses, `file ${path}`).toBeLessThan(40);
    await page.keyboard.press("ArrowDown");
  }
}

/** api's uncommitted changes, with `file` selected when given. */
async function uncommitted(page: Page, file?: string, nth = 0) {
  await treeTo(page, ["api"]);
  await historyTo(page, /Uncommitted changes/);
  if (file !== undefined) {
    await fileTo(page, file, nth);
  }
}

/** Types `text` into the field `field` reached by Tab. */
async function typeInto(page: Page, field: Locator, text: string) {
  await tabTo(page, field);
  await page.keyboard.type(text);
}

/** A second workspace, open in a tab after the first and active, made from the "+" menu. */
async function secondTab(page: Page) {
  await toRegion(page, ".tab-bar");
  await press(page, button(page, "Open workspace"));
  await chooseInMenu(page, "New workspace…");
  const dialog = page.getByRole("dialog", { name: "New workspace" });
  await typeInto(page, dialog.getByLabel("Name"), "Second");
  await press(page, dialog.getByRole("button", { name: "Create" }));
  // The dialog gives the focus back to "+"; the new tab is before it.
  await expect(button(page, "Open workspace")).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.getByRole("tab", { name: "Second", selected: true })).toBeFocused();
}

/** Opens the palette by its key, types `text` and waits for `first` to lead the list. */
async function search(page: Page, text: string, first: string) {
  await page.keyboard.press("Control+k");
  await expect(box(page)).toBeFocused();
  await page.keyboard.type(text);
  await expect(options(page).first()).toContainText(first);
}

/** What reached the fake core and what the window shows of it, as one string to compare (palette.spec.ts). */
const effect = (page: Page) =>
  page.evaluate(() => {
    type FakeCore = { state: Record<string, unknown>; groups: Record<string, unknown> };
    const core = (window as unknown as { fakeCore: FakeCore }).fakeCore;
    const state = core.state as Record<string, unknown> & { workspace: { repos: { id: string }[] } };
    const named = (element: Element) => {
      const id = element.getAttribute("aria-labelledby");
      return (id === null ? null : document.getElementById(id)?.textContent) ?? element.getAttribute("aria-label");
    };
    const dialogs = [...document.querySelectorAll("[role=dialog], [role=alertdialog]")]
      .map((dialog) => ({
        name: named(dialog),
        checked: [...dialog.querySelectorAll("input:checked")].map((input) => input.closest("label")?.textContent),
      }))
      .filter((dialog) => dialog.name !== "Command palette");
    const logs = [
      "branchRequests",
      "syncRequests",
      "integrateRequests",
      "stashRequests",
      "tagRequests",
      "resetRequests",
      "remoteRequests",
      "changeRequests",
      "commitRequests",
      "conflictRequests",
      "aiResolveRequests",
      "aiResolveAllRequests",
      "cloneRequests",
      "exports",
      "imports",
      "opened",
      "tabs",
      "active",
    ];
    return JSON.stringify({
      dialogs,
      logs: Object.fromEntries(logs.map((log) => [log, state[log]])),
      preflights: core.groups.preflights,
      groupStarts: core.groups.starts,
      repos: state.workspace.repos.map((repo) => repo.id),
      editor: document.querySelector(".conflict-editor .cm-content")?.textContent ?? null,
      message: [...document.querySelectorAll<HTMLInputElement>(".commit-subject, .commit-body")].map(
        (field) => field.value,
      ),
    });
  });

interface Case {
  /** The fake data set. */
  fake?: string;
  /** The palette row that does the same, which the keyboard path must match. */
  label: string;
  /** What must reach the core or open instead, for an operation the palette does not list. */
  expected?: RegExp;
  /** What both pages do first, by keyboard. */
  setup?: (page: Page) => Promise<void>;
  /** The operation from its own place in the window, by keyboard. */
  entry: (page: Page) => Promise<void>;
}

const api = (page: Page) => treeTo(page, ["api"]);
const login = (page: Page) => treeTo(page, ["api", "Local", "feature", "login"]);
const remoteLogin = (page: Page) => treeTo(page, ["api", "Remotes", "origin", "feature", "login"]);
const stashRow = (page: Page) => treeTo(page, ["api", "Stashes", "On main"]);
const tagRow = (page: Page) => treeTo(page, ["api", "Tags", "v1.0"]);
const origin = (page: Page) => treeTo(page, ["api", "Remotes", "origin"]);
const backend = (page: Page) => treeTo(page, ["Backend"], 1);
const aCommit = async (page: Page) => {
  await api(page);
  await historyTo(page, 2);
};
const editorOn = async (page: Page) => {
  await uncommitted(page, "README.md");
  await expect(page.getByRole("textbox", { name: "Resolution of README.md" })).toBeVisible();
};
/** The conflict editor's first hunk by F7: its Take ours has the focus. */
const inHunk = async (page: Page) => {
  await editorOn(page);
  await page.keyboard.press("F7");
  await expect(page.getByRole("button", { name: /^Take ours in conflict 1/ })).toBeFocused();
};
const inToolbar = async (page: Page, name: string) => {
  await toRegion(page, ".toolbar");
  await press(page, toolbar(page).getByRole("button", { name, exact: true }));
};
/** Chooses the version of the side named `side` of a choice conflict (OPS-6). */
const useThis = (page: Page, side: string) =>
  press(page, page.getByRole("region", { name: side }).getByRole("button", { name: "Use this version" }));
const resetTo = async (page: Page, mode: "Soft" | "Mixed" | "Hard") => {
  await menu(page, "Reset current branch to here…");
  const dialog = page.getByRole("dialog", { name: /^Reset main in api to / });
  await tabTo(page, dialog.getByRole("radio", { name: /^Mixed/ }));
  if (mode !== "Mixed") {
    await page.keyboard.press(mode === "Soft" ? "ArrowUp" : "ArrowDown");
  }
  await expect(dialog.getByRole("radio", { name: new RegExp(`^${mode}`) })).toBeChecked();
};

/** One row per operation the palette lists: sections 5 and 6, AI and the app (M5-07's table). */
const OPERATIONS = {
  add_folder: {
    label: "Add folder…",
    entry: async (page) => {
      await toRegion(page, ".pane-tree");
      await press(page, button(page, "Add repository"));
      await chooseInMenu(page, "Add folder…");
    },
  },
  clone: {
    label: "Clone repository…",
    entry: async (page) => {
      await toRegion(page, ".pane-tree");
      await press(page, button(page, "Add repository"));
      await chooseInMenu(page, "Clone…");
    },
  },
  remove_repo: { label: "Remove from workspace", setup: api, entry: (page) => menu(page, "Remove from workspace") },
  open_in_file_manager: {
    label: "Open in file manager",
    setup: api,
    entry: (page) => menu(page, "Open in file manager"),
  },
  open_in_terminal: { label: "Open in terminal", setup: api, entry: (page) => menu(page, "Open in terminal") },
  stage_file: {
    label: "Stage file",
    setup: (page) => uncommitted(page, "notes.txt"),
    entry: (page) => page.keyboard.press("Space"),
  },
  unstage_file: {
    label: "Unstage file",
    setup: (page) => uncommitted(page, "src/app.ts"),
    entry: (page) => page.keyboard.press("Space"),
  },
  discard_file: {
    label: "Discard file changes…",
    setup: (page) => uncommitted(page, "notes.txt"),
    entry: (page) => page.keyboard.press("Delete"),
  },
  stage_all: {
    label: "Stage all",
    setup: (page) => uncommitted(page),
    entry: (page) => press(page, button(page, "Stage all")),
  },
  unstage_all: {
    label: "Unstage all",
    setup: (page) => uncommitted(page),
    entry: (page) => press(page, button(page, "Unstage all")),
  },
  commit: {
    label: "Commit…",
    setup: async (page) => {
      await uncommitted(page);
      await typeInto(page, page.getByRole("textbox", { name: "Subject" }), "From the keyboard");
    },
    entry: (page) =>
      press(page, page.getByRole("region", { name: "Commit message" }).getByRole("button", { name: "Commit" })),
  },
  amend: {
    label: "Amend last commit…",
    setup: (page) => uncommitted(page),
    entry: (page) => press(page, button(page, "Amend")),
  },
  checkout: { label: "Check out branch…", setup: login, entry: (page) => menu(page, "Check out") },
  create_branch: { label: "Create branch…", setup: api, entry: (page) => inToolbar(page, "Branch") },
  rename_branch: { label: "Rename branch…", setup: login, entry: (page) => menu(page, "Rename…") },
  delete_branch: { label: "Delete branch…", setup: login, entry: (page) => menu(page, "Delete…") },
  set_upstream: { label: "Set upstream…", setup: login, entry: (page) => menu(page, "Set upstream…") },
  checkout_remote_branch: {
    label: "Check out remote branch",
    setup: remoteLogin,
    entry: (page) => menu(page, "Check out"),
  },
  checkout_commit: {
    label: "Check out commit (detached)",
    setup: aCommit,
    entry: (page) => menu(page, "Check out (detached)"),
  },
  fetch: { label: "Fetch", setup: api, entry: (page) => inToolbar(page, "Fetch") },
  fetch_with_options: { label: "Fetch with options…", setup: api, entry: (page) => inToolbar(page, "Fetch options") },
  pull: { label: "Pull", setup: api, entry: (page) => inToolbar(page, "Pull") },
  pull_with_options: { label: "Pull with options…", setup: api, entry: (page) => inToolbar(page, "Pull options") },
  push: { label: "Push", setup: api, entry: (page) => inToolbar(page, "Push") },
  push_with_options: { label: "Push with options…", setup: api, entry: (page) => inToolbar(page, "Push options") },
  merge: { label: "Merge into current…", setup: api, entry: (page) => inToolbar(page, "Merge") },
  rebase: {
    label: "Rebase current onto…",
    setup: api,
    entry: async (page) => {
      await inToolbar(page, "Merge");
      const rebase = page.getByRole("dialog").getByRole("checkbox", { name: /Rebase/ });
      await tabTo(page, rebase);
      await page.keyboard.press("Space");
      await expect(rebase).toBeChecked();
    },
  },
  cherry_pick: { label: "Cherry-pick commit", setup: aCommit, entry: (page) => menu(page, "Cherry-pick") },
  revert: { label: "Revert commit", setup: aCommit, entry: (page) => menu(page, "Revert") },
  abort: {
    fake: "conflict",
    label: "Abort",
    setup: api,
    entry: (page) => press(page, banner(page).getByRole("button", { name: "Abort" })),
  },
  continue: {
    fake: "conflict-choice",
    label: "Continue",
    setup: async (page) => {
      // The text conflict is resolved as it stands, the other file by a choice: nothing is left unmerged.
      await editorOn(page);
      await press(page, button(page, "Mark resolved"));
      await press(page, button(page, "Mark resolved anyway"));
      await fileTo(page, "src/app.ts");
      await useThis(page, "Ours: main");
      await expect(banner(page).getByRole("button", { name: "Continue" })).toBeEnabled();
    },
    entry: (page) => press(page, banner(page).getByRole("button", { name: "Continue" })),
  },
  mark_resolved: {
    fake: "conflict",
    label: "Mark resolved",
    setup: editorOn,
    entry: (page) => press(page, button(page, "Mark resolved")),
  },
  take_ours: {
    fake: "conflict",
    label: "Take ours",
    setup: inHunk,
    entry: (page) => press(page, page.getByRole("button", { name: /^Take ours in conflict 1/ })),
  },
  take_theirs: {
    fake: "conflict",
    label: "Take theirs",
    setup: inHunk,
    entry: (page) => press(page, page.getByRole("button", { name: /^Take theirs in conflict 1/ })),
  },
  take_both: {
    fake: "conflict",
    label: "Take both",
    setup: inHunk,
    entry: (page) => press(page, page.getByRole("button", { name: /^Take both in conflict 1/ })),
  },
  keep_ours: {
    fake: "conflict-choice",
    label: "Keep our version of the file",
    setup: (page) => uncommitted(page, "src/app.ts"),
    entry: (page) => useThis(page, "Ours: main"),
  },
  keep_theirs: {
    fake: "conflict-choice",
    label: "Keep their version of the file",
    setup: (page) => uncommitted(page, "src/app.ts"),
    entry: (page) => useThis(page, "Theirs: feature/login"),
  },
  stash: { label: "Stash changes…", setup: api, entry: (page) => inToolbar(page, "Stash") },
  stash_apply: { label: "Apply stash", setup: stashRow, entry: (page) => menu(page, "Apply") },
  stash_pop: { label: "Pop stash", setup: stashRow, entry: (page) => menu(page, "Pop") },
  stash_drop: { label: "Drop stash…", setup: stashRow, entry: (page) => menu(page, "Drop…") },
  create_tag: { label: "Create tag…", setup: api, entry: (page) => menu(page, "New tag…") },
  delete_tag: { label: "Delete tag…", setup: tagRow, entry: (page) => menu(page, "Delete…") },
  push_tag: { label: "Push tag", setup: tagRow, entry: (page) => menu(page, "Push") },
  reset_soft: { label: "Reset current branch: soft…", setup: aCommit, entry: (page) => resetTo(page, "Soft") },
  reset_mixed: { label: "Reset current branch: mixed…", setup: aCommit, entry: (page) => resetTo(page, "Mixed") },
  reset_hard: { label: "Reset current branch: hard…", setup: aCommit, entry: (page) => resetTo(page, "Hard") },
  add_remote: {
    label: "Add remote…",
    setup: (page) => treeTo(page, ["api", "Remotes"]),
    entry: (page) => menu(page, "Add remote…"),
  },
  rename_remote: { label: "Rename remote…", setup: origin, entry: (page) => menu(page, "Rename…") },
  remove_remote: { label: "Remove remote…", setup: origin, entry: (page) => menu(page, "Remove…") },
  edit_remote_url: { label: "Edit remote URL…", setup: origin, entry: (page) => menu(page, "Edit URL…") },
  resolve_with_ai: {
    fake: "ai-conflict",
    label: "Resolve with AI",
    setup: editorOn,
    entry: (page) => press(page, button(page, "Resolve with AI")),
  },
  resolve_all_with_ai: {
    fake: "ai-resolve-all",
    label: "Resolve all with AI",
    setup: api,
    entry: (page) => press(page, banner(page).getByRole("button", { name: "Resolve all with AI" })),
  },
  accept_high_confidence: {
    fake: "ai-conflict",
    label: "Accept all high-confidence",
    setup: async (page) => {
      await editorOn(page);
      await press(page, button(page, "Resolve with AI"));
      await expect(button(page, "Accept all high-confidence")).toBeEnabled();
    },
    entry: (page) => press(page, button(page, "Accept all high-confidence")),
  },
  settings: { label: "Settings", entry: (page) => inToolbar(page, "Settings") },
  new_workspace: {
    label: "New workspace…",
    entry: async (page) => {
      await toRegion(page, ".tab-bar");
      await press(page, button(page, "Open workspace"));
      await chooseInMenu(page, "New workspace…");
    },
  },
  import_workspace: {
    label: "Import workspace…",
    entry: async (page) => {
      await toRegion(page, ".tab-bar");
      await press(page, button(page, "Open workspace"));
      await chooseInMenu(page, "Import workspace…");
    },
  },
  export_workspace: {
    label: "Export workspace…",
    entry: async (page) => {
      await toRegion(page, ".tab-bar");
      await menu(page, "Export workspace…");
    },
  },
  job_list: {
    label: "Show jobs",
    entry: async (page) => {
      await toRegion(page, ".status-bar");
      await page.keyboard.press("Enter");
    },
  },
  close_tab: {
    label: "Close tab",
    entry: async (page) => {
      // Delete on the tab: its × is the mouse's.
      await toRegion(page, ".tab-bar");
      await expect(page.getByRole("tab", { name: "My workspace" })).toBeFocused();
      await page.keyboard.press("Delete");
    },
  },
  next_tab: {
    label: "Next tab",
    setup: async (page) => {
      await secondTab(page);
      await toRegion(page, ".tab-bar");
      await page.keyboard.press("Home");
      await expect(page.getByRole("tab", { name: "My workspace", selected: true })).toBeFocused();
    },
    entry: (page) => page.keyboard.press("ArrowRight"),
  },
  previous_tab: {
    label: "Previous tab",
    setup: async (page) => {
      await secondTab(page);
      await toRegion(page, ".tab-bar");
    },
    entry: (page) => page.keyboard.press("ArrowLeft"),
  },
  new_group: {
    label: "New group…",
    entry: async (page) => {
      await treeTo(page, ["Ungrouped"], 1);
      await menu(page, "New group…");
    },
  },
} satisfies Record<PaletteOp, Case>;

/** The request of `action` on hunk `hunk`, from 0, in what reached the core. */
const hunkRequest = (action: string, hunk: number) =>
  new RegExp(`"action":"${action}","target":\\{"kind":"hunk","file":\\{[^}]*\\},"hunk":${hunk}\\}`);

/**
 * A hunk's control (UI-15) by F7 and Tab: the palette has no hunk entries, so the row names what must happen.
 * `line` is in the file's diff only, as a diff passed on the way may still be shown.
 */
const hunkCase = (control: string, path: string, line: string, expected: RegExp): Case => ({
  label: control,
  expected,
  setup: async (page) => {
    await uncommitted(page, path);
    await expect(diffText(page)).toContainText(line);
  },
  entry: async (page) => {
    for (let presses = 0; presses < Number(control.at(-1)); presses++) {
      await page.keyboard.press("F7");
    }
    await tabTo(page, button(page, control));
    await page.keyboard.press("Enter");
  },
});

/** Operations the palette does not list, and other keyboard ways to the same operation. */
const MORE: Record<string, Case> = {
  stage_hunk: hunkCase("Stage hunk 2", "src/main.rs", "line three", hunkRequest("stage", 1)),
  unstage_hunk: hunkCase("Unstage hunk 1", "src/app.ts", "staged line", hunkRequest("unstage", 0)),
  // GRP-9: a discard asks first.
  discard_hunk: hunkCase("Discard hunk 2", "src/main.rs", "line three", /"name":"Discard[^"]*"/),
  checkout_by_enter: {
    label: "Check out branch…",
    setup: login,
    entry: (page) => page.keyboard.press("Enter"),
  },
  cherry_pick_two_by_shift: {
    label: "Cherry-pick commit",
    setup: async (page) => {
      await aCommit(page);
      await page.keyboard.press("Shift+ArrowDown");
      await expect(history(page).getByRole("option", { selected: true })).toHaveCount(2);
    },
    entry: (page) => menu(page, "Cherry-pick 2 commits"),
  },
  cherry_pick_two_by_ctrl_space: {
    label: "Cherry-pick commit",
    setup: async (page) => {
      await aCommit(page);
      await page.keyboard.press("Control+ArrowDown");
      await page.keyboard.press("Control+ArrowDown");
      await page.keyboard.press("Control+Space");
      await expect(history(page).getByRole("option", { selected: true })).toHaveCount(2);
    },
    entry: (page) => menu(page, "Cherry-pick 2 commits"),
  },
  settings_by_key: { label: "Settings", entry: (page) => page.keyboard.press("Control+Comma") },
  // Section 5's group forms, on Backend.
  group_fetch: { fake: "groups", label: "Fetch", setup: backend, entry: (page) => inToolbar(page, "Fetch") },
  group_pull: { fake: "groups", label: "Pull", setup: backend, entry: (page) => inToolbar(page, "Pull") },
  group_push: { fake: "groups", label: "Push", setup: backend, entry: (page) => inToolbar(page, "Push") },
  group_checkout: {
    fake: "groups",
    label: "Check out branch…",
    setup: backend,
    entry: (page) => menu(page, "Check out branch…"),
  },
  group_create_branch: {
    fake: "groups",
    label: "Create branch…",
    setup: backend,
    entry: (page) => menu(page, "New branch…"),
  },
  group_delete_branch: {
    fake: "groups",
    label: "Delete branch…",
    setup: backend,
    entry: (page) => menu(page, "Delete branch…"),
  },
  group_merge: { fake: "groups", label: "Merge into current…", setup: backend, entry: (page) => menu(page, "Merge…") },
  group_rebase: {
    fake: "groups",
    label: "Rebase current onto…",
    setup: backend,
    entry: (page) => menu(page, "Rebase…"),
  },
  group_create_tag: {
    fake: "groups",
    label: "Create tag…",
    setup: backend,
    entry: (page) => menu(page, "Create tag…"),
  },
  group_commit: {
    fake: "groups",
    label: "Commit…",
    setup: async (page) => {
      await backend(page);
      await historyTo(page, /Uncommitted changes/);
      await typeInto(page, page.getByRole("textbox", { name: "Subject" }), "From the keyboard");
    },
    entry: (page) =>
      press(page, page.getByRole("region", { name: "Commit message" }).getByRole("button", { name: /^Commit/ })),
  },
};

for (const [name, known] of [...Object.entries(OPERATIONS), ...Object.entries(MORE)] as [string, Case][]) {
  test(`ui_22_every_operation_reachable_without_a_mouse: ${name}`, async ({ browser }) => {
    const open = async () => {
      const page = await browser.newPage();
      await page.goto(`/?fake=${known.fake ?? "tree"}`);
      await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
      await known.setup?.(page);
      return page;
    };
    const fromKeys = await open();
    const before = await effect(fromKeys);
    const expected = known.expected;
    if (expected !== undefined) {
      await known.entry(fromKeys);
      await expect.poll(() => effect(fromKeys)).toMatch(expected);
      return;
    }
    const reference = await open();
    await search(reference, known.label, known.label);
    await expect(options(reference).first()).not.toHaveAttribute("aria-disabled", "true");
    await reference.keyboard.press("Enter");
    await expect(palette(reference)).toBeHidden();
    await known.entry(fromKeys);
    // The keyboard reaches what the palette does, which is what the operation's own control does (palette.spec.ts):
    // the same requests to the core, the same dialogs open.
    await expect
      .poll(async () => {
        const [shown, expected] = await Promise.all([effect(fromKeys), effect(reference)]);
        return shown === expected && shown !== before ? "same" : `keys    ${shown}\nexpected ${expected}`;
      })
      .toBe("same");
  });
}

test("ui_22_f6_cycles_the_regions", async ({ page }) => {
  await page.goto("/?fake=tree");
  await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
  await treeTo(page, ["api"]);
  await historyTo(page, /Uncommitted changes/);
  await expect(files(page)).toBeVisible();
  // From the history: the changes, the status bar, the tab bar, the toolbar, the tree, the history again.
  const stops: [string, Locator][] = [
    [".pane-changes", files(page)],
    [".status-bar", page.getByRole("button", { description: "Show the job list" })],
    [".tab-bar", page.getByRole("tab", { name: "My workspace" })],
    [".toolbar", toolbar(page).getByRole("button", { name: "Fetch", exact: true })],
    [".pane-tree", tree(page)],
    [".pane-history", history(page)],
  ];
  for (const [region, target] of stops) {
    await page.keyboard.press("F6");
    await expect(target, region).toBeFocused();
  }
  // The selected row stays selected: the focus lands on it.
  await expect(history(page).getByRole("option", { selected: true })).toContainText("Uncommitted changes");
  await expect(tree(page).getByRole("treeitem", { selected: true })).toContainText("api");
  for (const [region, target] of [...stops].reverse().slice(1)) {
    await page.keyboard.press("Shift+F6");
    await expect(target, region).toBeFocused();
  }
  await page.keyboard.press("Shift+F6");
  await expect(history(page)).toBeFocused();

  // Not while a dialog is open.
  await page.keyboard.press("Control+Comma");
  const settings = page.getByRole("dialog", { name: "Settings" });
  await expect(settings).toBeVisible();
  const focused = await page.evaluate(() => document.activeElement?.outerHTML);
  await page.keyboard.press("F6");
  expect(await page.evaluate(() => document.activeElement?.outerHTML)).toBe(focused);
});

test("ui_22_f7_moves_between_hunks", async ({ page }) => {
  // The diff: src/main.rs has two hunks; the focus goes to each one's first control, Discard, which asks first
  // (GRP-9).
  await page.goto("/?fake=tree");
  await uncommitted(page, "src/main.rs");
  // Its diff, not one of a file passed on the way.
  await expect(diffText(page)).toContainText("line three");
  await page.keyboard.press("F7");
  await expect(button(page, "Discard hunk 1")).toBeFocused();
  // Tab goes on to the hunk's other controls.
  await page.keyboard.press("Tab");
  await expect(button(page, "Stage hunk 1")).toBeFocused();
  await page.keyboard.press("F7");
  await expect(button(page, "Discard hunk 2")).toBeFocused();
  // Around the end.
  await page.keyboard.press("F7");
  await expect(button(page, "Discard hunk 1")).toBeFocused();
  await page.keyboard.press("Shift+F7");
  await expect(button(page, "Discard hunk 2")).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect.poll(() => effect(page)).toMatch(hunkRequest("stage", 1));

  // The conflict editor: README.md has two hunks, each with a proposal; the focus goes to Take ours of each.
  await page.goto("/?fake=ai-conflict");
  await editorOn(page);
  await press(page, button(page, "Resolve with AI"));
  await expect(button(page, "Accept all high-confidence")).toBeEnabled();
  await page.keyboard.press("F7");
  await expect(page.getByRole("button", { name: /^Take ours in conflict 1/ })).toBeFocused();
  await page.keyboard.press("F7");
  await expect(page.getByRole("button", { name: /^Take ours in conflict 2/ })).toBeFocused();
  await page.keyboard.press("Shift+F7");
  await expect(page.getByRole("button", { name: /^Take ours in conflict 1/ })).toBeFocused();
  // Take theirs by Tab and Enter; the focus goes back to the editor, and F7 to the hunk left, now the only one.
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: /^Take theirs in conflict 1/ })).toBeFocused();
  await page.keyboard.press("Enter");
  const editor = page.getByRole("textbox", { name: "Resolution of README.md" });
  await expect(editor).toContainText("Run it with just.");
  await expect(editor).toBeFocused();
  await page.keyboard.press("F7");
  await expect(page.getByRole("button", { name: /^Take ours in conflict/ })).toHaveCount(1);
  await expect(page.getByRole("button", { name: /^Take ours in conflict 1/ })).toBeFocused();
  // Escape goes back to the text, where AI-11's keys stay: Alt+4 accepts the proposal of the hunk at the cursor.
  await page.keyboard.press("Escape");
  await expect(editor).toBeFocused();
  await page.keyboard.press("Alt+4");
  await expect(editor).toContainText("Sign in with a token.");
});

test("ui_22_tab_switching_keys", async ({ page }) => {
  await page.goto("/?fake=tree");
  await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
  await secondTab(page);
  const first = page.getByRole("tab", { name: "My workspace" });
  const second = page.getByRole("tab", { name: "Second" });
  await page.keyboard.press("Control+PageUp");
  await expect(first).toHaveAttribute("aria-selected", "true");
  await expect(first).toBeFocused();
  // None before the first.
  await page.keyboard.press("Control+PageUp");
  await expect(first).toHaveAttribute("aria-selected", "true");
  // Wherever the focus is, also in a list that pages with PageUp and PageDown.
  await treeTo(page, ["api"]);
  await historyTo(page, 0);
  await page.keyboard.press("Control+PageDown");
  await expect(second).toHaveAttribute("aria-selected", "true");
  await expect(second).toBeFocused();
  // With Shift the tab moves (M3-02).
  await page.keyboard.press("Control+Shift+PageUp");
  await expect(page.getByRole("tab")).toHaveText(["Second", "My workspace"]);
  await expect(second).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Control+PageDown");
  await expect(first).toHaveAttribute("aria-selected", "true");
  // Not while a dialog is open.
  await page.keyboard.press("Control+Comma");
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Control+PageUp");
  await expect(first).toHaveAttribute("aria-selected", "true");
});

test("UI-22 menus move with the arrow keys and give the focus back on Escape", async ({ page }) => {
  await page.goto("/?fake=tree");
  await treeTo(page, ["api"]);
  await page.keyboard.press("Shift+F10");
  const entries = page.getByRole("menu").getByRole("menuitem");
  await expect(entries.first()).toBeFocused();
  await page.keyboard.press("End");
  await expect(entries.last()).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(entries.first()).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(entries.last()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(tree(page)).toBeFocused();

  await historyTo(page, 2);
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu", { name: "Commit" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(history(page)).toBeFocused();

  await toRegion(page, ".tab-bar");
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu").getByRole("menuitem").first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("tab", { name: "My workspace" })).toBeFocused();

  await press(page, button(page, "Open workspace"));
  await expect(page.getByRole("menu").getByRole("menuitem").first()).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(button(page, "Open workspace")).toBeFocused();
});

test("UI-21 UI-22 the job list, a job's output and its report are reachable by keyboard", async ({ page }) => {
  await page.goto("/?fake=jobs");
  await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
  await toRegion(page, ".status-bar");
  await page.keyboard.press("Enter");
  const jobs = page.getByRole("dialog", { name: "Jobs" });
  const list = jobs.getByRole("listbox", { name: "Jobs" });
  await expect(list).toBeFocused();
  // To the merge, whose output Tab reaches to scroll it.
  await page.keyboard.press("Home");
  const selected = list.getByRole("option", { selected: true });
  for (let presses = 0; (await selected.textContent())?.includes("Merge") !== true; presses++) {
    expect(presses, "the merge job").toBeLessThan(20);
    await page.keyboard.press("ArrowDown");
  }
  const output = jobs.locator(".job-output");
  await expect(output).toContainText("CONFLICT");
  await tabTo(page, output);
  await page.keyboard.press("Escape");
  await expect(jobs).toBeHidden();

  // A group job's report: its lines' controls move with the arrow keys and open each line's output.
  await page.goto("/?fake=groups");
  await backend(page);
  await inToolbar(page, "Fetch");
  // The preflight's dialog shows while it runs; F6 waits while a dialog is open.
  await expect(page.locator("[aria-modal=true]")).toHaveCount(0);
  await toRegion(page, ".status-bar");
  await expect(page.locator(".status-bar")).toContainText(/Fetch/);
  await page.keyboard.press("Enter");
  await expect(list).toBeFocused();
  const report = jobs.getByRole("list", { name: "Result report" });
  for (let presses = 0; (await report.count()) === 0; presses++) {
    expect(presses, "a job with a report").toBeLessThan(20);
    await page.keyboard.press("ArrowDown");
  }
  await tabTo(page, report.getByRole("button", { name: /^Git's output in/ }).first());
  await page.keyboard.press("ArrowDown");
  const second = report.getByRole("listitem").nth(1);
  await expect(second.getByRole("button", { name: /^Git's output in/ })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(second.getByRole("button", { name: /^Git's output in/ })).toHaveAttribute("aria-expanded", "true");
});
