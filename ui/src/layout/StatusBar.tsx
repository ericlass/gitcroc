// The status bar of UI-21: the running jobs of this workspace with their
// progress; after the last one ends, its outcome for a while, and a failed
// or conflicted job until the job list has shown it. Clicking it opens the
// job list. It is no live region, so progress is not read out on every step;
// the window's live region says when a job failed (M5-10).
import { createEffect, createSignal, For, type JSX, Match, on, onCleanup, Show, Switch } from "solid-js";
import type { JobId, WorkspaceId } from "../ipc";
import { JobProgressLabel, OutcomeLabel } from "../jobs/outcome";
import type { JobStore } from "../stores/jobs";
import { strings } from "../strings/en";

/** How long the outcome of the job that ended last stays, in ms. */
export const OUTCOME_SHOWN_MS = 10_000;

export default function StatusBar(props: {
  workspace: WorkspaceId;
  jobs: JobStore;
  onOpenJobs: () => void;
  children?: JSX.Element;
}) {
  const mine = () => props.jobs.jobs().filter((job) => job.workspace_id === props.workspace);
  const running = () => mine().filter((job) => job.state === "running");

  // The job that ended last, for OUTCOME_SHOWN_MS after its end.
  const [recent, setRecent] = createSignal<JobId | null>(null);
  createEffect(
    on(
      () => props.jobs.state.lastEnded[props.workspace],
      (id) => {
        if (id === undefined) {
          return;
        }
        setRecent(id);
        const timer = setTimeout(() => setRecent(null), OUTCOME_SHOWN_MS);
        onCleanup(() => clearTimeout(timer));
      },
    ),
  );

  /** The ended job to show: the recent one, else the newest failure not yet seen. */
  const ended = () => {
    const id = recent();
    const last = id === null ? undefined : props.jobs.state.byId[id];
    return (
      last ??
      mine().find(
        (job) =>
          (job.outcome?.kind === "failed" || job.outcome?.kind === "conflict") && !props.jobs.state.seen[job.job_id],
      )
    );
  };

  // Shown here, in its own tab, the report is seen for the tab's marker (UI-32).
  createEffect(() => {
    const job = running().length === 0 ? ended() : undefined;
    if (job !== undefined) {
      props.jobs.markShown(job.job_id);
    }
  });

  return (
    <footer class="status-bar">
      <button
        type="button"
        class="status-jobs"
        aria-haspopup="dialog"
        title={strings.statusBar.showJobs}
        onClick={() => props.onOpenJobs()}
      >
        <Switch fallback={strings.statusBar.noJobs}>
          <Match when={running().length > 0}>
            <For each={running()}>
              {(job) => (
                <span class="status-job">
                  {job.label} <JobProgressLabel job={job} />
                </span>
              )}
            </For>
          </Match>
          <Match when={ended()}>
            {(job) => (
              <span class="status-job">
                {job().label}: <Show when={job().outcome}>{(outcome) => <OutcomeLabel outcome={outcome()} />}</Show>
              </span>
            )}
          </Match>
        </Switch>
      </button>
      {props.children}
    </footer>
  );
}
