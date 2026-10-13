// Jobs for the fake core (fake.ts): one per outcome of GRP-7, a running job
// with progress and a queued one, from two workspaces. Test support only;
// the real jobs come from the core's job engine.
import type { JobView, Outcome } from "./bindings";

/** The second workspace whose jobs the job list shows (UI-21). */
export const OTHER_WORKSPACE = { id: "other-workspace", name: "Other workspace" };

/** Ten past noon on a fixed day, so start times read the same in every run. */
const NOON = Date.UTC(2026, 9, 13, 12, 10, 0);

/** A job of `workspace` with the given fields; ended and succeeded unless told otherwise. */
export function fakeJob(
  workspace: { id: string; name: string },
  job: Partial<JobView> & Pick<JobView, "job_id" | "label">,
): JobView {
  return {
    workspace_id: workspace.id,
    workspace_name: workspace.name,
    repos: ["r1"],
    repo_names: ["api"],
    state: "ended",
    done: 0,
    total: 0,
    started_at: NOON + job.job_id * 1000,
    duration_ms: 1200,
    outcome: { kind: "succeeded" },
    output: "",
    message: null,
    ...job,
  };
}

const ended = (outcome: Outcome) => ({ state: "ended" as const, outcome });

/** This run's jobs, newest first, as `jobs_get` returns them. */
export function jobsScenario(workspace: { id: string; name: string }): JobView[] {
  return [
    fakeJob(workspace, {
      job_id: 7,
      label: "Fetch",
      repos: ["r2"],
      repo_names: ["web"],
      state: "queued",
      outcome: null,
      started_at: null,
      duration_ms: null,
    }),
    fakeJob(workspace, {
      job_id: 6,
      label: "Fetch",
      state: "running",
      done: 3,
      total: 10,
      outcome: null,
      duration_ms: null,
    }),
    fakeJob(workspace, {
      job_id: 5,
      label: "Merge feature/login",
      repos: ["r3"],
      repo_names: ["login-service"],
      ...ended({ kind: "conflict" }),
      output: "Auto-merging src/login.ts\nCONFLICT (content): Merge conflict in src/login.ts\n",
    }),
    fakeJob(workspace, {
      job_id: 4,
      label: "Push",
      ...ended({ kind: "failed" }),
      output: " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs\n",
      message: "git exited with code 1",
    }),
    fakeJob(OTHER_WORKSPACE, {
      job_id: 3,
      label: "Pull",
      repos: ["o1"],
      repo_names: ["docs"],
      ...ended({ kind: "skipped", reason: "cancelled" }),
      message: "cancelled",
    }),
    fakeJob(workspace, {
      job_id: 2,
      label: "Fetch",
      repo_names: ["api"],
      ...ended({ kind: "nothing_to_do" }),
    }),
    fakeJob(workspace, {
      job_id: 1,
      label: "Writing the commit-graph file",
      ...ended({ kind: "succeeded" }),
      output: "Computing commit graph generation numbers: 100% (12/12), done.\n",
    }),
  ];
}
