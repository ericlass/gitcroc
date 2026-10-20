// The Settings dialog: the theme (UI-23), code themes (UI-24), fonts
// (UI-26), group operations (section 5), AI (section 7), the git in use,
// which can be chosen by hand (PLAT-3), the update check (D12), and which
// gitcroc is running.
import { For, onMount, Show } from "solid-js";
import AiSection from "./ai/AiSection";
import { Modal } from "./dialogs/parts";
import type { Appearance, AppInfo, CodeTheme, Theme } from "./ipc";
import type { AiStore } from "./stores/ai";
import type { GitStore } from "./stores/git";
import type { UpdateStore } from "./stores/update";
import { FONT_SIZE_LIMITS, PARALLELISM_LIMITS, type SettingsStore } from "./stores/settings";
import { CODE_THEME_NAMES, CODE_THEMES } from "./theme/codeThemes";
import CodeThemePreview from "./theme/CodeThemePreview";
import UpdateSection from "./update/UpdateSection";
import { strings } from "./strings/en";

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: strings.settings.themes.system },
  { value: "light", label: strings.settings.themes.light },
  { value: "dark", label: strings.settings.themes.dark },
];

type CodeThemeKey = "code_theme_light" | "code_theme_dark";
type FontKey = "ui_font" | "code_font";
type FontSizeKey = "ui_font_size" | "code_font_size";

const CODE_THEME_CHOICES: { key: CodeThemeKey; label: string }[] = [
  { key: "code_theme_light", label: strings.settings.codeThemeLight },
  { key: "code_theme_dark", label: strings.settings.codeThemeDark },
];

const FONTS: { font: FontKey; size: FontSizeKey; label: string; placeholder: string }[] = [
  {
    font: "ui_font",
    size: "ui_font_size",
    label: strings.settings.uiFont,
    placeholder: strings.settings.uiFontPlaceholder,
  },
  {
    font: "code_font",
    size: "code_font_size",
    label: strings.settings.codeFont,
    placeholder: strings.settings.codeFontPlaceholder,
  },
];

export default function SettingsDialog(props: {
  settings: SettingsStore;
  ai: AiStore;
  info: AppInfo | undefined;
  git: GitStore;
  update: UpdateStore;
  /**
   * Where it opens: at its top, at the AI section, as Resolve with AI without a provider asks (AI-3), or at the
   * Updates section, as the palette's Check for updates does.
   */
  at?: "top" | "ai" | "updates";
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

  onMount(() => {
    const section =
      props.at === "ai" || props.at === "updates"
        ? dialog?.querySelector<HTMLElement>(props.at === "ai" ? ".ai-settings" : ".update-settings")
        : null;
    section?.scrollIntoView?.({ block: "start" });
    (section ?? dialog)?.querySelector<HTMLElement>("input:checked, input:enabled, button:enabled")?.focus();
  });

  return (
    <Modal
      labelledby="settings-title"
      class="dialog"
      ref={(element) => (dialog = element)}
      onEscape={props.onClose}
      onBackdrop={props.onClose}
    >
      <h2 id="settings-title">{strings.settings.title}</h2>
      <fieldset disabled={readOnly()}>
        <legend>{strings.settings.theme}</legend>
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
        <legend>{strings.settings.codeThemes}</legend>
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
        <legend>{strings.settings.fonts}</legend>
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
                {strings.settings.fontSize(font.label)}{" "}
                <input
                  type="number"
                  min={FONT_SIZE_LIMITS.min}
                  max={FONT_SIZE_LIMITS.max}
                  value={appearance()[font.size]}
                  onChange={(event) => setSize(font.size, event.currentTarget)}
                />{" "}
                <span aria-hidden="true">{strings.settings.pixels}</span>
              </label>
            </div>
          )}
        </For>
      </fieldset>
      <fieldset disabled={readOnly()}>
        <legend>{strings.settings.groupOperations}</legend>
        <div class="setting">
          <label>
            {strings.settings.reposAtOnce}{" "}
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
          {strings.settings.alwaysShowPreflight}
        </label>
      </fieldset>
      <fieldset disabled={readOnly()}>
        <legend>{strings.settings.gitSection}</legend>
        <Show when={props.git.state.result?.kind === "usable" && props.git.state.result}>
          {(git) => (
            <>
              <p class="about">{strings.settings.git(git().version, git().path)}</p>
              <Show when={git().chosen_missing && git().chosen}>
                {(chosen) => <p role="alert">{strings.settings.chosenMissing(chosen(), git().path)}</p>}
              </Show>
            </>
          )}
        </Show>
        <Show when={props.git.state.error}>{(error) => <p role="alert">{error()}</p>}</Show>
        <div class="setting-actions">
          <button type="button" disabled={props.git.state.checking} onClick={() => void props.git.choose()}>
            {strings.gitSetup.chooseGit}
          </button>
          <Show when={(props.git.state.result?.chosen ?? null) !== null}>
            <button type="button" disabled={props.git.state.checking} onClick={() => void props.git.useAutomatic()}>
              {strings.gitSetup.useAutomatic}
            </button>
          </Show>
        </div>
      </fieldset>
      <UpdateSection update={props.update} settings={props.settings} readOnly={readOnly()} />
      <AiSection ai={props.ai} readOnly={readOnly()} />
      <Show when={props.settings.state.error}>{(error) => <p role="alert">{error()}</p>}</Show>
      <Show when={props.info}>
        {(info) => (
          <p class="about">{strings.settings.about(info().name, info().version, info().os, info().arch)}</p>
        )}
      </Show>
      <div class="dialog-actions">
        <button type="button" onClick={() => props.onClose()}>
          {strings.common.close}
        </button>
      </div>
    </Modal>
  );
}
