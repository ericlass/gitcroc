// How a job's outcome (GRP-7), progress, start time and duration read in
// the status bar and the job list (UI-21). Colors and icons are theme
// tokens (UI-23): `.outcome-<kind>` in App.css.
import { Match, Show, Switch } from "solid-js";
import type { JobView, Outcome, SkipReason } from "../ipc";

export const OUTCOME_TEXT: Record<Outcome["kind"], string> = {
  succeeded: "Succeeded",
  nothing_to_do: "Nothing to do",
  skipped: "Skipped",
  failed: "Failed",
  conflict: "Conflict",
};

/** Why a job or a repo was skipped, as the parentheses after "Skipped" say. */
function skipText(reason: SkipReason): string {
  return reason.kind === "cancelled" ? "cancelled" : reason.reason;
}

export function outcomeText(outcome: Outcome): string {
  return outcome.kind === "skipped"
    ? `${OUTCOME_TEXT.skipped} (${skipText(outcome.reason)})`
    : OUTCOME_TEXT[outcome.kind];
}

/** The outcome's icon and text, in its color. */
export function OutcomeLabel(props: { outcome: Outcome }) {
  return (
    <span class={`outcome outcome-${props.outcome.kind.replaceAll("_", "-")}`}>
      <span class="outcome-icon" aria-hidden="true" />
      {outcomeText(props.outcome)}
    </span>
  );
}

/**
 * Running progress: the phase git names, such as `Receiving objects`, then
 * `done/total`, or a spinner while the total is not known. A group job
 * counts its repos, Resolve all with AI its files.
 */
export function JobProgressLabel(props: { job: JobView }) {
  return (
    <span class="job-running">
      <Show when={props.job.step}>{(step) => <span class="job-step">{step()} </span>}</Show>
      <Switch>
        <Match when={props.job.total === 0}>
          <span class="spinner" role="img" aria-label="Running" />
        </Match>
        <Match when={true}>
          <span class="job-progress">
            {props.job.done}/{props.job.total}
            {props.job.sub_jobs.length > 0 ? " repos" : props.job.ai_files.length > 0 ? " files" : ""}
          </span>
        </Match>
      </Switch>
    </span>
  );
}

/** Where a job is: its outcome, its progress, or what it waits for. */
export function JobStateLabel(props: { job: JobView }) {
  return (
    <Switch>
      <Match when={props.job.outcome}>{(outcome) => <OutcomeLabel outcome={outcome()} />}</Match>
      <Match when={props.job.state === "queued"}>
        <span class="job-waiting">waiting for {props.job.repo_names.join(", ")}</span>
      </Match>
      <Match when={true}>
        <JobProgressLabel job={props.job} />
      </Match>
    </Switch>
  );
}

/** A start time as the local time of day. */
export function formatStart(epochMs: number | null): string {
  return epochMs === null ? "" : new Date(epochMs).toLocaleTimeString();
}

/** A duration as `850 ms`, `4.2 s` or `3 min 5 s`. */
export function formatDuration(ms: number | null): string {
  if (ms === null) {
    return "";
  }
  if (ms < 1000) {
    return `${ms} ms`;
  }
  if (ms < 60_000) {
    return `${(ms / 1000).toFixed(1)} s`;
  }
  const seconds = Math.round(ms / 1000);
  return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
}
