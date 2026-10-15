import { cleanup, fireEvent, render, screen, within } from "@solidjs/testing-library";
import { createSignal, Show } from "solid-js";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { JobView } from "../ipc";
import { emitJobEnded, emitJobProgress, fakeState, reset } from "../ipc/fake";
import { fakeJob, jobsScenario, OTHER_WORKSPACE } from "../ipc/fakeJobs";
import StatusBar, { OUTCOME_SHOWN_MS } from "../layout/StatusBar";
import { createJobStore } from "../stores/jobs";
import JobList from "./JobList";

const WORKSPACE = { id: "fake-workspace", name: "My workspace" };

// jsdom lays nothing out; give the list a height so rows are rendered.
beforeEach(() => {
  const real = HTMLElement.prototype.getBoundingClientRect;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("job-scroller") ? new DOMRect(0, 0, 800, 480) : real.call(this);
  });
});

afterEach(() => {
  cleanup();
  reset();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** The status bar and the job list it opens, as App puts them together. */
async function setup(jobs: JobView[] = []) {
  fakeState.jobs = jobs;
  const store = createJobStore();
  await store.subscribe();
  await settle();
  const [open, setOpen] = createSignal(false);
  render(() => (
    <>
      <StatusBar workspace={WORKSPACE.id} jobs={store} onOpenJobs={() => setOpen(true)} />
      <Show when={open()}>
        <JobList jobs={store} onClose={() => setOpen(false)} onSelectRepo={() => {}} onRetry={() => {}} />
      </Show>
    </>
  ));
  return store;
}

const status = () => screen.getByRole("status");
const openList = () => fireEvent.click(screen.getByRole("button", { description: "Show the job list" }));
const rows = () =>
  within(screen.getByRole("listbox", { name: "Jobs" }))
    .getAllByRole("option")
    .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index));

const running = (job: Partial<JobView> & Pick<JobView, "job_id" | "label">) =>
  fakeJob(WORKSPACE, { state: "running", outcome: null, duration_ms: null, ...job });

test("UI-21 status bar shows running jobs with progress", async () => {
  await setup();
  expect(status().textContent).toBe("No jobs running");

  emitJobProgress(running({ job_id: 1, label: "Fetch", done: 1, total: 4 }));
  emitJobProgress(running({ job_id: 2, label: "Write commit-graph", total: 0 }));
  // Another workspace's job and a queued one are not running here.
  emitJobProgress(fakeJob(OTHER_WORKSPACE, { job_id: 3, label: "Push", state: "running", outcome: null }));
  emitJobProgress(fakeJob(WORKSPACE, { job_id: 4, label: "Pull", state: "queued", outcome: null }));

  const shown = [...status().querySelectorAll(".status-job")];
  expect(shown.map((job) => job.textContent)).toEqual(["Write commit-graph ", "Fetch 1/4"]);
  expect(within(shown[0] as HTMLElement).getByRole("img", { name: "Running" })).toBeDefined();

  emitJobProgress(running({ job_id: 1, label: "Fetch", done: 3, total: 4 }));
  expect(status().textContent).toContain("Fetch 3/4");
});

test("UI-21 job list shows outcome and raw output of every job", async () => {
  await setup(jobsScenario(WORKSPACE));
  openList();
  const dialog = await screen.findByRole("dialog", { name: "Jobs" });
  expect(dialog).toBeDefined();

  // Newest first, across both workspaces, each labelled with its workspace.
  const texts = rows().map((row) => [...row.children].map((cell) => cell.textContent));
  expect(texts.map(([label, workspace, repo, state]) => [label, workspace, repo, state])).toEqual([
    ["Fetch", "My workspace", "web", "waiting for web"],
    ["Fetch", "My workspace", "api", "Receiving objects 3/10"],
    ["Merge feature/login", "My workspace", "login-service", "Conflict"],
    ["Push", "My workspace", "api", "Failed"],
    ["Pull", "Other workspace", "docs", "Skipped (cancelled)"],
    ["Fetch", "My workspace", "api", "Nothing to do"],
    ["Writing the commit-graph file", "My workspace", "api", "Succeeded"],
  ]);
  expect(texts[3]?.[5]).toBe("1.2 s");
  expect(texts[0]?.[4]).toBe("");

  // Selecting a row shows git's output as text, never as HTML (SEC-3).
  fireEvent.click(rows()[3] as HTMLElement);
  const output = within(dialog).getByRole("region", { name: "Job output" });
  expect(output.querySelector("pre")?.textContent).toBe(
    " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs\n",
  );
  expect(output.textContent).toContain("git exited with code 1");

  emitJobEnded(
    fakeJob(WORKSPACE, { job_id: 8, label: "Commit", output: "<b>not bold</b>", outcome: { kind: "succeeded" } }),
  );
  fireEvent.keyDown(screen.getByRole("listbox", { name: "Jobs" }), { key: "Home" });
  expect(output.querySelector("pre")?.textContent).toBe("<b>not bold</b>");
  expect(output.querySelector("b")).toBeNull();

  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("UI-21 a failed job stays in the status bar until seen", async () => {
  await setup();
  vi.useFakeTimers();

  emitJobEnded(fakeJob(WORKSPACE, { job_id: 1, label: "Fetch", outcome: { kind: "succeeded" } }));
  expect(status().textContent).toBe("Fetch: Succeeded");
  vi.advanceTimersByTime(OUTCOME_SHOWN_MS);
  expect(status().textContent).toBe("No jobs running");

  emitJobEnded(fakeJob(WORKSPACE, { job_id: 2, label: "Push", outcome: { kind: "failed" } }));
  vi.advanceTimersByTime(OUTCOME_SHOWN_MS * 3);
  expect(status().textContent).toBe("Push: Failed");
  // A newer job's outcome shows for its time, then the failure again.
  emitJobEnded(fakeJob(WORKSPACE, { job_id: 3, label: "Fetch", outcome: { kind: "nothing_to_do" } }));
  expect(status().textContent).toBe("Fetch: Nothing to do");
  vi.advanceTimersByTime(OUTCOME_SHOWN_MS);
  expect(status().textContent).toBe("Push: Failed");

  openList();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(status().textContent).toBe("No jobs running");
});

test("UI-21 cancel sends job_cancel", async () => {
  await setup(jobsScenario(WORKSPACE));
  openList();
  await screen.findByRole("dialog", { name: "Jobs" });
  const output = screen.getByRole("region", { name: "Job output" });

  // The newest job is selected first: the queued one.
  fireEvent.click(within(output).getByRole("button", { name: "Cancel" }));
  await settle();
  expect(fakeState.cancels).toEqual([7]);
  expect(output.textContent).toContain("Skipped (cancelled)");
  expect(within(output).queryByRole("button", { name: "Cancel" })).toBeNull();

  // A running job that cannot be stopped runs to its end (GRP-10).
  fireEvent.keyDown(screen.getByRole("listbox", { name: "Jobs" }), { key: "ArrowDown" });
  fireEvent.click(within(output).getByRole("button", { name: "Cancel" }));
  await settle();
  expect(fakeState.cancels).toEqual([7, 6]);
  expect(output.textContent).toContain("it runs to its end");

  // An ended job has no Cancel.
  fireEvent.keyDown(screen.getByRole("listbox", { name: "Jobs" }), { key: "End" });
  expect(within(output).queryByRole("button", { name: "Cancel" })).toBeNull();
});
