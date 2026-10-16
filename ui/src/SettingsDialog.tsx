// The Settings dialog: the theme (UI-23), code themes (UI-24), fonts
// (UI-26), group operations (section 5), AI (section 7), and which gitcroc
// and git are running.
import { For, onMount, Show } from "solid-js";
import AiSection from "./ai/AiSection";
import type { Appearance, AppInfo, CodeTheme, GitStatus, Theme } from "./ipc";
import type { AiStore } from "./stores/ai";
import { FONT_SIZE_LIMITS, PARALLELISM_LIMITS, type SettingsStore } from "./stores/settings";
import { CODE_THEME_NAMES, CODE_THEMES } from "./theme/codeThemes";
import CodeThemePreview from "./theme/CodeThemePreview";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Follow the system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

type CodeThemeKey = "code_theme_light" | "code_theme_dark";
type FontKey = "ui_font" | "code_font";
type FontSizeKey = "ui_font_size" | "code_font_size";

const CODE_THEME_CHOICES: { key: CodeThemeKey; label: string }[] = [
  { key: "code_theme_light", label: "Code theme in light mode" },
  { key: "code_theme_dark", label: "Code theme in dark mode" },
];

const FONTS: { font: FontKey; size: FontSizeKey; label: string; placeholder: string }[] = [
  { font: "ui_font", size: "ui_font_size", label: "UI font", placeholder: "System UI font" },
  { font: "code_font", size: "code_font_size", label: "Code font", placeholder: "System monospace font" },
];

export default function SettingsDialog(props: {
  settings: SettingsStore;
  ai: AiStore;
  info: AppInfo | undefined;
  git: GitStatus | null;
  onClose: () => void;
}) {
  let dialog: HTMLDivElement | undefined;
  const readOnly = () => props.settings.state.view?.read_only ?? true;
  const appearance = (): Appearance => props.settings.appearance();

  // A size that is not a number puts the saved one back; the core limits the rest.
  const setSize = (key: FontSizeKey, input: HTMLInputElement) => {
    const size = Number.parseInt(input.value, 10);
    if (Number.isNaN(size)) {
      input.value = String(appearance()[key]);
      return;
    }
    void props.settings.setAppearance({ [key]: size });
  };

  // A value the core refuses, or one that is not a number, puts the saved one back.
  const setParallelism = async (input: HTMLInputElement) => {
    const parallelism = Number.parseInt(input.value, 10);
    if (!Number.isNaN(parallelism)) {
      await props.settings.setGroups({ group_parallelism: parallelism });
    }
    input.value = String(props.settings.groups().group_parallelism);
  };

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
        <fieldset disabled={readOnly()}>
          <legend>Code themes</legend>
          <For each={CODE_THEME_CHOICES}>
            {(choice) => (
              <div class="setting">
                <label>
                  {choice.label}{" "}
                  <select
                    value={appearance()[choice.key]}
                    onChange={(event) =>
                      void props.settings.setAppearance({ [choice.key]: event.currentTarget.value as CodeTheme })
                    }
                  >
                    <For each={CODE_THEME_NAMES}>
                      {(name) => <option value={name}>{CODE_THEMES[name].label}</option>}
                    </For>
                  </select>
                </label>
                <CodeThemePreview theme={appearance()[choice.key]} />
              </div>
            )}
          </For>
        </fieldset>
        <fieldset disabled={readOnly()}>
          <legend>Fonts</legend>
          <For each={FONTS}>
            {(font) => (
              <div class="setting setting-font">
                <label>
                  {font.label}{" "}
                  <input
                    type="text"
                    placeholder={font.placeholder}
                    value={appearance()[font.font] ?? ""}
                    onChange={(event) => void props.settings.setAppearance({ [font.font]: event.currentTarget.value })}
                  />
                </label>
                <label>
                  {font.label} size{" "}
                  <input
                    type="number"
                    min={FONT_SIZE_LIMITS.min}
                    max={FONT_SIZE_LIMITS.max}
                    value={appearance()[font.size]}
                    onChange={(event) => setSize(font.size, event.currentTarget)}
                  />{" "}
                  <span aria-hidden="true">px</span>
                </label>
              </div>
            )}
          </For>
        </fieldset>
        <fieldset disabled={readOnly()}>
          <legend>Group operations</legend>
          <div class="setting">
            <label>
              Repos at once{" "}
              <input
                type="number"
                min={PARALLELISM_LIMITS.min}
                max={PARALLELISM_LIMITS.max}
                value={props.settings.groups().group_parallelism}
                onChange={(event) => void setParallelism(event.currentTarget)}
              />
            </label>
          </div>
          <label class="choice">
            <input
              type="checkbox"
              checked={props.settings.groups().always_show_preflight}
              onChange={(event) => void props.settings.setGroups({ always_show_preflight: event.currentTarget.checked })}
            />
            Always show the preflight dialog
          </label>
        </fieldset>
        <AiSection ai={props.ai} readOnly={readOnly()} />
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
