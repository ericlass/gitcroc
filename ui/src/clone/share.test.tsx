import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { MissingRepo } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
import { summary } from "../ipc/fakeTree";

// jsdom lays nothing out; give the tree a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("tree-scroller") ? new DOMRect(0, 0, 300, 800) : real.call(this);
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

/** Waits for `check` to pass, as for a fake job's end; the deadline only ends a hung test. */
const eventually = (check: () => void) => vi.waitFor(check, { timeout: 120_000 });

const id = () => fakeState.workspace.id;

const missing = (repo_id: string, folder: string, remote: string | null, parent = "/team"): MissingRepo => ({
  repo_id,
  name: folder,
  remote,
  parent,
  folder,
});

/** The app with repo `api`. */
async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
    groups: [{ id: "g1", name: "Backend", repos: [] }],
  };
  fakeState.summaries = [summary(id(), "r1", "main")];
  fakeState.cloneParent = "/picked";
  render(() => <App />);
  await screen.findByRole("treeitem", { name: /^api/ });
}

const missingDialog = () => screen.getByRole("dialog", { name: "Missing repositories" });

/** The row of the missing repos dialog for `name`. */
const row = (name: string) => {
  const rows = within(missingDialog()).getAllByRole("listitem");
  const found = rows.find((item) => item.querySelector(".missing-name")?.textContent === name);
  if (found === undefined) {
    throw new Error(`no row for ${name}`);
  }
  return found;
};

const check = (name: string) => within(row(name)).getByRole<HTMLInputElement>("checkbox");

const cloneSelected = () =>
  within(missingDialog()).getByRole<HTMLButtonElement>("button", { name: "Clone selected" });

test("DATA-2 import opens the new workspace and lists its missing repos; Clone selected clones the checked ones", async () => {
  await setup();
  fakeState.importAnswer = {
    workspace: {
      ...fakeState.workspace,
      id: "imported",
      name: "My workspace",
      repos: [
        { id: "r1", name: "api", path: "/team/api", remote: "https://example.com/api.git" },
        { id: "r2", name: "docs", path: "/team/docs", remote: null },
        { id: "r3", name: "web", path: "/team/nested/web", remote: "https://example.com/web.git" },
      ],
      groups: [],
    },
    missing: ["r1", "r2", "r3"],
  };
  fakeState.missingRepos = [
    missing("r1", "api", "https://example.com/api.git"),
    missing("r2", "docs", null),
    missing("r3", "web", "https://example.com/web.git", "/team/nested"),
  ];
  fakeState.folders = { "/team/api": "full" };

  fireEvent.click(screen.getByRole("button", { name: "Open workspace" }));
  fireEvent.click(
    within(screen.getByRole("menu", { name: "Open workspace" })).getByRole("menuitem", { name: "Import workspace…" }),
  );
  await settleAll();

  // The import is the active tab, named apart from the open one.
  const tab = screen.getByRole("tab", { name: /^My workspace \(2\)/ });
  expect(tab.getAttribute("aria-selected")).toBe("true");

  // Checked when there is a remote; a repo without one cannot be cloned.
  expect(check("api").checked).toBe(true);
  expect(check("web").checked).toBe(true);
  expect(check("docs").checked).toBe(false);
  expect(check("docs").disabled).toBe(true);
  expect(within(row("docs")).getByText("No remote to clone from")).toBeDefined();
  expect(within(row("web")).getByText("https://example.com/web.git")).toBeDefined();
  expect(within(row("web")).getByText("/team/nested")).toBeDefined();

  // The core refuses api's folder; another name, or another parent picked, is fine.
  expect(within(row("api")).getByText("the folder /team/api exists and is not empty")).toBeDefined();
  fireEvent.input(within(row("api")).getByRole("textbox", { name: "Folder name for api" }), {
    target: { value: "api-2" },
  });
  fireEvent.click(within(row("web")).getByRole("button", { name: "Choose the folder for web" }));
  await settleAll();
  expect(within(row("api")).queryByText(/exists and is not empty/)).toBeNull();
  expect(within(row("web")).getByText("/picked")).toBeDefined();

  fireEvent.click(check("web"));
  expect(cloneSelected().disabled).toBe(false);
  fireEvent.click(cloneSelected());
  await settleAll();
  expect(fakeState.cloneRequests).toEqual([
    { url: "https://example.com/api.git", parent: "/team", name: "api-2", repo: "r1", group: null },
  ]);
  await eventually(() => expect(within(row("api")).getByRole("status").textContent).toBe("Cloned"));
  expect(check("api").disabled).toBe(true);
  expect(within(row("web")).queryByRole("status")).toBeNull();

  // A clone git fails keeps the repo missing, with git's message.
  fakeState.cloneFailure = "fatal: repository 'https://example.com/web.git/' not found\n";
  fireEvent.click(check("web"));
  fireEvent.click(cloneSelected());
  await eventually(() =>
    expect(within(row("web")).getByRole("status").textContent).toBe(
      "Failed: git exited with code 128: fatal: repository 'https://example.com/web.git/' not found",
    ),
  );
  expect(fakeState.cloneRequests[1]).toEqual({
    url: "https://example.com/web.git",
    parent: "/picked",
    name: "web",
    repo: "r3",
    group: null,
  });

  fireEvent.click(within(missingDialog()).getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog", { name: "Missing repositories" })).toBeNull();
});

test("DATA-1 an import the core refuses shows why and opens nothing", async () => {
  await setup();
  const why = "cannot import /x/team.toml: it is from a newer gitcroc (schema 2; this version reads schema 1)";
  fakeState.importAnswer = { error: why };

  fireEvent.click(screen.getByRole("button", { name: "Open workspace" }));
  fireEvent.click(
    within(screen.getByRole("menu", { name: "Open workspace" })).getByRole("menuitem", { name: "Import workspace…" }),
  );
  await settleAll();
  expect(screen.getByRole("alert").textContent).toContain(`C${why.slice(1)}`);
  expect(screen.getAllByRole("tab")).toHaveLength(1);
  expect(screen.queryByRole("dialog", { name: "Missing repositories" })).toBeNull();
});

test("DATA-2 the tab's menu exports the workspace and reports paths kept absolute", async () => {
  await setup();
  fakeState.exportAnswer = { path: "/team/platform.toml", absolute: ["D:\\repos\\tool"] };

  fireEvent.contextMenu(screen.getByRole("tab", { name: /^My workspace/ }), { clientX: 5, clientY: 5 });
  fireEvent.click(within(screen.getByRole("menu", { name: "Tab actions" })).getByRole("menuitem", { name: "Export workspace…" }));
  await settleAll();
  expect(fakeState.exports).toEqual([id()]);
  expect(screen.getByRole("alert").textContent).toContain(
    "Exported the workspace to /team/platform.toml. No relative path reaches these repos, so they stay absolute: " +
      "D:\\repos\\tool.",
  );
});

test("DATA-2 a missing repo's menu reopens the dialog with only that repo checked", async () => {
  await setup();
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [
      ...fakeState.workspace.repos,
      { id: "r2", name: "web", path: "/team/web", remote: "https://example.com/web.git" },
      { id: "r3", name: "lib", path: "/team/lib", remote: "https://example.com/lib.git" },
    ],
  };
  fakeState.summaries = [
    summary(id(), "r1", "main"),
    { ...summary(id(), "r2", "main"), missing: true, summary: null },
    { ...summary(id(), "r3", "main"), missing: true, summary: null },
  ];
  fakeState.missingRepos = [missing("r2", "web", "https://example.com/web.git"), missing("r3", "lib", "x")];
  cleanup();
  render(() => <App />);
  await screen.findByRole("treeitem", { name: /^lib/ });

  // A repo that is there offers no clone.
  fireEvent.contextMenu(screen.getByRole("treeitem", { name: /^api/ }), { clientX: 10, clientY: 10 });
  const menu = screen.getByRole("menu", { name: "Repository" });
  expect(within(menu).queryByRole("menuitem", { name: "Clone from remote…" })).toBeNull();
  fireEvent.keyDown(document, { key: "Escape" });

  fireEvent.contextMenu(screen.getByRole("treeitem", { name: /^lib/ }), { clientX: 10, clientY: 10 });
  fireEvent.click(
    within(screen.getByRole("menu", { name: "Repository" })).getByRole("menuitem", { name: "Clone from remote…" }),
  );
  await settleAll();
  expect(check("lib").checked).toBe(true);
  expect(check("web").checked).toBe(false);
});

