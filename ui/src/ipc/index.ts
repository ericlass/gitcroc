// The only way into the core. Components import from here, never from
// `@tauri-apps/api` or `./bindings` directly (enforced by eslint.config.js).
import type {
  Appearance,
  BranchRequest,
  ChangeRequest,
  ChangesSettings,
  ChangesTarget,
  ChooseRequest,
  CommitRequest,
  DiffRequest,
  FileRef,
  HistoryTarget,
  ImageSide,
  IntegrateRequest,
  IpcError,
  JobId,
  PaneLayout,
  PerfTrace,
  RemoteRequest,
  RepoId,
  ResetRequest,
  ResolveRequest,
  StashRequest,
  SyncRequest,
  TagRequest,
  Theme,
  WorkspaceId,
} from "./bindings";
import type { Core } from "./core";
import { fake } from "./fake";
import { real } from "./core";

export type {
  AddReport,
  AppInfo,
  Appearance,
  AskpassPrompt,
  BranchChoices,
  BranchOp,
  BranchRequest,
  BranchStart,
  CancelResult,
  ChangeAction,
  ChoiceKind,
  ChooseRequest,
  Conflict,
  ConflictChoice,
  ConflictChunk,
  ConflictContent,
  ConflictLabels,
  ConflictRead,
  ConflictStart,
  ChangeRequest,
  ChangeStart,
  ChangeTarget,
  ConflictDetails,
  CodeTheme,
  ChangedFile,
  ChangesContent,
  ChangesSettings,
  ChangesTarget,
  ChangesView,
  CommitGraphWrittenEvent,
  CommitDetails,
  CommitMessage,
  CommitRequest,
  CommitRow,
  UnmergedKind,
  ConfigWarning,
  Context,
  DiffMode,
  DiffRequest,
  Dock,
  Edge,
  FileDiff,
  FileList,
  FileListMode,
  FileRef,
  FileStatus,
  GitStatus,
  GraphRow,
  Head,
  HistoryTarget,
  HistoryView,
  Hunk,
  Image,
  ImageFormat,
  ImageSide,
  IntegrateOp,
  IntegrateRequest,
  IntegrateStart,
  IpcError,
  JobId,
  JobState,
  JobView,
  Line,
  LineKind,
  MergeFastForward,
  ModeChange,
  Outcome,
  PaneLayout,
  PerfSample,
  PerfTrace,
  Person,
  Operation,
  PromptKind,
  Prune,
  PullStrategy,
  PushChoices,
  RefKind,
  RefLabel,
  Refusal,
  RemoteBranchRef,
  RemoteOp,
  RemoteRequest,
  RemoteStart,
  RemoteUrlsView,
  RepoId,
  RepoRef,
  RepoState,
  RepoSummary,
  RepoView,
  ResolveRequest,
  Settings,
  SideHolds,
  SettingsView,
  StartPoint,
  StashOp,
  StashRef,
  StashRequest,
  StashStart,
  TagOp,
  TagRequest,
  TagStart,
  ResetMode,
  ResetRequest,
  ResetStart,
  Step,
  SummaryView,
  SyncOp,
  SyncRequest,
  SyncStart,
  TextDiff,
  Theme,
  Time,
  TreeItem,
  TreeNode,
  UnmergedFile,
  Upstream,
  WatchWarning,
  WorkspaceId,
  WorkspaceView,
} from "./bindings";

// A build-time constant, so production builds drop the fake.
export const core: Core = __FAKE_CORE__ ? fake : real;

/** A command failed in the core. */
export class IpcFailure extends Error {
  constructor(readonly error: IpcError) {
    super(error.message);
    this.name = "IpcFailure";
  }
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: IpcError };

async function unwrap<T>(result: Promise<Result<T>>): Promise<T> {
  const settled = await result;
  if (settled.status === "error") {
    throw new IpcFailure(settled.error);
  }
  return settled.data;
}

export const appInfo = () => unwrap(core.commands.appInfo());

export const gitStatus = () => unwrap(core.commands.gitStatus());

export const gitRecheck = () => unwrap(core.commands.gitRecheck());

export const settingsGet = () => unwrap(core.commands.settingsGet());

