import { createResource, createSignal, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import DebugJobs from "./DebugJobs";
import GitSetup from "./GitSetup";
import { appInfo } from "./ipc";
import Panes from "./layout/Panes";
import StatusBar from "./layout/StatusBar";
import TabBar from "./layout/TabBar";
import Toolbar from "./layout/Toolbar";
import SettingsBanner from "./SettingsBanner";
import SettingsDialog from "./SettingsDialog";
import { createGitStore } from "./stores/git";
import { createJobStore } from "./stores/jobs";
import { createLayoutStore } from "./stores/layout";
import { createSettingsStore } from "./stores/settings";
import { createWorkspaceStore } from "./stores/workspaces";
import { followTheme } from "./theme/theme";

export default function App() {
  const [info] = createResource(appInfo);
  const jobs = createJobStore();
  const git = createGitStore();
  const settings = createSettingsStore();
  const layout = createLayoutStore();
  const workspaces = createWorkspaceStore();
  const [settingsOpen, setSettingsOpen] = createSignal(false);

  // The open tab's workspace: the only one until tabs exist (M3). It is
  // passed down, never read from a module, so tabs need no rework (ARCH-9).
  const workspace = () => workspaces.state.views?.[0];

  followTheme(settings.theme);

  onMount(() => {
    void settings.load();
    const unlisten = [jobs.subscribe(), git.subscribe(), workspaces.subscribe()];
    onCleanup(() => {
      for (const pending of unlisten) {
        void pending.then((stop) => stop());
      }
    });
  });

  // Not `usable` and not still looking: the setup screen replaces the app.
  const gitProblem = () => {
    const result = git.state.result;
    return result && result.kind !== "usable" ? result : undefined;
  };

  return (
    <Switch>
      <Match when={git.state.result === null}>
        <main class="message-screen">
          <p>Looking for git…</p>
        </main>
      </Match>
      <Match when={gitProblem()}>
        {(problem) => (
          <GitSetup problem={problem()} os={info()?.os} checking={git.state.checking} onRecheck={() => void git.recheck()} />
        )}
      </Match>
      <Match when={true}>
        <div class="app">
          <TabBar name={workspace()?.name} />
          <Toolbar onSettings={() => setSettingsOpen(true)} />
          <Show when={info.error as unknown}>
            <div role="alert" class="banner">
              Cannot reach the core: {String(info.error)}
            </div>
          </Show>
          <Show when={settings.state.view?.warning}>{(warning) => <SettingsBanner warning={warning()} />}</Show>
          <Show
            when={workspace()?.id}
            fallback={
              <main class="main">
                <p class="placeholder">
                  {workspaces.state.error === null
                    ? "Opening the workspace…"
                    : `Cannot open the workspace: ${workspaces.state.error}`}
                </p>
              </main>
            }
          >
            {(id) => (
              <>
                <main class="main">
                  <Panes workspace={id()} layout={layout} />
                </main>
                <StatusBar workspace={id()} jobs={jobs}>
                  <Show when={import.meta.env.DEV}>
                    <DebugJobs workspace={id()} />
                  </Show>
                </StatusBar>
              </>
            )}
          </Show>
          <Show when={settingsOpen()}>
            <SettingsDialog settings={settings} info={info()} git={git.state.result} onClose={() => setSettingsOpen(false)} />
          </Show>
        </div>
      </Match>
    </Switch>
  );
}
