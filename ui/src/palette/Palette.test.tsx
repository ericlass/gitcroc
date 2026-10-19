import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { PALETTE_CATALOG, type PaletteItem, type WorkspaceView } from "../ipc";
import { fakeState, reset } from "../ipc/fake";
import { fakePaletteQuery, MAX_PALETTE_ITEMS } from "../ipc/fakePalette";
import { fakeAi } from "../ipc/fakeAi";
import { createCommandStore, type Providers } from "./commands";
import Palette, { PALETTE_ROW_HEIGHT } from "./Palette";

const WORKSPACE = "fake-workspace";

// jsdom lays nothing out; give the list the height of ten rows so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("palette-list") ? new DOMRect(0, 0, 600, 10 * PALETTE_ROW_HEIGHT) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** api and web in Backend, and login-service, with branches as the `tree` data set has them. */
const workspace: WorkspaceView = {
  ...fakeState.workspace,
  repos: [
    { id: "r1", name: "api", path: "/src/api", remote: null },
    { id: "r2", name: "web", path: "/src/web", remote: null },
    { id: "r3", name: "login-service", path: "/src/login-service", remote: null },
  ],
  groups: [{ id: "g1", name: "Backend", repos: ["r2", "r1"] }],
};
const refs = {
  r1: { local: ["main", "feature/login"], current: "main", remotes: { origin: ["main", "feature/web"] } },
  r2: { local: ["main", "push-fix"], current: "main" },
  r3: { local: ["main"], current: "main" },
};

const query = (text: string, ai = true) => fakePaletteQuery(workspace, [], refs, ai, text);
const texts = (items: PaletteItem[]) =>
  items.map((item) => {
    switch (item.kind) {
      case "operation":
        return item.label;
      case "branch":
        return `${item.repo_name}:${item.name}`;
      default:
        return item.name;
    }
  });

test("UI-22 the fake core matches every word in any order and ranks as the core does", () => {
  // Group members first, in the group's order, then the ungrouped repos; local branches before remote ones.
  expect(texts(query("main").items)).toEqual(["web:main", "api:main", "login-service:main", "api:origin/main"]);
  expect(texts(query("WEB  Main").items)).toEqual(["web:main"]);
  expect(texts(query("main web").items)).toEqual(texts(query("web main").items));
  // The whole name, its start, a word's start, then anywhere; ties by kind.
  expect(texts(query("web").items)).toEqual(["web", "api:origin/feature/web", "web:main", "web:push-fix"]);
  expect(texts(query("push").items)).toEqual(["Push", "Push with options…", "Push tag", "web:push-fix"]);
  // Every operation for the empty query, in catalog order, and nothing else.
  expect(texts(query("").items)).toEqual(PALETTE_CATALOG.map((known) => known.label));
});

test("UI-22 the fake core answers at most 100 items with the total", () => {
  const many = Array.from({ length: 150 }, (_, index) => `topic-${String(index).padStart(3, "0")}`);
  const result = fakePaletteQuery(workspace, [], { r1: { local: many, current: "topic-000" } }, true, "topic");
  expect(result.items).toHaveLength(MAX_PALETTE_ITEMS);
  expect(result.total).toBe(150);
});

test("AI-16 the fake core leaves the AI entries out while AI features are off", () => {
  const ai: string[] = PALETTE_CATALOG.filter((known) => known.ai).map((known) => known.label);
  expect(ai.length).toBeGreaterThan(0);
  expect(texts(query("").items)).toEqual(expect.arrayContaining(ai));
  expect(texts(query("", false).items).filter((text) => ai.includes(text))).toEqual([]);
  expect(texts(query("ai", false).items).filter((text) => ai.includes(text))).toEqual([]);
});

/** The palette on the fake core's open workspace, with `providers` for the app's place. */
function setup(providers: Partial<Providers<"app">> = {}) {
  fakeState.workspace = workspace;
  fakeState.refModels = refs;
  const commands = createCommandStore();
  const none = Object.fromEntries(PALETTE_CATALOG.map((known) => [known.op, () => null]));
  commands.provide(null, "app", { ...none, ...providers } as Providers<"app">);
  const onClose = vi.fn();
  const onSelect = vi.fn();
  render(() => <Palette workspace={WORKSPACE} commands={commands} onClose={onClose} onSelect={onSelect} />);
  const box = () => screen.getByRole("combobox", { name: "Command palette" });
  const list = () => screen.getByRole("listbox", { name: "Matches" });
  const options = () =>
    within(list())
      .getAllByRole("option")
      .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index));
  const highlighted = () => options().find((option) => option.getAttribute("aria-selected") === "true");
  const type = (text: string) => fireEvent.input(box(), { target: { value: text } });
  const key = (name: string) => fireEvent.keyDown(box(), { key: name });
  return { box, list, options, highlighted, type, key, onClose, onSelect };
}

