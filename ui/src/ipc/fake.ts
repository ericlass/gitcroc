// A fake core for Vitest and Playwright, selected by `__FAKE_CORE__` (see
// vite.config.ts). It implements the generated interface, so a change to the
// Rust commands breaks it at compile time like the real one.
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddReport,
  AiEligibility,
  AiModelsReply,
  AiProposalGet,
  AiFileProgress,
  AiFileState,
  AiProposalReady,
  AiResolveAllRequest,
  AiResolveAllStart,
  AiResolveRequest,
  AiResolveStart,
  AiSelfTestReply,
  AiView,
  AskpassClosedEvent,
  AskpassPrompt,
  AppInfo,
  Appearance,
  BranchRequest,
  GroupSettings,
  PreflightReady,
  ChangeRequest,
  ChangesContent,
  ChangesSettings,
  ChangesTarget,
  ChooseRequest,
  CloneParentPickedEvent,
  CloneReport,
  CloneRequest,
  ExportReport,
  MissingRepo,
  WorkspaceExportedEvent,
  WorkspaceImportedEvent,
  CommitGraphWrittenEvent,
  CommitRequest,
  Conflict,
  ConflictDetails,
  DiffRequest,
  FileRef,
  GitStatus,
  GroupId,
  GroupSummary,
  GroupView,
  HistoryRow,
  HistoryTarget,
  IntegrateRequest,
  IpcError,
  JobId,
  JobView,
  Outcome,
  PaneLayout,
  RemoteRequest,
  RemoteStart,
  RepoChanges,
  RepoId,
  RepoRef,
  ResetRequest,
  ResetStart,
  ResolveRequest,
  SettingsView,
  StashRequest,
  SummaryView,
  SyncRequest,
  TagRequest,
  TargetSet,
  TreeNode,
  UpdateView,
  WatchWarning,
  WorkspaceId,
  WorkspaceTabs,
  WorkspaceView,
} from "./bindings";
import type { Core } from "./core";
import { fakeUpdate, offered, offerUpdate, resetUpdate, updateCommands } from "./fakeUpdate";
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
import {
  commitAt,
  defaultHistory,
  type FakeHistory,
  historyKey,
  mergedWalk,
  nextMergedPage,
  nextPage,
  parentOf,
} from "./fakeHistory";
import { applyBranchOp, choicesOf, destructiveOf, fakeNameCheck, labelOf, refusalOf } from "./fakeBranches";
import { type FakeFolder, fakeDefaultName, fakeTargetRefusal } from "./fakeClone";
import { fakeAi, fakeAiCommands, fakeAiRefusal, resetFakeAi } from "./fakeAi";
import { labelsOf, planResolution, textConflict } from "./fakeConflict";
import { mergeTargetsOf, planIntegrate } from "./fakeIntegrate";
import { fakeRemoteNameCheck, planRemote } from "./fakeRemotes";
import { planReset, planTag, type TagPlan } from "./fakeTags";
import { fakeDrop, fakeGroups, fakePreflight, fakeRetry, fakeStart, type GroupDeps, resetGroups } from "./fakeGroups";
import { fakeJob, jobsScenario } from "./fakeJobs";
import { fakePaletteQuery } from "./fakePalette";
import { applySync, planSync, pushChoicesOf } from "./fakeSync";
import {
  type FakeRefs,
  filterTree,
  groupBranchNodes,
  groupSummary,
  inPlace,
  refNodes,
  scenario,
  skeleton,
  stashHash,
} from "./fakeTree";

const defaultAppInfo: AppInfo = { name: "gitcroc", version: "0.0.0-fake", os: "fake-os", arch: "fake-arch", ui_font: null, code_font: null };
const defaultGitStatus: GitStatus = {
  kind: "usable",
  path: "/fake/bin/git",
  version: "2.47.0",
  chosen: null,
  chosen_missing: false,
};
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
export const defaultGroupSettings: GroupSettings = { group_parallelism: 8, always_show_preflight: false };
const defaultSettings: SettingsView = {
  settings: { theme: "system", ...defaultAppearance, ...defaultGroupSettings, changes: defaultChangesSettings },
  update_check: true,
  read_only: false,
  warning: null,
};
export const emptyLayout: PaneLayout = { dock: null, tree: null, history: null, changes: null };
const defaultWorkspace: WorkspaceView = {
  id: "fake-workspace",
  name: "My workspace",
  repos: [],
  groups: [],
  layout: emptyLayout,
  ai: true,
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
const updateStatusListeners: Listeners<UpdateView> = new Set();
const workspaceChangedListeners: Listeners<WorkspaceView> = new Set();
const groupSummaryListeners: Listeners<GroupSummary> = new Set();
const workspacesChangedListeners: Listeners<WorkspaceTabs> = new Set();
const workspaceAddReportListeners: Listeners<AddReport> = new Set();
const repoSummaryListeners: Listeners<SummaryView> = new Set();
const repoRefsChangedListeners: Listeners<RepoRef> = new Set();
const commitGraphWrittenListeners: Listeners<CommitGraphWrittenEvent> = new Set();
const watchWarningListeners: Listeners<WatchWarning> = new Set();
const jobEndedListeners: Listeners<JobView> = new Set();
const askpassPromptListeners: Listeners<AskpassPrompt> = new Set();
const askpassClosedListeners: Listeners<AskpassClosedEvent> = new Set();
const cloneParentPickedListeners: Listeners<CloneParentPickedEvent> = new Set();
const cloneReportListeners: Listeners<CloneReport> = new Set();
const workspaceExportedListeners: Listeners<WorkspaceExportedEvent> = new Set();
const workspaceImportedListeners: Listeners<WorkspaceImportedEvent> = new Set();
const preflightReadyListeners: Listeners<PreflightReady> = new Set();
const aiChangedListeners: Listeners<AiView> = new Set();
const aiModelsListeners: Listeners<AiModelsReply> = new Set();
const aiSelfTestListeners: Listeners<AiSelfTestReply> = new Set();
const aiProposalReadyListeners: Listeners<AiProposalReady> = new Set();

/** What `ai_resolve` answers until a test sets it: the core's answer without a provider (AI-3). */
/** The AI commands, which send their events as the core does. */
const aiCommands = fakeAiCommands({
  changed: (view) => emit(aiChangedListeners, "ai_changed", view),
  models: (requestId, result) => emit(aiModelsListeners, "ai_models", { request_id: requestId, result }),
  selfTest: (requestId, result) => emit(aiSelfTestListeners, "ai_self_test", { request_id: requestId, result }),
});

/** A repo's part of a changes view: all of a single repo's view, one repo of a merged one. */
interface FakeViewPart {
  handle: number;
  lists: Partial<Record<FileRef["list"], FakeFile[]>>;
  /** The unmerged paths by id, for the conflict calls. */
  unmerged: string[];
  /** The repo the part is of. */
  repo: RepoId;
}

/** Each workspace's changes view, as the core keeps it: one part per repo. */
const views = new Map<string, FakeViewPart[]>();

/** The part of workspace `workspaceId`'s view that `handle` names, while that view is the latest. */
const viewPart = (workspaceId: string, handle: number) =>
  views.get(workspaceId)?.find((part) => part.handle === handle);
/** The file whose image diff was read last, by workspace. */
const imageFiles = new Map<string, { handle: number; file: FakeFile; ref: FileRef }>();

/** Each workspace's walk, as the core keeps it: what reads its next page. */
const walks = new Map<string, { handle: number; repos: RepoId[]; next: () => { rows: HistoryRow[]; done: boolean } }>();
let nextHandle = 1;

/**
 * Like the core when a tab goes to the background (UI-31): the workspace's
 * walk and changes view end, so their handles give `null`.
 */
function releaseWorkspace(workspaceId: WorkspaceId): void {
  walks.delete(workspaceId);
  views.delete(workspaceId);
  imageFiles.delete(workspaceId);
}

/** Whether the fake core holds a walk or a changes view of the workspace. */
export function holdsViews(workspaceId: WorkspaceId): { walk: boolean; changes: boolean } {
  return { walk: walks.has(workspaceId), changes: views.has(workspaceId) };
}

/** What the fake returns; tests change it and call `reset` afterwards. */
export const fakeState: {
  appInfo: AppInfo;
  gitStatus: GitStatus;
  settings: SettingsView;
  /** The first workspace file; the repo and job fakes act on it. */
  workspace: WorkspaceView;
  /** More workspace files, after `workspace` (UI-27). */
  others: WorkspaceView[];
  /** The open tabs by workspace id, in order. */
  tabs: WorkspaceId[];
  /** The active tab; `null` when none is open. */
  active: WorkspaceId | null;
  /** What the folder dialog returns; `null` is a cancel. */
  pickedFolder: string | null;
  /** The git path chosen in Settings; `null` is automatic detection, which finds `gitStatus` (PLAT-3). */
  gitPath: string | null;
  /** The paths that hold a git, with the version each reports. */
  gits: Record<string, string>;
  /** What the git file dialog returns; `null` is a cancel. */
  pickedGit: string | null;
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
    | { command: "merged"; target: TargetSet }
    | { command: "diff"; file: FileRef; request: DiffRequest }
  )[];
  /** How long each `changes_get` and `diff_get` takes, in ms. */
  changesDelay: number;
  /**
   * `changes_get` calls wait in `heldChanges` instead of starting, so a test can start them in another order than
   * they were sent, as the real core's concurrent commands may.
   */
  holdChanges: boolean;
  /** The held `changes_get` calls, oldest first; calling one starts it. */
  heldChanges: (() => void)[];
  /** `palette_query` calls wait in `heldPalette` instead of answering, so a test can answer them in another order. */
  holdPalette: boolean;
  /** The held `palette_query` calls, oldest first; calling one answers it, `true` as superseded. */
  heldPalette: ((superseded: boolean) => void)[];
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
  /** Every `op_sync`, oldest first. */
  syncRequests: SyncRequest[];
  /** Sync jobs stay running, at their first step, until `finishSync`. */
  holdSync: boolean;
  /** Every `op_integrate`, oldest first. */
  integrateRequests: IntegrateRequest[];
  /** Repos in conflict state, as `conflict_details` reads them; set with `setConflict`. */
  conflicts: Record<RepoId, ConflictDetails>;
  /** The conflict the next integrate job stops in; `null` lets them succeed. */
  integrateConflict: ConflictDetails | null;
  /** Every `op_stash`, oldest first. */
  stashRequests: StashRequest[];
  /** The conflict the next apply or pop stops in; `null` lets them succeed. */
  stashConflict: ConflictDetails | null;
  /** Every `op_tag`, oldest first. */
  tagRequests: TagRequest[];
  /** Every `op_reset`, oldest first. */
  resetRequests: ResetRequest[];
  /** Every `op_remote`, oldest first. */
  remoteRequests: RemoteRequest[];
  /** The keys each repo's working copy offers to turn on (PERF-7); others get no offer. */
  fsmonitorOffers: Record<RepoId, string[]>;
  /** The answers to the fsmonitor offer, oldest first. */
  fsmonitorAnswers: { repo: RepoId; answer: "enable" | "decline" }[];
  /** What `conflict_get` answers by path; others get `textConflict`. */
  fileConflicts: Record<string, Conflict>;
  /** What `ai_eligibility` answers by path; others are eligible. */
  aiEligibility: Record<string, AiEligibility>;
  /** What `ai_resolve` answers for a file in conflict; `null` checks like the core, then starts a job. */
  aiResolve: AiResolveStart | null;
  /** Every `ai_resolve`, oldest first. */
  aiResolveRequests: AiResolveRequest[];
  /** What `ai_proposal_get` answers by path; others have none. A proposal job keeps its result here. */
  aiProposals: Record<string, AiProposalGet>;
  /** What a proposal job ends with, by path; others fail. */
  aiAnswers: Record<string, AiProposalGet>;
  /** Every `ai_resolve_all`, oldest first. */
  aiResolveAllRequests: AiResolveAllRequest[];
  /** Proposal jobs and Resolve all runs keep running until `finishAiJobs`. */
  holdAi: boolean;
  /** Every `conflict_resolve` and `conflict_choose`, oldest first. */
  conflictRequests: (ResolveRequest | ChooseRequest)[];
  /** What the clone's folder dialog returns; `null` is a cancel. */
  cloneParent: string | null;
  /** What is on disk by path, for the clone's target checks; others do not exist. */
  folders: Record<string, FakeFolder>;
  /** Every `clone_start` the core started, oldest first. */
  cloneRequests: CloneRequest[];
  /** What git printed when a clone failed; `null` lets clones succeed. */
  cloneFailure: string | null;
  /** What `workspace_missing_repos` lists, for every workspace; a clone that fills one in takes it off. */
  missingRepos: MissingRepo[];
  /** What the export's save dialog leads to: a report, an error, or `null` for a cancel. */
  exportAnswer: ExportReport | { error: string } | null;
  /** Every workspace `workspace_export` was asked for, oldest first. */
  exports: WorkspaceId[];
  /** How many times `workspace_import` was called. */
  imports: number;
  /** What the import's open dialog leads to: a workspace with its missing repos, an error, or `null` for a cancel. */
  importAnswer: { workspace: WorkspaceView; missing: RepoId[] } | { error: string } | null;
  /** Every repo opened in the file manager or a terminal, oldest first. */
  opened: { how: "file_manager" | "terminal"; repo: RepoId }[];
  /** Why opening fails; `null` lets it succeed. */
  openFailure: string | null;
} = {
  appInfo: defaultAppInfo,
  gitStatus: defaultGitStatus,
  settings: defaultSettings,
  workspace: defaultWorkspace,
  others: [],
  tabs: [defaultWorkspace.id],
  active: defaultWorkspace.id,
  pickedFolder: null,
  gitPath: null,
  gits: {},
  pickedGit: null,
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
  holdChanges: false,
  heldChanges: [],
  holdPalette: false,
  heldPalette: [],
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
  syncRequests: [],
  holdSync: false,
  integrateRequests: [],
  conflicts: {},
  integrateConflict: null,
  stashRequests: [],
  stashConflict: null,
  tagRequests: [],
  resetRequests: [],
  remoteRequests: [],
  fsmonitorOffers: {},
  fsmonitorAnswers: [],
  fileConflicts: {},
  aiEligibility: {},
  aiResolve: null,
  aiResolveRequests: [],
  aiProposals: {},
  aiAnswers: {},
  aiResolveAllRequests: [],
  holdAi: false,
  conflictRequests: [],
  cloneParent: null,
  folders: {},
  cloneRequests: [],
  cloneFailure: null,
  missingRepos: [],
  exportAnswer: null,
  exports: [],
  imports: 0,
  importAnswer: null,
  opened: [],
  openFailure: null,
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
    conflict: summary?.conflict ?? false,
    refs_version: summary?.refs_version ?? 0,
    busy: false,
  });
}

