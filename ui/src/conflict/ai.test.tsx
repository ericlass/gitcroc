import { EditorView } from "@codemirror/view";
import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import ChangesPane from "../changes/ChangesPane";
import {
  type AiProposalGet,
  type AiUsage,
  type Conflict,
  core,
  type ProposalConfidence,
  type ProposedHunk,
} from "../ipc";
import { fakeState, finishAiJobs, reset } from "../ipc/fake";
import { fakeAi, type FakeProvider } from "../ipc/fakeAi";
import { textFile } from "../ipc/fakeChanges";
import { summary } from "../ipc/fakeTree";
import { createAiStore } from "../stores/ai";
import { createChangesStore } from "../stores/changes";
import { createCommitStore } from "../stores/commit";
import { createConflictStore, followAiFeatures } from "../stores/conflicts";
import { createLayoutStore } from "../stores/layout";
import { createSettingsStore } from "../stores/settings";
import { createCommandStore } from "../palette/commands";

// jsdom lays nothing out; give the file list a height so rows are rendered,
// and text ranges the empty rectangles CodeMirror measures.
beforeEach(() => {
  Range.prototype.getClientRects = () =>
    ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] }) as DOMRectList;
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
function conflictOf(path: string, hunks: [ours: string, base: string, theirs: string][]): Conflict {
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
  };
}

const THREE = conflictOf("a.txt", [
  ["ours 1\n", "base 1\n", "theirs 1\n"],
  ["ours 2\n", "base 2\n", "theirs 2\n"],
  ["ours 3\n", "base 3\n", "theirs 3\n"],
]);

/** The proposal for hunk `id`; resolved unless `resolution` is `null`. */
const proposed = (
  id: number,
  confidence: ProposalConfidence,
  resolution: string | null,
  explanation = `Kept both sides of ${id}.`,
): ProposedHunk => ({
  id,
  status: resolution === null ? "unresolved" : "resolved",
  resolution,
  explanation,
  confidence,
});

const answer = (hunks: ProposedHunk[], usage: (AiUsage | null)[] = [null]): AiProposalGet => ({
  kind: "proposal",
  proposal: { hunks, usage },
});

/** A proposal resolving hunk n as "both n", with the confidences in order. */
const both = (...confidences: ProposalConfidence[]) =>
  answer(confidences.map((confidence, index) => proposed(index + 1, confidence, `both ${index + 1}\n`)));

const provider = (change: Partial<FakeProvider> = {}): FakeProvider => ({
  id: "p1",
  name: "Hosted",
  kind: "openai",
  model: "small-model",
  base_url: "https://api.example.com/v1",
  path: null,
  context_budget: 100_000,
  consent: "api.example.com",
  ...change,
});

/** A usable default provider the user consented to. */
function withProvider(change: Partial<FakeProvider> = {}) {
  fakeAi.providers = [provider(change)];
  fakeAi.default = "p1";
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
  };
  fakeState.fileConflicts = { "a.txt": THREE, "b.txt": conflictOf("b.txt", [["ours b\n", "base b\n", "theirs b\n"]]) };
  const settings = createSettingsStore();
  await settings.load();
  const ai = createAiStore();
  await ai.subscribe();
  const changes = createChangesStore(settings.changes);
  const conflicts = createConflictStore((workspace) => void changes.reload(workspace));
  const stops = [changes.subscribe(id()), conflicts.subscribe(id())];
  const openSettings = vi.fn();
  const [workspaceOn, setWorkspaceOn] = (() => {
    let on = true;
    return [() => on, (value: boolean) => (on = value)] as const;
  })();
  render(() => {
    followAiFeatures(ai, conflicts);
    return (
      <ChangesPane
        workspace={id()}
        commands={createCommandStore()}
        changes={changes}
        commit={createCommitStore()}
        conflicts={conflicts}
        settings={settings}
        layout={createLayoutStore()}
        ai={{ store: ai, workspaceOn, openSettings }}
        onSelectCommit={() => {}}
      />
    );
  });
  await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
  await settleAll();
  return { ai, changes, conflicts, openSettings, setWorkspaceOn, stops };
}

