// A group job in the job list (UI-21): its sub-jobs with their state while
// it runs, and its result report when it ended (section 5, step 4). The
// report has one line per repo of the target set, in target-set order, with
// its outcome (GRP-7) and reason, and counts per outcome at the top. A line
// expands to git's output, as text (SEC-3); the repo's name selects it in
// its workspace's tree. Retry failed (GRP-11) is enabled only when a repo
// failed. The arrow keys move between the lines (UI-22).
import { createSignal, For, Match, Show, Switch } from "solid-js";
import type { JobView, Outcome, RepoId, RepoResult, SubJob } from "../ipc";
import { JobProgressLabel, OUTCOME_TEXT, OutcomeLabel } from "./outcome";

/** GRP-7's outcomes, in the order the counts name them. */
const OUTCOMES: Outcome["kind"][] = ["failed", "conflict", "succeeded", "nothing_to_do", "skipped"];

/** Counts per outcome, such as `2 failed, 3 succeeded`. */
export function outcomeCounts(report: RepoResult[]): string {
  return OUTCOMES.map((kind) => ({ kind, n: report.filter((line) => line.outcome.kind === kind).length }))
    .filter((count) => count.n > 0)
    .map((count) => `${count.n} ${OUTCOME_TEXT[count.kind].toLowerCase()}`)
    .join(", ");
}

/** Whether any repo of a report failed or ended in conflict: the report then opens by itself. */
export const needsLook = (report: RepoResult[]) =>
  report.some((line) => line.outcome.kind === "failed" || line.outcome.kind === "conflict");

export function SubJobList(props: { job: JobView }) {
  const state = (sub: SubJob) => (
    <Switch>
      <Match when={sub.outcome}>{(outcome) => <OutcomeLabel outcome={outcome()} />}</Match>
      <Match when={sub.state === "queued"}>
        <span class="job-waiting">Waiting</span>
      </Match>
      <Match when={true}>
        <span class="spinner" role="img" aria-label="Running" />
      </Match>
    </Switch>
  );
  return (
    <>
      <p class="report-counts">
        <JobProgressLabel job={props.job} />
      </p>
      <ul class="sub-jobs" aria-label="Repos">
        <For each={props.job.sub_jobs}>
          {(sub) => (
            <li class="sub-job">
              <span class="report-name">{sub.repo_name}</span>
              {state(sub)}
            </li>
          )}
        </For>
      </ul>
    </>
  );
}

export function GroupReport(props: {
  job: JobView;
  report: RepoResult[];
  onSelectRepo: (repo: RepoId) => void;
  onRetry: () => void;
}) {
  let list: HTMLUListElement | undefined;
  const [open, setOpen] = createSignal<Record<RepoId, boolean>>({});
  const failed = () => props.report.some((line) => line.outcome.kind === "failed");

  /** The arrow keys, Home and End move to the same control of another line. */
  const onKeyDown = (event: KeyboardEvent) => {
    const from = (event.target as HTMLElement).closest<HTMLElement>("[data-line]");
    if (from === null || list === undefined) {
      return;
    }
    const lines = [...list.querySelectorAll<HTMLElement>("[data-line]")];
    const at = lines.indexOf(from);
    const to = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: lines.length - 1 }[event.key];
    if (to === undefined) {
      return;
    }
    event.preventDefault();
    const role = (event.target as HTMLElement).dataset.control ?? "toggle";
    lines[Math.max(0, Math.min(to, lines.length - 1))]?.querySelector<HTMLElement>(`[data-control="${role}"]`)?.focus();
  };

  return (
    <div class="group-report">
      <div class="report-header">
        <p class="report-counts">{outcomeCounts(props.report)}</p>
        <button
          type="button"
          disabled={!failed()}
          title={failed() ? "Run the operation again on the failed repos" : "No repo failed"}
          onClick={() => props.onRetry()}
        >
          Retry failed
        </button>
      </div>
      <ul class="report-lines" aria-label="Result report" ref={(element) => (list = element)} onKeyDown={onKeyDown}>
        <For each={props.report}>
          {(line) => {
            const expanded = () => open()[line.repo_id] === true;
            const reason = () => line.message;
            return (
              <li class="report-line" data-line>
                <button
                  type="button"
                  class="report-toggle"
                  data-control="toggle"
                  aria-expanded={expanded()}
                  aria-label={`Git's output in ${line.repo_name}`}
                  onClick={() => setOpen((current) => ({ ...current, [line.repo_id]: !expanded() }))}
                >
                  <span class="report-chevron" aria-hidden="true">
                    {expanded() ? "▾" : "▸"}
                  </span>
                </button>
                <button
                  type="button"
                  class="report-name link"
                  data-control="name"
                  title="Select it in the tree"
                  onClick={() => props.onSelectRepo(line.repo_id)}
                >
                  {line.repo_name}
                </button>
                <OutcomeLabel outcome={line.outcome} />
                <Show when={line.outcome.kind !== "skipped" && reason()}>
                  {(text) => <span class="report-reason">{text()}</span>}
                </Show>
                <Show when={expanded()}>
                  <Show when={line.output} fallback={<p class="placeholder report-output">No output.</p>}>
                    <pre class="job-output report-output">{line.output}</pre>
                  </Show>
                </Show>
              </li>
            );
          }}
        </For>
      </ul>
    </div>
  );
}
