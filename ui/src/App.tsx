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
import PreflightDialog from "./groups/PreflightDialog";
import { appInfo, type JobId, type WorkspaceId } from "./ipc";
import { needsLook } from "./jobs/GroupReport";
import JobList from "./jobs/JobList";
import Panes from "./layout/Panes";
import StatusBar from "./layout/StatusBar";
import TabBar from "./layout/TabBar";
import Toolbar, { type ToolbarReasons } from "./layout/Toolbar";
import SettingsBanner from "./SettingsBanner";
import SettingsDialog from "./SettingsDialog";
import StashDialogs, { StashOffer } from "./stash/StashDialogs";
import CloneDialog from "./clone/CloneDialog";
import RemoteDialogs from "./remote/RemoteDialogs";
import TagDialogs from "./tag/TagDialogs";
import ResetDialogs from "./reset/ResetDialogs";
import SyncDialogs, { SyncRefusal } from "./sync/SyncDialogs";
import BranchDialogs from "./tree/BranchDialogs";
import IntegrateDialogs from "./integrate/IntegrateDialogs";
import { createAskpassStore } from "./stores/askpass";
import { createBranchStore } from "./stores/branch";
import { createChangesStore } from "./stores/changes";
import { createCommitStore } from "./stores/commit";
import { createConflictStore } from "./stores/conflicts";
import { createGitStore } from "./stores/git";
import { createGroupOpStore } from "./stores/groupOps";
import { createHistoryStore, mergedSource, sourceOf } from "./stores/history";
import { createIntegrateStore } from "./stores/integrate";
import { createJobStore } from "./stores/jobs";
import { createLayoutStore } from "./stores/layout";
import { createSelectionStore } from "./stores/selection";
import { createSettingsStore } from "./stores/settings";
import { createStashStore } from "./stores/stash";
import { createCloneStore } from "./stores/clone";
import { createRemoteStore } from "./stores/remote";
import { createTagStore } from "./stores/tag";
import { createResetStore } from "./stores/reset";
import { createSummaryStore, isBlocked } from "./stores/summaries";
import { canFetch, canPullOrPush, createSyncStore, DEFAULT_OPS } from "./stores/sync";
import { createTreeStore } from "./stores/tree";
import { createWatchStore } from "./stores/watch";
import { createWorkspaceStore } from "./stores/workspaces";
import { followAppearance, followDesktopFonts, followTheme } from "./theme/theme";
import WorkspaceDialogsView, { createWorkspaceDialogs } from "./workspaces/WorkspaceDialogs";
import { WorkspaceStart } from "./workspaces/WorkspaceMenu";

