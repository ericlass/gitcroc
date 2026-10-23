import { expect, type Page, test } from "@playwright/test";
import type { PaletteOp } from "../src/ipc";

// Must match PALETTE_ROW_HEIGHT and PALETTE_OVERSCAN in src/palette/Palette.tsx.
const ROW_HEIGHT = 28;
const OVERSCAN = 6;

const tree = (page: Page) => page.getByRole("tree", { name: "Repositories" });
const item = (page: Page, name: string | RegExp) => tree(page).getByRole("treeitem", { name, exact: true });
const toolbar = (page: Page) => page.getByRole("toolbar", { name: "Main" });
const palette = (page: Page) => page.getByRole("dialog", { name: "Command palette" });
const box = (page: Page) => palette(page).getByRole("combobox", { name: "Command palette" });
const options = (page: Page) => palette(page).getByRole("listbox", { name: "Matches" }).getByRole("option");
const history = (page: Page) => page.getByRole("listbox", { name: "History" });

/** Opens the palette by its key, types `text` and waits for `first` to lead the list. */
async function search(page: Page, text: string, first: string | RegExp) {
  await page.keyboard.press("Control+k");
  await expect(box(page)).toBeFocused();
  await box(page).fill(text);
  await expect(options(page).first()).toContainText(first);
}

test("UI-22 the palette opens by key and by button, closes by Escape and by the key, and gives the focus back", async ({
  page,
}) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  await expect(tree(page)).toBeFocused();

  await page.keyboard.press("Control+k");
  await expect(palette(page)).toBeVisible();
  await expect(box(page)).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(palette(page)).toBeHidden();
  await expect(tree(page)).toBeFocused();

  const button = toolbar(page).getByRole("button", { name: "Command palette" });
  await button.click();
  await expect(box(page)).toBeFocused();
  await page.keyboard.press("Control+k");
  await expect(palette(page)).toBeHidden();
  await expect(button).toBeFocused();

  // Not while another dialog is open.
  await toolbar(page).getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(palette(page)).toBeHidden();
});

test("UI-22 a click outside the palette closes it and gives the focus back; a click inside does not", async ({
  page,
}) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  await page.keyboard.press("Control+k");
  await expect(box(page)).toBeFocused();

  // On the palette's own edge: it stays open.
  await palette(page).click({ position: { x: 2, y: 2 } });
  await expect(palette(page)).toBeVisible();

  // Over the tree, outside the palette: it closes, and the click selects nothing there.
  await page.mouse.click(8, 300);
  await expect(palette(page)).toBeHidden();
  await expect(tree(page)).toBeFocused();
  await expect(item(page, /^api/)).toHaveAttribute("aria-selected", "true");
});

test("UI-22 the palette opens from inside the conflict editor", async ({ page }) => {
  await page.goto("/?fake=conflict");
  await item(page, /^api/).click();
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  await page.getByRole("treegrid", { name: "Changed files" }).getByRole("row", { name: /README\.md/ }).click();
  const editor = page.getByRole("textbox", { name: "Resolution of README.md" });
  await editor.click();
  await page.keyboard.press("Control+k");
  await expect(box(page)).toBeFocused();
  await page.keyboard.press("Control+k");
  await expect(palette(page)).toBeHidden();
  await expect(editor).toBeFocused();
});

test("UI-22 Fetch on a group from the palette goes through its preflight; a hard reset asks first", async ({
  page,
}) => {
  await page.goto("/?fake=groups");
  // Every repo of the fake would run, which starts the job at once: the setting shows the preflight all the same.
  await toolbar(page).getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("tab", { name: "Git" }).click();
  await page.getByRole("checkbox", { name: "Always show the preflight dialog" }).check();
  await page.keyboard.press("Escape");
  await item(page, /^Backend/).click();
  await search(page, "fetch", "Fetch");
  await page.keyboard.press("Enter");
  await expect(palette(page)).toBeHidden();
  const preflight = page.getByRole("dialog", { name: "Fetch · Backend (2 repos)" });
  await expect(preflight).toBeVisible();
  await expect(preflight.getByRole("list", { name: "Repos" })).toContainText("web");
  await preflight.getByRole("button", { name: "Cancel" }).click();

  await item(page, /^api/).first().click();
  await history(page).getByRole("option").nth(2).click();
  await search(page, "reset hard", "Reset current branch: hard…");
  await page.keyboard.press("Enter");
  const reset = page.getByRole("dialog", { name: /^Reset main in api to / });
  await expect(reset.getByRole("radio", { name: /Hard/ })).toBeChecked();
  await reset.getByRole("button", { name: "Reset hard…" }).click();
  await expect(page.getByRole("alertdialog", { name: "Cannot be undone in api" })).toBeVisible();
});