/** The unmerged paths of `repoId`'s working copy now. */
const unmergedPaths = (repoId: RepoId): string[] =>
  fakeState.workingCopies[repoId]?.unmerged.map((file) => file.path) ?? [];

/**
 * `read` of unmerged file `file` of view `handle`, like the core: `null` for a replaced view, an
 * error for an unknown file and `already_resolved` for one no longer in conflict.
 */
function fakeUnmerged<T>(workspaceId: WorkspaceId, handle: number, file: number, read: (path: string) => T) {
  const view = viewPart(workspaceId, handle);
  if (view === undefined) {
    return Promise.resolve({ status: "ok" as const, data: null });
  }
  const path = view.unmerged[file];
  if (path === undefined) {
    return Promise.resolve({
      status: "error" as const,
      error: { code: "internal" as const, message: `the list has no file ${file}`, git_stderr: null },
    });
  }
  const data = unmergedPaths(view.repo).includes(path) ? read(path) : { kind: "already_resolved" as const };
  return Promise.resolve({ status: "ok" as const, data });
}

/**
 * Mark resolved or a chosen side, like the core: refused for a file no
 * longer in conflict or, without the override, text with a marker line;
 * otherwise a job takes the file out of the unmerged list.
 */
function fakeResolve(workspaceId: WorkspaceId, request: ResolveRequest | ChooseRequest) {
  fakeState.conflictRequests.push(request);
  const view = viewPart(workspaceId, request.handle);
  if (view === undefined) {
    return Promise.resolve({ status: "ok" as const, data: null });
  }
  const path = view.unmerged[request.file];
  const repo = fakeState.workspace.repos.find((known) => known.id === view.repo);
  if (path === undefined || repo === undefined) {
    return Promise.resolve({
      status: "error" as const,
      error: { code: "internal" as const, message: `the list has no file ${request.file}`, git_stderr: null },
    });
  }
  const text = "text" in request ? request.text : null;
  const override = "override_markers" in request && request.override_markers;
  const refusal = planResolution(unmergedPaths(repo.id), path, text, override);
  if (refusal) {
    return Promise.resolve({ status: "ok" as const, data: refusal });
  }
  const working = fakeState.workingCopies[repo.id] ?? { staged: [], unstaged: [], unmerged: [] };
  const unmerged = working.unmerged.filter((file) => file.path !== path);
  fakeState.workingCopies[repo.id] = { ...working, unmerged };
  const details = fakeState.conflicts[repo.id];
  const jobId = nextJobId++;
  const job = fakeJob(fakeState.workspace, {
    job_id: jobId,
    workspace_id: workspaceId,
    label: `Resolve ${path}`,
    repos: [repo.id],
    repo_names: [repo.name],
    state: "ended",
    outcome: { kind: "succeeded" },
    started_at: Date.now(),
    duration_ms: 5,
  });
  setTimeout(() => {
    // The banner's count of unmerged paths, read again (OPS-2).
    if (details !== undefined) {
      setConflict(repo.id, { ...details, unmerged_paths: unmerged.length });
    }
    endJob(workspaceId, repo.id, job);
  }, 0);
  return Promise.resolve({ status: "ok" as const, data: { kind: "started" as const, job_id: jobId } });
}

/** The proposal jobs running, by job id. */
const aiJobs = new Map<JobId, { job: JobView; path: string }>();

/** `ai_resolve`'s checks, in the core's order, then a proposal job that ends with `fakeState.aiAnswers`. */
function fakeAiResolve(workspaceId: WorkspaceId, repoId: RepoId, path: string): AiResolveStart {
  const eligibility = fakeState.aiEligibility[path] ?? { kind: "eligible" };
  const refused = fakeAiRefusal(fakeState.workspace.ai, eligibility);
  if (refused !== null) {
    return { kind: "refused", refused };
  }
  const repo = fakeState.workspace.repos.find((known) => known.id === repoId);
  const jobId = nextJobId++;
  const job = fakeJob(fakeState.workspace, {
    job_id: jobId,
    workspace_id: workspaceId,
    label: `Resolve with AI · ${path.split("/").pop() ?? path}`,
    repos: [repoId],
    repo_names: [repo?.name ?? repoId],
    state: "running",
    outcome: null,
    started_at: Date.now(),
    duration_ms: null,
  });
  aiJobs.set(jobId, { job, path });
  sendJob(job);
  if (!fakeState.holdAi) {
    setTimeout(() => endAiJob(jobId, aiAnswerOf(path)));
  }
  return { kind: "started", job_id: jobId };
}

/** Ends proposal job `jobId` with `answer`, kept and announced, or cancelled with `null`. */
function endAiJob(jobId: JobId, answer: AiProposalGet | null): void {
  const running = aiJobs.get(jobId);
  if (running === undefined) {
    return;
  }
  aiJobs.delete(jobId);
  const { job, path } = running;
  const ended = { ...job, state: "ended" as const, duration_ms: 5 };
  if (answer === null) {
    sendJob({ ...ended, outcome: { kind: "skipped", reason: { kind: "cancelled" } }, message: "cancelled" });
    return;
  }
  fakeState.aiProposals[path] = answer;
  sendJob(
    answer.kind === "failed"
      ? { ...ended, outcome: { kind: "failed" }, message: `AI failed: ${answer.reason}` }
      : { ...ended, outcome: { kind: "succeeded" } },
  );
  emit(aiProposalReadyListeners, "ai_proposal_ready", {
    workspace_id: job.workspace_id,
    repo_id: job.repos[0] ?? "",
    path,
    job_id: jobId,
  });
}

/** What a proposal job for `path` ends with. */
const aiAnswerOf = (path: string): AiProposalGet =>
  fakeState.aiAnswers[path] ?? { kind: "failed", reason: "the fake has no answer" };

/** Ends the held proposal jobs with their answers. */
export function finishAiJobs(): void {
  for (const [jobId, { path }] of [...aiJobs]) {
    endAiJob(jobId, aiAnswerOf(path));
  }
  for (const jobId of [...aiRuns.keys()]) {
    finishRun(jobId);
  }
}

/** Ends the running files of the held Resolve all runs, which lets waiting files run (AI-13). */
export function finishAiRunningFiles(): void {
  for (const jobId of [...aiRuns.keys()]) {
    finishRunningFiles(jobId);
    endRunIfDone(jobId);
  }
}

/** The Resolve all runs going on, by job id: the job as last sent. */
const aiRuns = new Map<JobId, JobView>();

/** At most this many files of a run are proposed for at once (AI-13). */
const AI_RUN_LIMIT = 4;

/** What a file's answer makes of it in the run's progress. */
function fileStateOf(answer: AiProposalGet): AiFileState {
  if (answer.kind !== "proposal") {
    return { kind: "failed", reason: answer.kind === "failed" ? answer.reason : "no answer" };
  }
  const count = (confidence: string) =>
    answer.proposal.hunks.filter((hunk) => hunk.status === "resolved" && hunk.confidence === confidence).length;
  const unresolved = answer.proposal.hunks.filter((hunk) => hunk.status === "unresolved").length;
  return { kind: "done", high: count("high"), medium: count("medium"), low: count("low"), unresolved };
}

/** Whether a file of a run still waits or runs. */
const unfinished = (file: AiFileProgress) => file.state.kind === "waiting" || file.state.kind === "running";

/** Sends run `jobId` with `files`, letting waiting files run while fewer than four do. */
function moveRun(jobId: JobId, files: AiFileProgress[]): void {
  const running = files.filter((file) => file.state.kind === "running").length;
  let free = AI_RUN_LIMIT - running;
  const moved = files.map((file) => {
    if (file.state.kind === "waiting" && free > 0) {
      free -= 1;
      return { ...file, state: { kind: "running" as const } };
    }
    return file;
  });
  const job = aiRuns.get(jobId);
  if (job === undefined) {
    return;
  }
  const done = moved.filter((file) => !unfinished(file)).length;
  const next = { ...job, ai_files: moved, done, total: moved.length };
  aiRuns.set(jobId, next);
  sendJob(next);
}

/** Ends run `jobId` when no file waits or runs any more, with the core's outcome. */
function endRunIfDone(jobId: JobId): void {
  const job = aiRuns.get(jobId);
  if (job === undefined || job.ai_files.some(unfinished)) {
    return;
  }
  aiRuns.delete(jobId);
  const sent = job.ai_files.filter(
    (file) => file.state.kind !== "ineligible" && file.state.kind !== "all_edited",
  ).length;
  const failed = job.ai_files.filter((file) => file.state.kind === "failed").length;
  const done = job.ai_files.some((file) => file.state.kind === "done");
  const of = `${sent} ${sent === 1 ? "file" : "files"}`;
  const ending: Pick<JobView, "outcome" | "message"> =
    sent === 0
      ? { outcome: { kind: "nothing_to_do" }, message: null }
      : failed > 0
        ? { outcome: { kind: "failed" }, message: `AI failed for ${failed} of ${of}` }
        : done
          ? { outcome: { kind: "succeeded" }, message: null }
          : { outcome: { kind: "skipped", reason: { kind: "cancelled" } }, message: "cancelled" };
  sendJob({ ...job, ...ending, state: "ended", duration_ms: 5 });
}