test("UI-22 the palette keeps the last answer until a newer one arrives, and drops a superseded one", async () => {
  fakeState.holdPalette = true;
  const palette = setup();
  await settle();
  // The empty query, held: nothing to show yet.
  expect(fakeState.heldPalette).toHaveLength(1);
  fakeState.heldPalette.shift()?.(false);
  await settle();
  expect(palette.options()[0]?.textContent).toContain("Add folder…");

  palette.type("we");
  palette.type("web");
  await settle();
  expect(fakeState.heldPalette).toHaveLength(2);
  // Typing never waits: the list still shows the last answer.
  expect(palette.box()).toHaveProperty("value", "web");
  expect(palette.options()[0]?.textContent).toContain("Add folder…");
  // The core superseded "we" with "web".
  fakeState.heldPalette.shift()?.(true);
  await settle();
  expect(palette.options()[0]?.textContent).toContain("Add folder…");
  fakeState.heldPalette.shift()?.(false);
  await settle();
  expect(palette.options()[0]?.textContent).toContain("web");
  expect(palette.options()[0]?.textContent).toContain("Repository");
});

test("UI-22 the keys move the highlight; Enter runs the highlighted operation", async () => {
  const fetch = vi.fn();
  const palette = setup({ fetch: () => ({ run: fetch }) });
  await settle();
  expect(palette.highlighted()?.textContent).toContain("Add folder…");
  palette.key("ArrowDown");
  palette.key("ArrowDown");
  expect(palette.highlighted()?.textContent).toContain("Remove from workspace");
  palette.key("ArrowUp");
  expect(palette.highlighted()?.textContent).toContain("Clone repository…");
  // The last row is out of view, which jsdom does not scroll to: the box names it.
  palette.key("End");
  expect(palette.box().getAttribute("aria-activedescendant")).toBe(`palette-row-${PALETTE_CATALOG.length - 1}`);
  palette.key("Home");
  expect(palette.highlighted()?.textContent).toContain("Add folder…");
  // Ten rows fit: a page down moves nine.
  palette.key("PageDown");
  expect(palette.highlighted()?.dataset.index).toBe("9");
  palette.key("PageUp");
  expect(palette.highlighted()?.dataset.index).toBe("0");

  palette.type("fetch");
  await settle();
  expect(palette.highlighted()?.textContent).toContain("Fetch");
  palette.key("Enter");
  expect(palette.onClose).toHaveBeenCalledTimes(1);
  expect(fetch).toHaveBeenCalledTimes(1);
});

test("UI-22 an operation that cannot apply shows why and Enter does nothing", async () => {
  const pull = vi.fn();
  const palette = setup({ pull: () => ({ reason: "Pull needs an upstream" }) });
  await settle();
  palette.type("pull");
  await settle();
  const row = palette.highlighted();
  expect(row?.textContent).toContain("Pull");
  expect(row?.getAttribute("aria-disabled")).toBe("true");
  expect(row?.textContent).toContain("Pull needs an upstream");
  palette.key("Enter");
  expect(pull).not.toHaveBeenCalled();
  expect(palette.onClose).not.toHaveBeenCalled();

  // An operation whose subject the selection does not give names what to select.
  palette.type("rename branch");
  await settle();
  expect(palette.highlighted()?.textContent).toContain("Select a branch in the tree");
});

test("UI-22 a repo, group or branch row is handed to the window to select", async () => {
  const palette = setup();
  await settle();
  palette.type("login");
  await settle();
  // A branch is found by its repo's name too.
  expect(palette.options().map((option) => option.textContent)).toEqual([
    expect.stringContaining("login-service"),
    expect.stringContaining("feature/login"),
    expect.stringMatching(/main.*login-service/),
  ]);
  palette.key("ArrowDown");
  palette.key("Enter");
  expect(palette.onClose).toHaveBeenCalledTimes(1);
  expect(palette.onSelect).toHaveBeenCalledWith(expect.objectContaining({ kind: "branch", name: "feature/login" }));
});

test("UI-22 a line says when more matched than are shown", async () => {
  const many = Array.from({ length: 340 }, (_, index) => `topic-${String(index).padStart(3, "0")}`);
  const palette = setup();
  fakeState.refModels = { r1: { local: many, current: "topic-000" } };
  palette.type("topic");
  await settle();
  expect(screen.getByText("100 of 340, keep typing")).toBeTruthy();
});

test("AI-16 the palette shows no AI entry while AI features are off", async () => {
  fakeAi.enabled = false;
  const palette = setup();
  await settle();
  const ai: string[] = PALETTE_CATALOG.filter((known) => known.ai).map((known) => known.label);
  palette.type("ai");
  await settle();
  const shown = within(palette.list()).queryAllByRole("option").map((option) => option.textContent ?? "");
  expect(shown.filter((text) => ai.some((label) => text.includes(label)))).toEqual([]);
});