test("UI-22 an operation that cannot apply shows why in the palette, and Enter does nothing", async ({ page }) => {
  await page.goto("/?fake=groups");
  await item(page, /^Backend/).click();
  await search(page, "stash changes", "Stash changes…");
  // The toolbar's own reason.
  await expect(options(page).first()).toHaveAttribute("aria-disabled", "true");
  await expect(options(page).first()).toContainText("Stash works on one repository at a time");
  await page.keyboard.press("Enter");
  await expect(palette(page)).toBeVisible();

  // Its subject missing: what to select.
  await box(page).fill("rename branch");
  await expect(options(page).first()).toContainText("Select a branch in the tree");
  await page.keyboard.press("Enter");
  await expect(palette(page)).toBeVisible();
});

test("UI-22 the palette selects a repo, a group and a remote branch in the tree; the history follows", async ({
  page,
}) => {
  await page.goto("/?fake=groups");
  await expect(item(page, /^Backend/)).toBeVisible();

  await search(page, "login-service", "login-service");
  await page.keyboard.press("Enter");
  await expect(palette(page)).toBeHidden();
  await expect(item(page, /^login-service/)).toHaveAttribute("aria-selected", "true");
  await expect(tree(page)).toBeFocused();
  await expect(history(page).getByRole("option").first()).toContainText("login-service");

  await search(page, "services", "Services");
  await page.keyboard.press("Enter");
  await expect(item(page, /^Services/)).toHaveAttribute("aria-selected", "true");
  // A group's merged history has a repo column (UI-9).
  await expect(page.locator(".history-columns")).toContainText("Repository");

  await search(page, "origin/feature/login", "origin/feature/login");
  await page.keyboard.press("Enter");
  const login = tree(page).getByRole("treeitem", { name: "login", selected: true });
  await expect(login).toBeVisible();
  await expect(login).toHaveAttribute("aria-level", "6");
  await expect(history(page).getByRole("option").first()).toContainText("origin/feature/login");
});

test("PERF-1 UI-22 the palette's list holds no more rows than fit and the overscan", async ({ page }) => {
  await page.goto("/?fake=tree");
  await item(page, /^api/).click();
  await search(page, "", "Add folder…");
  const list = palette(page).getByRole("listbox", { name: "Matches" });
  const height = await list.evaluate((element) => element.clientHeight);
  const limit = Math.ceil(height / ROW_HEIGHT) + 1 + 2 * OVERSCAN;
  expect(await options(page).count()).toBeLessThanOrEqual(limit);
  expect(await list.evaluate((element) => element.scrollHeight)).toBeGreaterThan(height * 2);
  // End selects the catalog's last entry.
  await page.keyboard.press("End");
  await expect(options(page).filter({ hasText: "Check for updates" })).toHaveAttribute("aria-selected", "true");
  expect(await options(page).count()).toBeLessThanOrEqual(limit);
});

test("AI-16 UI-22 the palette has no AI entries while AI features are off", async ({ page }) => {
  await page.goto("/?fake=ai-conflict");
  await item(page, /^api/).click();
  await search(page, "with ai", "Resolve with AI");
  await page.keyboard.press("Escape");

  await toolbar(page).getByRole("button", { name: "Settings" }).click();
  await page.getByRole("dialog", { name: "Settings" }).getByRole("tab", { name: "AI" }).click();
  await page.getByRole("checkbox", { name: "AI features" }).uncheck();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+k");
  await box(page).fill("ai");
  await expect(palette(page).getByText(/^\d+ of \d+|Nothing matches/).or(options(page).first())).toBeVisible();
  for (const label of ["Resolve with AI", "Resolve all with AI", "Accept all high-confidence"]) {
    await expect(options(page).filter({ hasText: label })).toHaveCount(0);
  }
});