/** Ends the running files of run `jobId` with their answers, kept and announced. */
function finishRunningFiles(jobId: JobId): void {
  const job = aiRuns.get(jobId);
  if (job === undefined) {
    return;
  }
  const files = job.ai_files.map((file) => {
    if (file.state.kind !== "running") {
      return file;
    }
    const answer = aiAnswerOf(file.path);
    fakeState.aiProposals[file.path] = answer;
    emit(aiProposalReadyListeners, "ai_proposal_ready", {
      workspace_id: job.workspace_id,
      repo_id: job.repos[0] ?? "",
      path: file.path,
      job_id: jobId,
    });
    return { ...file, state: fileStateOf(answer) };
  });
  moveRun(jobId, files);
}

/** Runs run `jobId` to its end, four files at a time. */
function finishRun(jobId: JobId): void {
  while (aiRuns.get(jobId)?.ai_files.some((file) => file.state.kind === "running")) {
    finishRunningFiles(jobId);
  }
  endRunIfDone(jobId);
}

/** Cancels run `jobId`: waiting and running files end cancelled and keep nothing (AI-13). */
function cancelRun(jobId: JobId): void {
  const job = aiRuns.get(jobId);
  if (job === undefined) {
    return;
  }
  const files = job.ai_files.map((file) =>
    file.state.kind === "waiting" || file.state.kind === "running"
      ? { ...file, state: { kind: "cancelled" as const } }
      : file,
  );
  aiRuns.set(jobId, { ...job, ai_files: files });
  endRunIfDone(jobId);
}

/**
 * `ai_resolve_all` like the core: the checks that are not a file's, then
 * one job listing every unmerged file of the view, ineligible ones and ones
 * whose buffer has every hunk edited not sent (AI-4, AI-13).
 */
