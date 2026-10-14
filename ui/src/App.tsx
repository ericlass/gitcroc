import {
  createEffect,
  createResource,
  createSignal,
  Match,
  on,
  onCleanup,
  onMount,
  Show,
  Switch,
  untrack,
} from "solid-js";
import AskpassDialog from "./AskpassDialog";
import GitSetup from "./GitSetup";
import { appInfo } from "./ipc";
import JobList from "./jobs/JobList";
import Panes from "./layout/Panes";
import StatusBar from "./layout/StatusBar";
import TabBar from "./layout/TabBar";
import Toolbar from "./layout/Toolbar";
import SettingsBanner from "./SettingsBanner";
import SettingsDialog from "./SettingsDialog";
import SyncDialogs, { SyncRefusal } from "./sync/SyncDialogs";
import BranchDialogs from "./tree/BranchDialogs";
import { createAskpassStore } from "./stores/askpass";
import { createBranchStore } from "./stores/branch";
import { createChangesStore } from "./stores/changes";
import { createCommitStore } from "./stores/commit";
import { createGitStore } from "./stores/git";
import { createHistoryStore, sourceOf } from "./stores/history";
import { createJobStore } from "./stores/jobs";
import { createLayoutStore } from "./stores/layout";
import { createSelectionStore } from "./stores/selection";
import { createSettingsStore } from "./stores/settings";
import { createSummaryStore, isBlocked } from "./stores/summaries";
import { canFetch, canPullOrPush, createSyncStore, DEFAULT_OPS } from "./stores/sync";
import { createTreeStore } from "./stores/tree";
import { createWatchStore } from "./stores/watch";
import { createWorkspaceStore } from "./stores/workspaces";
import { followAppearance, followDesktopFonts, followTheme } from "./theme/theme";

