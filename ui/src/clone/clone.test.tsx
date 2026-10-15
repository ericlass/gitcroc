import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
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

/** The app with repo `api`, the clone's folder dialog answering `/work`. */
async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.summaries = [summary(id(), "r1", "main")];
  fakeState.cloneParent = "/work";
  render(() => <App />);
  await screen.findByRole("treeitem", { name: /^api/ });
}

/** Opens the clone dialog from the add menu and picks the parent folder. */
async function openClone() {
  fireEvent.click(screen.getAllByRole("button", { name: "Add repository" })[0] as HTMLElement);
  const menu = screen.getByRole("menu", { name: "Add repository" });
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Clone…" }));
  const dialog = screen.getByRole("dialog", { name: "Clone a repository" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Choose…" }));
  await settleAll();
  return dialog;
}

/** The field labelled `name`; an error shown in its label follows the name. */
const field = (dialog: HTMLElement, name: string) =>
  within(dialog).getByRole<HTMLInputElement>("textbox", { name: new RegExp(`^${name}`) });

const cloneButton = (dialog: HTMLElement) => within(dialog).getByRole<HTMLButtonElement>("button", { name: "Clone" });

test("GRP-1 the clone dialog's folder name follows the URL until edited; the clone is added and selected", async () => {
  await setup();
  const dialog = await openClone();
  expect(within(dialog).getByText("/work")).toBeDefined();

  // The core's default: the URL's last path segment without `.git`.
  fireEvent.input(field(dialog, "URL"), { target: { value: "https://example.com/team/tool.git" } });
  await settleAll();
  expect(field(dialog, "Folder name").value).toBe("tool");
  fireEvent.input(field(dialog, "URL"), { target: { value: "git@example.com:team/lib.git" } });
  await settleAll();
  expect(field(dialog, "Folder name").value).toBe("lib");

  // Once the user typed a name, the URL leaves it alone.
  fireEvent.input(field(dialog, "Folder name"), { target: { value: "my-lib" } });
  fireEvent.input(field(dialog, "URL"), { target: { value: "https://example.com/team/other.git" } });
  await settleAll();
  expect(field(dialog, "Folder name").value).toBe("my-lib");
  expect(within(dialog).getByText("Clones into /work/my-lib")).toBeDefined();

  expect(cloneButton(dialog).disabled).toBe(false);
  fireEvent.click(cloneButton(dialog));
  await settleAll();
  expect(fakeState.cloneRequests).toEqual([
    { url: "https://example.com/team/other.git", parent: "/work", name: "my-lib" },
  ]);
  expect(screen.queryByRole("dialog", { name: "Clone a repository" })).toBeNull();

  // Added to the workspace and selected; the core is told.
  await eventually(() =>
    expect(screen.getByRole("treeitem", { name: /^my-lib/ }).getAttribute("aria-selected")).toBe("true"),
  );
  expect(fakeState.selected[id()]).toBe("r2");
  expect(fakeState.jobs[0]?.label).toBe("Clone my-lib");
});

test("GRP-1 the clone dialog refuses an existing folder as the core reports it", async () => {
  await setup();
  fakeState.folders = { "/work/tool": "full" };
  const dialog = await openClone();

  fireEvent.input(field(dialog, "URL"), { target: { value: "https://example.com/tool.git" } });
  await settleAll();
  expect(field(dialog, "Folder name").value).toBe("tool");
  expect(within(dialog).getByText("the folder /work/tool exists and is not empty")).toBeDefined();
  expect(cloneButton(dialog).disabled).toBe(true);

  fireEvent.input(field(dialog, "Folder name"), { target: { value: "tool-2" } });
  await settleAll();
  expect(within(dialog).queryByText(/exists and is not empty/)).toBeNull();
  expect(cloneButton(dialog).disabled).toBe(false);

  // A clone git fails leaves the workspace as it was.
  fakeState.cloneFailure = "fatal: repository 'https://example.com/tool.git/' not found\n";
  fireEvent.click(cloneButton(dialog));
  await eventually(() => expect(fakeState.jobs[0]?.outcome).toEqual({ kind: "failed" }));
  expect(fakeState.workspace.repos.map((repo) => repo.name)).toEqual(["api"]);
});

test("UI-7 a repo's menu opens it in the file manager and in a terminal", async () => {
  await setup();
  const open = (entry: string) => {
    fireEvent.contextMenu(screen.getByRole("treeitem", { name: /^api/ }), { clientX: 10, clientY: 10 });
    fireEvent.click(within(screen.getByRole("menu", { name: "Repository" })).getByRole("menuitem", { name: entry }));
  };

  open("Open in file manager");
  open("Open in terminal");
  await settleAll();
  expect(fakeState.opened).toEqual([
    { how: "file_manager", repo: "r1" },
    { how: "terminal", repo: "r1" },
  ]);

  // A missing terminal is reported with what was tried.
  fakeState.openFailure = "none of x-terminal-emulator, gnome-terminal, konsole, xterm was found";
  open("Open in terminal");
  await settleAll();
  expect(screen.getByRole("alert").textContent).toBe(fakeState.openFailure);
});