// ui_22_every_palette_operation_runs_its_action: each operation the core lists runs, from the palette, what its
// toolbar button, menu entry or pane control runs. Two fresh pages get the same setup; one runs the operation from
// the palette, the other from its own control, and both must end with the same requests to the fake core and the
// same dialogs open, and the same text in the conflict editor and the commit message box.

/** What reached the fake core and what the window shows of it, as one string to compare. */
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
      repos: state.workspace.repos.map((repo) => repo.id),
      editor: document.querySelector(".conflict-editor .cm-content")?.textContent ?? null,
      message: [...document.querySelectorAll<HTMLInputElement>(".commit-subject, .commit-body")].map(
        (field) => field.value,
      ),
    });
  });

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** The tree row `name` at `level`, by the start of its text after the chevron. */
const row = (page: Page, level: number, name: string) =>
  tree(page)
    .locator(`[role=treeitem][aria-level="${level}"]`)
    .filter({ hasText: new RegExp(`^[\\s▸▾]*${escape(name)}`) })
    .first();

/** Expands the rows of `path` below `level` and selects its last row. */
async function open(page: Page, path: string[], level = 2) {
  for (const [at, name] of path.entries()) {
    const shown = row(page, level + at, name);
    await expect(shown).toBeVisible();
    if (at === path.length - 1) {
      await shown.click();
    } else if ((await shown.getAttribute("aria-expanded")) === "false") {
      await shown.locator(".tree-chevron").click();
    }
  }
}

/** Opens the context menu of the tree row of `path` and chooses `entry`. */
async function treeMenu(page: Page, path: string[], entry: string) {
  await row(page, 1 + path.length, path.at(-1) ?? "").click({ button: "right" });
  await page.getByRole("menuitem", { name: entry, exact: true }).click();
}

const commit = (page: Page) => history(page).getByRole("option").nth(2);
const files = (page: Page) => page.getByRole("treegrid", { name: "Changed files" });

/** api's uncommitted changes, with `file` selected when given. */
async function uncommitted(page: Page, file?: string) {
  await open(page, ["api"]);
  await history(page).getByRole("option", { name: /Uncommitted changes/ }).click();
  if (file !== undefined) {
    await files(page).getByRole("row", { name: new RegExp(escape(file)) }).first().click();
  }
}

async function commitMenu(page: Page, entry: string) {
  await commit(page).click({ button: "right" });
  await page.getByRole("menuitem", { name: entry, exact: true }).click();
}

const button = (page: Page, name: string) => page.getByRole("button", { name, exact: true });
const banner = (page: Page) => page.getByRole("region", { name: "Conflict state" });
/** Chooses the version of the side named `side` of a choice conflict (OPS-6). */
const useThis = (page: Page, side: string) =>
  page.getByRole("region", { name: side }).getByRole("button", { name: "Use this version" }).click();

/** A second workspace, open in a tab after the first and active. */
async function secondTab(page: Page) {
  await button(page, "Open workspace").click();
  await page.getByRole("menuitem", { name: "New workspace…" }).click();
  const dialog = page.getByRole("dialog", { name: "New workspace" });
  await dialog.getByLabel("Name").fill("Second");
  await dialog.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("tab", { name: "Second", selected: true })).toBeVisible();
}

interface Case {
  /** The fake data set. */
  fake?: string;
  /** The palette row's label. */
  label: string;
  /** What both pages do first. */
  setup?: (page: Page) => Promise<void>;
  /** The operation from its own place in the window. */
  entry: (page: Page) => Promise<void>;
}

const api = (page: Page) => open(page, ["api"]);
const login = (page: Page) => open(page, ["api", "Local", "feature", "login"]);
const remoteLogin = (page: Page) => open(page, ["api", "Remotes", "origin", "feature", "login"]);
const stashRow = (page: Page) => open(page, ["api", "Stashes", "On main"]);
const tagRow = (page: Page) => open(page, ["api", "Tags", "v1.0"]);
const origin = (page: Page) => open(page, ["api", "Remotes", "origin"]);
const aCommit = async (page: Page) => {
  await api(page);
  await commit(page).click();
};
const editorOn = async (page: Page) => {
  await uncommitted(page, "README.md");
  await expect(page.getByRole("textbox", { name: "Resolution of README.md" })).toBeVisible();
};
const inHunk = async (page: Page) => {
  await editorOn(page);
  await button(page, "Next conflict").click();
};

