// The Settings dialog: the theme (UI-23), and which gitcroc and git are running.
import { For, onMount, Show } from "solid-js";
import type { AppInfo, GitStatus, Theme } from "./ipc";
import type { SettingsStore } from "./stores/settings";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Follow the system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export default function SettingsDialog(props: {
  settings: SettingsStore;
  info: AppInfo | undefined;
  git: GitStatus | null;
  onClose: () => void;
}) {
  let dialog: HTMLDivElement | undefined;
  const readOnly = () => props.settings.state.view?.read_only ?? true;

  onMount(() => dialog?.querySelector<HTMLElement>("input:checked, input, button")?.focus());

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      props.onClose();
    }
  };

  return (
    <div class="backdrop" onClick={(event) => event.target === event.currentTarget && props.onClose()}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        class="dialog"
        ref={(element) => (dialog = element)}
        onKeyDown={onKeyDown}
      >
        <h2 id="settings-title">Settings</h2>
        <fieldset disabled={readOnly()}>
          <legend>Theme</legend>
          <For each={THEMES}>
            {(theme) => (
              <label class="choice">
                <input
                  type="radio"
                  name="theme"
                  value={theme.value}
                  checked={props.settings.theme() === theme.value}
                  onChange={() => void props.settings.setTheme(theme.value)}
                />
                {theme.label}
              </label>
            )}
          </For>
        </fieldset>
        <Show when={props.settings.state.error}>{(error) => <p role="alert">{error()}</p>}</Show>
        <Show when={props.info}>
          {(info) => (
            <p class="about">
              {info().name} version {info().version} on {info().os} ({info().arch})
            </p>
          )}
        </Show>
        <Show when={props.git?.kind === "usable" && props.git}>
          {(git) => (
            <p class="about">
              git version {git().version} at {git().path}
            </p>
          )}
        </Show>
        <div class="dialog-actions">
          <button type="button" onClick={() => props.onClose()}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