function fakeAiResolveAll(workspaceId: WorkspaceId, request: AiResolveAllRequest): AiResolveAllStart | null {
  fakeState.aiResolveAllRequests.push(request);
  const view = viewPart(workspaceId, request.handle);
  if (view === undefined || view.repo !== request.repo) {
    return null;
  }
  const refused = fakeAiRefusal(fakeState.workspace.ai, { kind: "eligible" });
  if (refused !== null && refused.kind !== "consent_needed") {
    return { kind: "refused", refused };
  }
  const unmerged = unmergedPaths(view.repo);
  const paths = view.unmerged.filter((path) => unmerged.includes(path));
  if (paths.length === 0) {
    return { kind: "already_resolved" };
  }
  const files: AiFileProgress[] = paths.map((path) => {
    const eligibility = fakeState.aiEligibility[path];
    const buffer = request.files.find((file) => view.unmerged[file.file] === path);
    const state: AiFileState =
      eligibility?.kind === "ineligible"
        ? { kind: "ineligible", message: eligibility.message }
        : buffer?.hunks.length === 0
          ? { kind: "all_edited" }
          : { kind: "waiting" };
    return { path, state };
  });
  const sent = files.filter((file) => file.state.kind === "waiting").length;
  if (refused !== null && sent > 0) {
    return { kind: "refused", refused };
  }
  const repo = fakeState.workspace.repos.find((known) => known.id === view.repo);
  const jobId = nextJobId++;
  const job = fakeJob(fakeState.workspace, {
    job_id: jobId,
    workspace_id: workspaceId,
    label: `Resolve all with AI · ${repo?.name ?? view.repo} (${sent} ${sent === 1 ? "file" : "files"})`,
    repos: [view.repo],
    repo_names: [repo?.name ?? view.repo],
    state: "running",
    outcome: null,
    started_at: Date.now(),
    duration_ms: null,
  });
  aiRuns.set(jobId, job);
  moveRun(jobId, files);
  if (!fakeState.holdAi) {
    setTimeout(() => finishRun(jobId));
  } else if (sent === 0) {
    endRunIfDone(jobId);
  }
  return { kind: "started", job_id: jobId };
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

/** Sync jobs waiting for `finishSync`, oldest first. */
const heldSyncs: (() => void)[] = [];

/** Ends the oldest sync job held by `fakeState.holdSync`. */
export function finishSync(): void {
  heldSyncs.shift()?.();
}

const refused = (message: string) =>
  Promise.resolve({ status: "error" as const, error: { code: "internal" as const, message, git_stderr: null } });

/**
 * Runs sync operation `request` on the fake's ref model, like the core:
 * refused for a blocked repo (fetch may run in conflict state) or what it
 * cannot apply to, a remote to choose or a confirmation first when the
 * core would ask, then a job that reports a transfer step and ends with
 * the repo's refs and ahead and behind read again.
 */
function fakeSyncOp(workspaceId: WorkspaceId, request: SyncRequest) {
  fakeState.syncRequests.push(request);
  const repo = fakeState.workspace.repos.find((known) => known.id === request.repo_id);
  const refs = fakeState.refModels[request.repo_id];
  const summary = fakeState.summaries.find((known) => known.repo_id === request.repo_id);
  if (repo === undefined || refs === undefined) {
    return refused("the repo has no remote");
  }
  if (summary?.busy) {
    return refused("another operation is running on the repo");
  }
  if (summary?.missing) {
    return refused("the repo's folder is missing");
  }
  if (summary?.conflict && request.op.kind !== "fetch") {
    return refused("the repo is in conflict state; continue or abort first");
  }
  const plan = planSync(refs, request.op, request.confirmed);
  switch (plan.kind) {
    case "refused":
      return refused(plan.why);
    case "choose_remote":
      return Promise.resolve({ status: "ok" as const, data: { kind: "choose_remote" as const, remotes: plan.remotes } });
    case "confirm":
      return Promise.resolve({
        status: "ok" as const,
        data: { kind: "confirm" as const, repo: repo.name, actions: plan.actions },
      });
  }
  const jobId = nextJobId++;
  const job = (change: Partial<JobView>) =>
    fakeJob(fakeState.workspace, {
      job_id: jobId,
      workspace_id: workspaceId,
      label: plan.label,
      repos: [repo.id],
      repo_names: [repo.name],
      started_at: Date.now(),
      ...change,
    });
  const end = () => {
    const after = applySync(fakeState.refModels[repo.id] ?? refs, request.op, plan);
    fakeState.refModels[repo.id] = after.refs;
    fakeState.refs[repo.id] = refNodes(repo.id, after.refs);
    const counts = after.refs.current === undefined ? undefined : after.refs.counts?.[after.refs.current];
    fakeState.summaries = fakeState.summaries.map((known) =>
      known.repo_id === repo.id && known.summary !== null
        ? {
            ...known,
            summary: {
              ...known.summary,
              upstream:
                known.summary.upstream === null || counts === undefined
                  ? known.summary.upstream
                  : { ...known.summary.upstream, ahead: counts[0], behind: counts[1] },
            },
            refs_version: known.refs_version + 1,
          }
        : known,
    );
    const failed = after.outcome.kind === "failed";
    endJob(
      workspaceId,
      repo.id,
      job({
        state: "ended",
        outcome: after.outcome,
        output: after.output,
        message: failed ? "git exited with code 1" : null,
        duration_ms: 5,
      }),
    );
    emitRepoRefsChanged({ workspace_id: workspaceId, repo_id: repo.id });
  };
  setTimeout(() => {
    if (summary !== undefined) {
      emitRepoSummary({ ...summary, busy: true });
    }
    sendJob(
      job({
        state: "running",
        done: 1,
        total: 2,
        step: request.op.kind === "push" ? "Writing objects" : "Receiving objects",
        outcome: null,
        duration_ms: null,
      }),
    );
    if (fakeState.holdSync) {
      heldSyncs.push(end);
    } else {
      setTimeout(end, 0);
    }
  }, 0);
  return Promise.resolve({ status: "ok" as const, data: { kind: "started" as const, job_id: jobId } });
}

const NO_CONFLICT: ConflictDetails = { operation: null, applying: null, onto: null, step: null, unmerged_paths: 0 };

/**
 * Puts repo `repoId` in conflict state as `details` say, or out of it with
 * `null`: what `conflict_details` reads and the repo's summary, sent as the
 * core would after a read (OPS-1).
 */
export function setConflict(repoId: RepoId, details: ConflictDetails | null): void {
  const others = Object.entries(fakeState.conflicts).filter(([known]) => known !== repoId);
  fakeState.conflicts = Object.fromEntries(details === null ? others : [...others, [repoId, details]]);
  const state = { operation: details?.operation ?? null, unmerged_paths: details?.unmerged_paths ?? 0 };
  fakeState.summaries = fakeState.summaries.map((known) =>
    known.repo_id === repoId && known.summary !== null
      ? {
          ...known,
          summary: { ...known.summary, state },
          conflict: state.operation !== null || state.unmerged_paths > 0,
          refs_version: known.refs_version + 1,
        }
      : known,
  );
  const summary = fakeState.summaries.find((known) => known.repo_id === repoId);
  if (summary !== undefined) {
    emitRepoSummary(summary);
  }
}

/**
 * Runs integrate operation `request` like the core: refused for a blocked
 * repo (abort and continue need conflict state) or what it cannot apply
 * to, a mainline to choose for a merge commit's revert, then a job. It
 * stops in `fakeState.integrateConflict` when set; an abort or a continue
 * that does not stop takes the repo out of conflict state.
 */
function fakeIntegrateOp(workspaceId: WorkspaceId, request: IntegrateRequest) {
  fakeState.integrateRequests.push(request);
  const repo = fakeState.workspace.repos.find((known) => known.id === request.repo_id);
  const refs = fakeState.refModels[request.repo_id];
  const summary = fakeState.summaries.find((known) => known.repo_id === request.repo_id);
  const resuming = request.op.kind === "abort" || request.op.kind === "continue";
  if (repo === undefined || refs === undefined) {
    return refused("the repo has no branches");
  }
  if (summary?.busy) {
    return refused("another operation is running on the repo");
  }
  if (summary?.missing) {
    return refused("the repo's folder is missing");
  }
  if (summary?.conflict && !resuming) {
    return refused("the repo is in conflict state; continue or abort first");
  }
  const parents = (hash: string) => fakeState.commits[hash]?.details.parents ?? [];
  const plan = planIntegrate(refs, request.op, fakeState.conflicts[repo.id], parents, request.confirmed);
  if (plan.kind === "refused") {
    return refused(plan.why);
  }
  if (plan.kind === "confirm") {
    return Promise.resolve({
      status: "ok" as const,
      data: { kind: "confirm" as const, repo: repo.name, actions: plan.actions },
    });
  }
  if (plan.kind === "choose_mainline") {
    return Promise.resolve({
      status: "ok" as const,
      data: { kind: "choose_mainline" as const, commit: plan.commit, parents: plan.parents },
    });
  }
  const jobId = nextJobId++;
  const stops = request.op.kind === "abort" ? null : fakeState.integrateConflict;
  setTimeout(() => {
    const working = fakeState.workingCopies[repo.id];
    if (stops === null && resuming && working !== undefined) {
      // Abort puts the files back; Continue needs none unmerged.
      fakeState.workingCopies[repo.id] = { ...working, unmerged: [], inConflict: false };
    }
    setConflict(repo.id, stops);
    const job = fakeJob(fakeState.workspace, {
      job_id: jobId,
      workspace_id: workspaceId,
      label: plan.label,
      repos: [repo.id],
      repo_names: [repo.name],
      state: "ended",
      outcome: stops === null ? { kind: "succeeded" } : { kind: "conflict" },
      output: stops === null ? "" : "CONFLICT (content): Merge conflict in a.txt\n",
      message: null,
      started_at: Date.now(),
      duration_ms: 5,
    });
    endJob(workspaceId, repo.id, job);
    emitRepoRefsChanged({ workspace_id: workspaceId, repo_id: repo.id });
  }, 0);
  return Promise.resolve({ status: "ok" as const, data: { kind: "started" as const, job_id: jobId } });
}

/** Why the fake core refuses any write on `repo` now (GRP-8), or `null`. */
function blockedWhy(repo: RepoId): string | null {
  const summary = fakeState.summaries.find((known) => known.repo_id === repo);
  if (summary?.busy) {
    return "another operation is running on the repo";
  }
  if (summary?.missing) {
    return "the repo's folder is missing";
  }
  return summary?.conflict ? "the repo is in conflict state; continue or abort first" : null;
}

/** Answers `plan` before a job, or runs `then` as a job that ends at once and re-reads the repo. */
function startPlanned(workspaceId: WorkspaceId, repoId: RepoId, plan: TagPlan, then: () => void) {
  const repo = fakeState.workspace.repos.find((known) => known.id === repoId);
  if (repo === undefined) {
    return refused(`no repo ${repoId}`);
  }
  switch (plan.kind) {
    case "refused":
      return refused(plan.why);
    case "choose_remote":
      return Promise.resolve({ status: "ok" as const, data: { kind: "choose_remote" as const, remotes: plan.remotes } });
    case "confirm":
      return Promise.resolve({
        status: "ok" as const,
        data: { kind: "confirm" as const, repo: repo.name, actions: plan.actions },
      });
  }
  const jobId = nextJobId++;
  setTimeout(() => {
    then();
    const job = fakeJob(fakeState.workspace, {
      job_id: jobId,
      workspace_id: workspaceId,
      label: plan.label,
      repos: [repo.id],
      repo_names: [repo.name],
      state: "ended",
      outcome: { kind: "succeeded" },
      output: "",
      message: null,
      started_at: Date.now(),
      duration_ms: 5,
    });
    endJob(workspaceId, repo.id, job);
    emitRepoRefsChanged({ workspace_id: workspaceId, repo_id: repo.id });
  }, 0);
  return Promise.resolve({ status: "ok" as const, data: { kind: "started" as const, job_id: jobId } });
}

/** Runs tag operation `request` on the fake's ref model, like the core. */
function fakeTagOp(workspaceId: WorkspaceId, request: TagRequest) {
  fakeState.tagRequests.push(request);
  const refs = fakeState.refModels[request.repo_id];
  const blocked = blockedWhy(request.repo_id);
  if (refs === undefined || blocked !== null) {
    return refused(blocked ?? "the repo has no refs");
  }
  const plan = planTag(refs, request.op, request.confirmed);
  return startPlanned(workspaceId, request.repo_id, plan, () => {
    if (plan.kind === "run") {
      fakeState.refModels[request.repo_id] = plan.refs;
      fakeState.refs[request.repo_id] = refNodes(request.repo_id, plan.refs);
    }
  });
}

/** Runs remote operation `request` on the fake's ref model, like the core. */
function fakeRemoteOp(workspaceId: WorkspaceId, request: RemoteRequest) {
  fakeState.remoteRequests.push(request);
  const refs = fakeState.refModels[request.repo_id];
  const blocked = blockedWhy(request.repo_id);
  if (refs === undefined || blocked !== null) {
    return refused(blocked ?? "the repo has no refs");
  }
  const plan = planRemote(refs, request.op, request.confirmed);
  return startPlanned(workspaceId, request.repo_id, plan, () => {
    if (plan.kind === "run") {
      fakeState.refModels[request.repo_id] = plan.refs;
      fakeState.refs[request.repo_id] = refNodes(request.repo_id, plan.refs);
    }
  });
}

/** Answers the fsmonitor offer of `repo` like the core: either way it is not made again. */
function fakeFsmonitorAnswer(repo: RepoId, answer: "enable" | "decline") {
  fakeState.fsmonitorAnswers.push({ repo, answer });
  fakeState.fsmonitorOffers = Object.fromEntries(
    Object.entries(fakeState.fsmonitorOffers).filter(([known]) => known !== repo),
  );
}

/**
 * Runs reset `request` like the core: a hard reset asks first, naming the
 * fake history's commit and the tracked files of the repo's working copy;
 * done, it leaves nothing staged, and with `hard` nothing changed but
 * untracked files.
 */
function fakeResetOp(workspaceId: WorkspaceId, request: ResetRequest) {
  fakeState.resetRequests.push(request);
  const repo = request.repo_id;
  const blocked = blockedWhy(repo);
  if (blocked !== null) {
    return refused(blocked);
  }
  const index = parentOf(request.commit) === null ? -1 : Number.parseInt(request.commit.slice(0, 8), 16);
  // The history the history pane shows, which has a default for a repo without one.
  const name = fakeState.workspace.repos.find((known) => known.id === repo)?.name ?? repo;
  const history = fakeState.histories[repo] ?? defaultHistory(name, { kind: "repo" });
  if (index < 0 || index >= history.length) {
    return refused(`'${request.commit}' names no commit`);
  }
  const row = commitAt(history, index);
  const head = fakeState.summaries.find((known) => known.repo_id === repo)?.summary?.head;
  const working = fakeState.workingCopies[repo] ?? { staged: [], unstaged: [], unmerged: [] };
  const tracked = [...working.staged, ...working.unstaged.filter((file) => file.file.status !== "untracked")];
  const lost = new Set(tracked.map((file) => file.file.path)).size;
  const branch = head?.kind === "branch" ? head.name : null;
  const plan = planReset(branch, `${row.short_hash} ${row.subject}`, request.mode, lost, request.confirmed);
  return startPlanned(workspaceId, repo, plan, () => {
    const untracked = working.unstaged.filter((file) => file.file.status === "untracked");
    const unstaged = request.mode === "hard" ? untracked : [...working.staged, ...working.unstaged];
    fakeState.workingCopies[repo] = { ...working, staged: request.mode === "soft" ? working.staged : [], unstaged };
  });
}

/**
 * Runs stash operation `request` on the fake's ref model, like the core:
 * refused for a blocked repo or an entry no longer at its index, a drop
 * asked to confirm first, then a job. A push of a clean repo is *nothing
 * to do*; an apply or pop stops in `fakeState.stashConflict` when set, and
 * a pop keeps its stash then.
 */
function fakeStashOp(workspaceId: WorkspaceId, request: StashRequest) {
  fakeState.stashRequests.push(request);
  const repo = fakeState.workspace.repos.find((known) => known.id === request.repo_id);
  const refs = fakeState.refModels[request.repo_id];
  const summary = fakeState.summaries.find((known) => known.repo_id === request.repo_id);
  if (repo === undefined || refs === undefined) {
    return refused("the repo has no branches");
  }
  if (summary?.busy) {
    return refused("another operation is running on the repo");
  }
  if (summary?.conflict) {
    return refused("the repo is in conflict state; continue or abort first");
  }
  const stashes = refs.stashes ?? [];
  const op = request.op;
  let entry = "";
  if (op.kind !== "push") {
    const message = stashes[op.stash.index];
    if (message === undefined || stashHash(message) !== op.stash.hash) {
      return refused(`the stash list changed since it was read: stash@{${op.stash.index}} is no longer that stash`);
    }
    entry = `stash@{${op.stash.index}} (${message})`;
    if (op.kind === "drop" && !request.confirmed) {
      return Promise.resolve({
        status: "ok" as const,
        data: {
          kind: "confirm" as const,
          repo: repo.name,
          actions: [`Drop ${entry}; its changes are lost unless they were applied`],
        },
      });
    }
  }
  const jobId = nextJobId++;
  const stops = op.kind === "apply" || op.kind === "pop" ? fakeState.stashConflict : null;
  setTimeout(() => {
    const dirty = summary?.summary?.dirty ?? false;
    let after = stashes;
    let outcome: Outcome = { kind: "succeeded" };
    let label: string;
    if (op.kind === "push") {
      const message = op.message?.trim() ? op.message.trim() : null;
      label = message === null ? "Stash changes" : `Stash changes as "${message}"`;
      if (dirty) {
        after = [`On ${refs.current ?? "HEAD"}: ${message ?? "WIP"}`, ...stashes];
        fakeState.workingCopies[repo.id] = { staged: [], unstaged: [], unmerged: [], inConflict: false };
      } else {
        outcome = { kind: "nothing_to_do" };
      }
    } else {
      label = `${op.kind === "drop" ? "Drop" : op.kind === "pop" ? "Pop" : "Apply"} ${entry}`;
      if (stops !== null) {
        outcome = { kind: "conflict" };
      } else if (op.kind !== "apply") {
        after = stashes.filter((_, index) => index !== op.stash.index);
      }
    }
    const model = { ...refs, stashes: after };
    fakeState.refModels[repo.id] = model;
    fakeState.refs[repo.id] = refNodes(repo.id, model);
    if (op.kind === "push" && dirty) {
      fakeState.summaries = fakeState.summaries.map((known) =>
        known.repo_id === repo.id && known.summary !== null
          ? { ...known, summary: { ...known.summary, dirty: false, changes: 0 } }
          : known,
      );
    }
    if (stops !== null) {
      setConflict(repo.id, stops);
    }
    const job = fakeJob(fakeState.workspace, {
      job_id: jobId,
      workspace_id: workspaceId,
      label,
      repos: [repo.id],
      repo_names: [repo.name],
      state: "ended",
      outcome,
      output: stops === null ? "" : "CONFLICT (content): Merge conflict in a.txt\n",
      message: null,
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

/** What the fake's group operations use of the fake core. */
const groupDeps: GroupDeps = {
  emitReady: (ready) => emit(preflightReadyListeners, "preflight_ready", ready),
  sendJob,
  newJobId: () => nextJobId++,
  alwaysShow: () => fakeState.settings.settings.always_show_preflight,
  stagedFiles: (repo, stageAll) => {
    const working = fakeState.workingCopies[repo];
    const tracked = stageAll ? (working?.unstaged ?? []).filter((file) => file.file.status !== "untracked") : [];
    return new Set([...(working?.staged ?? []), ...tracked].map((file) => file.file.path)).size;
  },
};

/** What the fake's updater uses of the fake core (PLAT-4). */
const updateDeps = {
  sendJob,
  newJobId: () => nextJobId++,
  workspace: () => (fakeState.active === null ? null : (openWorkspace(fakeState.active) ?? null)),
  jobsRunning: () => fakeState.jobs.some((job) => job.state !== "ended"),
  changed: (view: UpdateView) => emit(updateStatusListeners, "update_status_changed", view),
};

/**
 * What detection finds, like the core (PLAT-3): the chosen git if it holds one, too old or not; otherwise
 * what automatic detection finds, saying the chosen path had none.
 */
function fakeDetection(): GitStatus {
  const chosen = fakeState.gitPath;
  const auto = fakeState.gitStatus;
  if (chosen === null || auto.kind === "checking") {
    return auto;
  }
  const version = fakeState.gits[chosen];
  if (version !== undefined) {
    const [major = 0, minor = 0] = version.split(".").map(Number);
    return major < 2 || (major === 2 && minor < 40)
      ? { kind: "too_old", path: chosen, found: version, required: "2.40.0", chosen, chosen_missing: false }
      : { kind: "usable", path: chosen, version, chosen, chosen_missing: false };
  }
  if (auto.kind === "not_found") {
    return { ...auto, searched: [chosen, ...auto.searched], chosen, chosen_missing: true };
  }
  return { ...auto, chosen, chosen_missing: true };
}

/** Like the core: `checking` at once, then the result as a later event. */
function fakeRecheck() {
  emitGitStatus({ kind: "checking" });
  setTimeout(() => emitGitStatus(fakeDetection()), 10);
  return Promise.resolve({ status: "ok" as const, data: null });
}

/** Like the core: saves the chosen git, refused while the settings are read-only, and detects again. */
function fakeSetGitPath(path: string | null) {
  if (fakeState.settings.read_only) {
    return Promise.resolve({
      status: "error" as const,
      error: {
        code: "settings_read_only" as const,
        message: "the settings are read-only, so the change was not saved",
        git_stderr: null,
      },
    });
  }
  fakeState.gitPath = path;
  return fakeRecheck();
}

/** Delivers a `git_status` event to every listener, as the core would. */
export function emitGitStatus(payload: GitStatus): void {
  emit(gitStatusListeners, "git_status", payload);
}

/** Delivers a `workspace_changed` event to every listener, as the core would. */
export function emitWorkspaceChanged(payload: WorkspaceView): void {
  emit(workspaceChangedListeners, "workspace_changed", payload);
}

/** Every workspace file of the fake. */
const fakeWorkspaces = () => [fakeState.workspace, ...fakeState.others];

/** Workspace `id` while its tab is open. */
function openWorkspace(id: WorkspaceId): WorkspaceView | undefined {
  return fakeState.tabs.includes(id) ? fakeWorkspaces().find((workspace) => workspace.id === id) : undefined;
}

/** Replaces the workspace file with `view`'s id. */
function setFakeWorkspace(view: WorkspaceView): void {
  if (view.id === fakeState.workspace.id) {
    fakeState.workspace = view;
  } else {
    fakeState.others = fakeState.others.map((other) => (other.id === view.id ? view : other));
  }
}

/** The open tabs as `workspace_get` returns them. */
export function fakeTabs(): WorkspaceTabs {
  const open = fakeState.tabs.flatMap((id) => fakeWorkspaces().filter((workspace) => workspace.id === id));
  const active = open.some((workspace) => workspace.id === fakeState.active) ? fakeState.active : null;
  return { open, active, missing: [] };
}

/** Delivers a `workspaces_changed` event with the fake's tabs, as the core would. */
export function emitWorkspacesChanged(): void {
  emit(workspacesChangedListeners, "workspaces_changed", fakeTabs());
}

/** Delivers a `repo_summary` event to every listener, as the core would. */
export function emitRepoSummary(payload: SummaryView): void {
  emit(repoSummaryListeners, "repo_summary", payload);
  // Like the app: the rows of the groups the repo is in follow it (GRP-4).
  const workspace = fakeWorkspaces().find((known) => known.id === payload.workspace_id);
  for (const group of workspace?.groups ?? []) {
    if (group.repos.includes(payload.repo_id)) {
      emit(groupSummaryListeners, "group_summary", groupSummary(payload.workspace_id, group, fakeState.summaries));
    }
  }
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
  fakeState.cloneParent = null;
  fakeState.folders = {};
  fakeState.cloneRequests = [];
  fakeState.cloneFailure = null;
  fakeState.missingRepos = [];
  fakeState.exportAnswer = null;
  fakeState.exports = [];
  fakeState.imports = 0;
  fakeState.importAnswer = null;
  fakeState.opened = [];
  fakeState.openFailure = null;
  fakeState.fileConflicts = {};
  fakeState.aiEligibility = {};
  fakeState.aiResolve = null;
  fakeState.aiResolveRequests = [];
  fakeState.aiProposals = {};
  fakeState.aiAnswers = {};
  fakeState.aiResolveAllRequests = [];
  fakeState.holdAi = false;
  aiJobs.clear();
  aiRuns.clear();
  fakeState.conflictRequests = [];
  fakeState.appInfo = defaultAppInfo;
  fakeState.gitStatus = defaultGitStatus;
  fakeState.settings = defaultSettings;
  fakeState.workspace = defaultWorkspace;
  fakeState.others = [];
  fakeState.tabs = [defaultWorkspace.id];
  fakeState.active = defaultWorkspace.id;
  fakeState.pickedFolder = null;
  fakeState.gitPath = null;
  fakeState.gits = {};
  fakeState.pickedGit = null;
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
  fakeState.syncRequests = [];
  fakeState.holdSync = false;
  heldSyncs.length = 0;
  fakeState.integrateRequests = [];
  fakeState.conflicts = {};
  fakeState.integrateConflict = null;
  fakeState.stashRequests = [];
  fakeState.stashConflict = null;
  fakeState.tagRequests = [];
  fakeState.resetRequests = [];
  fakeState.remoteRequests = [];
  fakeState.fsmonitorOffers = {};
  fakeState.fsmonitorAnswers = [];
  fakeState.changesDelay = 0;
  fakeState.holdChanges = false;
  fakeState.heldChanges = [];
  fakeState.holdPalette = false;
  fakeState.heldPalette = [];
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
  updateStatusListeners.clear();
  resetUpdate();
  workspaceChangedListeners.clear();
  groupSummaryListeners.clear();
  workspacesChangedListeners.clear();
  workspaceAddReportListeners.clear();
  repoSummaryListeners.clear();
  repoRefsChangedListeners.clear();
  commitGraphWrittenListeners.clear();
  watchWarningListeners.clear();
  jobEndedListeners.clear();
  askpassPromptListeners.clear();
  askpassClosedListeners.clear();
  preflightReadyListeners.clear();
  aiChangedListeners.clear();
  aiModelsListeners.clear();
  aiSelfTestListeners.clear();
  aiProposalReadyListeners.clear();
  resetFakeAi();
  resetGroups();
}

// Playwright runs the fake build; `?fake=<name>` picks a data set.
const picked = typeof location === "undefined" ? null : new URLSearchParams(location.search).get("fake");
// `jobs` is the `tree` data set with this run's jobs (UI-21).
// `update` and `update-deb` are the `tree` data set with version 0.2.0 on offer, for an AppImage, which installs it,
// and for a `.deb`, which only notifies (PLAT-4).
const withUpdate = picked === "update" || picked === "update-deb";
// `ai-conflict` and `ai-resolve-all` are the `conflict` data set with AI to resolve it.
const withAi = picked === "ai-conflict" || picked === "ai-resolve-all";
// `conflict-choice` is the `conflict` data set with src/app.ts a choice between the two versions (OPS-6).
const choice = picked === "conflict-choice";
const data =
  picked === null
    ? null
    : scenario(picked === "jobs" || withUpdate ? "tree" : withAi || choice ? "conflict" : picked, defaultWorkspace.id);
if (withUpdate) {
  offerUpdate(offered, picked === "update");
}
if (picked === "jobs") {
  fakeState.jobs = jobsScenario(defaultWorkspace);
  nextJobId = 100;
}
// `ai`: no provider yet, and an OpenAI-compatible provider offers two models (AI-1).
if (picked === "ai") {
  fakeAi.models = { kind: "offered", models: ["small-model", "large-model"] };
}
if (choice) {
  fakeState.fileConflicts = {
    "src/app.ts": {
      path: "src/app.ts",
      labels: { ours: "main", theirs: "feature/login" },
      content: {
        kind: "choice",
        reason: "deleted_by_them",
        ours: { kind: "file", size: 120, executable: false },
        theirs: { kind: "deleted" },
      },
      edited_outside: false,
      on_disk: null,
    },
  };
}
// `no-git`: no git found, so the window shows the git setup screen (M5-10).
if (picked === "no-git") {
  fakeState.gitStatus = {
    kind: "not_found",
    searched: ["/usr/bin/git", "/usr/local/bin/git"],
    chosen: null,
    chosen_missing: false,
  };
  fakeState.gits = { "/opt/git/bin/git": "2.51.0" };
  fakeState.pickedGit = "/opt/git/bin/git";
}
// `no-tabs`: no workspace open, so the window shows its empty state (M5-10).
if (picked === "no-tabs") {
  fakeState.tabs = [];
  fakeState.active = null;
}
// Playwright reads what reached the fake core, to tell which handler ran (UI-22).
if (picked !== null && typeof window !== "undefined") {
  Object.assign(window, { fakeCore: { state: fakeState, groups: fakeGroups, update: fakeUpdate } });
}
if (data) {
  fakeState.workspace = { ...defaultWorkspace, repos: data.repos, groups: data.groups ?? [] };
  fakeState.refs = data.refs;
  fakeState.summaries = data.summaries;
  fakeState.pickedFolder = data.pickedFolder ?? null;
  fakeState.cloneParent = data.cloneParent ?? null;
  fakeState.histories = data.histories ?? {};
  fakeState.historyDelay = data.historyDelay ?? 0;
  fakeState.slowRepos = data.slowRepos ?? {};
  fakeState.commits = data.commits ?? {};
  fakeState.workingCopies = data.workingCopies ?? {};
  fakeState.refModels = data.refModels ?? {};
  fakeState.conflicts = data.conflicts ?? {};
}

// `ai-conflict`: api stopped on README.md alone, two hunks with a proposal each from a usable provider (AI-11).
// `ai-resolve-all`: the same with src/app.ts, whose hunk has a proposal too, and .env, which the exclude list keeps
// from AI (AI-4, AI-13).
if (withAi) {
  fakeAi.providers = [
    {
      id: "p1",
      name: "Hosted",
      kind: "openai",
      model: "small-model",
      base_url: "https://api.example.com/v1",
      path: null,
      context_budget: 100_000,
      consent: "api.example.com",
    },
  ];
  fakeAi.default = "p1";
  const all = picked === "ai-resolve-all";
  const working = fakeState.workingCopies.r1;
  if (working !== undefined) {
    const unmerged = all
      ? [...working.unmerged, { path: ".env", conflict: "both_modified" as const }]
      : working.unmerged.slice(0, 1);
    fakeState.workingCopies.r1 = { ...working, unmerged };
  }
  const paths = all ? 3 : 1;
  const details = fakeState.conflicts.r1;
  if (details !== undefined) {
    fakeState.conflicts.r1 = { ...details, unmerged_paths: paths };
  }
  fakeState.summaries = fakeState.summaries.map((known) =>
    known.repo_id === "r1" && known.summary?.state
      ? { ...known, summary: { ...known.summary, state: { ...known.summary.state, unmerged_paths: paths } } }
      : known,
  );
  if (all) {
    fakeState.aiEligibility = {
      ".env": {
        kind: "ineligible",
        reason: { kind: "excluded", pattern: ".env*" },
        message: "matches the exclude list (.env*)",
      },
    };
  }
  fakeState.fileConflicts = {
    "README.md": {
      ...textConflict("README.md", { ours: "main", theirs: "feature/login" }),
      content: {
        kind: "text",
        chunks: [
          { kind: "resolved", text: "# api\n" },
          { kind: "hunk", ours: "Run it with make.\n", base: "Run it.\n", theirs: "Run it with just.\n" },
          { kind: "resolved", text: "\n## Login\n" },
          { kind: "hunk", ours: "Sign in.\n", base: "Log in.\n", theirs: "Log in with a token.\n" },
        ],
      },
    },
  };
  fakeState.aiAnswers = {
    "README.md": {
      kind: "proposal",
      proposal: {
        hunks: [
          {
            id: 1,
            status: "resolved",
            resolution: "Run it with just.\n",
            explanation: "Theirs replaced make with just; ours only reworded.",
            confidence: "high",
          },
          {
            id: 2,
            status: "resolved",
            resolution: "Sign in with a token.\n",
            explanation: "Kept the wording of ours and the token of theirs.",
            confidence: "medium",
          },
        ],
        usage: [{ input_tokens: 1450, output_tokens: 120 }],
      },
    },
    "src/app.ts": {
      kind: "proposal",
      proposal: {
        hunks: [
          {
            id: 1,
            status: "resolved",
            resolution: "theirs\n",
            explanation: "Only theirs changed the line.",
            confidence: "high",
          },
        ],
        usage: [null],
      },
    },
  };
}

/** Resolves after the fake's paging delay, as a slow disk would. */
const later = <T>(value: T): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value), fakeState.historyDelay));

/** Resolves after the fake's changes delay. */
const slowly = <T>(value: () => T): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value()), fakeState.changesDelay));