const files = () => screen.getByRole("listbox", { name: "Changed files" });
const pane = () => within(screen.getByRole("region", { name: "Conflict" }));
const toolbar = () => within(pane().getByRole("toolbar", { name: "Conflict" }));

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
  editor().dispatch({
    changes: { from: at, insert },
    selection: { anchor: at + insert.length },
    userEvent: "input.type",
  });

const selectFile = async (name: string) => {
  fireEvent.click(within(files()).getByText(name));
  await settleAll();
};

const proposalOf = (number: number) => pane().getByRole("group", { name: `Proposal for conflict ${number}` });
const queryProposal = (number: number) => pane().queryByRole("group", { name: `Proposal for conflict ${number}` });

/** Clicks Resolve with AI and lets the fake's job end. */
async function resolveWithAi() {
  fireEvent.click(toolbar().getByRole("button", { name: "Resolve with AI" }));
  await settleAll();
}

/** Records the commands sent to the core from now on, by name. */
function recordCommands(): string[] {
  const calls: string[] = [];
  const commands = core.commands as unknown as Record<string, (...args: unknown[]) => unknown>;
  for (const [name, original] of Object.entries(commands)) {
    vi.spyOn(commands, name).mockImplementation((...args: unknown[]) => {
      calls.push(name);
      return original(...args);
    });
  }
  return calls;
}

test("ai_11_each_hunk_shows_text_explanation_and_confidence", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium", "low"),
  };
  await setup();
  await resolveWithAi();
  // Only the untouched hunks were asked for, counted from 1 (AI-5).
  expect(fakeState.aiResolveRequests).toEqual([{ handle: expect.any(Number), file: 0, hunks: [1, 2, 3] }]);

  const confidences = [
    [1, "high", "High confidence"],
    [2, "medium", "Medium confidence"],
    [3, "low", "Low confidence"],
  ] as const;
  for (const [number, level, word] of confidences) {
    const group = within(proposalOf(number));
    expect(group.getByText(`both ${number}`, { selector: "pre" })).toBeDefined();
    expect(group.getByText(`Kept both sides of ${number}.`)).toBeDefined();
    // A word and a marker shaped by the level, not a color alone (UI-25).
    const confidence = proposalOf(number).querySelector(`.cm-proposal-confidence-${level}`);
    expect(confidence?.textContent).toContain(word);
    expect(confidence?.querySelector("[aria-hidden=true]")?.textContent).not.toBe("");
    for (const button of ["Accept", "Reject", "Edit"]) {
      expect(group.getByRole("button", { name: `${button} the proposal for conflict ${number}` })).toBeDefined();
    }
  }
  // The proposal sits under its hunk: after the hunk's end, before the next resolved line.
  const lines = [...document.querySelectorAll(".cm-content > *")].map((element) => element.textContent ?? "");
  const first = lines.findIndex((line) => line.startsWith("End of conflict 1"));
  expect(lines[first + 1]).toContain("both 1");
  expect(lines[first + 2]).toBe("after 1");
  // The buffer is not changed by showing a proposal.
  expect(text()).toContain("<<<<<<< main\nours 1\n=======\ntheirs 1\n>>>>>>> feature\n");
});

test("ai_11 an unresolved hunk shows its explanation and no Accept", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": answer([
      proposed(1, "low", null, "Both sides rename the function differently."),
      proposed(2, "high", "both 2\n"),
      proposed(3, "high", "both 3\n"),
    ]),
  };
  await setup();
  await resolveWithAi();
  const group = within(proposalOf(1));
  expect(group.getByText("Both sides rename the function differently.")).toBeDefined();
  expect(group.getByText("Not resolved")).toBeDefined();
  expect(group.queryByRole("button", { name: /^Accept/ })).toBeNull();
  expect(group.queryByRole("button", { name: /^Edit/ })).toBeNull();
  expect(group.getByRole("button", { name: "Reject the proposal for conflict 1" })).toBeDefined();
});

test("ai_11 the explanation renders as text, never HTML (SEC-3)", async () => {
  withProvider();
  const html = '<img src=x onerror="alert(1)"><b>bold</b>';
  fakeState.aiAnswers = {
    "a.txt": answer([
      proposed(1, "high", "<script>run()</script>\n", html),
      proposed(2, "high", "both 2\n"),
      proposed(3, "high", "both 3\n"),
    ]),
  };
  await setup();
  await resolveWithAi();
  const group = proposalOf(1);
  expect(within(group).getByText(html)).toBeDefined();
  expect(group.querySelector("img, b, script")).toBeNull();
  expect(within(group).getByText("<script>run()</script>", { selector: "pre" })).toBeDefined();
});

