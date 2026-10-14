// The toolbar of UI-20. Buttons that cannot apply are disabled, not hidden.
// Fetch, Pull and Push act on the selected repo: a click runs with the
// defaults, the arrow beside Pull and Push opens their options.
import { For } from "solid-js";

const LATER = ["Merge", "Stash"];

/** What a button does; `null` disables it. */
type Action = (() => void) | null;

export interface SyncActions {
  fetch: Action;
  pull: Action;
  pullOptions: Action;
  push: Action;
  pushOptions: Action;
}

/** A button with an arrow beside it that opens its options. */
function SplitButton(props: { name: string; title: string; run: Action; options: Action }) {
  return (
    <span class="split-button">
      <button type="button" disabled={props.run === null} title={props.title} onClick={() => props.run?.()}>
        {props.name}
      </button>
      <button
        type="button"
        class="split-arrow"
        disabled={props.options === null}
        aria-label={`${props.name} options`}
        title={`${props.name} with options`}
        onClick={() => props.options?.()}
      >
        <span aria-hidden="true">▾</span>
      </button>
    </span>
  );
}

export default function Toolbar(props: {
  sync: SyncActions;
  /** Opens the create branch dialog for the selected repo; `null` when no repo can take it. */
  onBranch: Action;
  onSettings: () => void;
}) {
  return (
    <div role="toolbar" aria-label="Main" class="toolbar">
      <button
        type="button"
        disabled={props.sync.fetch === null}
        title="Fetch all remotes of the selected repository"
        onClick={() => props.sync.fetch?.()}
      >
        Fetch
      </button>
      <SplitButton
        name="Pull"
        title="Update the current branch from its upstream"
        run={props.sync.pull}
        options={props.sync.pullOptions}
      />
      <SplitButton
        name="Push"
        title="Push the current branch to its upstream"
        run={props.sync.push}
        options={props.sync.pushOptions}
      />
      <button
        type="button"
        disabled={props.onBranch === null}
        title="Create a branch in the selected repository"
        onClick={() => props.onBranch?.()}
      >
        Branch
      </button>
      <For each={LATER}>
        {(name) => (
          <button type="button" disabled>
            {name}
          </button>
        )}
      </For>
      <span class="toolbar-gap" />
      <button type="button" disabled>
        Command palette
      </button>
      <button type="button" onClick={() => props.onSettings()}>
        Settings
      </button>
    </div>
  );
}
