// Development builds only: starts the core's debug job and shows the progress
// events it sends, to check the event path by hand.
import { For } from "solid-js";
import { debugEmitProgress } from "./ipc";
import type { JobStore } from "./stores/jobs";

const WORKSPACE = "debug";

export default function DebugJobs(props: { jobs: JobStore }) {
  const progress = () => Object.values(props.jobs.jobs[WORKSPACE] ?? {});

  return (
    <section>
      <button type="button" onClick={() => void debugEmitProgress(WORKSPACE)}>
        Run debug job
      </button>
      <ul>
        <For each={progress()}>
          {(job) => (
            <li>
              Job {job.job_id}: {job.label} {job.done}/{job.total}
            </li>
          )}
        </For>
      </ul>
    </section>
  );
}