test("ai_11_accept_all_high_confidence_only_in_the_file", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium", "high"),
  };
  await setup();
  // The other file has an unsaved edit of its own.
  await selectFile("b.txt");
  type(0, "kept ");
  const other = text();
  await selectFile("a.txt");
  await resolveWithAi();

  fireEvent.click(toolbar().getByRole("button", { name: "Accept all high-confidence" }));
  expect(text()).toBe(
    "top\nboth 1\nafter 1\n<<<<<<< main\nours 2\n=======\ntheirs 2\n>>>>>>> feature\nafter 2\nboth 3\nafter 3\n",
  );
  // Medium stays, numbered as the one conflict left.
  expect(within(proposalOf(1)).getByText("both 2", { selector: "pre" })).toBeDefined();
  expect(queryProposal(2)).toBeNull();
  expect(toolbar().getByRole("button", { name: "Accept all high-confidence" })).toHaveProperty("disabled", true);

  await selectFile("b.txt");
  expect(text()).toBe(other);
  expect(fakeState.conflictRequests).toEqual([]);
});

test("ai_12_accept_changes_only_the_buffer", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium", "low"),
  };
  await setup();
  await resolveWithAi();

  const calls = recordCommands();
  fireEvent.click(within(proposalOf(2)).getByRole("button", { name: "Accept the proposal for conflict 2" }));
  // The whole hunk, markers included, is replaced, exactly as a take does (OPS-3).
  expect(text()).toContain("after 1\nboth 2\nafter 2\n");
  expect(toolbar().getByRole("status").textContent).toBe("2 conflicts left");
  // Accept the keyboard's way: the hunk at the cursor (UI-22).
  const view = editor();
  view.dispatch({ selection: { anchor: 0 } });
  fireEvent.keyDown(view.contentDOM, { key: "ArrowDown", altKey: true });
  fireEvent.keyDown(view.contentDOM, { key: "4", code: "Digit4", altKey: true });
  expect(text()).toContain("top\nboth 1\nafter 1\n");
  // Nothing went to the core, not even a read.
  expect(calls).toEqual([]);

  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Accept the proposal for conflict 1" }));
  fireEvent.click(toolbar().getByRole("button", { name: "Mark resolved" }));
  await settleAll();
  expect(fakeState.conflictRequests).toEqual([
    {
      handle: expect.any(Number),
      file: 0,
      text: "top\nboth 1\nafter 1\nboth 2\nafter 2\nboth 3\nafter 3\n",
      override_markers: false,
    },
  ]);
});

test("ai_12_accept_refused_on_an_edited_hunk", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "high", "high"),
  };
  await setup();
  await resolveWithAi();
  type(text().indexOf("ours 1"), "edited ");

  const group = within(proposalOf(1));
  expect(group.getByText("Hunk changed")).toBeDefined();
  expect(group.getByRole("button", { name: "Accept the proposal for conflict 1" })).toHaveProperty("disabled", true);
  expect(group.getByRole("button", { name: "Edit the proposal for conflict 1" })).toHaveProperty("disabled", true);
  // Neither the key nor Accept all takes it.
  const view = editor();
  view.dispatch({ selection: { anchor: text().indexOf("edited") } });
  fireEvent.keyDown(view.contentDOM, { key: "4", code: "Digit4", altKey: true });
  fireEvent.click(toolbar().getByRole("button", { name: "Accept all high-confidence" }));
  expect(text()).toContain("<<<<<<< main\nedited ours 1\n=======\ntheirs 1\n>>>>>>> feature\nafter 1\nboth 2\n");
  expect(group.getByText("Hunk changed")).toBeDefined();
});

