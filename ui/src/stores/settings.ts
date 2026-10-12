// The settings as the core reports them (DATA-1). The store only mirrors
// `settings_get` and the results of changes; the core decides and saves.
import { createStore } from "solid-js/store";
import { settingsGet, settingsSetTheme, type SettingsView, type Theme } from "../ipc";

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
    load: () => track(settingsGet()),
    setTheme: (theme: Theme) => track(settingsSetTheme(theme)),
  };
}

export type SettingsStore = ReturnType<typeof createSettingsStore>;