test("GRP-2 the clone dialog adds the repo to an existing group or a new one the core accepts", async () => {
  await setup();
  const openClone = async () => {
    fireEvent.click(screen.getAllByRole("button", { name: "Add repository" })[0] as HTMLElement);
    fireEvent.click(within(screen.getByRole("menu", { name: "Add repository" })).getByRole("menuitem", { name: "Clone…" }));
    const dialog = screen.getByRole("dialog", { name: "Clone a repository" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Choose…" }));
    fireEvent.input(within(dialog).getByRole("textbox", { name: "URL" }), {
      target: { value: "https://example.com/lib.git" },
    });
    await settleAll();
    return dialog;
  };
  const group = (dialog: HTMLElement) => within(dialog).getByRole<HTMLSelectElement>("combobox", { name: "Add to group" });
  const clone = (dialog: HTMLElement) => within(dialog).getByRole<HTMLButtonElement>("button", { name: "Clone" });

  let dialog = await openClone();
  expect([...group(dialog).options].map((option) => option.textContent)).toEqual(["None", "Backend", "New group…"]);
  fireEvent.change(group(dialog), { target: { value: "g1" } });
  fireEvent.click(clone(dialog));
  await settleAll();
  expect(fakeState.cloneRequests[0]?.group).toEqual({ kind: "existing", group_id: "g1" });
  await eventually(() => expect(fakeState.workspace.groups[0]?.repos).toEqual(["r2"]));

  // A new group's name: needed, and refused by the core when another group has it (GRP-2).
  dialog = await openClone();
  fireEvent.input(within(dialog).getByRole("textbox", { name: /^Folder name/ }), { target: { value: "lib-2" } });
  fireEvent.change(group(dialog), { target: { value: "+new" } });
  await settleAll();
  expect(clone(dialog).disabled).toBe(true);
  fireEvent.input(within(dialog).getByRole("textbox", { name: "Group name" }), { target: { value: "backend" } });
  fireEvent.click(clone(dialog));
  await settleAll();
  expect(within(dialog).getByRole("alert").textContent).toBe('a group named "Backend" exists already');
  expect(fakeState.cloneRequests).toHaveLength(1);

  fireEvent.input(within(dialog).getByRole("textbox", { name: "Group name" }), { target: { value: "Libs" } });
  fireEvent.click(clone(dialog));
  await settleAll();
  expect(fakeState.cloneRequests[1]?.group).toEqual({ kind: "new", name: "Libs" });
  await eventually(() =>
    expect(fakeState.workspace.groups.map((known) => [known.name, known.repos])).toEqual([
      ["Backend", ["r2"]],
      ["Libs", ["r3"]],
    ]),
  );
});