export const settingsSetTheme = (theme: Theme) => unwrap(core.commands.settingsSetTheme(theme));

export const workspaceGet = () => unwrap(core.commands.workspaceGet());

export const workspaceAddPath = (workspaceId: WorkspaceId, path: string) =>
  unwrap(core.commands.workspaceAddPath(workspaceId, path));

export const workspacePickAndAdd = (workspaceId: WorkspaceId) => unwrap(core.commands.workspacePickAndAdd(workspaceId));

export const workspaceRemoveRepo = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.workspaceRemoveRepo(workspaceId, repoId));

export const workspaceSummaries = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceSummaries(workspaceId));

export const repoSelect = (workspaceId: WorkspaceId, repoId: RepoId | null) =>
  unwrap(core.commands.repoSelect(workspaceId, repoId));

/** Reads every repo again, as when the window regains focus (OPS-7). */
export const workspaceRefresh = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceRefresh(workspaceId));

/** Why watching the workspace's files failed, if it did. */
export const watchWarning = (workspaceId: WorkspaceId) => unwrap(core.commands.watchWarning(workspaceId));

export const treeGet = (workspaceId: WorkspaceId) => unwrap(core.commands.treeGet(workspaceId));

export const refsGet = (workspaceId: WorkspaceId, repoId: RepoId) => unwrap(core.commands.refsGet(workspaceId, repoId));

/** `null` when a newer filter of the workspace superseded this one. */
export const treeFilter = (workspaceId: WorkspaceId, text: string) =>
  unwrap(core.commands.treeFilter(workspaceId, text));

/** `null` when a newer walk of the workspace superseded this one. */
export const historyGet = (workspaceId: WorkspaceId, repoId: RepoId, target: HistoryTarget, text: string) =>
  unwrap(core.commands.historyGet(workspaceId, repoId, target, text));

/** `null` when a newer walk of the workspace replaced walk `handle`. */
export const historyMore = (workspaceId: WorkspaceId, handle: number) =>
  unwrap(core.commands.historyMore(workspaceId, handle));

export const commitGraphWrite = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.commitGraphWrite(workspaceId, repoId));

/** The user's answer to an askpass prompt (D10); `null` cancels it. */
export const askpassAnswer = (promptId: number, answer: string | null) =>
  unwrap(core.commands.askpassAnswer(promptId, answer));

/** This run's jobs of every workspace, newest first (UI-21). */
export const jobsGet = () => unwrap(core.commands.jobsGet());

/** Cancels a job (GRP-10); its end arrives as `job_ended`. */
export const jobCancel = (jobId: JobId) => unwrap(core.commands.jobCancel(jobId));

export const commitGraphDecline = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.commitGraphDecline(workspaceId, repoId));

export const settingsSetChanges = (changes: ChangesSettings) => unwrap(core.commands.settingsSetChanges(changes));
export const settingsSetAppearance = (appearance: Appearance) =>
  unwrap(core.commands.settingsSetAppearance(appearance));

export const workspaceSetLayout = (workspaceId: WorkspaceId, layout: PaneLayout) =>
  unwrap(core.commands.workspaceSetLayout(workspaceId, layout));

/** `null` when a newer view of the workspace superseded this one. */
export const changesGet = (workspaceId: WorkspaceId, repoId: RepoId, target: ChangesTarget) =>
  unwrap(core.commands.changesGet(workspaceId, repoId, target));

/** `null` when a newer view of the workspace replaced view `handle`. */
export const diffGet = (workspaceId: WorkspaceId, handle: number, file: FileRef, request: DiffRequest) =>
  unwrap(core.commands.diffGet(workspaceId, handle, file, request));

/** Stages, unstages or discards files or a hunk (UI-15); `null` when a newer view replaced the request's. */
export const changesApply = (workspaceId: WorkspaceId, request: ChangeRequest) =>
  unwrap(core.commands.changesApply(workspaceId, request));

/** Commits or amends as a job (UI-15); `null` when a newer view replaced the request's. */
export const changesCommit = (workspaceId: WorkspaceId, request: CommitRequest) =>
  unwrap(core.commands.changesCommit(workspaceId, request));

