// The job list of UI-21: every job of this run across all workspaces,
// newest first, virtualized (PERF-1). The selected job shows git's raw
// output as text (SEC-3) and, while it has not ended, Cancel (GRP-10); a
// group job shows its sub-jobs while it runs and its result report when it
// ended (section 5). Opening it marks every ended job seen; Escape closes it.
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import type { CancelResult, JobId, RepoId, WorkspaceId } from "../ipc";
import type { JobStore } from "../stores/jobs";
import AiFileList from "../conflict/AiFileList";
import { Modal } from "../dialogs/parts";
import { GroupReport, SubJobList } from "./GroupReport";
import { formatDuration, formatStart, JobStateLabel } from "./outcome";
import { strings } from "../strings/en";

/** Height of one row in CSS pixels; `.job-row` in App.css sets it. */
export const ROW_HEIGHT = 24;

/** Rows rendered beyond each edge of the visible ones. */
export const OVERSCAN = 8;

/** What a cancel did, as the user reads it. */
const CANCEL_TEXT: Record<CancelResult, string | null> = {
  cancelled: null,
  killing: strings.jobs.stopping,
  finishing: strings.jobs.cannotStop,
  ended: null,
};

export default function JobList(props: {
  jobs: JobStore;
  /** The job selected when it opens; the newest without. */
  initial?: JobId;
  onClose: () => void;
  /** Selects a repo of a report in its workspace's tree. */
  onSelectRepo: (workspace: WorkspaceId, repo: RepoId) => void;
  /** Opens a file of a Resolve all with AI run in its workspace's conflict editor (AI-13). */
  onOpenFile?: (workspace: WorkspaceId, repo: RepoId, path: string) => void;
  /** Retry failed of a group job (GRP-11). */
  onRetry: (workspace: WorkspaceId, job: JobId) => void;
}) {
  let scroller: HTMLDivElement | undefined;
  const [selected, setSelected] = createSignal<JobId | null>(props.initial ?? props.jobs.jobs()[0]?.job_id ?? null);
  const [cancelled, setCancelled] = createSignal<Record<JobId, string>>({});

  const rows = () => props.jobs.jobs();
  const selectedIndex = () => rows().findIndex((job) => job.job_id === selected());
  const job = () => {
    const id = selected();
    return id === null ? undefined : props.jobs.state.byId[id];
  };

  const virtualizer = createVirtualizer({
    get count() {
      return rows().length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });

  onMount(() => {
    props.jobs.markSeen();
    if (selectedIndex() > 0) {
      virtualizer.scrollToIndex(selectedIndex(), { align: "auto" });
    }
    scroller?.focus();
  });

  const select = (index: number) => {
    const row = rows()[index];
    if (row !== undefined) {
      setSelected(row.job_id);
      virtualizer.scrollToIndex(index, { align: "auto" });
    }
  };

  const onListKeyDown = (event: KeyboardEvent) => {
    const index = selectedIndex();
    switch (event.key) {
      case "ArrowDown":
        select(Math.min(index + 1, rows().length - 1));
        break;
      case "ArrowUp":
        select(Math.max(index - 1, 0));
        break;
      case "Home":
        select(0);
        break;
      case "End":
        select(rows().length - 1);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  const cancel = async (id: JobId) => {
    let text: string | null;
    try {
      text = CANCEL_TEXT[await props.jobs.cancel(id)];
    } catch (error) {
      text = strings.jobs.cannotCancel(error instanceof Error ? error.message : String(error));
    }
    setCancelled((current) => ({ ...current, [id]: text ?? "" }));
  };

  return (
    <Modal labelledby="jobs-title" class="dialog job-list" onEscape={props.onClose} onBackdrop={props.onClose}>
      <div class="job-list-header">
        <h2 id="jobs-title">{strings.jobs.title}</h2>
        <button type="button" onClick={() => props.onClose()}>
          {strings.common.close}
        </button>
      </div>
      <Show when={props.jobs.state.error}>
        {(error) => <p role="alert">{strings.jobs.cannotRead(error())}</p>}
      </Show>
      <div class="job-columns" aria-hidden="true">
        <span>{strings.jobs.columns.job}</span>
        <span>{strings.jobs.columns.workspace}</span>
        <span>{strings.jobs.columns.repo}</span>
        <span>{strings.jobs.columns.state}</span>
        <span>{strings.jobs.columns.started}</span>
        <span>{strings.jobs.columns.duration}</span>
      </div>
      <div
        class="job-scroller"
        ref={(element) => (scroller = element)}
        role="listbox"
        aria-label={strings.jobs.title}
        tabindex="0"
        aria-activedescendant={selectedIndex() >= 0 ? `job-row-${selectedIndex()}` : undefined}
        onKeyDown={onListKeyDown}
      >
        <Show when={rows().length > 0} fallback={<p class="placeholder">{strings.jobs.noJobs}</p>}>
          <div class="job-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
            <For each={virtualizer.getVirtualItems()}>
              {(item) => (
                <Show when={rows()[item.index]}>
                  {(row) => (
                    <div
                      id={`job-row-${item.index}`}
                      class="job-row"
                      classList={{ selected: row().job_id === selected() }}
                      role="option"
                      aria-selected={row().job_id === selected()}
                      data-index={item.index}
                      style={{ transform: `translateY(${item.start}px)` }}
                      onClick={() => select(item.index)}
                    >
                      <span class="job-label">{row().label}</span>
                      <span>{row().workspace_name}</span>
                      <span>{row().repo_names.join(", ")}</span>
                      <JobStateLabel job={row()} />
                      <span>{formatStart(row().started_at)}</span>
                      <span>{formatDuration(row().duration_ms)}</span>
                    </div>
                  )}
                </Show>
              )}
            </For>
          </div>
        </Show>
      </div>
      <section class="job-detail" aria-label={strings.jobs.output}>
        <Show when={job()} fallback={<p class="placeholder">{strings.jobs.selectJob}</p>}>
          {(shown) => (
            <>
              <div class="job-detail-header">
                <span class="job-label">{shown().label}</span>
                <JobStateLabel job={shown()} />
                <Show when={shown().state !== "ended"}>
                  <button type="button" onClick={() => void cancel(shown().job_id)}>
                    {strings.common.cancel}
                  </button>
                </Show>
              </div>
              <Show when={cancelled()[shown().job_id]}>{(text) => <p aria-live="polite">{text()}</p>}</Show>
              <Show when={shown().message}>{(message) => <p class="job-message">{message()}</p>}</Show>
              <Switch
                fallback={
                  <Show when={shown().output} fallback={<p class="placeholder">{strings.jobs.noOutput}</p>}>
                    <pre class="job-output" tabindex="0">
                      {shown().output}
                    </pre>
                  </Show>
                }
              >
                <Match when={shown().report}>
                  {(report) => (
                    <GroupReport
                      job={shown()}
                      report={report()}
                      onSelectRepo={(repo) => props.onSelectRepo(shown().workspace_id, repo)}
                      onRetry={() => props.onRetry(shown().workspace_id, shown().job_id)}
                    />
                  )}
                </Match>
                <Match when={shown().sub_jobs.length > 0}>
                  <SubJobList job={shown()} />
                </Match>
                <Match when={shown().ai_files.length > 0}>
                  <AiFileList
                    job={shown()}
                    onOpen={
                      props.onOpenFile &&
                      ((path) => props.onOpenFile?.(shown().workspace_id, shown().repos[0] ?? "", path))
                    }
                  />
                  <Show when={shown().output}>
                    <pre class="job-output" tabindex="0">
                      {shown().output}
                    </pre>
                  </Show>
                </Match>
              </Switch>
            </>
          )}
        </Show>
      </section>
    </Modal>
  );
}
