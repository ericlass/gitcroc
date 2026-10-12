import { createResource, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import DebugJobs from "./DebugJobs";
import { appInfo } from "./ipc";
import { createJobStore } from "./stores/jobs";

export default function App() {
  const [info] = createResource(appInfo);
  const jobs = createJobStore();

  onMount(() => {
    const unlisten = jobs.subscribe();
    onCleanup(() => void unlisten.then((stop) => stop()));
  });

  return (
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
  );
}