/** Runs a branch operation as a job; a destructive one answers `confirm` until confirmed (GRP-9). */
export const opBranch = (workspaceId: WorkspaceId, request: BranchRequest) =>
  unwrap(core.commands.opBranch(workspaceId, request));

/** What the branch dialogs offer for a repo: start points, default branch, remote branches. */
export const branchChoices = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.branchChoices(workspaceId, repoId));

/** Fetches, pulls or pushes a repo as a job; a force push answers `confirm` until confirmed (GRP-9). */
export const opSync = (workspaceId: WorkspaceId, request: SyncRequest) =>
  unwrap(core.commands.opSync(workspaceId, request));

/** What the push options offer for a repo: branch, upstream, remotes. */
export const pushChoices = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.pushChoices(workspaceId, repoId));

/** Merges, rebases, cherry-picks or reverts as a job, or aborts or continues the repo's operation (OPS-2). */
export const opIntegrate = (workspaceId: WorkspaceId, request: IntegrateRequest) =>
  unwrap(core.commands.opIntegrate(workspaceId, request));

export const opStash = (workspaceId: WorkspaceId, request: StashRequest) =>
  unwrap(core.commands.opStash(workspaceId, request));

export const opTag = (workspaceId: WorkspaceId, request: TagRequest) => unwrap(core.commands.opTag(workspaceId, request));

/** Why `name` is not a valid tag name, or `null` (SEC-4). */
export const tagNameCheck = (name: string) => unwrap(core.commands.tagNameCheck(name));

export const opReset = (workspaceId: WorkspaceId, request: ResetRequest) =>
  unwrap(core.commands.opReset(workspaceId, request));

export const opRemote = (workspaceId: WorkspaceId, request: RemoteRequest) =>
  unwrap(core.commands.opRemote(workspaceId, request));

/** Why `name` is not a valid name for a new remote, or `null` (SEC-4). */
export const remoteNameCheck = (name: string) => unwrap(core.commands.remoteNameCheck(name));

/** A remote's URLs, for the edit dialog. */
export const remoteUrls = (workspaceId: WorkspaceId, repoId: RepoId, name: string) =>
  unwrap(core.commands.remoteUrls(workspaceId, repoId, name));

/** Accepts the offer to speed up a large repo's status (PERF-7); the core turns the settings on as a job. */
export const fsmonitorEnable = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.fsmonitorEnable(workspaceId, repoId));

export const fsmonitorDecline = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.fsmonitorDecline(workspaceId, repoId));

/** The refs the merge dialog offers for a repo, filtered in the core (UI-20). */
export const mergeTargets = (workspaceId: WorkspaceId, repoId: RepoId, filter: string) =>
  unwrap(core.commands.mergeTargets(workspaceId, repoId, filter));

/** What the conflict banner names for a repo (OPS-2). */
export const conflictDetails = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.conflictDetails(workspaceId, repoId));

/** Unmerged file `file` of changes view `handle` as the conflict editor shows it (OPS-8). */
export const conflictGet = (workspaceId: WorkspaceId, handle: number, file: number) =>
  unwrap(core.commands.conflictGet(workspaceId, handle, file));

/** Mark resolved with the editor's text, as a job (OPS-5). */
export const conflictResolve = (workspaceId: WorkspaceId, request: ResolveRequest) =>
  unwrap(core.commands.conflictResolve(workspaceId, request));

/** Resolves an unmerged file with one side, as a job (OPS-6). */
export const conflictChoose = (workspaceId: WorkspaceId, request: ChooseRequest) =>
  unwrap(core.commands.conflictChoose(workspaceId, request));

/** Why `name` is not a valid branch name, or `null` (SEC-4). */
export const branchNameCheck = (name: string) => unwrap(core.commands.branchNameCheck(name));

/** One side of the image diff last read for `file`, as base64; `null` when there is none. */
export const diffImage = (workspaceId: WorkspaceId, handle: number, file: FileRef, side: ImageSide) =>
  unwrap(core.commands.diffImage(workspaceId, handle, file, side));

/** `perf-trace` builds only (`just bench-ui`): writes what the UI measured. */
export const perfTraceWrite = (trace: PerfTrace) => unwrap(core.commands.perfTraceWrite(trace));
