import { createResource, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import DebugJobs from "./DebugJobs";
import GitSetup from "./GitSetup";
import { appInfo } from "./ipc";
import { createGitStore } from "./stores/git";
import { createJobStore } from "./stores/jobs";

export default function App() {
  const [info] = createResource(appInfo);
  const jobs = createJobStore();
  const git = createGitStore();

  onMount(() => {
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
        <main>
          <p>Looking for git…</p>
        </main>
      </Match>
      <Match when={gitProblem()}>
        {(problem) => (
          <GitSetup problem={problem()} os={info()?.os} checking={git.state.checking} onRecheck={() => void git.recheck()} />
        )}
      </Match>
      <Match when={true}>
        <main>
          <Switch>
            <Match when={info.error as unknown}>
              <p role="alert">Cannot reach the core: {String(info.error)}</p>
            </Match>
            <Match when={info()}>
              {(info) => (
                <>
                  <h1>{info().name}</h1>
                  <p>
                    Version {info().version} on {info().os} ({info().arch})
                  </p>
                </>
              )}
            </Match>
          </Switch>
          <Show when={import.meta.env.DEV}>
            <DebugJobs jobs={jobs} />
          </Show>
        </main>
      </Match>
    </Switch>
  );
}
