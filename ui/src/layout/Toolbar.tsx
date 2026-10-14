// The toolbar of UI-20. Buttons that cannot apply are disabled, not hidden.
import { For } from "solid-js";

const LATER = ["Merge", "Stash"];

export default function Toolbar(props: {
  /** Opens the create branch dialog for the selected repo; `null` when no repo can take it. */
  onBranch: (() => void) | null;
  onSettings: () => void;
}) {
  return (
    <div role="toolbar" aria-label="Main" class="toolbar">
      <For each={["Fetch", "Pull", "Push"]}>
        {(name) => (
          <button type="button" disabled>
            {name}
          </button>
        )}
      </For>
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
