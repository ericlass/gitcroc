// Development builds only: starts the core's debug job, whose progress shows
// in the status bar, to check the event path by hand.
import { debugEmitProgress, type WorkspaceId } from "./ipc";

export default function DebugJobs(props: { workspace: WorkspaceId }) {
  return (
    <button type="button" onClick={() => void debugEmitProgress(props.workspace)}>
      Run debug job
    </button>
  );
}