const CASES = {
  add_folder: {
    label: "Add folder…",
    entry: async (page) => {
      await button(page, "Add repository").click();
      await page.getByRole("menuitem", { name: "Add folder…" }).click();
    },
  },
  clone: {
    label: "Clone repository…",
    entry: async (page) => {
      await button(page, "Add repository").click();
      await page.getByRole("menuitem", { name: "Clone…" }).click();
    },
  },
  remove_repo: {
    label: "Remove from workspace",
    setup: api,
    entry: (page) => treeMenu(page, ["api"], "Remove from workspace"),
  },
  open_in_file_manager: {
    label: "Open in file manager",
    setup: api,
    entry: (page) => treeMenu(page, ["api"], "Open in file manager"),
  },
  open_in_terminal: {
    label: "Open in terminal",
    setup: api,
    entry: (page) => treeMenu(page, ["api"], "Open in terminal"),
  },
  stage_file: {
    label: "Stage file",
    setup: (page) => uncommitted(page, "notes.txt"),
    entry: (page) => button(page, "Stage notes.txt").click(),
  },
  unstage_file: {
    label: "Unstage file",
    setup: (page) => uncommitted(page, "src/app.ts"),
    entry: (page) => button(page, "Unstage src/app.ts").click(),
  },
  discard_file: {
    label: "Discard file changes…",
    setup: (page) => uncommitted(page, "notes.txt"),
    entry: (page) => button(page, "Discard notes.txt").click(),
  },
  stage_all: {
    label: "Stage all",
    setup: (page) => uncommitted(page),
    entry: (page) => button(page, "Stage all").click(),
  },
  unstage_all: {
    label: "Unstage all",
    setup: (page) => uncommitted(page),
    entry: (page) => button(page, "Unstage all").click(),
  },
  commit: {
    label: "Commit…",
    setup: async (page) => {
      await uncommitted(page);
      await page.getByRole("textbox", { name: "Subject" }).fill("From the palette");
    },
    entry: (page) =>
      page.getByRole("region", { name: "Commit message" }).getByRole("button", { name: "Commit" }).click(),
  },
  amend: {
    label: "Amend last commit…",
    setup: (page) => uncommitted(page),
    entry: (page) => button(page, "Amend").click(),
  },
  checkout: {
    label: "Check out branch…",
    setup: login,
    entry: (page) => treeMenu(page, ["api", "Local", "feature", "login"], "Check out"),
  },
  create_branch: {
    label: "Create branch…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Branch" }).click(),
  },
  rename_branch: {
    label: "Rename branch…",
    setup: login,
    entry: (page) => treeMenu(page, ["api", "Local", "feature", "login"], "Rename…"),
  },
  delete_branch: {
    label: "Delete branch…",
    setup: login,
    entry: (page) => treeMenu(page, ["api", "Local", "feature", "login"], "Delete…"),
  },
  set_upstream: {
    label: "Set upstream…",
    setup: login,
    entry: (page) => treeMenu(page, ["api", "Local", "feature", "login"], "Set upstream…"),
  },
  checkout_remote_branch: {
    label: "Check out remote branch",
    setup: remoteLogin,
    entry: (page) => treeMenu(page, ["api", "Remotes", "origin", "feature", "login"], "Check out"),
  },
  checkout_commit: {
    label: "Check out commit (detached)",
    setup: aCommit,
    entry: (page) => commitMenu(page, "Check out (detached)"),
  },
  fetch: {
    label: "Fetch",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Fetch", exact: true }).click(),
  },
  fetch_with_options: {
    label: "Fetch with options…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Fetch options" }).click(),
  },
  pull: {
    label: "Pull",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Pull", exact: true }).click(),
  },
  pull_with_options: {
    label: "Pull with options…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Pull options" }).click(),
  },
  push: {
    label: "Push",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Push", exact: true }).click(),
  },
  push_with_options: {
    label: "Push with options…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Push options" }).click(),
  },
  merge: {
    label: "Merge into current…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Merge" }).click(),
  },
  rebase: {
    label: "Rebase current onto…",
    setup: api,
    entry: async (page) => {
      await toolbar(page).getByRole("button", { name: "Merge" }).click();
      await page.getByRole("dialog").getByRole("checkbox", { name: /Rebase/ }).check();
    },
  },
  cherry_pick: { label: "Cherry-pick commit", setup: aCommit, entry: (page) => commitMenu(page, "Cherry-pick") },
  revert: { label: "Revert commit", setup: aCommit, entry: (page) => commitMenu(page, "Revert") },
  abort: {
    fake: "conflict",
    label: "Abort",
    setup: api,
    entry: (page) => banner(page).getByRole("button", { name: "Abort" }).click(),
  },
  continue: {
    fake: "conflict-choice",
    label: "Continue",
    setup: async (page) => {
      // The text conflict is resolved as it stands, the other file by a choice: nothing is left unmerged.
      await editorOn(page);
      await button(page, "Mark resolved").click();
      await button(page, "Mark resolved anyway").click();
      await files(page).getByRole("row", { name: /src\/app\.ts/ }).click();
      await useThis(page, "Ours: main");
      await expect(banner(page).getByRole("button", { name: "Continue" })).toBeEnabled();
    },
    entry: (page) => banner(page).getByRole("button", { name: "Continue" }).click(),
  },
  mark_resolved: {
    fake: "conflict",
    label: "Mark resolved",
    setup: editorOn,
    entry: (page) => button(page, "Mark resolved").click(),
  },
  take_ours: { fake: "conflict", label: "Take ours", setup: inHunk, entry: (page) => page.keyboard.press("Alt+1") },
  take_theirs: { fake: "conflict", label: "Take theirs", setup: inHunk, entry: (page) => page.keyboard.press("Alt+2") },
  take_both: { fake: "conflict", label: "Take both", setup: inHunk, entry: (page) => page.keyboard.press("Alt+3") },
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
  stash: {
    label: "Stash changes…",
    setup: api,
    entry: (page) => toolbar(page).getByRole("button", { name: "Stash" }).click(),
  },
  stash_apply: {
    label: "Apply stash",
    setup: stashRow,
    entry: (page) => treeMenu(page, ["api", "Stashes", "On main"], "Apply"),
  },
  stash_pop: {
    label: "Pop stash",
    setup: stashRow,
    entry: (page) => treeMenu(page, ["api", "Stashes", "On main"], "Pop"),
  },
  stash_drop: {
    label: "Drop stash…",
    setup: stashRow,
    entry: (page) => treeMenu(page, ["api", "Stashes", "On main"], "Drop…"),
  },
  create_tag: { label: "Create tag…", setup: api, entry: (page) => treeMenu(page, ["api"], "New tag…") },
  delete_tag: {
    label: "Delete tag…",
    setup: tagRow,
    entry: (page) => treeMenu(page, ["api", "Tags", "v1.0"], "Delete…"),
  },
  push_tag: { label: "Push tag", setup: tagRow, entry: (page) => treeMenu(page, ["api", "Tags", "v1.0"], "Push") },
  reset_soft: {
    label: "Reset current branch: soft…",
    setup: aCommit,
    entry: async (page) => {
      await commitMenu(page, "Reset current branch to here…");
      await page.getByRole("radio", { name: /^Soft/ }).check();
    },
  },
  reset_mixed: {
    label: "Reset current branch: mixed…",
    setup: aCommit,
    entry: (page) => commitMenu(page, "Reset current branch to here…"),
  },
  reset_hard: {
    label: "Reset current branch: hard…",
    setup: aCommit,
    entry: async (page) => {
      await commitMenu(page, "Reset current branch to here…");
      await page.getByRole("radio", { name: /^Hard/ }).check();
    },
  },
  add_remote: {
    label: "Add remote…",
    setup: (page) => open(page, ["api", "Remotes"]),
    entry: (page) => treeMenu(page, ["api", "Remotes"], "Add remote…"),
  },
  rename_remote: {
    label: "Rename remote…",
    setup: origin,
    entry: (page) => treeMenu(page, ["api", "Remotes", "origin"], "Rename…"),
  },
  remove_remote: {
    label: "Remove remote…",
    setup: origin,
    entry: (page) => treeMenu(page, ["api", "Remotes", "origin"], "Remove…"),
  },
  edit_remote_url: {
    label: "Edit remote URL…",
    setup: origin,
    entry: (page) => treeMenu(page, ["api", "Remotes", "origin"], "Edit URL…"),
  },
  resolve_with_ai: {
    fake: "ai-conflict",
    label: "Resolve with AI",
    setup: editorOn,
    entry: (page) => button(page, "Resolve with AI").click(),
  },
  resolve_all_with_ai: {
    fake: "ai-resolve-all",
    label: "Resolve all with AI",
    setup: api,
    entry: (page) => banner(page).getByRole("button", { name: "Resolve all with AI" }).click(),
  },
  accept_high_confidence: {
    fake: "ai-conflict",
    label: "Accept all high-confidence",
    setup: async (page) => {
      await editorOn(page);
      await button(page, "Resolve with AI").click();
      await expect(button(page, "Accept all high-confidence")).toBeEnabled();
    },
    entry: (page) => button(page, "Accept all high-confidence").click(),
  },
  settings: { label: "Settings", entry: (page) => toolbar(page).getByRole("button", { name: "Settings" }).click() },
  new_workspace: {
    label: "New workspace…",
    entry: async (page) => {
      await button(page, "Open workspace").click();
      await page.getByRole("menuitem", { name: "New workspace…" }).click();
    },
  },
  import_workspace: {
    label: "Import workspace…",
    entry: async (page) => {
      await button(page, "Open workspace").click();
      await page.getByRole("menuitem", { name: "Import workspace…" }).click();
    },
  },
  export_workspace: {
    label: "Export workspace…",
    entry: async (page) => {
      await page.getByRole("tab", { name: "My workspace" }).click({ button: "right" });
      await page.getByRole("menuitem", { name: "Export workspace…" }).click();
    },
  },
  job_list: {
    label: "Show jobs",
    entry: (page) => page.getByRole("button", { description: "Show the job list" }).click(),
  },
  close_tab: { label: "Close tab", entry: (page) => page.getByTitle("Close My workspace").click() },
  next_tab: {
    label: "Next tab",
    setup: async (page) => {
      await secondTab(page);
      await page.getByRole("tab", { name: "My workspace" }).click();
    },
    entry: (page) => page.getByRole("tab", { name: "My workspace" }).press("ArrowRight"),
  },
  previous_tab: {
    label: "Previous tab",
    setup: secondTab,
    entry: (page) => page.getByRole("tab", { name: "Second" }).press("ArrowLeft"),
  },
  new_group: {
    label: "New group…",
    entry: async (page) => {
      await row(page, 1, "Ungrouped").click({ button: "right" });
      await page.getByRole("menuitem", { name: "New group…", exact: true }).click();
    },
  },
  // With a version on offer, both open the update dialog (PLAT-4).
  check_for_updates: {
    fake: "update",
    label: "Check for updates",
    entry: (page) => page.getByRole("button", { name: "gitcroc 0.2.0 is available" }).click(),
  },
} satisfies Record<PaletteOp, Case>;

for (const [op, known] of Object.entries(CASES) as [PaletteOp, Case][]) {
  test(`ui_22_every_palette_operation_runs_its_action: ${op}`, async ({ browser }) => {
    const pages = [await browser.newPage(), await browser.newPage()] as const;
    for (const page of pages) {
      await page.goto(`/?fake=${known.fake ?? "tree"}`);
      await expect(tree(page).getByRole("treeitem").first()).toBeVisible();
      await known.setup?.(page);
    }
    const [fromPalette, fromEntry] = pages;
    const before = await effect(fromPalette);
    await search(fromPalette, known.label, known.label);
    await expect(options(fromPalette).first()).not.toHaveAttribute("aria-disabled", "true");
    await fromPalette.keyboard.press("Enter");
    await expect(palette(fromPalette)).toBeHidden();
    await known.entry(fromEntry);
    // Both reach the same end: the same requests to the core, the same dialogs open.
    await expect
      .poll(async () => {
        const [shown, expected] = await Promise.all([effect(fromPalette), effect(fromEntry)]);
        return shown === expected && shown !== before ? "same" : `palette ${shown}\nentry   ${expected}`;
      })
      .toBe("same");
  });
}