/** Runs `start` now, or when the test starts it while `holdChanges` is on. */
const held = <T>(start: () => Promise<T>): Promise<T> =>
  fakeState.holdChanges
    ? new Promise((resolve) => fakeState.heldChanges.push(() => resolve(start())))
    : start();

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

const nameRefused = (message: string) =>
  Promise.resolve({ status: "error" as const, error: { code: "refused" as const, message, git_stderr: null } });

/** Like the core: the tabs after a change, also sent as `workspaces_changed`. */
const changedTabs = () => {
  emitWorkspacesChanged();
  return Promise.resolve({ status: "ok" as const, data: fakeTabs() });
};

let nextWorkspace = 1;

/** The name checked as the core checks it (UI-27): trimmed, 1 to 64 characters, unique ignoring case. */
function checkName(name: string, own: WorkspaceId | null) {
  const trimmed = name.trim();
  if (trimmed === "") {
    return { refused: "a workspace name cannot be blank" };
  }
  if ([...trimmed].length > 64) {
    return { refused: "a workspace name has at most 64 characters" };
  }
  const lower = trimmed.toLowerCase();
  if (fakeWorkspaces().some((workspace) => workspace.id !== own && workspace.name.toLowerCase() === lower)) {
    return { refused: `a workspace named ${trimmed} exists` };
  }
  return { name: trimmed };
}

