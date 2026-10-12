import { createResource, createSignal, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import DebugJobs from "./DebugJobs";
import GitSetup from "./GitSetup";
import { appInfo, type WorkspaceId } from "./ipc";
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
import { followTheme } from "./theme/theme";

export default function App() {
  const [info] = createResource(appInfo);
  const jobs = createJobStore();
  const git = createGitStore();
  const settings = createSettingsStore();
  const layout = createLayoutStore();
  const [settingsOpen, setSettingsOpen] = createSignal(false);

  // Stands in for the open tab's workspace until workspaces exist (M3). It is
  // passed down, never read from a module, so tabs need no rework (ARCH-9).
  const workspace: WorkspaceId = "placeholder";

  followTheme(settings.theme);

  onMount(() => {
    void settings.load();
    const unlisten = [jobs.subscribe(), git.subscribe()];
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
          <TabBar />
          <Toolbar onSettings={() => setSettingsOpen(true)} />
          <Show when={info.error as unknown}>
            <div role="alert" class="banner">
              Cannot reach the core: {String(info.error)}
            </div>
          </Show>
          <Show when={settings.state.view?.warning}>{(warning) => <SettingsBanner warning={warning()} />}</Show>
          <main class="main">
            <Panes workspace={workspace} layout={layout} />
          </main>
          <StatusBar workspace={workspace} jobs={jobs}>
            <Show when={import.meta.env.DEV}>
              <DebugJobs workspace={workspace} />
            </Show>
          </StatusBar>
          <Show when={settingsOpen()}>
            <SettingsDialog settings={settings} info={info()} git={git.state.result} onClose={() => setSettingsOpen(false)} />
          </Show>
        </div>
      </Match>
    </Switch>
  );
}