export default function App() {
  const [info] = createResource(appInfo);
  // A group job's report opens by itself when a repo failed or is in
  // conflict and its tab is active; otherwise its tab shows the marker
  // (UI-32).
  const jobs = createJobStore((view) => {
    if (view.report !== null && needsLook(view.report) && view.workspace_id === workspaces.active()?.id) {
      setJobsOpen({ initial: view.job_id });
    }
  });
  const groupOps = createGroupOpStore();
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
  // A commit from a list a refresh replaced reads the list again.
  const commit = createCommitStore((id) => void changes.reload(id));
  const conflicts = createConflictStore((id) => void changes.reload(id));
  const branch = createBranchStore();
  const sync = createSyncStore();
  const stash = createStashStore();
  const tag = createTagStore();
  const remote = createRemoteStore();
  // A clone added to the workspace is selected (section 6 "Repos").
  const clone = createCloneStore((id, repo) => tree.selectRepo(id, repo));
  const reset = createResetStore();
  // An aborted stash apply leaves nothing to offer dropping.
  const integrate = createIntegrateStore((id, repo) => stash.forget(id, repo));
  const watch = createWatchStore();
  const workspaceDialogs = createWorkspaceDialogs();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** The job list, when open, and the job it opened on. */
  const [jobsOpen, setJobsOpen] = createSignal<{ initial?: JobId } | null>(null);
  const [missingDismissed, setMissingDismissed] = createSignal(false);
  /** Why a workspace chosen while no tab is open did not open. */
  const [startError, setStartError] = createSignal<string | null>(null);

  // The active tab's workspace (UI-27). It is passed down, never read from
  // a module, so switching tabs needs no rework (ARCH-9).
  const workspace = () => workspaces.active();

  /** What the toolbar acts on (GRP-5). */
  const target = () => {
    const id = workspace()?.id;
    return id === undefined ? null : tree.target(id);
  };

  /** The repo of the tree's selection, and its summary, when the target set is that one repo. */
  const selectedRepo = () => {
    const id = workspace()?.id;
    const current = target();
    if (id === undefined || current?.kind !== "repo") {
      return null;
    }
    return { id, repo: current.repo_id, view: summaries.view(id, current.repo_id) };
  };

  /**
   * Why each button is disabled for a group or several repos (UI-20): their
   * group operations do not exist yet, and Stash is single-repo only.
   */
  const disabledWhy = (): ToolbarReasons => {
    const kind = target()?.kind;
    if (kind !== "group" && kind !== "repos") {
      return {};
    }
    const later = "Not available yet for a group or several repositories";
    return {
      fetch: later,
      pull: later,
      push: later,
      branch: later,
      merge: later,
      stash: "Stash works on one repository at a time",
    };
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

  /** Opens the merge dialog for the selected repo: not blocked (GRP-8) and on a branch. */
  const mergeAction = () => {
    const target = selectedRepo();
    if (target === null || !canPullOrPush(target.view)) {
      return null;
    }
    return () => integrate.open(target.id, { kind: "merge", repo: target.repo });
  };

  /** Opens the stash dialog for the selected repo: not blocked (GRP-8) and with changes to stash. */
  const stashAction = () => {
    const target = branchRepo();
    if (target === null || target.view?.summary?.dirty !== true) {
      return null;
    }
    return () => stash.open(target.id, { kind: "push", repo: target.repo });
  };

  /** The current branch of `repo` in the open workspace, or `null`. */
  const branchOf = (repo: string) => {
    const id = workspace()?.id;
    const head = id === undefined ? undefined : summaries.view(id, repo)?.summary?.head;
    return head?.kind === "branch" ? head.name : null;
  };

  const repoName = (repo: string) => workspace()?.repos.find((known) => known.id === repo)?.name ?? repo;

  /** Closes a tab, after asking when it holds unsaved conflict edits (OPS-9). Its jobs keep running (UI-21). */
  const closeTab = (id: WorkspaceId) => {
    const view = workspaces.state.views?.find((open) => open.id === id);
    const files = conflicts.unsavedFiles(id);
    if (view === undefined || files.length === 0) {
      return closeConfirmed(id);
    }
    const name = (repo: string) => view.repos.find((known) => known.id === repo)?.name ?? repo;
    workspaceDialogs.open({
      kind: "close",
      id,
      name: view.name,
      files: files.map((file) => `${name(file.repo)}: ${file.path}`),
    });
    return Promise.resolve();
  };

  /** Closes a tab; the unsaved conflict edits it held are dropped with it. */
  const closeConfirmed = async (id: WorkspaceId) => {
    await workspaces.close(id);
    conflicts.forgetWorkspace(id);
  };

  followAppearance(settings.appearance, followTheme(settings.theme));
  followDesktopFonts(() => ({ ui: info()?.ui_font ?? null, code: info()?.code_font ?? null }));

  // The active workspace's tree and summaries, kept current while its tab
  // is active; its repos are read again when the window regains focus
  // (OPS-7). Background tabs hear nothing (UI-30).
  createEffect(
    on(
      () => workspace()?.id,
      (id, previous) => {
        // The tab left lets go of its history rows and diffs (UI-31); the
        // core ended its walk and view.
        if (previous !== undefined && previous !== id) {
          history.release(previous);
          changes.release(previous);
        }
        if (id === undefined) {
          return;
        }
        const unlisten = [
          tree.subscribe(id),
          summaries.subscribe(id),
          history.subscribe(id),
          changes.subscribe(id),
          conflicts.subscribe(id),
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

  // The history pane follows the tree's selection (section 8): a group or
  // several repos show their merged history (PERF-5).
  createEffect(() => {
    const id = workspace()?.id;
    if (id === undefined) {
      return;
    }
    const target = tree.target(id);
    const shown = tree.single(id);
    const source =
      target?.kind === "group" || target?.kind === "repos"
        ? mergedSource(target)
        : sourceOf(shown?.row ?? null, shown?.item ?? null);
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
      groupOps.subscribe(),
      askpass.subscribe(),
      git.subscribe(),
      workspaces.subscribe(),
      commit.subscribe(),
      stash.subscribe(),
      clone.subscribe(),
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
          <TabBar workspaces={workspaces} jobs={jobs} dialogs={workspaceDialogs} onClose={closeTab} />
          <Show when={!missingDismissed() && workspaces.state.missing.length > 0}>
            <div role="alert" class="banner banner-dismissible">
              <span>
                These workspace files were not found, so their tabs were not restored:{" "}
                {workspaces.state.missing.join(", ")}
              </span>
              <button type="button" onClick={() => setMissingDismissed(true)}>
                Dismiss
              </button>
            </div>
          </Show>
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
            onMerge={mergeAction()}
            onStash={stashAction()}
            disabledWhy={disabledWhy()}
            onSettings={() => setSettingsOpen(true)}
          />
          <Show when={workspace()?.id}>{(id) => <SyncRefusal workspace={id()} sync={sync} />}</Show>
          <Show when={workspace()?.id}>
            {(id) => (
              <StashOffer
                workspace={id()}
                stash={stash}
                view={(repo) => summaries.view(id(), repo)}
                repoName={repoName}
              />
            )}
          </Show>
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
          {/* Keyed: each tab's panes are built anew from its own state, so nothing of one tab shows in another
              (UI-29). */}
          <Show
            when={workspace()?.id}
            keyed
            fallback={
              <main class="main">
                <Switch
                  fallback={
                    <div class="empty-state">
                      <p class="placeholder">No workspace is open. Open one or create a new one.</p>
                      <Show when={startError()}>{(text) => <p role="alert">{text()}</p>}</Show>
                      <WorkspaceStart workspaces={workspaces} dialogs={workspaceDialogs} onError={setStartError} />
                    </div>
                  }
                >
                  <Match when={workspaces.state.error !== null}>
                    <p class="placeholder">Cannot open the workspace: {workspaces.state.error}</p>
                  </Match>
                  <Match when={workspaces.state.views === null}>
                    <p class="placeholder">Opening the workspace…</p>
                  </Match>
                </Switch>
              </main>
            }
          >
            {(id) => (
              <>
                <main class="main">
                  <Panes
                    workspace={id}
                    layout={layout}
                    tree={tree}
                    summaries={summaries}
                    workspaces={workspaces}
                    history={history}
                    changes={changes}
                    commit={commit}
                    conflicts={conflicts}
                    branch={branch}
                    stash={stash}
                    tag={tag}
                    remote={remote}
                    clone={clone}
                    reset={reset}
                    integrate={integrate}
                    settings={settings}
                  />
                </main>
                <BranchDialogs workspace={id} branch={branch} repoName={repoName} />
                <SyncDialogs workspace={id} sync={sync} repoName={repoName} />
                <StashDialogs workspace={id} stash={stash} repoName={repoName} />
                <TagDialogs workspace={id} tag={tag} repoName={repoName} />
                <RemoteDialogs workspace={id} remote={remote} repoName={repoName} />
                <Show when={clone.isOpen(id)}>
                  <CloneDialog workspace={id} clone={clone} />
                </Show>
                <ResetDialogs workspace={id} reset={reset} repoName={repoName} />
                <IntegrateDialogs workspace={id} integrate={integrate} repoName={repoName} branchOf={branchOf} />
                <StatusBar workspace={id} jobs={jobs} onOpenJobs={() => setJobsOpen({})} />
              </>
            )}
          </Show>
          <WorkspaceDialogsView dialogs={workspaceDialogs} workspaces={workspaces} onCloseTab={closeConfirmed} />
          <Show when={jobsOpen()} keyed>
            {(open) => (
              <JobList
                jobs={jobs}
                initial={open.initial}
                onClose={() => setJobsOpen(null)}
                onSelectRepo={(id, repo) => {
                  setJobsOpen(null);
                  void workspaces.open(id).then(() => tree.selectRepo(id, repo));
                }}
                onRetry={(id, job) => {
                  setJobsOpen(null);
                  void groupOps.retry(id, job);
                }}
              />
            )}
          </Show>
          <Show when={groupOps.state.pending}>
            {(pending) => <PreflightDialog pending={pending()} groupOps={groupOps} />}
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