test("ai_12 Edit then Accept puts the edited text; Cancel goes back; Reject hides the proposal", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium", "low"),
  };
  await setup();
  await resolveWithAi();

  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Edit the proposal for conflict 1" }));
  let box = within(proposalOf(1)).getByRole("textbox", { name: "Edited proposal for conflict 1" });
  expect((box as HTMLTextAreaElement).value).toBe("both 1\n");
  fireEvent.input(box, { target: { value: "mine\n" } });
  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Cancel editing the proposal for conflict 1" }));
  expect(within(proposalOf(1)).queryByRole("textbox")).toBeNull();
  expect(within(proposalOf(1)).getByText("both 1", { selector: "pre" })).toBeDefined();

  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Edit the proposal for conflict 1" }));
  box = within(proposalOf(1)).getByRole("textbox", { name: "Edited proposal for conflict 1" });
  fireEvent.input(box, { target: { value: "edited both 1\n" } });
  fireEvent.click(
    within(proposalOf(1)).getByRole("button", { name: "Accept the edited proposal for conflict 1" }),
  );
  expect(text()).toContain("top\nedited both 1\nafter 1\n");

  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Reject the proposal for conflict 1" }));
  expect(queryProposal(1)).toBeNull();
  // The hunk itself is untouched by Reject.
  expect(text()).toContain("<<<<<<< main\nours 2\n=======\ntheirs 2\n>>>>>>> feature\n");
  expect(within(proposalOf(2)).getByText("both 3", { selector: "pre" })).toBeDefined();
});

test("ai_12 proposals and their view survive switching files", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "medium", "low"),
  };
  await setup();
  await resolveWithAi();
  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Reject the proposal for conflict 1" }));
  fireEvent.click(within(proposalOf(2)).getByRole("button", { name: "Edit the proposal for conflict 2" }));
  fireEvent.input(within(proposalOf(2)).getByRole("textbox"), { target: { value: "half done\n" } });

  await selectFile("b.txt");
  expect(queryProposal(1)).toBeNull();
  await selectFile("a.txt");
  expect(queryProposal(1)).toBeNull();
  expect((within(proposalOf(2)).getByRole("textbox") as HTMLTextAreaElement).value).toBe("half done\n");
  expect(within(proposalOf(3)).getByText("both 3", { selector: "pre" })).toBeDefined();
  // Read once, when it was ready.
  expect(fakeState.aiResolveRequests).toHaveLength(1);
});

test("ai_14_token_usage_shown_per_request", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": answer(
      [proposed(1, "high", "both 1\n"), proposed(2, "high", "both 2\n"), proposed(3, "high", "both 3\n")],
      [{ input_tokens: 1200, output_tokens: 340 }, null, { input_tokens: 980, output_tokens: 75 }],
    ),
    "b.txt": answer([proposed(1, "high", "both b\n")], [null]),
  };
  await setup();
  await resolveWithAi();
  const usage = toolbar().getByRole("list", { name: "Token usage" });
  expect(within(usage).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "Request 1: 1,200 input tokens, 340 output tokens",
    "Request 3: 980 input tokens, 75 output tokens",
  ]);

  // Nothing without usage.
  await selectFile("b.txt");
  await resolveWithAi();
  expect(within(proposalOf(1)).getByText("both b", { selector: "pre" })).toBeDefined();
  expect(toolbar().queryByRole("list", { name: "Token usage" })).toBeNull();
  expect(pane().queryByText(/tokens/)).toBeNull();
});

test("ai_3_no_provider_opens_the_settings", async () => {
  const { openSettings } = await setup();
  await resolveWithAi();
  expect(openSettings).toHaveBeenCalledTimes(1);
  expect(queryProposal(1)).toBeNull();
  expect(toolbar().getByRole("button", { name: "Resolve with AI" })).toHaveProperty("disabled", false);
});

test("ai_6 consent first, then asked again; Cancel sends nothing", async () => {
  withProvider({ consent: null });
  fakeState.aiAnswers = {
    "a.txt": both("high", "high", "high"),
  };
  const { ai } = await setup();

  fireEvent.click(toolbar().getByRole("button", { name: "Resolve with AI" }));
  await settleAll();
  expect(ai.state.consent?.request.provider_name).toBe("Hosted");
  ai.cancel();
  await settleAll();
  expect(fakeState.aiResolveRequests).toHaveLength(1);
  expect(queryProposal(1)).toBeNull();

  fireEvent.click(toolbar().getByRole("button", { name: "Resolve with AI" }));
  await settleAll();
  await ai.allow();
  await settleAll();
  expect(fakeState.aiResolveRequests).toHaveLength(3);
  expect(within(proposalOf(1)).getByText("both 1", { selector: "pre" })).toBeDefined();
});

