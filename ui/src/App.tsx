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
import ConsentDialog from "./ai/ConsentDialog";
import AskpassDialog from "./AskpassDialog";
import GitSetup from "./GitSetup";
import GroupNameDialog from "./groups/GroupNameDialog";
import GroupSyncDialog from "./groups/GroupSyncDialog";
import PreflightDialog from "./groups/PreflightDialog";
import { appInfo, type GroupOp, type JobId, type PaletteItem, type TargetSet, type WorkspaceId } from "./ipc";
import { needsLook } from "./jobs/GroupReport";
import JobList from "./jobs/JobList";
import { outcomeText } from "./jobs/outcome";
import { matches, modalOpen, setMac } from "./keys";
import Panes from "./layout/Panes";
import { moveToRegion } from "./layout/regions";
import { commandOf, createCommandStore } from "./palette/commands";
import Palette from "./palette/Palette";
import StatusBar from "./layout/StatusBar";
import TabBar from "./layout/TabBar";
import { createWindowChrome, TitleBar } from "./layout/TitleBar";
import Toolbar, { type ToolbarReasons } from "./layout/Toolbar";
import SettingsBanner from "./SettingsBanner";
import SettingsDialog from "./SettingsDialog";
import StashDialogs, { StashOffer } from "./stash/StashDialogs";
import CloneDialog from "./clone/CloneDialog";
import MissingReposDialog from "./clone/MissingReposDialog";
import RemoteDialogs from "./remote/RemoteDialogs";
import TagDialogs from "./tag/TagDialogs";
import ResetDialogs from "./reset/ResetDialogs";
import SyncDialogs, { SyncRefusal } from "./sync/SyncDialogs";
import BranchDialogs from "./tree/BranchDialogs";
import IntegrateDialogs from "./integrate/IntegrateDialogs";
import { createAiStore } from "./stores/ai";
import { createAnnouncer } from "./stores/announcer";
import { createAskpassStore } from "./stores/askpass";
import { createBranchStore } from "./stores/branch";
import { createChangesStore } from "./stores/changes";
import { createCommitStore } from "./stores/commit";
import { createConflictStore, followAiFeatures } from "./stores/conflicts";
import { createResolveAllStore } from "./stores/resolveAll";
import { createGitStore } from "./stores/git";
import { createGroupOpStore, DEFAULT_GROUP_OPS, type SyncKind } from "./stores/groupOps";
import { createHistoryStore, mergedSource, sourceOf } from "./stores/history";
import { createIntegrateStore } from "./stores/integrate";
import { createJobStore } from "./stores/jobs";
import { createLayoutStore } from "./stores/layout";
import { createSelectionStore } from "./stores/selection";
import { createSettingsStore } from "./stores/settings";
import { createStashStore } from "./stores/stash";
import { createCloneStore } from "./stores/clone";
import { createShareStore } from "./stores/share";
import { createRemoteStore } from "./stores/remote";
import { createTagStore } from "./stores/tag";
import { createResetStore } from "./stores/reset";
import { createSummaryStore, isBlocked } from "./stores/summaries";
import { canFetch, canPullOrPush, createSyncStore, DEFAULT_OPS } from "./stores/sync";
import { createTreeStore } from "./stores/tree";
import { noticeOf, createUpdateStore } from "./stores/update";
import { createWatchStore } from "./stores/watch";
import { createWorkspaceStore } from "./stores/workspaces";
import { followAppearance, followDesktopFonts, followTheme } from "./theme/theme";
import UpdateDialog from "./update/UpdateDialog";
import UpdateNotice from "./update/UpdateNotice";
import WorkspaceDialogsView, { createWorkspaceDialogs } from "./workspaces/WorkspaceDialogs";
import { WorkspaceStart } from "./workspaces/WorkspaceMenu";
import { strings } from "./strings/en";