export default function App() {
  const [info] = createResource(appInfo);
  const jobs = createJobStore();
  const askpass = createAskpassStore();
  const git = createGitStore();
  const settings = createSettingsStore();
  const layout = createLayoutStore();
  const workspaces = createWorkspaceStore();
  const summaries = createSummaryStore();
  const tree = createTreeStore();
  const selection = createSelectionStore();
  const history = createHistoryStore(selection);
  const changes = createChangesStore(settings.changes);
  const commit = createCommitStore();
  const branch = createBranchStore();
  const sync = createSyncStore();
  const watch = createWatchStore();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [jobsOpen, setJobsOpen] = createSignal(false);

  // The open tab's workspace: the only one until tabs exist (M3). It is
  // passed down, never read from a module, so tabs need no rework (ARCH-9).
  const workspace = () => workspaces.state.views?.[0];

  /** The repo of the tree's selection, and its summary. */
  const selectedRepo = () => {
    const id = workspace()?.id;
    const item = id === undefined ? null : tree.tree(id).selectedItem;
    const repo = item === null || item.kind === "ungrouped" ? null : item.repo_id;
    return id === undefined || repo === null ? null : { id, repo, view: summaries.view(id, repo) };
  };

  /** The selected repo, when no write is blocked on it (GRP-8). */
  const branchRepo = () => {
    const target = selectedRepo();
    return target === null || isBlocked(target.view) ? null : target;
  };

  /** What the toolbar's Fetch, Pull and Push do for the selected repo (UI-20). */
  const syncActions = () => {
    const target = selectedRepo();
    if (target === null) {
      return { fetch: null, pull: null, pullOptions: null, push: null, pushOptions: null };
    }
    const { id, repo, view } = target;
    const pullOrPush = canPullOrPush(view);
    return {
      fetch: canFetch(view) ? () => void sync.run(id, repo, DEFAULT_OPS.fetch) : null,
      pull: pullOrPush ? () => void sync.run(id, repo, DEFAULT_OPS.pull) : null,
      pullOptions: pullOrPush ? () => sync.open(id, { kind: "pull", repo }) : null,
      push: pullOrPush ? () => void sync.run(id, repo, DEFAULT_OPS.push) : null,
      pushOptions: pullOrPush ? () => sync.open(id, { kind: "push", repo, remotes: null }) : null,
    };
  };

  const repoName = (repo: string) => workspace()?.repos.find((known) => known.id === repo)?.name ?? repo;

  followAppearance(settings.appearance, followTheme(settings.theme));
  followDesktopFonts(() => ({ ui: info()?.ui_font ?? null, code: info()?.code_font ?? null }));

  // The open workspace's tree and summaries, kept current while it is open;
  // its repos are read again when the window regains focus (OPS-7).
  createEffect(
    on(
      () => workspace()?.id,
      (id) => {
        if (id === undefined) {
          return;
        }
        const unlisten = [
          tree.subscribe(id),
          summaries.subscribe(id),
          history.subscribe(id),
          changes.subscribe(id),
          watch.subscribe(id),
        ];
        onCleanup(() => {
          for (const pending of unlisten) {
            void pending.then((stop) => stop());
          }
        });
      },
    ),
  );

  // The history pane follows the tree's selection (section 8).
  createEffect(() => {
    const id = workspace()?.id;
    if (id === undefined) {
      return;
    }
    const current = tree.tree(id);
    const source = sourceOf(current.selected, current.selectedItem);
    untrack(() => history.show(id, source));
  });

  // The changes pane follows the history's selection (UI-14, UI-15).
  createEffect(() => {
    const id = workspace()?.id;
    if (id === undefined) {
      return;
    }
    const selected = selection.selected(id);
    untrack(() => void changes.show(id, selected));
  });

  // A diff option the core applies reads the diff again (UI-17).
  createEffect(
    on(
      () => {
        const view = settings.changes();
        return [workspace()?.id, view.diff_mode, view.ignore_whitespace, view.context] as const;
      },
      ([id]) => {
        if (id !== undefined) {
          untrack(() => void changes.reloadDiff(id));
        }
      },
      { defer: true },
    ),
  );

  // The pane layout kept in the workspace file (UI-19), applied once.
  createEffect(() => {
    const view = workspace();
    if (view !== undefined) {
      untrack(() => layout.load(view.id, view.layout));
    }
  });

  onMount(() => {
    void settings.load();
    const unlisten = [
      jobs.subscribe(),
      askpass.subscribe(),
      git.subscribe(),
      workspaces.subscribe(),
      commit.subscribe(),
    ];
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
          <Toolbar
            sync={syncActions()}
            onBranch={
              branchRepo() === null
                ? null
                : () => {
                    const target = branchRepo();
                    if (target !== null) {
                      branch.open(target.id, { kind: "create", repo: target.repo, start: { kind: "head" } });
                    }
                  }
            }
            onSettings={() => setSettingsOpen(true)}
          />
          <Show when={workspace()?.id}>{(id) => <SyncRefusal workspace={id()} sync={sync} />}</Show>
          <Show when={info.error as unknown}>
            <div role="alert" class="banner">
              Cannot reach the core: {String(info.error)}
            </div>
          </Show>
          <Show when={settings.state.view?.warning}>{(warning) => <SettingsBanner warning={warning()} />}</Show>
          <Show when={workspace()?.id}>
            {(id) => (
              <Show when={!watch.watch(id()).dismissed && watch.watch(id()).warning}>
                {(warning) => (
                  <div role="alert" class="banner banner-dismissible">
                    <span>
                      Changes made outside gitcroc show only when the window regains focus: watching files failed (
                      {warning().message}).
                    </span>
                    <button type="button" onClick={() => watch.dismiss(id())}>
                      Dismiss
                    </button>
                  </div>
                )}
              </Show>
            )}
          </Show>
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
                  <Panes
                    workspace={id()}
                    layout={layout}
                    tree={tree}
                    summaries={summaries}
                    workspaces={workspaces}
                    history={history}
                    changes={changes}
                    commit={commit}
                    branch={branch}
                    settings={settings}
                  />
                </main>
                <BranchDialogs workspace={id()} branch={branch} repoName={repoName} />
                <SyncDialogs workspace={id()} sync={sync} repoName={repoName} />
                <StatusBar workspace={id()} jobs={jobs} onOpenJobs={() => setJobsOpen(true)} />
              </>
            )}
          </Show>
          <Show when={jobsOpen()}>
            <JobList jobs={jobs} onClose={() => setJobsOpen(false)} />
          </Show>
          <Show when={askpass.current()} keyed>
            {(prompt) => (
              <AskpassDialog prompt={prompt} onAnswer={(answer) => void askpass.answer(prompt.prompt_id, answer)} />
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
