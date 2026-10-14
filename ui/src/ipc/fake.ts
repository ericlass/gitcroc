// A fake core for Vitest and Playwright, selected by `__FAKE_CORE__` (see
// vite.config.ts). It implements the generated interface, so a change to the
// Rust commands breaks it at compile time like the real one.
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddReport,
  AskpassClosedEvent,
  AskpassPrompt,
  AppInfo,
  Appearance,
  BranchRequest,
  ChangeRequest,
  ChangesContent,
  ChangesSettings,
  ChangesTarget,
  CommitGraphWrittenEvent,
  CommitRequest,
  DiffRequest,
  FileRef,
  GitStatus,
  JobId,
  JobView,
  PaneLayout,
  RepoId,
  RepoRef,
  SettingsView,
  SummaryView,
  TreeNode,
  WatchWarning,
  WorkspaceId,
  WorkspaceView,
} from "./bindings";
import type { Core } from "./core";
import {
  applyChange,
  defaultDetails,
  defaultFiles,
  diffOf,
  type FakeCommit,
  type FakeFile,
  type FakeWorkingCopy,
  LAST_MESSAGE,
  numbered,
} from "./fakeChanges";
import { defaultHistory, type FakeHistory, type FakeWalk, historyKey, nextPage, parentOf } from "./fakeHistory";
import { applyBranchOp, choicesOf, destructiveOf, fakeNameCheck, labelOf, refusalOf } from "./fakeBranches";
import { fakeJob, jobsScenario } from "./fakeJobs";
import { type FakeRefs, filterTree, refNodes, scenario, skeleton } from "./fakeTree";

const defaultAppInfo: AppInfo = { name: "gitcroc", version: "0.0.0-fake", os: "fake-os", arch: "fake-arch", ui_font: null, code_font: null };
const defaultGitStatus: GitStatus = { kind: "usable", path: "/fake/bin/git", version: "2.47.0" };
export const defaultChangesSettings: ChangesSettings = {
  diff_mode: "inline",
  file_list: "flat",
  ignore_whitespace: false,
  wrap_lines: false,
  context: "three",
};
export const defaultAppearance: Appearance = {
  code_theme_light: "github_light",
  code_theme_dark: "github_dark",
  ui_font: null,
  ui_font_size: 13,
  code_font: null,
  code_font_size: 13,
};
const defaultSettings: SettingsView = {
  settings: { theme: "system", ...defaultAppearance, changes: defaultChangesSettings },
  read_only: false,
  warning: null,
};
export const emptyLayout: PaneLayout = { dock: null, tree: null, history: null, changes: null };
const defaultWorkspace: WorkspaceView = {
  id: "fake-workspace",
  name: "My workspace",
  repos: [],
  layout: emptyLayout,
  read_only: false,
  warning: null,
};

type Listeners<T> = Set<EventCallback<T>>;

function fakeEvent<T>(listeners: Listeners<T>) {
  return {
    listen: (cb: EventCallback<T>): Promise<UnlistenFn> => {
      listeners.add(cb);
      return Promise.resolve(() => listeners.delete(cb));
    },
    once: (cb: EventCallback<T>): Promise<UnlistenFn> => {
      const wrapper: EventCallback<T> = (event) => {
        listeners.delete(wrapper);
        cb(event);
      };
      listeners.add(wrapper);
      return Promise.resolve(() => listeners.delete(wrapper));
    },
  };
}

let nextEventId = 1;
let nextJobId = 1;
const jobProgressListeners: Listeners<JobView> = new Set();
const gitStatusListeners: Listeners<GitStatus> = new Set();
const workspaceChangedListeners: Listeners<WorkspaceView> = new Set();
const workspaceAddReportListeners: Listeners<AddReport> = new Set();
const repoSummaryListeners: Listeners<SummaryView> = new Set();
const repoRefsChangedListeners: Listeners<RepoRef> = new Set();
const commitGraphWrittenListeners: Listeners<CommitGraphWrittenEvent> = new Set();
const watchWarningListeners: Listeners<WatchWarning> = new Set();
const jobEndedListeners: Listeners<JobView> = new Set();
const askpassPromptListeners: Listeners<AskpassPrompt> = new Set();
const askpassClosedListeners: Listeners<AskpassClosedEvent> = new Set();

/** Each workspace's changes view, as the core keeps it. */
const views = new Map<string, { handle: number; lists: Partial<Record<FileRef["list"], FakeFile[]>> }>();
/** The file whose image diff was read last, by workspace. */
const imageFiles = new Map<string, { handle: number; file: FakeFile; ref: FileRef }>();

/** Each workspace's walk, as the core keeps it. */
const walks = new Map<string, FakeWalk & { handle: number }>();
let nextHandle = 1;

