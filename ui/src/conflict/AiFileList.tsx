// The files of a Resolve all with AI run and where each is (AI-13): waiting,
// running, done with its hunks by confidence, AI failed or not sent with the
// reason, or cancelled, as the core reports them in the job's view. Shown
// under the conflict banner and in the job list (UI-21). A file's name opens
// it in the conflict editor with its proposal when `onOpen` is given. Paths
// and reasons are text only (SEC-3).
import { For, Match, Show, Switch } from "solid-js";
import type { AiFileProgress, AiFileState, JobView } from "../ipc";
import { JobStateLabel } from "../jobs/outcome";

/** A done file's hunks, such as `2 high, 1 medium, 1 unresolved`. */
function doneText(state: Extract<AiFileState, { kind: "done" }>): string {
  const counts = [
    [state.high, "high"],
    [state.medium, "medium"],
    [state.low, "low"],
    [state.unresolved, "unresolved"],
  ] as const;
  const parts = counts.filter(([n]) => n > 0).map(([n, name]) => `${n} ${name}`);
  return parts.length === 0 ? "Done" : `Done: ${parts.join(", ")}`;
}

/** Where a file is, in words. */
export function fileStateText(state: AiFileState): string {
  switch (state.kind) {
    case "waiting":
      return "Waiting";
    case "running":
      return "Running";
    case "done":
      return doneText(state);
    case "failed":
      return `AI failed: ${state.reason}`;
    case "ineligible":
      return `Not sent: ${state.message}`;
    case "all_edited":
      return "Not sent: every conflict is already edited";
    case "cancelled":
      return "Cancelled";
  }
}

export default function AiFileList(props: { job: JobView; onOpen?: (path: string) => void }) {
  const state = (file: AiFileProgress) => (
    <Switch fallback={<span class={`ai-file-state ai-file-${file.state.kind}`}>{fileStateText(file.state)}</span>}>
      <Match when={file.state.kind === "running"}>
        <span class="spinner" role="img" aria-label="Running" />
      </Match>
    </Switch>
  );
  return (
    <>
      <p class="report-counts">
        <JobStateLabel job={props.job} />
      </p>
      <ul class="ai-files" aria-label="Files">
        <For each={props.job.ai_files}>
          {(file) => (
            <li class="ai-file">
              <Show when={props.onOpen} fallback={<span class="report-name">{file.path}</span>}>
                {(open) => (
                  <button type="button" class="report-name link" onClick={() => open()(file.path)}>
                    {file.path}
                  </button>
                )}
              </Show>
              {state(file)}
            </li>
          )}
        </For>
      </ul>
    </>
  );
}
