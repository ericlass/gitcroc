// This run's jobs as the core reports them (ARCH-7, UI-21): read with
// `jobs_get` at start, then kept current by `job_progress` and `job_ended`.
// The store decides nothing about a job (ARCH-6). Besides the mirror it
// keeps only what the user saw: the job that ended last in each workspace
// (ARCH-9), which ended jobs the job list has shown, and which the status
// bar has shown in their own tab. Tab marks only count (UI-32).
import { createMemo } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { core, type JobId, type JobView, jobCancel, jobsGet, type WorkspaceId } from "../ipc";

interface JobsState {
  byId: Record<JobId, JobView>;
  /** The job whose `job_ended` arrived last, by workspace. */
  lastEnded: Record<WorkspaceId, JobId>;
  /** Ended jobs the job list has shown. */
  seen: Record<JobId, true>;
  /** Ended jobs the status bar has shown, in their workspace's tab. */
  shown: Record<JobId, true>;
  /** Why `jobs_get` failed, if it did. */
  error: string | null;
}

/** `onEnded` hears each `job_ended`, after the store has it. */
export function createJobStore(onEnded: (view: JobView) => void = () => {}) {
  const [state, setState] = createStore<JobsState>({
    byId: {},
    lastEnded: {},
    seen: {},
    shown: {},
    error: null,
  });

  const record = (view: JobView) => setState("byId", view.job_id, reconcile(view));

  const ended = (view: JobView) => {
    record(view);
    setState("lastEnded", view.workspace_id, view.job_id);
    onEnded(view);
  };

  /** Every job, newest first: ids grow with each job the core starts. */
  const jobs = createMemo(() => Object.values(state.byId).sort((a, b) => b.job_id - a.job_id));

  /**
   * Reads this run's jobs. An event that arrived meanwhile is as new as
   * the list or newer, so a job already known is kept.
   */
  const load = async () => {
    try {
      const views = await jobsGet();
      setState("error", null);
      for (const view of views) {
        if (state.byId[view.job_id] === undefined) {
          record(view);
        }
      }
    } catch (error) {
      setState("error", error instanceof Error ? error.message : String(error));
    }
  };

  /** Starts mirroring the job events and reads the jobs; resolves to the unsubscribe function. */
  const subscribe = async () => {
    const stops = await Promise.all([
      core.events.jobProgress.listen((event) => record(event.payload)),
      core.events.jobEnded.listen((event) => ended(event.payload)),
    ]);
    void load();
    return () => {
      for (const stop of stops) {
        stop();
      }
    };
  };

  /** The job list was opened: every job that has ended is seen. */
  const markSeen = () => {
    for (const job of jobs()) {
      if (job.state === "ended" && !state.seen[job.job_id]) {
        setState("seen", job.job_id, true);
      }
    }
  };

  /** The status bar of the job's own tab shows its report. */
  const markShown = (jobId: JobId) => {
    if (!state.shown[jobId]) {
      setState("shown", jobId, true);
    }
  };

  /**
   * What workspace `id`'s tab shows (UI-32): how many of its jobs run or
   * wait, and whether one ended failed or in conflict and the user has not
   * seen it, in the job list or in the status bar of that tab.
   */
  const marks = (id: WorkspaceId) => {
    const mine = jobs().filter((job) => job.workspace_id === id);
    return {
      active: mine.filter((job) => job.state !== "ended").length,
      unseen: mine.some(
        (job) =>
          job.state === "ended" &&
          (job.outcome?.kind === "failed" || job.outcome?.kind === "conflict") &&
          !state.seen[job.job_id] &&
          !state.shown[job.job_id],
      ),
    };
  };

  /** Asks the core to cancel a job (GRP-10); its end arrives as `job_ended`. */
  const cancel = (jobId: JobId) => jobCancel(jobId);

  return { state, jobs, record, subscribe, load, markSeen, markShown, marks, cancel };
}

export type JobStore = ReturnType<typeof createJobStore>;
