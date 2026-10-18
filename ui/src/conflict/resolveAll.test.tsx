import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "../App";
import type { AiProposalGet, Conflict, ProposalConfidence } from "../ipc";
import { fakeState, finishAiRunningFiles, reset, setConflict } from "../ipc/fake";
import { fakeAi } from "../ipc/fakeAi";
import { summary } from "../ipc/fakeTree";

// jsdom lays nothing out; give the tree, the history and the file list a
// height so rows are rendered, and text ranges the empty rectangles
// CodeMirror measures.
beforeEach(() => {
  Range.prototype.getClientRects = () =>
    ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect();
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tree-scroller")) {
      return new DOMRect(0, 0, 300, 800);
    }
    if (this.classList.contains("history-scroller")) {
      return new DOMRect(0, 0, 600, 480);
    }
    return this.classList.contains("file-list") ? new DOMRect(0, 0, 400, 400) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls, jobs and events finish. */
const settleAll = async () => {
  for (let round = 0; round < 12; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

/** A text conflict of `path`, one hunk per entry of `hunks`, a resolved line around each. */
function conflictOf(path: string, hunks: number): Conflict {
  return {
    path,
    labels: { ours: "main", theirs: "feature" },
    content: {
      kind: "text",
      chunks: [
        { kind: "resolved", text: "top\n" },
        ...Array.from({ length: hunks }, (_, index) => {
          const n = index + 1;
          return [
            { kind: "hunk" as const, ours: `ours ${n}\n`, base: `base ${n}\n`, theirs: `theirs ${n}\n` },
            { kind: "resolved" as const, text: `after ${n}\n` },
          ];
        }).flat(),
      ],
    },
    edited_outside: false,
    on_disk: null,
  };
}

/** A proposal resolving hunk n as "both n", with the confidences in order. */
const both = (...confidences: ProposalConfidence[]): AiProposalGet => ({
  kind: "proposal",
  proposal: {
    hunks: confidences.map((confidence, index) => ({
      id: index + 1,
      status: "resolved",
      resolution: `both ${index + 1}\n`,
      explanation: `Kept both sides of ${index + 1}.`,
      confidence,
    })),
    usage: [null],
  },
});

const PATHS = ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt", ".env"];

/** A usable default provider the user consented to. */
function withProvider() {
  fakeAi.providers = [
    {
      id: "p1",
      name: "Hosted",
      kind: "openai",
      model: "small-model",
      base_url: "https://api.example.com/v1",
      path: null,
      context_budget: 100_000,
      consent: "api.example.com",
    },
  ];
  fakeAi.default = "p1";
}

/**
 * The app with repo `api` selected, merging in conflict on five text files
 * and `.env`, which the exclude list keeps from AI; `a.txt` has two hunks.
 */
async function setup() {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  const clean = summary(id(), "r1", "main");
  fakeState.summaries = [{ ...clean, summary: clean.summary && { ...clean.summary, dirty: true, changes: 6 } }];
  fakeState.histories = { r1: { length: 3, prefix: "api" } };
  fakeState.workingCopies = {
    r1: {
      staged: [],
      unstaged: [],
      unmerged: PATHS.map((path) => ({ path, conflict: "both_modified" as const })),
      inConflict: true,
    },
  };
  fakeState.fileConflicts = Object.fromEntries(
    PATHS.map((path) => [path, conflictOf(path, path === "a.txt" ? 2 : 1)]),
  );
  fakeState.aiEligibility = {
    ".env": {
      kind: "ineligible",
      reason: { kind: "excluded", pattern: ".env*" },
      message: "matches the exclude list (.env*)",
    },
  };
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium"),
    "b.txt": both("low"),
    "c.txt": { kind: "failed", reason: "the reply listed hunk 1 twice" },
    "d.txt": both("high"),
    "e.txt": both("high"),
  };
  render(() => <App />);
  setConflict("r1", { operation: "merge", applying: "feature", onto: "main", step: null, unmerged_paths: 6 });
  fireEvent.click(await screen.findByRole("treeitem", { name: /^api/ }));
  await settleAll();
}

const banner = () => within(screen.getByRole("region", { name: "Conflict state" }));
const resolveAllButton = () => banner().queryByRole<HTMLButtonElement>("button", { name: "Resolve all with AI" });
const run = () => within(banner().getByRole("region", { name: "Resolve all with AI" }));
const queryRun = () => banner().queryByRole("region", { name: "Resolve all with AI" });

/** Each file of the run's list and where it is, as the list reads. */
const progress = () =>
  run()
    .getAllByRole("listitem")
    .map((item) => [
      item.querySelector(".report-name")?.textContent,
      item.querySelector(".ai-file-state")?.textContent ??
        item.querySelector("[role=img]")?.getAttribute("aria-label"),
    ]);

async function resolveAll() {
  const button = resolveAllButton();
  if (button === null) {
    throw new Error("no Resolve all with AI");
  }
  fireEvent.click(button);
  await settleAll();
}

/** The shown conflict editor. */
function editor(): EditorView {
  const element = document.querySelector<HTMLElement>(".conflict-editor .cm-editor");
  const view = element === null ? null : EditorView.findFromDOM(element);
  if (view === null) {
    throw new Error("no conflict editor is shown");
  }
  return view;
}

const conflictPane = () => within(screen.getByRole("region", { name: "Conflict" }));

/** History row `index`: the uncommitted changes first (UI-10), then the commits. */
function historyRow(index: number): HTMLElement {
  const row = within(screen.getByRole("listbox", { name: "History" })).getAllByRole("option")[index];
  if (row === undefined) {
    throw new Error(`no history row ${index}`);
  }
  return row;
}

test("ai_3_banner_resolve_all_opens_settings_without_a_provider", async () => {
  await setup();

  await resolveAll();

  expect(await screen.findByRole("dialog", { name: "Settings" })).toBeDefined();
  expect(fakeState.aiResolveAllRequests).toHaveLength(1);
  expect(queryRun()).toBeNull();
});

test("ai_13_progress_lists_each_file_and_opens_one_with_its_proposal", async () => {
  withProvider();
  await setup();
  // The history's first commit is shown, not the lists: Resolve all shows them first (PLAT-2).
  fireEvent.click(historyRow(1));
  await settleAll();

  await resolveAll();

  expect(run().getByText("Resolve all with AI · api (5 files)")).toBeDefined();
  expect(progress()).toEqual([
    ["a.txt", "Done: 1 high, 1 medium"],
    ["b.txt", "Done: 1 low"],
    ["c.txt", "AI failed: the reply listed hunk 1 twice"],
    ["d.txt", "Done: 1 high"],
    ["e.txt", "Done: 1 high"],
    [".env", "Not sent: matches the exclude list (.env*)"],
  ]);
  expect(fakeState.aiResolveAllRequests[0]?.files).toEqual([]);

  fireEvent.click(run().getByRole("button", { name: "a.txt" }));
  await settleAll();
  // Opened with its proposal; nothing is accepted for the user (D9).
  expect(conflictPane().getByRole("group", { name: "Proposal for conflict 1" })).toBeDefined();
  expect(conflictPane().getByRole("group", { name: "Proposal for conflict 2" })).toBeDefined();
  expect(editor().state.doc.toString()).toContain("ours 1");
  expect(fakeState.conflictRequests).toEqual([]);
});

test("ai_13_cancel_shows_cancelled_files_and_keeps_finished_proposals", async () => {
  withProvider();
  fakeState.holdAi = true;
  await setup();
  await resolveAll();
  expect(progress()).toEqual([
    ["a.txt", "Running"],
    ["b.txt", "Running"],
    ["c.txt", "Running"],
    ["d.txt", "Running"],
    ["e.txt", "Waiting"],
    [".env", "Not sent: matches the exclude list (.env*)"],
  ]);
  // The first four end; the fifth goes once one of them is free.
  finishAiRunningFiles();
  await settleAll();
  expect(progress()[4]).toEqual(["e.txt", "Running"]);

  fireEvent.click(run().getByRole("button", { name: "Cancel" }));
  await settleAll();

  expect(progress()).toEqual([
    ["a.txt", "Done: 1 high, 1 medium"],
    ["b.txt", "Done: 1 low"],
    ["c.txt", "AI failed: the reply listed hunk 1 twice"],
    ["d.txt", "Done: 1 high"],
    ["e.txt", "Cancelled"],
    [".env", "Not sent: matches the exclude list (.env*)"],
  ]);
  expect(run().queryByRole("button", { name: "Cancel" })).toBeNull();
  expect(Object.keys(fakeState.aiProposals).sort()).toEqual(["a.txt", "b.txt", "c.txt", "d.txt"]);
  expect(resolveAllButton()?.disabled).toBe(false);
});

test("ai_13_buffers_send_only_their_untouched_hunks", async () => {
  withProvider();
  await setup();
  fireEvent.click(historyRow(0));
  await settleAll();
  // a.txt open with its first hunk edited, b.txt with its only one.
  for (const [path, at] of [
    ["a.txt", "ours 1"],
    ["b.txt", "ours 1"],
  ] as const) {
    fireEvent.click(within(screen.getByRole("listbox", { name: "Changed files" })).getByText(path));
    await settleAll();
    const view = editor();
    const from = view.state.doc.toString().indexOf(at);
    view.dispatch({ changes: { from, insert: "mine " }, userEvent: "input.type" });
  }

  await resolveAll();

  expect(fakeState.aiResolveAllRequests[0]?.files).toEqual([
    { file: 0, hunks: [2] },
    { file: 1, hunks: [] },
  ]);
  expect(progress()[1]).toEqual(["b.txt", "Not sent: every conflict is already edited"]);
  expect(run().getByText("Resolve all with AI · api (4 files)")).toBeDefined();
});

test("ai_6_resolve_all_disabled_with_the_reason_when_the_workspace_switch_is_off", async () => {
  withProvider();
  fakeState.workspace = { ...fakeState.workspace, ai: false };
  await setup();

  expect(resolveAllButton()?.disabled).toBe(true);
  expect(banner().getByText("AI is off for this workspace.")).toBeDefined();
});

test("ai_16_off_hides_resolve_all", async () => {
  withProvider();
  await setup();
  await resolveAll();
  expect(queryRun()).not.toBeNull();

  fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
  const settings = within(await screen.findByRole("dialog", { name: "Settings" }));
  fireEvent.click(settings.getByRole("checkbox", { name: "AI features" }));
  await settleAll();

  expect(resolveAllButton()).toBeNull();
  expect(queryRun()).toBeNull();
  expect(fakeState.aiProposals).toEqual({});
  // On again, the run is gone with its proposals (AI-16).
  fireEvent.click(settings.getByRole("checkbox", { name: "AI features" }));
  await settleAll();
  expect(resolveAllButton()).not.toBeNull();
  expect(queryRun()).toBeNull();
});