/** Like the core's group name rules (GRP-2): the trimmed name, or why it is refused. */
function checkGroupName(
  groups: GroupView[],
  name: string,
  own: GroupId | null,
): { name: string } | { refused: string } {
  const trimmed = name.trim();
  if (trimmed === "") {
    return { refused: "a group name cannot be blank" };
  }
  if ([...trimmed].length > 64) {
    return { refused: "a group name has at most 64 characters" };
  }
  const lower = trimmed.toLowerCase();
  if (lower === "ungrouped") {
    return { refused: '"Ungrouped" is the name of the repos in no group' };
  }
  const taken = groups.find((group) => group.id !== own && group.name.toLowerCase() === lower);
  return taken === undefined ? { name: trimmed } : { refused: `a group named "${taken.name}" exists already` };
}

/** `repos` inserted into `members` at `at`, or at the end; members among them move there. */
function placeRepos(members: RepoId[], repos: RepoId[], at: number | null): RepoId[] {
  const moved = [...new Set(repos)];
  const others = members.filter((member) => !moved.includes(member));
  const index = at === null ? others.length : Math.min(at, others.length);
  return [...others.slice(0, index), ...moved, ...others.slice(index)];
}

/**
 * Applies `change` to the groups of open workspace `workspaceId`, keeps the
 * result, sends `workspace_changed` and answers with `data`, like the core.
 */
function changeGroups<T>(
  workspaceId: WorkspaceId,
  change: (groups: GroupView[], workspace: WorkspaceView) => { groups: GroupView[]; data: T } | { refused: string },
) {
  const workspace = openWorkspace(workspaceId);
  if (workspace === undefined) {
    return notFound(`no open workspace has the id ${workspaceId}`);
  }
  const result = change(workspace.groups, workspace);
  if ("refused" in result) {
    return nameRefused(result.refused);
  }
  const changed = { ...workspace, groups: result.groups };
  setFakeWorkspace(changed);
  emitWorkspaceChanged(changed);
  return Promise.resolve({ status: "ok" as const, data: result.data });
}

/** Like the core's group changes (GRP-2, GRP-3): only the workspace changes. */
const fakeGroupCommands: Pick<
  Core["commands"],
  | "groupCreate"
  | "groupRename"
  | "groupDelete"
  | "groupAddRepos"
  | "groupRemoveRepos"
  | "groupMoveRepos"
  | "groupMoveRepoBetween"
  | "groupReorder"
> = {
  groupCreate: (workspaceId, name, repoIds) =>
    changeGroups(workspaceId, (groups) => {
      const checked = checkGroupName(groups, name, null);
      if ("refused" in checked) {
        return checked;
      }
      const used = groups.map((group) => Number(group.id.slice(1))).filter(Number.isFinite);
      const id = `g${Math.max(0, ...used) + 1}`;
      return { groups: [...groups, { id, name: checked.name, repos: [...new Set(repoIds)] }], data: id };
    }),
  groupRename: (workspaceId, groupId, name) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const checked = checkGroupName(groups, name, groupId);
      if ("refused" in checked) {
        return checked;
      }
      const newName = checked.name;
      const renamed = groups.map((group) => (group.id === groupId ? { ...group, name: newName } : group));
      return { groups: renamed, data: { ...workspace, groups: renamed } };
    }),
  groupDelete: (workspaceId, groupId) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const kept = groups.filter((group) => group.id !== groupId);
      return { groups: kept, data: { ...workspace, groups: kept } };
    }),
  groupAddRepos: (workspaceId, groupId, repoIds, at) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const changed = groups.map((group) =>
        group.id === groupId ? { ...group, repos: placeRepos(group.repos, repoIds, at) } : group,
      );
      return { groups: changed, data: { ...workspace, groups: changed } };
    }),
  groupRemoveRepos: (workspaceId, groupId, repoIds) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const changed = groups.map((group) =>
        group.id === groupId ? { ...group, repos: group.repos.filter((repo) => !repoIds.includes(repo)) } : group,
      );
      return { groups: changed, data: { ...workspace, groups: changed } };
    }),
  groupMoveRepos: (workspaceId, groupId, repoIds, at) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const changed = groups.map((group) =>
        group.id === groupId ? { ...group, repos: placeRepos(group.repos, repoIds, at) } : group,
      );
      return { groups: changed, data: { ...workspace, groups: changed } };
    }),
  groupMoveRepoBetween: (workspaceId, repoId, from, to, at) =>
    changeGroups(workspaceId, (groups, workspace) => {
      const changed = groups.map((group) => {
        if (group.id === from) {
          return { ...group, repos: group.repos.filter((repo) => repo !== repoId) };
        }
        return group.id === to ? { ...group, repos: placeRepos(group.repos, [repoId], at) } : group;
      });
      return { groups: changed, data: { ...workspace, groups: changed } };
    }),
  groupReorder: (workspaceId, groupIds) =>
    changeGroups<WorkspaceView>(workspaceId, (groups, workspace) => {
      const ordered = groupIds.flatMap((id) => groups.filter((group) => group.id === id));
      if (ordered.length !== groups.length || new Set(groupIds).size !== groupIds.length) {
        return { refused: "the group order must name each group once" };
      }
      return { groups: ordered, data: { ...workspace, groups: ordered } };
    }),
};

/** Like the core's `target_repos` (GRP-5): a group's members, or repos in tree order, each once. */
const fakeTargetRepos: Core["commands"]["targetRepos"] = (workspaceId, target) => {
  const workspace = openWorkspace(workspaceId);
  if (workspace === undefined) {
    return notFound(`no open workspace has the id ${workspaceId}`);
  }
  const known = new Set(workspace.repos.map((repo) => repo.id));
  const wanted = target.kind === "repo" ? [target.repo_id] : target.kind === "repos" ? target.repo_ids : [];
  const unknown = wanted.find((repo) => !known.has(repo));
  if (unknown !== undefined) {
    return notFound(`the workspace has no repo with the id ${unknown}`);
  }
  if (target.kind === "group") {
    const group = workspace.groups.find((known) => known.id === target.group_id);
    return group === undefined
      ? notFound(`the workspace has no group with the id ${target.group_id}`)
      : Promise.resolve({ status: "ok" as const, data: group.repos });
  }
  const order = [...new Set([...workspace.groups.flatMap((group) => group.repos), ...known])];
  return Promise.resolve({ status: "ok" as const, data: order.filter((repo) => wanted.includes(repo)) });
};

/** Like the core's tab changes, for the fake's workspace files (UI-27, UI-28). */
const fakeTabCommands: Pick<
  Core["commands"],
  | "workspaceList"
  | "workspaceCreate"
  | "workspaceOpen"
  | "workspaceClose"
  | "workspaceRename"
  | "workspaceDelete"
  | "workspaceReorder"
  | "workspaceActivate"
> = {
  workspaceList: () => {
    const items = fakeWorkspaces().map((workspace) => ({
      id: workspace.id,
      name: workspace.name,
      open: fakeState.tabs.includes(workspace.id),
      warning: workspace.warning,
    }));
    items.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()) || a.id.localeCompare(b.id));
    return Promise.resolve({ status: "ok", data: items });
  },
  workspaceCreate: (name) => {
    const checked = checkName(name, null);
    if (checked.name === undefined) {
      return nameRefused(checked.refused);
    }
    const id = `fake-created-${nextWorkspace++}`;
    fakeState.others = [...fakeState.others, { ...defaultWorkspace, id, name: checked.name }];
    fakeState.tabs = [...fakeState.tabs, id];
    fakeState.active = id;
    return changedTabs();
  },
  workspaceOpen: (workspaceId) => {
    if (!fakeWorkspaces().some((workspace) => workspace.id === workspaceId)) {
      return notFound(`no workspace file has the id ${workspaceId}`);
    }
    if (!fakeState.tabs.includes(workspaceId)) {
      fakeState.tabs = [...fakeState.tabs, workspaceId];
    }
    fakeState.active = workspaceId;
    return changedTabs();
  },
  workspaceClose: (workspaceId) => {
    const index = fakeState.tabs.indexOf(workspaceId);
    if (index < 0) {
      return notFound(`no open workspace has the id ${workspaceId}`);
    }
    fakeState.tabs = fakeState.tabs.filter((id) => id !== workspaceId);
    // Like the core: the tab that takes its place, else the one before it.
    if (fakeState.active === workspaceId) {
      fakeState.active = fakeState.tabs[index] ?? fakeState.tabs[fakeState.tabs.length - 1] ?? null;
    }
    releaseWorkspace(workspaceId);
    return changedTabs();
  },
  workspaceRename: (workspaceId, name) => {
    const workspace = fakeWorkspaces().find((known) => known.id === workspaceId);
    if (workspace === undefined) {
      return notFound(`no workspace file has the id ${workspaceId}`);
    }
    const checked = checkName(name, workspaceId);
    if (checked.name === undefined) {
      return nameRefused(checked.refused);
    }
    const renamed = { ...workspace, name: checked.name };
    setFakeWorkspace(renamed);
    if (fakeState.tabs.includes(workspaceId)) {
      emitWorkspaceChanged(renamed);
    }
    return changedTabs();
  },
  workspaceDelete: (workspaceId, confirmed) => {
    if (!fakeWorkspaces().some((workspace) => workspace.id === workspaceId)) {
      return notFound(`no workspace file has the id ${workspaceId}`);
    }
    if (!confirmed) {
      return nameRefused("deleting a workspace needs confirmation");
    }
    if (fakeState.tabs.includes(workspaceId)) {
      return nameRefused("the workspace is open; close its tab first");
    }
    if (workspaceId === fakeState.workspace.id) {
      return nameRefused("the fake core keeps its first workspace");
    }
    fakeState.others = fakeState.others.filter((workspace) => workspace.id !== workspaceId);
    return Promise.resolve({ status: "ok", data: null });
  },
  workspaceReorder: (workspaceIds) => {
    const sorted = [...workspaceIds].sort();
    const open = [...fakeState.tabs].sort();
    if (sorted.length !== open.length || sorted.some((id, index) => id !== open[index])) {
      return nameRefused("the tab order must name each open workspace once");
    }
    fakeState.tabs = [...workspaceIds];
    return changedTabs();
  },
  workspaceActivate: (workspaceId) => {
    if (!fakeState.tabs.includes(workspaceId)) {
      return notFound(`no open workspace has the id ${workspaceId}`);
    }
    const before = fakeState.active;
    fakeState.active = workspaceId;
    if (before !== null && before !== workspaceId) {
      releaseWorkspace(before);
    }
    return changedTabs();
  },
};

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

/**
 * Like the core's clone: refused before anything runs for a bad URL, target
 * or group; else a job with a transfer step that ends, when it succeeds,
 * with the repo in the workspace, or filling in the missing one, and in its
 * group, then `clone_report` naming it.
 */