export default function App() {
  const [info] = createResource(appInfo);
  // A group job's report opens by itself when a repo failed or is in
  // conflict and its tab is active; otherwise its tab shows the marker
  // (UI-32).
  // A job that ended failed or in conflict is announced, in any tab (M5-10).
  const announcer = createAnnouncer();
  const jobs = createJobStore((view) => {
    if (view.report !== null && needsLook(view.report) && view.workspace_id === workspaces.active()?.id) {
      setJobsOpen({ initial: view.job_id });
    }
    if (view.outcome?.kind === "failed" || view.outcome?.kind === "conflict") {
      announcer.announce(strings.app.jobEnded(view.label, view.workspace_name, outcomeText(view.outcome)));
    }
  });
  const groupOps = createGroupOpStore();
  const askpass = createAskpassStore();
  const git = createGitStore();
  const settings = createSettingsStore();
  // One for every tab: a change of AI features reaches all of them at once (AI-16).
  const ai = createAiStore();
  const layout = createLayoutStore();
  const workspaces = createWorkspaceStore();
  const summaries = createSummaryStore();
  const tree = createTreeStore();
  const selection = createSelectionStore();
  const history = createHistoryStore(selection);
  const changes = createChangesStore(settings.changes);
  // A commit from a list a refresh replaced reads the list again; a group
  // commit goes through the preflight (UI-34).
  const commit = createCommitStore(
    (id) => void changes.reload(id),
    (id, target, op, started) => void groupOps.run(id, target, op, started),
  );
  const conflicts = createConflictStore((id) => void changes.reload(id));
  const resolveAll = createResolveAllStore({
    history,
    changes,
    conflicts,
    jobs,
    onResolvedElsewhere: (id) => void changes.reload(id),
  });
  followAiFeatures(ai, conflicts, resolveAll);
  const branch = createBranchStore();
  const sync = createSyncStore();
  const stash = createStashStore();
  const tag = createTagStore();
  const remote = createRemoteStore();
  // A clone added to the workspace is selected (section 6 "Repos").
  const clone = createCloneStore((id, repo) => tree.selectRepo(id, repo));
  const share = createShareStore();
  const reset = createResetStore();
  // An aborted stash apply leaves nothing to offer dropping.
  const integrate = createIntegrateStore((id, repo) => stash.forget(id, repo));
  const watch = createWatchStore();
  const update = createUpdateStore();
  const workspaceDialogs = createWorkspaceDialogs();
  // What the palette's operations run, provided by the places that have them (UI-22).
  const commands = createCommandStore();
  /** Settings is open, at its top, at its AI section (AI-3) or at its Updates section. */
  const [settingsOpen, setSettingsOpen] = createSignal<"top" | "ai" | "updates" | null>(null);
  /** The update dialog is open (PLAT-4). */
  const [updateOpen, setUpdateOpen] = createSignal(false);
  /** The options dialog of Fetch, Pull or Push: what it is for, and what runs the picked options. */
  const [syncOptions, setSyncOptions] = createSignal<{
    kind: SyncKind;
    targetName: string;
    run: (op: GroupOp) => void;
  } | null>(null);
  /** The job list, when open, and the job it opened on. */
  const [jobsOpen, setJobsOpen] = createSignal<{ initial?: JobId } | null>(null);
  const [missingDismissed, setMissingDismissed] = createSignal(false);
  /** Why a workspace chosen while no tab is open did not open. */
  const [startError, setStartError] = createSignal<string | null>(null);
  /** The palette, when open: the workspace it searches and what had the focus before (UI-22). */
  const [palette, setPalette] = createSignal<{ workspace: WorkspaceId; opener: Element | null } | null>(null);

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

  /** Why each button is disabled for a group or several repos (UI-20): Stash is single-repo only. */
  const disabledWhy = (): ToolbarReasons => {
    const kind = target()?.kind;
    if (kind !== "group" && kind !== "repos") {
      return {};
    }
    return { stash: strings.app.stashOneRepo };
  };

  /** How the options dialog names a target set of a group or several repos. */
  const targetName = (current: TargetSet) => {
    if (current.kind === "group") {
      return workspace()?.groups.find((group) => group.id === current.group_id)?.name ?? strings.app.theGroup;
    }
    return current.kind === "repos" ? strings.app.repositories(current.repo_ids.length) : repoName(current.repo_id);
  };

  /**
   * Fetch, Pull and Push for a group or several repos (GRP-12): a click
   * runs with the defaults and the arrow opens the options, both through
   * the preflight, which skips or blocks the repos that cannot take them.
   */
  const groupSyncActions = (id: WorkspaceId, current: TargetSet) => {
    const run = (op: GroupOp) => void groupOps.run(id, current, op);
    const click = (kind: SyncKind) => () => run(DEFAULT_GROUP_OPS[kind]);
    const options = (kind: SyncKind) => () => setSyncOptions({ kind, targetName: targetName(current), run });
    return {
      fetch: click("fetch"),
      fetchOptions: options("fetch"),
      pull: click("pull"),
      pullOptions: options("pull"),
      push: click("push"),
      pushOptions: options("push"),
    };
  };

  /**
   * What the toolbar's Branch does (UI-20): the create dialog of the
   * selected repo, when no write is blocked on it (GRP-8), or the group
   * create dialog of a group or several repos, whose preflight skips or
   * blocks each repo that cannot take it.
   */
  const branchAction = () => {
    const current = target();
    const workspaceId = workspace()?.id;
    if (workspaceId !== undefined && (current?.kind === "group" || current?.kind === "repos")) {
      return () =>
        groupOps.openName({
          workspaceId,
          target: current,
          targetName: targetName(current),
          kind: "create",
          name: "",
        });
    }
    const selected = branchRepo();
    if (selected === null) {
      return null;
    }
    return () => branch.open(selected.id, { kind: "create", repo: selected.repo, start: { kind: "head" } });
  };

  /** The selected repo, when no write is blocked on it (GRP-8). */
  const branchRepo = () => {
    const target = selectedRepo();
    return target === null || isBlocked(target.view) ? null : target;
  };

  /** What the toolbar's Fetch, Pull and Push do for the target set (UI-20). */
  const syncActions = () => {
    const current = target();
    const workspaceId = workspace()?.id;
    if (workspaceId !== undefined && (current?.kind === "group" || current?.kind === "repos")) {
      return groupSyncActions(workspaceId, current);
    }
    const selected = selectedRepo();
    if (selected === null) {
      return { fetch: null, fetchOptions: null, pull: null, pullOptions: null, push: null, pushOptions: null };
    }
    const { id, repo, view } = selected;
    const fetch = canFetch(view);
    const pullOrPush = canPullOrPush(view);
    const fetchWith = (op: GroupOp) => {
      if (op.kind === "fetch") {
        void sync.run(id, repo, { kind: "fetch", prune: op.prune });
      }
    };
    return {
      fetch: fetch ? () => void sync.run(id, repo, DEFAULT_OPS.fetch) : null,
      fetchOptions: fetch ? () => setSyncOptions({ kind: "fetch", targetName: repoName(repo), run: fetchWith }) : null,
      pull: pullOrPush ? () => void sync.run(id, repo, DEFAULT_OPS.pull) : null,
      pullOptions: pullOrPush ? () => sync.open(id, { kind: "pull", repo }) : null,
      push: pullOrPush ? () => void sync.run(id, repo, DEFAULT_OPS.push) : null,
      pushOptions: pullOrPush ? () => sync.open(id, { kind: "push", repo, remotes: null }) : null,
    };
  };

  /**
   * What the toolbar's Merge does (UI-20): the merge dialog of the selected
   * repo, when it is not blocked (GRP-8) and on a branch, or the group merge
   * and rebase dialog of a group or several repos, whose preflight skips,
   * blocks or flags each repo (GRP-13).
   */
  const mergeAction = () => {
    const current = target();
    const workspaceId = workspace()?.id;
    if (workspaceId !== undefined && (current?.kind === "group" || current?.kind === "repos")) {
      return () =>
        groupOps.openName({ workspaceId, target: current, targetName: targetName(current), kind: "merge", name: "" });
    }
    const selected = selectedRepo();
    if (selected === null || !canPullOrPush(selected.view)) {
      return null;
    }
    return () => integrate.open(selected.id, { kind: "merge", repo: selected.repo });
  };

  /**
   * Rebase onto a ref: the merge dialog with rebase chosen for the selected
   * repo, by the rules of Merge, or the group rebase dialog of a group or
   * several repos (GRP-13).
   */
  const rebaseAction = () => {
    const current = target();
    const workspaceId = workspace()?.id;
    if (workspaceId !== undefined && (current?.kind === "group" || current?.kind === "repos")) {
      return () =>
        groupOps.openName({ workspaceId, target: current, targetName: targetName(current), kind: "rebase", name: "" });
    }
    const selected = selectedRepo();
    if (selected === null || !canPullOrPush(selected.view)) {
      return null;
    }
    return () => integrate.open(selected.id, { kind: "merge", repo: selected.repo, rebase: true });
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

  /** The files with unsaved conflict edits in every tab, which a restart into an update loses (OPS-9). */
  const unsavedEverywhere = () =>
    (workspaces.state.views ?? []).flatMap((view) => {
      const name = (repo: string) => view.repos.find((known) => known.id === repo)?.name ?? repo;
      return conflicts.unsavedFiles(view.id).map((file) => `${view.name}: ${name(file.repo)}: ${file.path}`);
    });

  /** Check for updates (PLAT-4): the dialog of a version on offer, else a check, with its result in Settings. */
  const checkForUpdates = () => {
    if (update.state.view?.status.kind === "available") {
      setUpdateOpen(true);
    } else {
      void update.checkNow();
      setSettingsOpen("updates");
    }
  };

  /** Closes a tab; the unsaved conflict edits it held are dropped with it. */
  const closeConfirmed = async (id: WorkspaceId) => {
    await workspaces.close(id);
    conflicts.forgetWorkspace(id);
    resolveAll.forgetWorkspace(id);
  };

  /** Opens the palette on the active workspace, unless another dialog is open (UI-22). */
  const openPalette = () => {
    const id = workspace()?.id;
    if (id === undefined || document.querySelector("[aria-modal=true]") !== null) {
      return;
    }
    setPalette({ workspace: id, opener: document.activeElement });
  };

  /** Closes the palette; the focus goes back where it was. */
  const closePalette = () => {
    const open = palette();
    setPalette(null);
    if (open?.opener instanceof HTMLElement) {
      open.opener.focus();
    }
  };

  /** A repo, group or branch chosen in the palette is selected in the tree as a click does; the tree has the focus. */
  const selectFromPalette = (id: WorkspaceId, item: Exclude<PaletteItem, { kind: "operation" }>) => {
    const place =
      item.kind === "repo"
        ? ({ kind: "repo", repo: item.repo_id } as const)
        : item.kind === "group"
          ? ({ kind: "group", group: item.group_id } as const)
          : ({ kind: "branch", repo: item.repo_id, name: item.name, remote: item.remote } as const);
    void tree.reveal(id, place).then(() => document.querySelector<HTMLElement>("[role=tree]")?.focus());
  };

  // The app-wide shortcuts of keys.ts, caught before the lists and editors see them. Ctrl+K, or Cmd+K on macOS,
  // opens and closes the palette wherever the focus is, also in the diff and conflict editors; the others wait
  // while a dialog is open. The tabs' keys are the tab bar's, the hunks' the diff's and the conflict editor's.
  const onAppKey = (event: KeyboardEvent) => {
    if (matches("palette", event)) {
      event.preventDefault();
      event.stopPropagation();
      if (palette() === null) {
        openPalette();
      } else {
        closePalette();
      }
      return;
    }
    const step = matches("nextRegion", event) ? 1 : matches("previousRegion", event) ? -1 : null;
    const settings = matches("settings", event);
    if ((step === null && !settings) || modalOpen()) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (step !== null) {
      moveToRegion(step);
    } else {
      setSettingsOpen("top");
    }
  };
  window.addEventListener("keydown", onAppKey, true);
  onCleanup(() => window.removeEventListener("keydown", onAppKey, true));
  createEffect(() => setMac(info()?.os === "macos"));
  // The tab bar is the window's title bar unless the user chose the system's (UI-35).
  const chrome = createWindowChrome(() => info()?.os, settings.systemTitleBar);

  // The toolbar's operations and the window's own, as their controls run them (UI-22).
  commands.provide(null, "app", {
    fetch: () => (target() === null ? null : commandOf(syncActions().fetch, disabledWhy().fetch)),
    fetch_with_options: () => (target() === null ? null : commandOf(syncActions().fetchOptions, disabledWhy().fetch)),
    pull: () => (target() === null ? null : commandOf(syncActions().pull, disabledWhy().pull)),
    pull_with_options: () => (target() === null ? null : commandOf(syncActions().pullOptions, disabledWhy().pull)),
    push: () => (target() === null ? null : commandOf(syncActions().push, disabledWhy().push)),
    push_with_options: () => (target() === null ? null : commandOf(syncActions().pushOptions, disabledWhy().push)),
    create_branch: () => (target() === null ? null : commandOf(branchAction(), disabledWhy().branch)),
    merge: () => (target() === null ? null : commandOf(mergeAction(), disabledWhy().merge)),
    rebase: () => (target() === null ? null : commandOf(rebaseAction(), disabledWhy().merge)),
    stash: () => (target() === null ? null : commandOf(stashAction(), disabledWhy().stash)),
    settings: () => commandOf(() => setSettingsOpen("top")),
    job_list: () => commandOf(() => setJobsOpen({})),
    check_for_updates: () => commandOf(checkForUpdates),
    new_workspace: () => commandOf(() => workspaceDialogs.open({ kind: "new" })),
    import_workspace: () => commandOf(() => share.importWorkspace()),
    export_workspace: () => {
      const id = workspace()?.id;
      return id === undefined ? null : commandOf(() => share.exportWorkspace(id));
    },
    close_tab: () => {
      const id = workspace()?.id;
      return id === undefined ? null : commandOf(() => void closeTab(id));
    },
  });

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
      share.subscribe(),
      ai.subscribe(),
      update.subscribe(),
    ];
    onCleanup(() => {
      for (const pending of unlisten) {
        void pending.then((stop) => stop());
      }
    });
  });

  // A new version on offer is said once, as it first appears; the daily check finding it again says nothing
  // (PLAT-4, M5-10).
  let announced: string | null = null;
  createEffect(
    on(
      () => noticeOf(update.state.view),
      (version) => {
        if (version !== null && version !== announced) {
          announced = version;
          announcer.announce(strings.update.notice(version));
        }
      },
    ),
  );

  // Not `usable` and not still looking: the setup screen replaces the app.
  const gitProblem = () => {
    const result = git.state.result;
    return result && result.kind !== "usable" ? result : undefined;
  };

  return (
    <>
      <div class="visually-hidden" role="status" aria-live="polite" aria-atomic="true">
        {announcer.text()}
      </div>
      <Switch>
        <Match when={git.state.result === null}>
          <div class="app">
            <TitleBar chrome={chrome} name={info()?.name ?? ""} />
            <main class="message-screen">
              <p>{strings.app.lookingForGit}</p>
            </main>
          </div>
        </Match>
        <Match when={gitProblem()}>
          {(problem) => (
            <div class="app">
              <TitleBar chrome={chrome} name={info()?.name ?? ""} />
              <GitSetup
                problem={problem()}
                os={info()?.os}
                checking={git.state.checking}
                error={git.state.error}
                onRecheck={() => void git.recheck()}
                onChoose={() => void git.choose()}
                onAutomatic={() => void git.useAutomatic()}
              />
            </div>
          )}
        </Match>
        <Match when={true}>
          <div class="app">
            <TabBar
              commands={commands}
              workspaces={workspaces}
              jobs={jobs}
              dialogs={workspaceDialogs}
              share={share}
              ai={ai}
              chrome={chrome}
              onClose={closeTab}
            />
            <Show when={share.state.notice}>
              {(notice) => (
                <div role={notice().error ? "alert" : "status"} class="banner banner-dismissible">
                  <span>{notice().text}</span>
                  <button type="button" onClick={() => share.dismiss()}>
                    {strings.common.dismiss}
                  </button>
                </div>
              )}
            </Show>
            <Show when={!missingDismissed() && workspaces.state.missing.length > 0}>
              <div role="alert" class="banner banner-dismissible">
                <span>{strings.app.missingWorkspaces(workspaces.state.missing.join(", "))}</span>
                <button type="button" onClick={() => setMissingDismissed(true)}>
                  {strings.common.dismiss}
                </button>
              </div>
            </Show>
            <Toolbar
              sync={syncActions()}
              onBranch={branchAction()}
              onMerge={mergeAction()}
              onStash={stashAction()}
              disabledWhy={disabledWhy()}
              onPalette={workspace() === undefined ? null : openPalette}
              onSettings={() => setSettingsOpen("top")}
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
                {strings.app.cannotReachCore(String(info.error))}
              </div>
            </Show>
            <Show when={settings.state.view?.warning}>{(warning) => <SettingsBanner warning={warning()} />}</Show>
            <Show when={workspace()?.id}>
              {(id) => (
                <Show when={!watch.watch(id()).dismissed && watch.watch(id()).warning}>
                  {(warning) => (
                    <div role="alert" class="banner banner-dismissible">
                      <span>{strings.app.watchFailed(warning().message)}</span>
                      <button type="button" onClick={() => watch.dismiss(id())}>
                        {strings.common.dismiss}
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
                        <p class="placeholder">{strings.app.noWorkspace}</p>
                        <Show when={startError()}>{(text) => <p role="alert">{text()}</p>}</Show>
                        <WorkspaceStart
                          workspaces={workspaces}
                          dialogs={workspaceDialogs}
                          onError={setStartError}
                          onImport={() => share.importWorkspace()}
                        />
                      </div>
                    }
                  >
                    <Match when={workspaces.state.error !== null}>
                      <p class="placeholder">{strings.app.cannotOpenWorkspace(workspaces.state.error ?? "")}</p>
                    </Match>
                    <Match when={workspaces.state.views === null}>
                      <p class="placeholder">{strings.app.openingWorkspace}</p>
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
                      share={share}
                      groupOps={groupOps}
                      reset={reset}
                      integrate={integrate}
                      settings={settings}
                      ai={ai}
                      resolveAll={resolveAll}
                      commands={commands}
                      onAiSettings={() => setSettingsOpen("ai")}
                    />
                  </main>
                  <BranchDialogs workspace={id} branch={branch} repoName={repoName} />
                  <SyncDialogs workspace={id} sync={sync} repoName={repoName} />
                  <StashDialogs workspace={id} stash={stash} repoName={repoName} />
                  <TagDialogs workspace={id} tag={tag} repoName={repoName} />
                  <RemoteDialogs workspace={id} remote={remote} repoName={repoName} />
                  <Show when={clone.isOpen(id)}>
                    <CloneDialog workspace={id} clone={clone} groups={workspace()?.groups ?? []} />
                  </Show>
                  <Show when={share.missingOpen(id)}>
                    <MissingReposDialog workspace={id} share={share} />
                  </Show>
                  <ResetDialogs workspace={id} reset={reset} repoName={repoName} />
                  <IntegrateDialogs workspace={id} integrate={integrate} repoName={repoName} branchOf={branchOf} />
                  <StatusBar workspace={id} jobs={jobs} onOpenJobs={() => setJobsOpen({})}>
                    <UpdateNotice update={update} onOpen={() => setUpdateOpen(true)} />
                  </StatusBar>
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
                  onOpenFile={(id, repo, path) => {
                    setJobsOpen(null);
                    void workspaces
                      .open(id)
                      .then(() => tree.selectRepo(id, repo))
                      .then(() => resolveAll.open(id, repo, path));
                  }}
                  onRetry={(id, job) => {
                    setJobsOpen(null);
                    void groupOps.retry(id, job);
                  }}
                />
              )}
            </Show>
            <Show when={syncOptions()} keyed>
              {(open) => (
                <GroupSyncDialog
                  kind={open.kind}
                  targetName={open.targetName}
                  onRun={(op) => {
                    setSyncOptions(null);
                    open.run(op);
                  }}
                  onClose={() => setSyncOptions(null)}
                />
              )}
            </Show>
            <Show when={groupOps.state.nameDialog} keyed>
              {(dialog) => (
                <GroupNameDialog
                  dialog={dialog}
                  onRun={(op) => {
                    groupOps.closeName();
                    void groupOps.run(dialog.workspaceId, dialog.target, op);
                  }}
                  onClose={() => groupOps.closeName()}
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
            <Show when={settingsOpen()} keyed>
              {(at) => (
                <SettingsDialog
                  settings={settings}
                  ai={ai}
                  info={info()}
                  git={git}
                  update={update}
                  at={at}
                  onClose={() => setSettingsOpen(null)}
                />
              )}
            </Show>
            <Show when={palette()} keyed>
              {(open) => (
                <Palette
                  workspace={open.workspace}
                  commands={commands}
                  onClose={closePalette}
                  onSelect={(item) => selectFromPalette(open.workspace, item)}
                />
              )}
            </Show>
            <Show when={updateOpen()}>
              <UpdateDialog
                update={update}
                unsaved={unsavedEverywhere}
                onClose={() => {
                  update.clearError();
                  setUpdateOpen(false);
                }}
              />
            </Show>
            <ConsentDialog ai={ai} />
          </div>
        </Match>
      </Switch>
    </>
  );
}