/** What the fake returns; tests change it and call `reset` afterwards. */
export const fakeState: {
  appInfo: AppInfo;
  gitStatus: GitStatus;
  settings: SettingsView;
  workspace: WorkspaceView;
  /** What the folder dialog returns; `null` is a cancel. */
  pickedFolder: string | null;
  /** What `workspace_summaries` returns, of any workspace. */
  summaries: SummaryView[];
  /** The last `repo_select` of each workspace. */
  selected: Record<string, RepoId | null>;
  /** What `refs_get` returns, by repo; a repo without is unreadable. */
  refs: Record<RepoId, TreeNode[]>;
  /** How many `refs_get` calls each repo had. */
  refsCalls: Record<RepoId, number>;
  /** Histories by `historyKey`; others get `defaultHistory`. */
  histories: Record<string, FakeHistory>;
  /** How long each history page takes, in ms. */
  historyDelay: number;
  /** Every `history_get` and `history_more`, oldest first. */
  historyCalls: { command: "get" | "more"; text?: string }[];
  /** Repos whose first page offers to write a commit-graph file. */
  slowRepos: Record<RepoId, boolean>;
  /** Repos whose commit-graph offer was declined. */
  declined: Record<RepoId, boolean>;
  /** Repos whose commit-graph writes were started, in order. */
  commitGraphWrites: RepoId[];
  /** Why a commit-graph write fails; `null` succeeds. */
  commitGraphError: string | null;
  /** Commits by hash; others get a header and `defaultFiles`. */
  commits: Record<string, FakeCommit>;
  /** Working copies by repo; a repo without one has none changed. */
  workingCopies: Record<RepoId, FakeWorkingCopy>;
  /** Every `changes_get` and `diff_get`, oldest first. */
  changesCalls: (
    | { command: "changes"; repo: RepoId; target: ChangesTarget }
    | { command: "diff"; file: FileRef; request: DiffRequest }
  )[];
  /** How long each `changes_get` and `diff_get` takes, in ms. */
  changesDelay: number;
  /** Every `workspace_set_layout`, oldest first. */
  layouts: PaneLayout[];
  /** Every `workspace_refresh`, oldest first. */
  refreshes: WorkspaceId[];
  /** What `watch_warning` returns. */
  watchWarning: WatchWarning | null;
  /** This run's jobs, newest first, as `jobs_get` returns them. */
  jobs: JobView[];
  /** Every `job_cancel`, oldest first. */
  cancels: JobId[];
  /** Every `askpass_answer`, oldest first. */
  askpassAnswers: { promptId: number; answer: string | null }[];
  /** Every `changes_apply`, oldest first. */
  changeRequests: ChangeRequest[];
  /** Paths whose hunks answer `file_changed`, as if edited since the diff was read. */
  editedFiles: string[];
  /** Every `changes_commit`, oldest first. */
  commitRequests: CommitRequest[];
  /** What a hook printed when it rejected the commit; `null` lets commits succeed. */
  commitRejection: string | null;
  /** Commit jobs wait for `finishCommit` instead of ending at once. */
  holdCommits: boolean;
  /** The refs `refs_get` is built from, by repo; branch operations change them. */
  refModels: Record<RepoId, FakeRefs>;
  /** Every `op_branch`, oldest first. */
  branchRequests: BranchRequest[];
  /** What git printed when a branch operation failed; `null` lets them succeed. */
  branchFailure: string | null;
} = {
  appInfo: defaultAppInfo,
  gitStatus: defaultGitStatus,
  settings: defaultSettings,
  workspace: defaultWorkspace,
  pickedFolder: null,
  summaries: [],
  selected: {},
  refs: {},
  refsCalls: {},
  histories: {},
  historyDelay: 0,
  historyCalls: [],
  slowRepos: {},
  declined: {},
  commitGraphWrites: [],
  commitGraphError: null,
  commits: {},
  workingCopies: {},
  changesCalls: [],
  changesDelay: 0,
  layouts: [],
  refreshes: [],
  watchWarning: null,
  jobs: [],
  cancels: [],
  askpassAnswers: [],
  changeRequests: [],
  editedFiles: [],
  commitRequests: [],
  commitRejection: null,
  holdCommits: false,
  refModels: {},
  branchRequests: [],
  branchFailure: null,
};

function emit<T>(listeners: Listeners<T>, name: string, payload: T): void {
  const event: Event<T> = { event: name, id: nextEventId++, payload };
  for (const listener of [...listeners]) {
    listener(event);
  }
}

/** Delivers a `job_progress` event to every listener, as the core would. */
export function emitJobProgress(payload: JobView): void {
  emit(jobProgressListeners, "job_progress", payload);
}

/** Delivers a `job_ended` event to every listener, as the core would. */
export function emitJobEnded(payload: JobView): void {
  emit(jobEndedListeners, "job_ended", payload);
}

/** Delivers an `askpass_prompt` event to every listener, as the core would. */
export function emitAskpassPrompt(payload: AskpassPrompt): void {
  emit(askpassPromptListeners, "askpass_prompt", payload);
}