function fakeClone(workspaceId: WorkspaceId, request: CloneRequest) {
  const owner = fakeWorkspaces().find((workspace) => workspace.id === workspaceId) ?? fakeState.workspace;
  const group = request.group;
  const named = group?.kind === "new" ? checkGroupName(owner.groups, group.name, null) : null;
  const why =
    (request.url.trim() === "" ? "the URL is empty" : null) ??
    fakeTargetRefusal(fakeState.folders, request.parent, request.name) ??
    (named !== null && "refused" in named ? named.refused : null);
  if (why !== null) {
    return refused(why);
  }
  fakeState.cloneRequests.push(request);
  const jobId = nextJobId++;
  const job = fakeJob(fakeState.workspace, {
    job_id: jobId,
    workspace_id: workspaceId,
    label: `Clone ${request.name}`,
    repos: [],
    repo_names: [],
    state: "running",
    outcome: null,
    started_at: Date.now(),
    duration_ms: null,
  });
  setTimeout(() => {
    sendJob({ ...job, step: "Receiving objects", done: 1, total: 2 });
    const failure = fakeState.cloneFailure;
    const path = `${request.parent}/${request.name}`;
    const known = fakeWorkspaces().find((workspace) => workspace.id === workspaceId) ?? fakeState.workspace;
    const id = request.repo ?? `r${known.repos.length + 1}`;
    const repo = failure === null ? { id, name: request.name, path, remote: request.url.trim() } : null;
    if (repo !== null) {
      const repos =
        request.repo === null
          ? [...known.repos, repo]
          : known.repos.map((other) => (other.id === request.repo ? { ...other, path, name: request.name } : other));
      let groups = known.groups;
      if (group?.kind === "existing") {
        groups = groups.map((other) =>
          other.id === group.group_id ? { ...other, repos: [...other.repos.filter((r) => r !== id), id] } : other,
        );
      } else if (group?.kind === "new") {
        groups = [...groups, { id: `g${groups.length + 1}`, name: group.name.trim(), repos: [id] }];
      }
      const changed = { ...known, repos, groups };
      setFakeWorkspace(changed);
      fakeState.missingRepos = fakeState.missingRepos.filter((missing) => missing.repo_id !== request.repo);
      fakeState.folders[path] = "full";
      emitWorkspaceChanged(changed);
    }
    const message = failure === null ? null : `git exited with code 128: ${failure.trim()}`;
    sendJob({
      ...job,
      state: "ended",
      outcome: repo === null ? { kind: "failed" } : { kind: "succeeded" },
      output: failure ?? `Cloning into '${path}'...\n`,
      message,
      duration_ms: 10,
    });
    emit(cloneReportListeners, "clone_report", { workspace_id: workspaceId, job_id: jobId, repo, message });
  }, 10);
  return Promise.resolve({ status: "ok" as const, data: jobId });
}

/** Like the core's export, import and missing repos, with the dialogs' answers from `fakeState` (DATA-2). */
const fakeShareCommands: Pick<Core["commands"], "workspaceExport" | "workspaceImport" | "workspaceMissingRepos"> = {
  workspaceExport: (workspaceId) => {
    if (openWorkspace(workspaceId) === undefined) {
      return notFound(`no open workspace has the id ${workspaceId}`);
    }
    fakeState.exports.push(workspaceId);
    const answer = fakeState.exportAnswer;
    if (answer !== null) {
      const payload =
        "error" in answer
          ? { workspace_id: workspaceId, report: null, error: answer.error }
          : { workspace_id: workspaceId, report: answer, error: null };
      setTimeout(() => emit(workspaceExportedListeners, "workspace_exported", payload), 0);
    }
    return Promise.resolve({ status: "ok", data: null });
  },
  workspaceImport: () => {
    fakeState.imports += 1;
    const answer = fakeState.importAnswer;
    if (answer !== null) {
      setTimeout(() => {
        if ("error" in answer) {
          emit(workspaceImportedListeners, "workspace_imported", { report: null, error: answer.error });
          return;
        }
        // Like the core: " (2)" and so on when the name is taken.
        const taken = new Set(fakeWorkspaces().map((workspace) => workspace.name.toLowerCase()));
        let name = answer.workspace.name;
        for (let number = 2; taken.has(name.toLowerCase()); number++) {
          name = `${answer.workspace.name} (${number})`;
        }
        const workspace = { ...answer.workspace, name };
        fakeState.others = [...fakeState.others, workspace];
        fakeState.tabs = [...fakeState.tabs, workspace.id];
        fakeState.active = workspace.id;
        emitWorkspacesChanged();
        const report = { workspace_id: workspace.id, missing: answer.missing };
        emit(workspaceImportedListeners, "workspace_imported", { report, error: null });
      }, 0);
    }
    return Promise.resolve({ status: "ok", data: null });
  },
  workspaceMissingRepos: (workspaceId) =>
    openWorkspace(workspaceId) === undefined
      ? notFound(`no open workspace has the id ${workspaceId}`)
      : Promise.resolve({ status: "ok", data: fakeState.missingRepos }),
};

/** Like the core's file manager and terminal commands: recorded, or failing with `openFailure`. */
function fakeOpen(how: "file_manager" | "terminal", repoId: RepoId) {
  if (!fakeState.workspace.repos.some((repo) => repo.id === repoId)) {
    return notFound(`the workspace has no repo with the id ${repoId}`);
  }
  if (fakeState.openFailure !== null) {
    return refused(fakeState.openFailure);
  }
  fakeState.opened.push({ how, repo: repoId });
  return Promise.resolve({ status: "ok" as const, data: null });
}

