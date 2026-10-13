// The settings as the core reports them (DATA-1). The store only mirrors
// `settings_get` and the results of changes; the core decides and saves.
import { createStore } from "solid-js/store";
import {
  type ChangesSettings,
  settingsGet,
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
    load: () => track(settingsGet()),
    setTheme: (theme: Theme) => track(settingsSetTheme(theme)),
    /** Changes some of the changes pane's settings; the core saves them. */
    setChanges: (change: Partial<ChangesSettings>) =>
      track(settingsSetChanges({ ...(state.view?.settings.changes ?? DEFAULT_CHANGES_SETTINGS), ...change })),
  };
}

export type SettingsStore = ReturnType<typeof createSettingsStore>;