/** Delivers an `askpass_closed` event to every listener, as the core would. */
export function emitAskpassClosed(payload: AskpassClosedEvent): void {
  emit(askpassClosedListeners, "askpass_closed", payload);
}

/** Commit jobs waiting for `finishCommit`, oldest first. */
const heldCommits: (() => void)[] = [];

/** Ends the oldest commit job held by `fakeState.holdCommits`. */
export function finishCommit(): void {
  heldCommits.shift()?.();
}

/** Sends `job`'s end, then the repo's summary as just read, as the core does after a job (OPS-7). */
function endJob(workspaceId: WorkspaceId, repoId: RepoId, job: JobView): void {
  sendJob(job);
  const summary = fakeState.summaries.find((known) => known.repo_id === repoId);
  emitRepoSummary({
    workspace_id: workspaceId,
    repo_id: repoId,
    summary: summary?.summary ?? null,
    stale: false,
    missing: false,
    error: null,
    refreshing: false,
    conflict: false,
    refs_version: summary?.refs_version ?? 0,
    busy: false,
  });
}

/**
 * Runs branch operation `request` on the fake's ref model, like the core:
 * refused for a blocked repo or an invalid request, a confirmation first
 * when destructive, then a job whose end re-reads the repo's refs.
 */
function fakeBranchOp(workspaceId: WorkspaceId, request: BranchRequest) {
  fakeState.branchRequests.push(request);
  const repo = fakeState.workspace.repos.find((known) => known.id === request.repo_id);
  const refs = fakeState.refModels[request.repo_id];
  const summary = fakeState.summaries.find((known) => known.repo_id === request.repo_id);
  const refusal =
    repo === undefined || refs === undefined
      ? "the repo has no branches"
      : summary?.busy
        ? "another operation is running on the repo"
        : summary?.missing
          ? "the repo's folder is missing"
          : summary?.conflict
            ? "the repo is in conflict state; continue or abort first"
            : refusalOf(refs, request.op);
  if (repo === undefined || refs === undefined || refusal !== null) {
    return Promise.resolve({
      status: "error" as const,
      error: { code: "internal" as const, message: refusal ?? "", git_stderr: null },
    });
  }
  const actions = destructiveOf(refs, request.op);
  if (actions.length > 0 && !request.confirmed) {
    return Promise.resolve({ status: "ok" as const, data: { kind: "confirm" as const, repo: repo.name, actions } });
  }
  const jobId = nextJobId++;
  const failure = fakeState.branchFailure;
  setTimeout(() => {
    const after = applyBranchOp(refs, request.op);
    if (failure === null) {
      fakeState.refModels[repo.id] = after.refs;
      fakeState.refs[repo.id] = refNodes(repo.id, after.refs);
      fakeState.summaries = fakeState.summaries.map((known) =>
        known.repo_id === repo.id && known.summary !== null
          ? {
              ...known,
              summary: {
                ...known.summary,
                head:
                  after.refs.current === undefined
                    ? { kind: "detached", hash: known.summary.head.hash ?? "0123456789abcdef0123456789abcdef01234567" }
                    : { kind: "branch", name: after.refs.current, hash: known.summary.head.hash },
              },
              refs_version: known.refs_version + 1,
            }
          : known,
      );
    }
    const job = fakeJob(fakeState.workspace, {
      job_id: jobId,
      workspace_id: workspaceId,
      label: labelOf(request.op),
      repos: [repo.id],
      repo_names: [repo.name],
      state: "ended",
      outcome:
        failure !== null ? { kind: "failed" } : after.nothingToDo ? { kind: "nothing_to_do" } : { kind: "succeeded" },
      output: failure ?? "",
      message: failure === null ? null : "git exited with code 1",
      started_at: Date.now(),
      duration_ms: 5,
    });
    endJob(workspaceId, repo.id, job);
    emitRepoRefsChanged({ workspace_id: workspaceId, repo_id: repo.id });
  }, 0);
  return Promise.resolve({ status: "ok" as const, data: { kind: "started" as const, job_id: jobId } });
}

/** Keeps `view` in the fake's job list and sends it as the core would. */
function sendJob(view: JobView): void {
  const known = fakeState.jobs.findIndex((job) => job.job_id === view.job_id);
  fakeState.jobs = known < 0 ? [view, ...fakeState.jobs] : fakeState.jobs.map((job, at) => (at === known ? view : job));
  if (view.state === "ended") {
    emitJobEnded(view);
  } else {
    emitJobProgress(view);
  }
}

/** Delivers a `git_status` event to every listener, as the core would. */
export function emitGitStatus(payload: GitStatus): void {
  emit(gitStatusListeners, "git_status", payload);
}

/** Delivers a `workspace_changed` event to every listener, as the core would. */
export function emitWorkspaceChanged(payload: WorkspaceView): void {
  emit(workspaceChangedListeners, "workspace_changed", payload);
}

