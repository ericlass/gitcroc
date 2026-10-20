// The settings as the core reports them (DATA-1). The store only mirrors
// `settings_get` and the results of changes; the core decides and saves.
import { createStore } from "solid-js/store";
import {
  type Appearance,
  type ChangesSettings,
  type GroupSettings,
  settingsGet,
  settingsSetAppearance,
  settingsSetChanges,
  settingsSetGroups,
  settingsSetTheme,
  settingsSetUpdateCheck,
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

/** The group operations' settings until the settings are read; the core's defaults. */
export const DEFAULT_GROUP_SETTINGS: Readonly<GroupSettings> = { group_parallelism: 8, always_show_preflight: false };

/** How many repos a group operation may run at once, as the core allows (section 5, step 3). */
export const PARALLELISM_LIMITS = { min: 1, max: 32 } as const;

/** The font size limits the core keeps to (UI-26). */
export const FONT_SIZE_LIMITS = { min: 8, max: 32 } as const;

/** The appearance part of the settings. */
function appearanceOf(settings: Appearance): Appearance {
  const { code_theme_light, code_theme_dark, ui_font, ui_font_size, code_font, code_font_size } = settings;
  return { code_theme_light, code_theme_dark, ui_font, ui_font_size, code_font, code_font_size };
}

/** The group operations' part of the settings. */
function groupsOf(settings: GroupSettings): GroupSettings {
  const { group_parallelism, always_show_preflight } = settings;
  return { group_parallelism, always_show_preflight };
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
    /** The group operations' settings: the saved ones, or the defaults until read. */
    groups: (): GroupSettings => (state.view ? groupsOf(state.view.settings) : DEFAULT_GROUP_SETTINGS),
    /** The app checks for updates by itself (D12): the saved choice, or on until read. */
    updateCheck: (): boolean => state.view?.update_check ?? true,
    load: () => track(settingsGet()),
    /** Turns the automatic update check on or off; the core saves it. */
    setUpdateCheck: (enabled: boolean) => track(settingsSetUpdateCheck(enabled)),
    setTheme: (theme: Theme) => track(settingsSetTheme(theme)),
    /** Changes some of the changes pane's settings; the core saves them. */
    setChanges: (change: Partial<ChangesSettings>) =>
      track(settingsSetChanges({ ...(state.view?.settings.changes ?? DEFAULT_CHANGES_SETTINGS), ...change })),
    /** Changes some of the group operations' settings; the core refuses a parallelism outside 1 to 32. */
    setGroups: (change: Partial<GroupSettings>) =>
      track(settingsSetGroups({ ...(state.view ? groupsOf(state.view.settings) : DEFAULT_GROUP_SETTINGS), ...change })),
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
