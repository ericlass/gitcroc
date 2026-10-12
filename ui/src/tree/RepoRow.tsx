// What a repo row shows (UI-3), from the core's summary. Repo text goes in
// as text nodes only, never as HTML (SEC-3).
import { Show } from "solid-js";
import type { SummaryView } from "../ipc";

/** Ahead and behind as `↑1 ↓2`; zero counts are left out. */
export function AheadBehind(props: { ahead: number | null; behind: number | null }) {
  return (
    <>
      <Show when={props.ahead}>
        {(ahead) => (
          <span class="tree-count" aria-label={`${ahead()} ahead`}>
            ↑{ahead()}
          </span>
        )}
      </Show>
      <Show when={props.behind}>
        {(behind) => (
          <span class="tree-count" aria-label={`${behind()} behind`}>
            ↓{behind()}
          </span>
        )}
      </Show>
    </>
  );
}

export default function RepoRow(props: { name: string; view: SummaryView | undefined }) {
  const summary = () => props.view?.summary ?? null;
  const head = () => {
    const current = summary()?.head;
    if (!current) {
      return null;
    }
    return current.kind === "branch" ? current.name : current.hash.slice(0, 7);
  };
  const upstream = () => summary()?.upstream ?? null;

  return (
    <span class="tree-repo" classList={{ stale: props.view?.stale ?? true }} data-stale={props.view?.stale ?? true}>
      <span class="tree-label">{props.name}</span>
      <Show when={head()}>
        {(head) => (
          <span class="tree-head" classList={{ detached: summary()?.head.kind === "detached" }}>
            {head()}
          </span>
        )}
      </Show>
      <Show when={upstream()}>{(up) => <AheadBehind ahead={up().ahead} behind={up().behind} />}</Show>
      <Show when={summary()?.dirty}>
        <span class="tree-dirty" role="img" aria-label="Uncommitted changes" title="Uncommitted changes">
          ●
        </span>
      </Show>
      <Show when={props.view?.conflict}>
        <span class="tree-conflict" role="img" aria-label="Conflict state" title="Conflict state">
          ⚔
        </span>
      </Show>
      <Show when={props.view?.missing}>
        <span class="tree-warning" role="img" aria-label="Folder is missing" title="Folder is missing">
          ⚠
        </span>
      </Show>
      <Show when={props.view?.error}>
        {(error) => (
          <span class="tree-warning" role="img" aria-label={`Cannot read: ${error()}`} title={error()}>
            ⚠
          </span>
        )}
      </Show>
      <Show when={props.view?.refreshing}>
        <span class="spinner" role="img" aria-label="Refreshing" title="Refreshing" />
      </Show>
    </span>
  );
}