/** Delivers a `repo_summary` event to every listener, as the core would. */
export function emitRepoSummary(payload: SummaryView): void {
  emit(repoSummaryListeners, "repo_summary", payload);
}

/** Delivers a `repo_refs_changed` event to every listener, as the core would. */
export function emitRepoRefsChanged(payload: RepoRef): void {
  emit(repoRefsChangedListeners, "repo_refs_changed", payload);
}

/** Delivers a `watch_warning` event to every listener, as the core would. */
export function emitWatchWarning(payload: WatchWarning): void {
  emit(watchWarningListeners, "watch_warning", payload);
}

export function reset(): void {
  fakeState.appInfo = defaultAppInfo;
  fakeState.gitStatus = defaultGitStatus;
  fakeState.settings = defaultSettings;
  fakeState.workspace = defaultWorkspace;
  fakeState.pickedFolder = null;
  fakeState.summaries = [];
  fakeState.selected = {};
  fakeState.refs = {};
  fakeState.refsCalls = {};
  fakeState.histories = {};
  fakeState.historyDelay = 0;
  fakeState.historyCalls = [];
  fakeState.slowRepos = {};
  fakeState.declined = {};
  fakeState.commitGraphWrites = [];
  fakeState.commitGraphError = null;
  fakeState.commits = {};
  fakeState.workingCopies = {};
  fakeState.changesCalls = [];
  fakeState.changeRequests = [];
  fakeState.editedFiles = [];
  fakeState.commitRequests = [];
  fakeState.commitRejection = null;
  fakeState.holdCommits = false;
  heldCommits.length = 0;
  fakeState.refModels = {};
  fakeState.branchRequests = [];
  fakeState.branchFailure = null;
  fakeState.changesDelay = 0;
  fakeState.layouts = [];
  fakeState.refreshes = [];
  fakeState.watchWarning = null;
  fakeState.jobs = [];
  fakeState.cancels = [];
  fakeState.askpassAnswers = [];
  walks.clear();
  views.clear();
  imageFiles.clear();
  jobProgressListeners.clear();
  gitStatusListeners.clear();
  workspaceChangedListeners.clear();
  workspaceAddReportListeners.clear();
  repoSummaryListeners.clear();
  repoRefsChangedListeners.clear();
  commitGraphWrittenListeners.clear();
  watchWarningListeners.clear();
  jobEndedListeners.clear();
  askpassPromptListeners.clear();
  askpassClosedListeners.clear();
}

// Playwright runs the fake build; `?fake=<name>` picks a data set.
const picked = typeof location === "undefined" ? null : new URLSearchParams(location.search).get("fake");
// `jobs` is the `tree` data set with this run's jobs (UI-21).
const data = picked === null ? null : scenario(picked === "jobs" ? "tree" : picked, defaultWorkspace.id);
if (picked === "jobs") {
  fakeState.jobs = jobsScenario(defaultWorkspace);
  nextJobId = 100;
}
if (data) {
  fakeState.workspace = { ...defaultWorkspace, repos: data.repos };
  fakeState.refs = data.refs;
  fakeState.summaries = data.summaries;
  fakeState.pickedFolder = data.pickedFolder ?? null;
  fakeState.histories = data.histories ?? {};
  fakeState.historyDelay = data.historyDelay ?? 0;
  fakeState.slowRepos = data.slowRepos ?? {};
  fakeState.commits = data.commits ?? {};
  fakeState.workingCopies = data.workingCopies ?? {};
  fakeState.refModels = data.refModels ?? {};
}

/** Resolves after the fake's paging delay, as a slow disk would. */
const later = <T>(value: T): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value), fakeState.historyDelay));

/** Resolves after the fake's changes delay. */
const slowly = <T>(value: () => T): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value()), fakeState.changesDelay));

/** The commit view of `hash` against parent `parent`, from the fake state or the defaults. */
function commitContent(hash: string, parent: number): { content: ChangesContent; files: FakeFile[] } {
  const known = fakeState.commits[hash];
  const parentHash = parentOf(hash);
  const details = known?.details ?? defaultDetails(hash, parentHash === null ? [] : [parentHash]);
  const files = numbered(known?.files[parent] ?? defaultFiles());
  return {
    content: { kind: "commit", details, parent, files: files.map((file, id) => ({ id, ...file.file })) },
    files,
  };
}

const notFound = (message: string) =>
  Promise.resolve({ status: "error" as const, error: { code: "not_found" as const, message, git_stderr: null } });

/**
 * Like the core's add job, for one repo at `path`: progress, then the fresh
 * workspace, then the report. A path already in the workspace is reported
 * as present.
 */
