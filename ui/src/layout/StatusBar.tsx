// The status bar of UI-21: running jobs of this workspace with progress. The
// job list it opens arrives with the job engine.
import { For, type JSX, Show } from "solid-js";
import type { WorkspaceId } from "../ipc";
import type { JobStore } from "../stores/jobs";

export default function StatusBar(props: { workspace: WorkspaceId; jobs: JobStore; children?: JSX.Element }) {
  // A total of 0 means the core does not know the number of steps yet.
  const running = () =>
    Object.values(props.jobs.jobs[props.workspace] ?? {}).filter((job) => job.total === 0 || job.done < job.total);

  return (
    <footer class="status-bar">
      <div role="status" class="status-jobs">
        <Show when={running().length > 0} fallback="No jobs running">
          <For each={running()}>
            {(job) => (
              <span class="status-job">
                {job.label} {job.total === 0 ? job.done : `${job.done}/${job.total}`}
              </span>
            )}
          </For>
        </Show>
      </div>
      {props.children}
    </footer>
  );
}
