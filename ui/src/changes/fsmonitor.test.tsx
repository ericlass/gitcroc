import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, expect, test } from "vitest";
import { fakeState, reset } from "../ipc/fake";
import { textFile } from "../ipc/fakeChanges";
import { summary } from "../ipc/fakeTree";
import { createChangesStore } from "../stores/changes";
import { createCommitStore } from "../stores/commit";
import { createConflictStore } from "../stores/conflicts";
import { createLayoutStore } from "../stores/layout";
import { createSettingsStore } from "../stores/settings";
import ChangesPane from "./ChangesPane";

afterEach(() => {
  cleanup();
  reset();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Lets several rounds of fake calls and events finish. */
const settleAll = async () => {
  for (let round = 0; round < 8; round++) {
    await settle();
  }
};

const id = () => fakeState.workspace.id;

/** The changes pane showing `api`'s uncommitted changes, `offer` the keys its core offers. */
async function setup(offer: string[] | null) {
  fakeState.workspace = {
    ...fakeState.workspace,
    repos: [{ id: "r1", name: "api", path: "/src/api", remote: null }],
  };
  fakeState.workingCopies = { r1: { staged: [], unstaged: [textFile("a.txt", [{ add: "a" }])], unmerged: [] } };
  if (offer !== null) {
    fakeState.fsmonitorOffers = { r1: offer };
  }
  const settings = createSettingsStore();
  await settings.load();
  const changes = createChangesStore(settings.changes);
  render(() => (
    <ChangesPane
      workspace={id()}
      changes={changes}
      commit={createCommitStore()}
      conflicts={createConflictStore(() => {})}
      settings={settings}
      layout={createLayoutStore()}
      onSelectCommit={() => undefined}
    />
  ));
  const show = async () => {
    await changes.show(id(), null);
    await changes.show(id(), { kind: "uncommitted", repo_id: "r1" });
    await settleAll();
  };
  await show();
  return show;
}

const offer = () => screen.queryByRole("group", { name: "Status settings offer" });

test("PERF-7 the offer names the keys the core offers; turning them on is the core's job, asked once", async () => {
  const show = await setup(["core.untrackedCache"]);
  const shown = offer();
  expect(shown?.textContent).toContain("core.untrackedCache");
  fireEvent.click(within(shown ?? document.body).getByRole("button", { name: "Turn on" }));
  await settleAll();
  expect(fakeState.fsmonitorAnswers).toEqual([{ repo: "r1", answer: "enable" }]);
  expect(offer()).toBeNull();
  // Read again, the core no longer offers it.
  await show();
  expect(offer()).toBeNull();
});

test("PERF-7 don't ask again is kept by the core; no offer, nothing shown", async () => {
  const show = await setup(["core.fsmonitor", "core.untrackedCache"]);
  expect(offer()?.textContent).toContain("core.fsmonitor and core.untrackedCache");
  fireEvent.click(screen.getByRole("button", { name: "Don't ask again" }));
  await settleAll();
  expect(fakeState.fsmonitorAnswers).toEqual([{ repo: "r1", answer: "decline" }]);
  await show();
  expect(offer()).toBeNull();

  cleanup();
  await setup(null);
  expect(offer()).toBeNull();
});

test("a refused answer shows why until dismissed", async () => {
  await setup(["core.untrackedCache"]);
  fakeState.summaries = [summary(id(), "r1", "main", { busy: true })];
  fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
  await settleAll();
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toContain("Cannot turn the settings on: another operation is running on the repo");
  fireEvent.click(within(alert).getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).toBeNull();
});