test("ai_4 the ineligible reason shows beside the disabled button, and the workspace switch's", async () => {
  withProvider();
  fakeState.aiEligibility = {
    "b.txt": {
      kind: "ineligible",
      reason: { kind: "excluded", pattern: "b.*" },
      message: "matches the exclude list (b.*)",
    },
  };
  const { setWorkspaceOn } = await setup();
  expect(toolbar().getByRole("button", { name: "Resolve with AI" })).toHaveProperty("disabled", false);

  await selectFile("b.txt");
  const button = toolbar().getByRole("button", { name: "Resolve with AI" });
  expect(button).toHaveProperty("disabled", true);
  expect(toolbar().getByText("Not sent to AI: matches the exclude list (b.*)")).toBeDefined();

  setWorkspaceOn(false);
  await selectFile("a.txt");
  expect(toolbar().getByRole("button", { name: "Resolve with AI" })).toHaveProperty("disabled", true);
  expect(toolbar().getByText("AI is off for this workspace.")).toBeDefined();
  expect(fakeState.aiResolveRequests).toEqual([]);
});

test("ai_9 AI failed shows the reason with Try again", async () => {
  withProvider();
  fakeState.aiAnswers = { "a.txt": { kind: "failed", reason: "the reply was not valid JSON" } };
  await setup();
  await resolveWithAi();
  expect(toolbar().getByRole("alert").textContent).toBe("AI failed: the reply was not valid JSON");
  fakeState.aiAnswers = {
    "a.txt": both("high", "high", "high"),
  };
  fireEvent.click(toolbar().getByRole("button", { name: "Try again" }));
  await settleAll();
  expect(toolbar().queryByRole("alert")).toBeNull();
  expect(within(proposalOf(1)).getByText("both 1", { selector: "pre" })).toBeDefined();
});

test("ai_3 while the job runs the button is Cancel, with the provider's name", async () => {
  withProvider();
  fakeState.holdAi = true;
  fakeState.aiAnswers = {
    "a.txt": both("high", "high", "high"),
  };
  await setup();
  await resolveWithAi();
  expect(toolbar().queryByRole("button", { name: "Resolve with AI" })).toBeNull();
  fireEvent.click(toolbar().getByRole("button", { name: "Cancel (Hosted)" }));
  await settleAll();
  expect(fakeState.cancels).toHaveLength(1);
  expect(toolbar().getByRole("button", { name: "Resolve with AI" })).toBeDefined();
  finishAiJobs();
  await settleAll();
  expect(queryProposal(1)).toBeNull();
});

test("ai_16_off_hides_resolve_with_ai_and_proposals", async () => {
  withProvider();
  fakeState.aiAnswers = {
    "a.txt": both("high", "high", "high"),
  };
  const { ai } = await setup();
  await resolveWithAi();
  fireEvent.click(within(proposalOf(1)).getByRole("button", { name: "Accept the proposal for conflict 1" }));
  type(0, "mine ");
  const edited = text();

  // Turned off while a proposal is shown: it goes at once; the buffer and its edits stay.
  await ai.setOptions({ enabled: false });
  await settleAll();
  expect(queryProposal(1)).toBeNull();
  expect(toolbar().queryByRole("button", { name: "Resolve with AI" })).toBeNull();
  expect(toolbar().queryByRole("button", { name: "Accept all high-confidence" })).toBeNull();
  expect(text()).toBe(edited);
  const view = editor();
  view.dispatch({ selection: { anchor: text().indexOf("ours 2") } });
  fireEvent.keyDown(view.contentDOM, { key: "4", code: "Digit4", altKey: true });
  expect(text()).toBe(edited);

  // On again, the proposal is gone: the core dropped it too.
  await ai.setOptions({ enabled: true });
  await settleAll();
  expect(toolbar().getByRole("button", { name: "Resolve with AI" })).toBeDefined();
  expect(queryProposal(1)).toBeNull();
  expect(text()).toBe(edited);
});
