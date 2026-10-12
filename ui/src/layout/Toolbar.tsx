// The toolbar of UI-20. Buttons that cannot apply yet are disabled, not hidden.
import { For } from "solid-js";

const OPERATIONS = ["Fetch", "Pull", "Push", "Branch", "Merge", "Stash"];

export default function Toolbar(props: { onSettings: () => void }) {
  return (
    <div role="toolbar" aria-label="Main" class="toolbar">
      <For each={OPERATIONS}>
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