function fakeAdd(workspaceId: string, path: string): number {
  const jobId = nextJobId++;
  const job = fakeJob(fakeState.workspace, {
    job_id: jobId,
    workspace_id: workspaceId,
    label: `Add repos from ${path}`,
    repos: [],
    repo_names: [],
    state: "running",
    outcome: null,
    started_at: Date.now(),
    duration_ms: null,
  });
  setTimeout(() => {
    sendJob({ ...job, total: 2 });
    const present = fakeState.workspace.repos.filter((repo) => repo.path === path);
    const added =
      present.length > 0
        ? []
        : [
            {
              id: `r${fakeState.workspace.repos.length + 1}`,
              name: path.split("/").filter(Boolean).pop() ?? path,
              path,
              remote: null,
            },
          ];
    fakeState.workspace = { ...fakeState.workspace, repos: [...fakeState.workspace.repos, ...added] };
    sendJob({ ...job, done: 2, total: 2 });
    emitWorkspaceChanged(fakeState.workspace);
    sendJob({
      ...job,
      done: 2,
      total: 2,
      state: "ended",
      outcome: added.length > 0 ? { kind: "succeeded" } : { kind: "nothing_to_do" },
      output: added.length > 0 ? `added ${path}\n` : `already present ${path}\n`,
      duration_ms: 10,
    });
    emit(workspaceAddReportListeners, "workspace_add_report", {
      workspace_id: workspaceId,
      job_id: jobId,
      added,
      already_present: present,
      refused: [],
    });
  }, 10);
  return jobId;
}

