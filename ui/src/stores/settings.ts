// The settings as the core reports them (DATA-1). The store only mirrors
// `settings_get` and the results of changes; the core decides and saves.
import { createStore } from "solid-js/store";
import {
  type Appearance,
  type ChangesSettings,
  settingsGet,
  settingsSetAppearance,
  settingsSetChanges,
  settingsSetTheme,
  type SettingsView,
  type Theme,
} from "../ipc";

/** The changes pane's view until the settings are read; the core's defaults. */
export const DEFAULT_CHANGES_SETTINGS: Readonly<ChangesSettings> = {
  diff_mode: "inline",
  file_list: "flat",
  ignore_whitespace: false,
  wrap_lines: false,
  context: "three",
};

/** Code themes and fonts until the settings are read; the core's defaults. */
export const DEFAULT_APPEARANCE: Readonly<Appearance> = {
  code_theme_light: "github_light",
  code_theme_dark: "github_dark",
  ui_font: null,
  ui_font_size: 13,
  code_font: null,
  code_font_size: 13,
};

/** The font size limits the core keeps to (UI-26). */
export const FONT_SIZE_LIMITS = { min: 8, max: 32 } as const;

/** The appearance part of the settings. */
function appearanceOf(settings: Appearance): Appearance {
  const { code_theme_light, code_theme_dark, ui_font, ui_font_size, code_font, code_font_size } = settings;
  return { code_theme_light, code_theme_dark, ui_font, ui_font_size, code_font, code_font_size };
}

export interface SettingsState {
  /** `null` until the first read. */
  view: SettingsView | null;
  /** Why the last read or change failed. */
  error: string | null;
}

export function createSettingsStore() {
  const [state, setState] = createStore<SettingsState>({ view: null, error: null });

  const track = async (request: Promise<SettingsView>) => {
    try {
      setState({ view: await request, error: null });
    } catch (error) {
      setState("error", error instanceof Error ? error.message : String(error));
    }
  };

  return {
    state,
    /** The theme to show: the saved one, or the default until it is read. */
    theme: (): Theme => state.view?.settings.theme ?? "system",
    /** The changes pane's view (UI-14, UI-17): the saved one, or the default until it is read. */
    changes: (): ChangesSettings => state.view?.settings.changes ?? DEFAULT_CHANGES_SETTINGS,
    /** Code themes and fonts (UI-24, UI-26): the saved ones, or the defaults until read. */
    appearance: (): Appearance => (state.view ? appearanceOf(state.view.settings) : DEFAULT_APPEARANCE),
    load: () => track(settingsGet()),
    setTheme: (theme: Theme) => track(settingsSetTheme(theme)),
    /** Changes some of the changes pane's settings; the core saves them. */
    setChanges: (change: Partial<ChangesSettings>) =>
      track(settingsSetChanges({ ...(state.view?.settings.changes ?? DEFAULT_CHANGES_SETTINGS), ...change })),
    /** Changes some of the code themes and fonts; the core limits the sizes and saves them. */
    setAppearance: (change: Partial<Appearance>) =>
      track(
        settingsSetAppearance({
          ...(state.view ? appearanceOf(state.view.settings) : DEFAULT_APPEARANCE),
          ...change,
        }),
      ),
  };
}

export type SettingsStore = ReturnType<typeof createSettingsStore>;