export const fake: Core = {
  commands: {
    appInfo: () => Promise.resolve({ status: "ok", data: fakeState.appInfo }),
    gitStatus: () => Promise.resolve({ status: "ok", data: fakeDetection() }),
    gitRecheck: fakeRecheck,
    settingsSetGitPath: fakeSetGitPath,
    // Like the core: the dialog answers later; a cancel does nothing.
    gitPickPath: () => {
      const picked = fakeState.pickedGit;
      if (picked !== null) {
        setTimeout(() => void fakeSetGitPath(picked), 0);
      }
      return Promise.resolve({ status: "ok", data: null });
    },
    settingsGet: () => Promise.resolve({ status: "ok", data: fakeState.settings }),
    ...updateCommands(updateDeps),
    // Like the core: read-only settings refuse the change.
    settingsSetUpdateCheck: (enabled) => {
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
      fakeState.settings = { ...fakeState.settings, update_check: enabled };
      return Promise.resolve({ status: "ok", data: fakeState.settings });
    },
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
    workspaceGet: () => Promise.resolve({ status: "ok", data: fakeTabs() }),
    ...fakeTabCommands,
    ...fakeGroupCommands,
    ...fakeShareCommands,
    targetRepos: fakeTargetRepos,
    // Unlike the core until the group operations arrive (M3-10 to M3-13):
    // every operation runs, as fakeGroups.ts sets it (M3-09).
    groupPreflight: async (workspaceId, target, op) => {
      const workspace = openWorkspace(workspaceId);
      const repos = await fakeTargetRepos(workspaceId, target);
      if (workspace === undefined || repos.status === "error") {
        return repos.status === "error" ? repos : notFound(`no open workspace has the id ${workspaceId}`);
      }
      if (repos.data.length === 0) {
        return nameRefused("the target set has no repo");
      }
      return { status: "ok", data: fakePreflight(groupDeps, workspace, target, repos.data, op) };
    },
    groupStart: (preflightId, confirmed) => {
      const name = (id: WorkspaceId) => fakeWorkspaces().find((known) => known.id === id)?.name ?? id;
      return Promise.resolve(fakeStart(groupDeps, name, preflightId, confirmed));
    },
    groupDrop: (preflightId) => {
      fakeDrop(preflightId);
      return Promise.resolve();
    },
    groupRetryFailed: (jobId) => Promise.resolve(fakeRetry(groupDeps, fakeState.jobs, openWorkspace, jobId)),
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
      if (openWorkspace(workspaceId) === undefined) {
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
      if (openWorkspace(workspaceId) === undefined) {
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
      const workspace = openWorkspace(workspaceId);
      if (workspace === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      return Promise.resolve({ status: "ok", data: skeleton(workspace, fakeState.summaries) });
    },
    refsGet: (workspaceId, repoId, groupId) => {
      if (!openWorkspace(workspaceId)?.repos.some((repo) => repo.id === repoId)) {
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
      return Promise.resolve({ status: "ok", data: inPlace(refs, groupId) });
    },
    groupBranches: (workspaceId, groupId) => {
      const group = openWorkspace(workspaceId)?.groups.find((known) => known.id === groupId);
      if (group === undefined) {
        return notFound(`the workspace has no group with the id ${groupId}`);
      }
      return Promise.resolve({ status: "ok", data: groupBranchNodes(group, fakeState.refs) });
    },
    treeFilter: (workspaceId, text) => {
      const workspace = openWorkspace(workspaceId);
      if (workspace === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      return Promise.resolve({ status: "ok", data: filterTree(workspace, fakeState.refs, text, fakeState.summaries) });
    },
    // Nothing matches yet: the palette's window and the fake's matching come with M5-08.
    paletteQuery: (workspaceId, text) => {
      const workspace = openWorkspace(workspaceId);
      if (workspace === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      const answer = () => fakePaletteQuery(workspace, fakeState.summaries, fakeState.refModels, fakeAi.enabled, text);
      if (!fakeState.holdPalette) {
        return Promise.resolve({ status: "ok", data: answer() });
      }
      return new Promise((resolve) =>
        fakeState.heldPalette.push((superseded) => resolve({ status: "ok", data: superseded ? null : answer() })),
      );
    },
    historyGet: async (workspaceId, repoId, target, text) => {
      const workspace = openWorkspace(workspaceId);
      const named = (id: RepoId) => workspace?.repos.find((candidate) => candidate.id === id);
      const walkOf = (id: RepoId, shown: HistoryTarget) => ({
        history: fakeState.histories[historyKey(id, shown)] ?? defaultHistory(named(id)?.name ?? id, shown),
        text,
        next: 0,
      });
      let walk: { repos: RepoId[]; next: () => { rows: HistoryRow[]; done: boolean } };
      if (target.kind === "merged") {
        const resolved = await fakeTargetRepos(workspaceId, target.target);
        if (resolved.status === "error") {
          return resolved;
        }
        // Each repo's walk from HEAD: its whole fake history.
        const merged = mergedWalk(resolved.data.map((id) => ({ repo: id, walk: walkOf(id, { kind: "repo" }) })));
        walk = { repos: resolved.data, next: () => nextMergedPage(merged) };
      } else {
        const repo = repoId === null ? undefined : named(repoId);
        if (repoId === null || repo === undefined) {
          return notFound(`the workspace has no repo with the id ${repoId}`);
        }
        const single = walkOf(repoId, target);
        walk = {
          repos: [repoId],
          next: () => {
            const page = nextPage(single);
            return { ...page, rows: page.rows.map((row) => ({ ...row, repo_id: repoId })) };
          },
        };
      }
      fakeState.historyCalls.push({ command: "get", text });
      const handle = nextHandle++;
      walks.set(workspaceId, { handle, ...walk });
      const page = walk.next();
      const offer = repoId !== null && !!fakeState.slowRepos[repoId] && !fakeState.declined[repoId];
      // Like the core: `null` once a newer walk of the workspace began.
      return later({
        status: "ok" as const,
        data:
          walks.get(workspaceId)?.handle === handle
            ? { handle, ...page, repos: walk.repos, offer_commit_graph: offer }
            : null,
      });
    },
    historyMore: (workspaceId, handle) => {
      fakeState.historyCalls.push({ command: "more" });
      const walk = walks.get(workspaceId);
      if (walk?.handle !== handle) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const page = walk.next();
      return later({
        status: "ok" as const,
        data:
          walks.get(workspaceId)?.handle === handle
            ? { handle, ...page, repos: walk.repos, offer_commit_graph: false }
            : null,
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
    // Like the core: a parallelism outside 1 to 32 is refused.
    settingsSetGroups: (groups) => {
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
      if (groups.group_parallelism < 1 || groups.group_parallelism > 32) {
        return nameRefused("group operations run 1 to 32 repos at once");
      }
      fakeState.settings = { ...fakeState.settings, settings: { ...fakeState.settings.settings, ...groups } };
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
    workspaceSetAi: (workspaceId, enabled) => {
      const workspace = openWorkspace(workspaceId);
      if (workspace === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      const changed = { ...workspace, ai: enabled };
      setFakeWorkspace(changed);
      emitWorkspaceChanged(changed);
      return Promise.resolve({ status: "ok", data: changed });
    },
    workspaceSetLayout: (workspaceId, layout) => {
      const workspace = openWorkspace(workspaceId);
      if (workspace === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      fakeState.layouts.push(layout);
      setFakeWorkspace({ ...workspace, layout });
      return Promise.resolve({ status: "ok", data: null });
    },
    changesGet: (workspaceId, repoId, target) =>
      held<Awaited<ReturnType<Core["commands"]["changesGet"]>>>(() => {
        if (!openWorkspace(workspaceId)?.repos.some((repo) => repo.id === repoId)) {
          return notFound(`the workspace has no repo with the id ${repoId}`);
        }
        fakeState.changesCalls.push({ command: "changes", repo: repoId, target });
        const handle = nextHandle++;
        let content: ChangesContent;
        let lists: Partial<Record<FileRef["list"], FakeFile[]>>;
        let unmerged: string[] = [];
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
            offer_fsmonitor: fakeState.fsmonitorOffers[repoId] ?? null,
          };
          lists = { staged, unstaged };
          unmerged = working.unmerged.map((file) => file.path);
        }
        views.set(workspaceId, [{ handle, lists, unmerged, repo: repoId }]);
        imageFiles.delete(workspaceId);
        // Like the core: `null` once a newer view of the workspace began.
        return slowly(() => ({
          status: "ok" as const,
          data: viewPart(workspaceId, handle) !== undefined ? { handle, content } : null,
        }));
      }),
    // Like the core: one part per dirty repo of the target set, each with its own handle (UI-34).
    changesGetMerged: async (workspaceId, target) => {
      const workspace = openWorkspace(workspaceId);
      const resolved = await fakeTargetRepos(workspaceId, target);
      if (workspace === undefined || resolved.status === "error") {
        return resolved.status === "error" ? resolved : notFound(`no open workspace has the id ${workspaceId}`);
      }
      fakeState.changesCalls.push({ command: "merged", target });
      const members = resolved.data;
      const parts: FakeViewPart[] = [];
      const repos: RepoChanges[] = [];
      for (const repoId of members) {
        const working = fakeState.workingCopies[repoId];
        if (
          working === undefined ||
          working.staged.length + working.unstaged.length + working.unmerged.length === 0
        ) {
          continue;
        }
        const staged = numbered(working.staged);
        const unstaged = numbered(working.unstaged);
        const handle = nextHandle++;
        parts.push({ handle, lists: { staged, unstaged }, unmerged: working.unmerged.map((file) => file.path), repo: repoId });
        repos.push({
          repo_id: repoId,
          repo_name: workspace.repos.find((repo) => repo.id === repoId)?.name ?? repoId,
          handle,
          staged: staged.map((file, id) => ({ id, ...file.file })),
          unstaged: unstaged.map((file, id) => ({ id, ...file.file })),
          unmerged: working.unmerged.map((file, id) => ({ id, ...file })),
          blocked: working.inConflict ? "the repo is in conflict state; continue or abort first" : null,
        });
      }
      views.set(workspaceId, parts);
      imageFiles.delete(workspaceId);
      const first = parts[0]?.handle;
      return slowly(() => ({
        status: "ok" as const,
        data:
          first === undefined || viewPart(workspaceId, first) !== undefined
            ? { handle: 0, content: { kind: "merged" as const, repos, members } }
            : null,
      }));
    },
    diffGet: (workspaceId, handle, file, request) => {
      fakeState.changesCalls.push({ command: "diff", file, request });
      const view = viewPart(workspaceId, handle);
      if (view === undefined) {
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
        data: viewPart(workspaceId, handle) !== undefined ? diff : null,
      }));
    },
    // Like the core: a discard asks for confirmation first, then a job
    // changes the working copy and the repo's summary is read again.
    changesApply: (workspaceId, request) => {
      fakeState.changeRequests.push(request);
      const view = viewPart(workspaceId, request.handle);
      if (view === undefined) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const target = request.target;
      const refs = target.kind === "files" ? target.files : [target.file];
      const files = refs.flatMap((ref) => view.lists[ref.list]?.[ref.id] ?? []);
      const repo = fakeState.workspace.repos.find((known) => known.id === view.repo);
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
      if (viewPart(workspaceId, request.handle) === undefined) {
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
    opSync: (workspaceId, request) => fakeSyncOp(workspaceId, request),
    pushChoices: (_workspaceId, repoId) => {
      const refs = fakeState.refModels[repoId];
      return refs === undefined
        ? notFound(`no repo ${repoId}`)
        : Promise.resolve({ status: "ok" as const, data: pushChoicesOf(refs) });
    },
    opIntegrate: (workspaceId, request) => fakeIntegrateOp(workspaceId, request),
    opStash: (workspaceId, request) => fakeStashOp(workspaceId, request),
    opTag: (workspaceId, request) => fakeTagOp(workspaceId, request),
    tagNameCheck: (name) => Promise.resolve({ status: "ok", data: fakeNameCheck(name) }),
    // A reset never asks for a remote.
    opRemote: (workspaceId, request) =>
      // A remote operation never asks for a remote.
      fakeRemoteOp(workspaceId, request) as Promise<
        { status: "ok"; data: RemoteStart } | { status: "error"; error: IpcError }
      >,
    remoteNameCheck: (name) => Promise.resolve({ status: "ok", data: fakeRemoteNameCheck(name) }),
    remoteUrls: (_workspaceId, repoId, name) => {
      const refs = fakeState.refModels[repoId];
      if (refs?.remotes?.[name] === undefined) {
        return refused(`there is no remote ${name}`);
      }
      return Promise.resolve({ status: "ok", data: refs.urls?.[name] ?? { url: null, push_url: null } });
    },
    // The settings are on at once; no job is shown.
    fsmonitorEnable: (_workspaceId, repoId) => {
      const blocked = blockedWhy(repoId);
      if (blocked !== null) {
        return refused(blocked);
      }
      fakeFsmonitorAnswer(repoId, "enable");
      const jobId = nextJobId++;
      return Promise.resolve({ status: "ok", data: jobId });
    },
    clonePickParent: (workspaceId, key) => {
      if (openWorkspace(workspaceId) === undefined) {
        return notFound(`no open workspace has the id ${workspaceId}`);
      }
      const path = fakeState.cloneParent;
      if (path !== null) {
        const picked = { workspace_id: workspaceId, path, key };
        setTimeout(() => emit(cloneParentPickedListeners, "clone_parent_picked", picked), 0);
      }
      return Promise.resolve({ status: "ok", data: null });
    },
    cloneDefaultName: (url) => Promise.resolve({ status: "ok", data: fakeDefaultName(url) }),
    cloneTargetCheck: (parent, name) =>
      Promise.resolve({ status: "ok", data: fakeTargetRefusal(fakeState.folders, parent, name) }),
    cloneStart: (workspaceId, request) =>
      fakeClone(workspaceId, request) as Promise<{ status: "ok"; data: JobId } | { status: "error"; error: IpcError }>,
    repoOpenFileManager: (_workspaceId, repoId) => fakeOpen("file_manager", repoId),
    repoOpenTerminal: (_workspaceId, repoId) => fakeOpen("terminal", repoId),
    fsmonitorDecline: (_workspaceId, repoId) => {
      fakeFsmonitorAnswer(repoId, "decline");
      return Promise.resolve({ status: "ok", data: null });
    },
    opReset: (workspaceId, request) =>
      fakeResetOp(workspaceId, request) as Promise<{ status: "ok"; data: ResetStart } | { status: "error"; error: IpcError }>,
    conflictGet: (workspaceId, handle, file) => {
      const view = viewPart(workspaceId, handle);
      if (view === undefined) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const path = view.unmerged[file];
      if (path === undefined) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: `the list has no file ${file}`, git_stderr: null },
        });
      }
      return Promise.resolve({
        status: "ok",
        data: unmergedPaths(view.repo).includes(path)
          ? {
              kind: "conflict",
              conflict: fakeState.fileConflicts[path] ?? textConflict(path, labelsOf(fakeState.conflicts[view.repo])),
            }
          : { kind: "already_resolved" },
      });
    },
    aiEligibility: (workspaceId, handle, file) => {
      const view = viewPart(workspaceId, handle);
      if (view === undefined) {
        return Promise.resolve({ status: "ok", data: null });
      }
      const path = view.unmerged[file];
      if (path === undefined) {
        return Promise.resolve({
          status: "error",
          error: { code: "internal", message: `the list has no file ${file}`, git_stderr: null },
        });
      }
      return Promise.resolve({
        status: "ok",
        data: unmergedPaths(view.repo).includes(path)
          ? { kind: "read", eligibility: fakeState.aiEligibility[path] ?? { kind: "eligible" } }
          : { kind: "already_resolved" },
      });
    },
    aiResolve: (workspaceId, request) => {
      fakeState.aiResolveRequests.push(request);
      const repo = viewPart(workspaceId, request.handle)?.repo;
      return fakeUnmerged(
        workspaceId,
        request.handle,
        request.file,
        (path): AiResolveStart => fakeState.aiResolve ?? fakeAiResolve(workspaceId, repo ?? "", path),
      );
    },
    aiResolveAll: (workspaceId, request) =>
      Promise.resolve({ status: "ok", data: fakeAiResolveAll(workspaceId, request) }),
    aiProposalGet: (workspaceId, handle, file) =>
      fakeUnmerged(
        workspaceId,
        handle,
        file,
        (path): AiProposalGet => fakeState.aiProposals[path] ?? { kind: "no_proposal" },
      ),
    conflictResolve: (workspaceId, request) => fakeResolve(workspaceId, request),
    conflictChoose: (workspaceId, request) => fakeResolve(workspaceId, request),
    mergeTargets: (_workspaceId, repoId, filter) => {
      const refs = fakeState.refModels[repoId];
      return refs === undefined
        ? notFound(`no repo ${repoId}`)
        : Promise.resolve({ status: "ok" as const, data: mergeTargetsOf(refs, filter) });
    },
    conflictDetails: (_workspaceId, repoId) =>
      Promise.resolve({ status: "ok" as const, data: fakeState.conflicts[repoId] ?? NO_CONFLICT }),
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
      if (aiJobs.has(jobId)) {
        endAiJob(jobId, null);
        return Promise.resolve({ status: "ok", data: "cancelled" });
      }
      if (aiRuns.has(jobId)) {
        cancelRun(jobId);
        return Promise.resolve({ status: "ok", data: "killing" });
      }
      const job = fakeState.jobs.find((known) => known.job_id === jobId);
      if (job?.state === "queued") {
        sendJob({
          ...job,
          state: "ended",
          outcome: { kind: "skipped", reason: { kind: "cancelled" } },
          message: "cancelled",
        });
        return Promise.resolve({ status: "ok", data: "cancelled" });
      }
      return Promise.resolve({ status: "ok", data: job?.state === "running" ? "finishing" : "ended" });
    },
    ...aiCommands,
    // Turning AI features off cancels the proposal jobs and drops the proposals (AI-16).
    aiSettingsSet: async (options) => {
      const answer = await aiCommands.aiSettingsSet(options);
      if (!options.enabled) {
        for (const jobId of [...aiJobs.keys()]) {
          endAiJob(jobId, null);
        }
        for (const jobId of [...aiRuns.keys()]) {
          cancelRun(jobId);
        }
        fakeState.aiProposals = {};
      }
      return answer;
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
    cloneParentPicked: fakeEvent(cloneParentPickedListeners),
    cloneReport: fakeEvent(cloneReportListeners),
    workspaceExported: fakeEvent(workspaceExportedListeners),
    workspaceImported: fakeEvent(workspaceImportedListeners),
    commitGraphWritten: fakeEvent(commitGraphWrittenListeners),
    gitStatus: fakeEvent(gitStatusListeners),
    jobEnded: fakeEvent(jobEndedListeners),
    jobProgress: fakeEvent(jobProgressListeners),
    repoRefsChanged: fakeEvent(repoRefsChangedListeners),
    repoSummary: fakeEvent(repoSummaryListeners),
    watchWarning: fakeEvent(watchWarningListeners),
    workspaceAddReport: fakeEvent(workspaceAddReportListeners),
    workspaceChanged: fakeEvent(workspaceChangedListeners),
    workspacesChanged: fakeEvent(workspacesChangedListeners),
    groupSummary: fakeEvent(groupSummaryListeners),
    preflightReady: fakeEvent(preflightReadyListeners),
    aiChanged: fakeEvent(aiChangedListeners),
    aiModels: fakeEvent(aiModelsListeners),
    aiSelfTest: fakeEvent(aiSelfTestListeners),
    aiProposalReady: fakeEvent(aiProposalReadyListeners),
    updateStatusChanged: fakeEvent(updateStatusListeners),
  },
};
