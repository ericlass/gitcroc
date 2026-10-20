// Settings' Updates section (D12, SEC-7): the automatic check, on by default and saved with the settings; Check
// now, which also runs while automatic checks are off; and the last result with its time. A failed check shows
// here only, never as an error elsewhere.
import { Show } from "solid-js";
import type { UpdateView } from "../ipc";
import type { SettingsStore } from "../stores/settings";
import type { UpdateStore } from "../stores/update";
import { strings } from "../strings/en";

/** A check's time as the local date and time. */
export const formatChecked = (at: number) => new Date(at).toLocaleString();

/** Where the updater is, as the section says it: the last result with its time. */
export function statusText(view: UpdateView): string {
  const status = view.status;
  switch (status.kind) {
    case "idle":
      return strings.settings.neverChecked;
    case "checking":
      return strings.settings.checking;
    case "up_to_date":
      return strings.settings.upToDate(view.current_version, formatChecked(status.checked_at));
    case "available":
      return strings.settings.updateAvailable(status.version, formatChecked(status.checked_at));
    case "failed":
      return strings.settings.updateFailed(status.reason, formatChecked(status.checked_at));
    case "downloading":
      return strings.settings.downloading(status.version);
    case "ready_to_restart":
      return strings.settings.readyToRestart(status.version);
  }
}

export default function UpdateSection(props: { update: UpdateStore; settings: SettingsStore; readOnly: boolean }) {
  const view = () => props.update.state.view;
  const busy = () => {
    const kind = view()?.status.kind;
    return kind === "checking" || kind === "downloading" || kind === "ready_to_restart";
  };

  return (
    <fieldset class="update-settings">
      <legend>{strings.settings.updatesSection}</legend>
      <label class="choice">
        <input
          type="checkbox"
          disabled={props.readOnly}
          checked={props.settings.updateCheck()}
          onChange={(event) => void props.settings.setUpdateCheck(event.currentTarget.checked)}
        />
        {strings.settings.checkAutomatically}
      </label>
      <Show when={view()}>
        {(current) => <p class="about">{current().checks ? statusText(current()) : strings.settings.noChecks}</p>}
      </Show>
      <Show when={props.update.state.error}>{(error) => <p role="alert">{error()}</p>}</Show>
      <div class="setting-actions">
        <button
          type="button"
          disabled={view() === null || view()?.checks === false || busy()}
          onClick={() => void props.update.checkNow()}
        >
          {strings.settings.checkNow}
        </button>
      </div>
    </fieldset>
  );
}