export const fake: Core = {
  commands: {
    appInfo: () => Promise.resolve({ status: "ok", data: fakeState.appInfo }),
    gitStatus: () => Promise.resolve({ status: "ok", data: fakeState.gitStatus }),
    // Like the core: `checking` at once, then the result as a later event.
    gitRecheck: () => {
      emitGitStatus({ kind: "checking" });
      setTimeout(() => emitGitStatus(fakeState.gitStatus), 10);
      return Promise.resolve({ status: "ok", data: null });
    },
    settingsGet: () => Promise.resolve({ status: "ok", data: fakeState.settings }),
    // Like the core: read-only settings refuse every change.
    settingsSetTheme: (theme) => {
      if (fakeState.settings.read_only) {
        return Promise.resolve({
          status: "error",
          error: {
            code: "settings_read_only",
            message: "the settings are read-only, so the change was not saved",
            git_stderr: null,
          },
        });
      }
      fakeState.settings = { ...fakeState.settings, settings: { ...fakeState.settings.settings, theme } };
      return Promise.resolve({ status: "ok", data: fakeState.settings });
    },
    workspaceGet: () => Promise.resolve({ status: "ok", data: [fakeState.workspace] }),
    workspaceAddPath: (workspaceId, path) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      return Promise.resolve({ status: "ok", data: fakeAdd(workspaceId, path) });
    },
    workspacePickAndAdd: (workspaceId) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      if (fakeState.pickedFolder !== null) {
        fakeAdd(workspaceId, fakeState.pickedFolder);
      }
      return Promise.resolve({ status: "ok", data: null });
    },
    workspaceRemoveRepo: (workspaceId, repoId) => {
      const workspace = fakeState.workspace;
      if (workspaceId !== workspace.id || !workspace.repos.some((repo) => repo.id === repoId)) {
        return notFound(`the workspace has no repo with the id ${repoId}`);
      }
      fakeState.workspace = { ...workspace, repos: workspace.repos.filter((repo) => repo.id !== repoId) };
      emitWorkspaceChanged(fakeState.workspace);
      return Promise.resolve({ status: "ok", data: fakeState.workspace });
    },
    workspaceSummaries: (workspaceId) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      const summaries = fakeState.summaries.filter((view) => view.workspace_id === workspaceId);
      return Promise.resolve({ status: "ok", data: summaries });
    },
    repoSelect: (workspaceId, repoId) => {
      fakeState.selected[workspaceId] = repoId;
      return Promise.resolve({ status: "ok", data: null });
    },
    workspaceRefresh: (workspaceId) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      fakeState.refreshes.push(workspaceId);
      // Like the core: every repo read again, the selected one first.
      const selected = fakeState.selected[workspaceId] ?? null;
      const views = fakeState.summaries.filter((view) => view.workspace_id === workspaceId);
      const ordered = [
        ...views.filter((view) => view.repo_id === selected),
        ...views.filter((view) => view.repo_id !== selected),
      ];
      setTimeout(() => ordered.forEach(emitRepoSummary), 0);
      return Promise.resolve({ status: "ok", data: null });
    },
    watchWarning: (workspaceId) =>
      Promise.resolve({
        status: "ok",
        data: fakeState.watchWarning?.workspace_id === workspaceId ? fakeState.watchWarning : null,
      }),
    treeGet: (workspaceId) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      return Promise.resolve({ status: "ok", data: skeleton(fakeState.workspace) });
    },
    refsGet: (workspaceId, repoId) => {
      if (workspaceId !== fakeState.workspace.id || !fakeState.workspace.repos.some((repo) => repo.id === repoId)) {
        return notFound(`the workspace has no repo with the id ${repoId}`);
      }
      fakeState.refsCalls[repoId] = (fakeState.refsCalls[repoId] ?? 0) + 1;
      const refs = fakeState.refs[repoId];
      if (!refs) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: `cannot read the refs of ${repoId}`, git_stderr: null },
        });
      }
      return Promise.resolve({ status: "ok", data: refs });
    },
    treeFilter: (workspaceId, text) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      return Promise.resolve({ status: "ok", data: filterTree(fakeState.workspace, fakeState.refs, text) });
    },
    historyGet: (workspaceId, repoId, target, text) => {
      const repo = fakeState.workspace.repos.find((candidate) => candidate.id === repoId);
      if (workspaceId !== fakeState.workspace.id || !repo) {
        return notFound(`the workspace has no repo with the id ${repoId}`);
      }
      fakeState.historyCalls.push({ command: "get", text });
      const handle = nextHandle++;
      const history = fakeState.histories[historyKey(repoId, target)] ?? defaultHistory(repo.name, target);
      const walk = { handle, history, text, next: 0 };
      walks.set(workspaceId, walk);
      const page = nextPage(walk);
      const offer = !!fakeState.slowRepos[repoId] && !fakeState.declined[repoId];
      // Like the core: `null` once a newer walk of the workspace began.
      return later({
        status: "ok" as const,
        data: walks.get(workspaceId)?.handle === handle ? { handle, ...page, offer_commit_graph: offer } : null,
      });
    },
    historyMore: (workspaceId, handle) => {
      fakeState.historyCalls.push({ command: "more" });
      const walk = walks.get(workspaceId);
      if (walk?.handle !== handle) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const page = nextPage(walk);
      return later({
        status: "ok" as const,
        data: walks.get(workspaceId)?.handle === handle ? { handle, ...page, offer_commit_graph: false } : null,
      });
    },
    commitGraphWrite: (workspaceId, repoId) => {
      const jobId = nextJobId++;
      fakeState.commitGraphWrites.push(repoId);
      const name = fakeState.workspace.repos.find((repo) => repo.id === repoId)?.name ?? repoId;
      const job = fakeJob(fakeState.workspace, {
        job_id: jobId,
        workspace_id: workspaceId,
        label: "Writing the commit-graph file",
        repos: [repoId],
        repo_names: [name],
        state: "running",
        outcome: null,
        started_at: Date.now(),
        duration_ms: null,
      });
      setTimeout(() => {
        sendJob(job);
        const error = fakeState.commitGraphError;
        if (error === null) {
          fakeState.slowRepos[repoId] = false;
        }
        sendJob({
          ...job,
          state: "ended",
          outcome: error === null ? { kind: "succeeded" } : { kind: "failed" },
          output: error === null ? "Computing commit graph generation numbers: 100% (3/3), done.\n" : "",
          message: error,
          duration_ms: 10,
        });
        emit(commitGraphWrittenListeners, "commit_graph_written", {
          workspace_id: workspaceId,
          repo_id: repoId,
          job_id: jobId,
          error,
          git_stderr: null,
        });
      }, 10);
      return Promise.resolve({ status: "ok", data: jobId });
    },
    // Like the core: read-only settings refuse every change.
    settingsSetChanges: (changes) => {
      if (fakeState.settings.read_only) {
        return Promise.resolve({
          status: "error",
          error: {
            code: "settings_read_only",
            message: "the settings are read-only, so the change was not saved",
            git_stderr: null,
          },
        });
      }
      fakeState.settings = { ...fakeState.settings, settings: { ...fakeState.settings.settings, changes } };
      return Promise.resolve({ status: "ok", data: fakeState.settings });
    },
    // Like the core: sizes limited to 8 to 32, a blank font the system font.
    settingsSetAppearance: (appearance) => {
      if (fakeState.settings.read_only) {
        return Promise.resolve({
          status: "error",
          error: {
            code: "settings_read_only",
            message: "the settings are read-only, so the change was not saved",
            git_stderr: null,
          },
        });
      }
      const size = (value: number) => Math.min(32, Math.max(8, value));
      const font = (name: string | null) => (name?.trim() ? name.trim() : null);
      fakeState.settings = {
        ...fakeState.settings,
        settings: {
          ...fakeState.settings.settings,
          ...appearance,
          ui_font: font(appearance.ui_font),
          ui_font_size: size(appearance.ui_font_size),
          code_font: font(appearance.code_font),
          code_font_size: size(appearance.code_font_size),
        },
      };
      return Promise.resolve({ status: "ok", data: fakeState.settings });
    },
    workspaceSetLayout: (workspaceId, layout) => {
      if (workspaceId !== fakeState.workspace.id) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      fakeState.layouts.push(layout);
      fakeState.workspace = { ...fakeState.workspace, layout };
      return Promise.resolve({ status: "ok", data: null });
    },
    changesGet: (workspaceId, repoId, target) => {
      if (workspaceId !== fakeState.workspace.id || !fakeState.workspace.repos.some((repo) => repo.id === repoId)) {
        return notFound(`the workspace has no repo with the id ${repoId}`);
      }
      fakeState.changesCalls.push({ command: "changes", repo: repoId, target });
      const handle = nextHandle++;
      let content: ChangesContent;
      let lists: Partial<Record<FileRef["list"], FakeFile[]>>;
      if (target.kind === "commit") {
        const commit = commitContent(target.hash, target.parent);
        content = commit.content;
        lists = { commit: commit.files };
      } else {
        const working = fakeState.workingCopies[repoId] ?? { staged: [], unstaged: [], unmerged: [] };
        const staged = numbered(working.staged);
        const unstaged = numbered(working.unstaged);
        content = {
          kind: "uncommitted",
          staged: staged.map((file, id) => ({ id, ...file.file })),
          unstaged: unstaged.map((file, id) => ({ id, ...file.file })),
          unmerged: working.unmerged.map((file, id) => ({ id, ...file })),
          last_message: working.lastMessage === undefined ? LAST_MESSAGE : working.lastMessage,
          in_conflict: working.inConflict ?? false,
        };
        lists = { staged, unstaged };
      }
      views.set(workspaceId, { handle, lists });
      imageFiles.delete(workspaceId);
      // Like the core: `null` once a newer view of the workspace began.
      return slowly(() => ({
        status: "ok" as const,
        data: views.get(workspaceId)?.handle === handle ? { handle, content } : null,
      }));
    },
    diffGet: (workspaceId, handle, file, request) => {
      fakeState.changesCalls.push({ command: "diff", file, request });
      const view = views.get(workspaceId);
      if (view?.handle !== handle) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const found = view.lists[file.list]?.[file.id];
      if (!found) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: `the list has no file ${file.id}`, git_stderr: null },
        });
      }
      const diff = diffOf(found, request);
      if (diff.kind === "image") {
        imageFiles.set(workspaceId, { handle, file: found, ref: file });
      } else {
        imageFiles.delete(workspaceId);
      }
      return slowly(() => ({
        status: "ok" as const,
        data: views.get(workspaceId)?.handle === handle ? diff : null,
      }));
    },
    // Like the core: a discard asks for confirmation first, then a job
    // changes the working copy and the repo's summary is read again.
    changesApply: (workspaceId, request) => {
      fakeState.changeRequests.push(request);
      const view = views.get(workspaceId);
      if (view?.handle !== request.handle) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const target = request.target;
      const refs = target.kind === "files" ? target.files : [target.file];
      const files = refs.flatMap((ref) => view.lists[ref.list]?.[ref.id] ?? []);
      const repoId = fakeState.selected[workspaceId] ?? fakeState.workspace.repos[0]?.id;
      const repo = fakeState.workspace.repos.find((known) => known.id === repoId);
      if (files.length === 0 || repo === undefined) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: "the list has no such file", git_stderr: null },
        });
      }
      if (target.kind === "hunk" && files.some((file) => fakeState.editedFiles.includes(file.file.path))) {
        return Promise.resolve({ status: "ok", data: { kind: "file_changed" } });
      }
      if (request.action === "discard" && !request.confirmed) {
        return Promise.resolve({
          status: "ok",
          data: { kind: "confirm", repo: repo.name, files: files.map((file) => file.file.path) },
        });
      }
      const jobId = nextJobId++;
      const partial =
        target.kind === "hunk" &&
        files.some((file) => {
          const diff = diffOf(file, { ignore_whitespace: false, context: "three", load_anyway: true, full_text: false });
          return diff.kind === "text" && diff.hunks.length > 1;
        });
      const working = fakeState.workingCopies[repo.id] ?? { staged: [], unstaged: [], unmerged: [] };
      fakeState.workingCopies[repo.id] = applyChange(working, request.action, files, partial);
      const label = `${request.action[0]?.toUpperCase()}${request.action.slice(1)} ${files.map((file) => file.file.path).join(", ")}`;
      const job = fakeJob(fakeState.workspace, {
        job_id: jobId,
        workspace_id: workspaceId,
        label,
        repos: [repo.id],
        repo_names: [repo.name],
        state: "ended",
        outcome: { kind: "succeeded" },
        started_at: Date.now(),
        duration_ms: 5,
      });
      setTimeout(() => endJob(workspaceId, repo.id, job), 0);
      return Promise.resolve({ status: "ok", data: { kind: "started", job_id: jobId } });
    },
    // Like the core: refused without a subject, in conflict state, with
    // nothing staged or nothing to amend; otherwise a job commits the
    // staged files, or fails with what a rejecting hook printed.
    changesCommit: (workspaceId, request) => {
      fakeState.commitRequests.push(request);
      const view = views.get(workspaceId);
      if (view?.handle !== request.handle) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const repoId = fakeState.selected[workspaceId] ?? fakeState.workspace.repos[0]?.id;
      const repo = fakeState.workspace.repos.find((known) => known.id === repoId);
      const working = (repo && fakeState.workingCopies[repo.id]) ?? { staged: [], unstaged: [], unmerged: [] };
      const refusal =
        repo === undefined
          ? "only uncommitted changes are committed"
          : request.message.subject.trim() === ""
            ? "the commit message has no subject"
            : working.inConflict
              ? "the repo is in conflict state; Continue makes the commit"
              : request.amend && working.lastMessage === null
                ? "the branch has no commit to amend"
                : !request.amend && working.staged.length === 0
                  ? "nothing is staged"
                  : null;
      if (repo === undefined || refusal !== null) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: refusal ?? "", git_stderr: null },
        });
      }
      const jobId = nextJobId++;
      const rejection = fakeState.commitRejection;
      const end = () => {
        if (rejection === null) {
          const now = fakeState.workingCopies[repo.id] ?? working;
          fakeState.workingCopies[repo.id] = { ...now, staged: [], lastMessage: request.message };
        }
        const job = fakeJob(fakeState.workspace, {
          job_id: jobId,
          workspace_id: workspaceId,
          label: request.amend ? "Amend the last commit" : "Commit",
          repos: [repo.id],
          repo_names: [repo.name],
          state: "ended",
          outcome: rejection === null ? { kind: "succeeded" } : { kind: "failed" },
          output: rejection ?? "",
          message: rejection === null ? null : "git exited with code 1",
          started_at: Date.now(),
          duration_ms: 5,
        });
        endJob(workspaceId, repo.id, job);
      };
      if (fakeState.holdCommits) {
        heldCommits.push(end);
      } else {
        setTimeout(end, 0);
      }
      return Promise.resolve({ status: "ok", data: jobId });
    },
    opBranch: (workspaceId, request) => fakeBranchOp(workspaceId, request),
    branchChoices: (_workspaceId, repoId) => {
      const refs = fakeState.refModels[repoId];
      return refs === undefined
        ? notFound(`no repo ${repoId}`)
        : Promise.resolve({ status: "ok" as const, data: choicesOf(refs) });
    },
    branchNameCheck: (name) => Promise.resolve({ status: "ok", data: fakeNameCheck(name) }),
    diffImage: (workspaceId, handle, file, side) => {
      const last = imageFiles.get(workspaceId);
      const same = last?.handle === handle && last.ref.list === file.list && last.ref.id === file.id;
      return Promise.resolve({ status: "ok", data: (same && last.file.images?.[side]) || null });
    },
    commitGraphDecline: (_workspaceId, repoId) => {
      fakeState.declined[repoId] = true;
      return Promise.resolve({ status: "ok", data: null });
    },
    askpassAnswer: (promptId, answer) => {
      fakeState.askpassAnswers.push({ promptId, answer });
      return Promise.resolve({ status: "ok", data: null });
    },
    jobsGet: () => Promise.resolve({ status: "ok", data: fakeState.jobs }),
    // Like the core: a queued job never runs; a running one finishes
    // (GRP-10). The fake's jobs do not move on by themselves.
    jobCancel: (jobId) => {
      fakeState.cancels.push(jobId);
      const job = fakeState.jobs.find((known) => known.job_id === jobId);
      if (job?.state === "queued") {
        sendJob({ ...job, state: "ended", outcome: { kind: "skipped", reason: "cancelled" }, message: "cancelled" });
        return Promise.resolve({ status: "ok", data: "cancelled" });
      }
      return Promise.resolve({ status: "ok", data: job?.state === "running" ? "finishing" : "ended" });
    },
    // Only `just bench-ui` builds measure; the fake has nowhere to write.
    perfTraceWrite: () =>
      Promise.resolve({
        status: "error",
        error: { code: "internal", message: "the fake core has no perf-trace", git_stderr: null },
      }),
  },
  events: {
    askpassClosed: fakeEvent(askpassClosedListeners),
    askpassPrompt: fakeEvent(askpassPromptListeners),
    commitGraphWritten: fakeEvent(commitGraphWrittenListeners),
    gitStatus: fakeEvent(gitStatusListeners),
    jobEnded: fakeEvent(jobEndedListeners),
    jobProgress: fakeEvent(jobProgressListeners),
    repoRefsChanged: fakeEvent(repoRefsChangedListeners),
    repoSummary: fakeEvent(repoSummaryListeners),
    watchWarning: fakeEvent(watchWarningListeners),
    workspaceAddReport: fakeEvent(workspaceAddReportListeners),
    workspaceChanged: fakeEvent(workspaceChangedListeners),
  },
};
