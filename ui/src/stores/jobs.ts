// Progress of running jobs as reported by the core (ARCH-7), keyed by
// workspace id (ARCH-9). The store only mirrors events; it decides nothing.
import { createStore } from "solid-js/store";
import { core, type JobId, type JobProgress, type WorkspaceId } from "../ipc";

export type JobsByWorkspace = Record<WorkspaceId, Record<JobId, JobProgress>>;

export function createJobStore() {
  const [jobs, setJobs] = createStore<JobsByWorkspace>({});

  const record = (progress: JobProgress) => {
    setJobs(progress.workspace_id, (byId) => ({ ...byId, [progress.job_id]: progress }));
  };

  /** Starts recording `job_progress` events; resolves to the unsubscribe function. */
  const subscribe = () => core.events.jobProgress.listen((event) => record(event.payload));

  return { jobs, record, subscribe };
}

export type JobStore = ReturnType<typeof createJobStore>;
