// The only way into the core. Components import from here, never from
// `@tauri-apps/api` or `./bindings` directly (enforced by eslint.config.js).
import type {
  AiOptions,
  AiResolveAllRequest,
  AiResolveRequest,
  Appearance,
  BranchRequest,
  ChangeRequest,
  ChangesSettings,
  ChangesTarget,
  ChooseRequest,
  CloneRequest,
  CommitRequest,
  DiffRequest,
  FileRef,
  GroupId,
  GroupOp,
  GroupSettings,
  HistoryTarget,
  ImageSide,
  IntegrateRequest,
  IpcError,
  JobId,
  PaneLayout,
  PerfTrace,
  PreflightId,
  ProviderDraft,
  ProviderId,
  RemoteRequest,
  RepoId,
  TargetSet,
  ResetRequest,
  ResolveRequest,
  StashRequest,
  SyncRequest,
  TagRequest,
  Theme,
  WorkspaceId,
} from "./bindings";
import type { Core } from "./core";

/** The palette's operations as the core lists them (UI-22): for the fake core and the tests; the UI asks the core. */
export { PALETTE_CATALOG } from "./bindings";
import { fake } from "./fake";
import { real } from "./core";

export type {
  AddReport,
  AiEligibility,
  AiEligibilityRead,
  AiFileProgress,
  AiFileState,
  AiOptions,
  AiProposalGet,
  AiProposalReady,
  AiRefused,
  AiResolveAllFile,
  AiResolveAllRequest,
  AiResolveAllStart,
  AiResolveRequest,
  AiResolveStart,
  AiRequestId,
  AiUsage,
  AiView,
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
  CloneGroup,
  CloneParentPickedEvent,
  CloneReport,
  CloneRequest,
  Conflict,
  ConflictChoice,
  ConflictChunk,
  ConflictContent,
  ConflictLabels,
  ConsentRequest,
  ConsentTarget,
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
  HistoryRow,
  UnmergedKind,
  ConfigWarning,
  Context,
  DiffMode,
  DiffRequest,
  DetectedCli,
  Dock,
  Edge,
  FileDiff,
  FileList,
  FileListMode,
  FileProposal,
  FileRef,
  FileStatus,
  GitStatus,
  GraphRow,
  Class,
  GroupBranch,
  GroupId,
  GroupOp,
  GroupSettings,
  GroupSummary,
  GroupView,
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
  MissingBranch,
  ModeChange,
  ModelsResult,
  Outcome,
  PaneLayout,
  PerfSample,
  PerfTrace,
  Person,
  Operation,
  PaletteItem,
  PaletteOp,
  PaletteResult,
  PaletteSubject,
  PreflightId,
  PreflightReady,
  PromptKind,
  ProposalConfidence,
  ProposalStatus,
  ProposedHunk,
  ProviderDraft,
  ProviderId,
  ProviderKind,
  ProviderView,
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
  RepoClass,
  RepoId,
  RepoChanges,
  RepoResult,
  RepoRef,
  RepoState,
  RepoSummary,
  RepoView,
  ResolveRequest,
  Settings,
  SideHolds,
  SkipReason,
  SettingsView,
  StartPoint,
  StashOp,
  StashRef,
  StashRequest,
  StashStart,
  SubJob,
  TagOp,
  TagRequest,
  TagStart,
  TargetSet,
  ResetMode,
  ResetRequest,
  ResetStart,
  SelfTest,
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
  UpdateStatus,
  UpdateView,
  Upstream,
  WatchWarning,
  WorkspaceId,
  WorkspaceListItem,
  WorkspaceTabs,
  WorkspaceView,
  ExportReport,
  ImportReport,
  MissingRepo,
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

export const settingsSetGitPath = (path: string | null) => unwrap(core.commands.settingsSetGitPath(path));

export const gitPickPath = () => unwrap(core.commands.gitPickPath());

export const settingsGet = () => unwrap(core.commands.settingsGet());

export const settingsSetTheme = (theme: Theme) => unwrap(core.commands.settingsSetTheme(theme));

export const settingsSetUpdateCheck = (enabled: boolean) => unwrap(core.commands.settingsSetUpdateCheck(enabled));

export const updateStatus = () => unwrap(core.commands.updateStatus());

/** Starts a check and returns at once; the result arrives as `update_status_changed` (D12). */
export const updateCheckNow = () => unwrap(core.commands.updateCheckNow());

export const updateLater = () => unwrap(core.commands.updateLater());

/** Starts the install job, refused while another job runs; the app restarts when it is installed (PLAT-4). */
export const updateInstall = () => unwrap(core.commands.updateInstall());

export const updateOpenDownloadPage = () => unwrap(core.commands.updateOpenDownloadPage());

export const workspaceGet = () => unwrap(core.commands.workspaceGet());

export const workspaceList = () => unwrap(core.commands.workspaceList());

export const workspaceCreate = (name: string) => unwrap(core.commands.workspaceCreate(name));

export const workspaceOpen = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceOpen(workspaceId));

export const workspaceClose = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceClose(workspaceId));

/** Opens the save dialog; how the export went arrives as `workspace_exported` (DATA-2, SEC-2). */
export const workspaceExport = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceExport(workspaceId));

/** Opens the open dialog; how the import went arrives as `workspace_imported` (DATA-2, SEC-2). */
export const workspaceImport = () => unwrap(core.commands.workspaceImport());

/** The repos whose folder is missing or is no repo, with their remote and default target (DATA-2). */
export const workspaceMissingRepos = (workspaceId: WorkspaceId) =>
  unwrap(core.commands.workspaceMissingRepos(workspaceId));

export const workspaceRename = (workspaceId: WorkspaceId, name: string) =>
  unwrap(core.commands.workspaceRename(workspaceId, name));

export const workspaceDelete = (workspaceId: WorkspaceId, confirmed: boolean) =>
  unwrap(core.commands.workspaceDelete(workspaceId, confirmed));

export const workspaceReorder = (workspaceIds: WorkspaceId[]) => unwrap(core.commands.workspaceReorder(workspaceIds));

export const workspaceActivate = (workspaceId: WorkspaceId) => unwrap(core.commands.workspaceActivate(workspaceId));

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

/** A repo's ref nodes where it appears under group `groupId`, or under Ungrouped for `null`. */
export const refsGet = (workspaceId: WorkspaceId, repoId: RepoId, groupId: GroupId | null) =>
  unwrap(core.commands.refsGet(workspaceId, repoId, groupId));

/** The children of a group's Branches node (UI-5). */
export const groupBranches = (workspaceId: WorkspaceId, groupId: GroupId) =>
  unwrap(core.commands.groupBranches(workspaceId, groupId));

/** Creates a group with `repoIds` as its members (GRP-2); resolves to its id. */
export const groupCreate = (workspaceId: WorkspaceId, name: string, repoIds: RepoId[]) =>
  unwrap(core.commands.groupCreate(workspaceId, name, repoIds));

export const groupRename = (workspaceId: WorkspaceId, groupId: GroupId, name: string) =>
  unwrap(core.commands.groupRename(workspaceId, groupId, name));

/** Deletes a group; its repos stay in the workspace (GRP-3). */
export const groupDelete = (workspaceId: WorkspaceId, groupId: GroupId) =>
  unwrap(core.commands.groupDelete(workspaceId, groupId));

/** Adds repos to a group at `at`, or at the end. */
export const groupAddRepos = (workspaceId: WorkspaceId, groupId: GroupId, repoIds: RepoId[], at: number | null) =>
  unwrap(core.commands.groupAddRepos(workspaceId, groupId, repoIds, at));

export const groupRemoveRepos = (workspaceId: WorkspaceId, groupId: GroupId, repoIds: RepoId[]) =>
  unwrap(core.commands.groupRemoveRepos(workspaceId, groupId, repoIds));

/** Moves members of a group to `at` among the others. */
export const groupMoveRepos = (workspaceId: WorkspaceId, groupId: GroupId, repoIds: RepoId[], at: number) =>
  unwrap(core.commands.groupMoveRepos(workspaceId, groupId, repoIds, at));

/** Moves a repo from one group to another, at `at` or at the end. */
export const groupMoveRepoBetween = (
  workspaceId: WorkspaceId,
  repoId: RepoId,
  from: GroupId,
  to: GroupId,
  at: number | null,
) => unwrap(core.commands.groupMoveRepoBetween(workspaceId, repoId, from, to, at));

export const groupReorder = (workspaceId: WorkspaceId, groupIds: GroupId[]) =>
  unwrap(core.commands.groupReorder(workspaceId, groupIds));

/**
 * Starts the preflight of group operation `op` on `target` (section 5); its result arrives as `preflight_ready`,
 * with whether the user must confirm.
 */
export const groupPreflight = (workspaceId: WorkspaceId, target: TargetSet, op: GroupOp) =>
  unwrap(core.commands.groupPreflight(workspaceId, target, op));

/** Starts preflight `preflightId`'s job; refused without `confirmed` when the preflight said to confirm. */
export const groupStart = (preflightId: PreflightId, confirmed: boolean) =>
  unwrap(core.commands.groupStart(preflightId, confirmed));

/** Drops a preflight whose dialog the user cancelled. */
export const groupDrop = (preflightId: PreflightId) => core.commands.groupDrop(preflightId);

/** Retry failed (GRP-11): a new preflight on the job's failed repos, which the core takes from its report. */
export const groupRetryFailed = (jobId: JobId) => unwrap(core.commands.groupRetryFailed(jobId));

/** The repos of `target` as the workspace is now (GRP-5); refused for a group or repo that no longer exists. */
export const targetRepos = (workspaceId: WorkspaceId, target: TargetSet) =>
  unwrap(core.commands.targetRepos(workspaceId, target));

/** The palette's answer to `text` (UI-22); `null` when a newer query of the workspace superseded this one. */
export const paletteQuery = (workspaceId: WorkspaceId, text: string) =>
  unwrap(core.commands.paletteQuery(workspaceId, text));

/** `null` when a newer filter of the workspace superseded this one. */
export const treeFilter = (workspaceId: WorkspaceId, text: string) =>
  unwrap(core.commands.treeFilter(workspaceId, text));

/**
 * The first page of `target`'s history: of `repoId`, or merged with `repoId` null (PERF-5). `null` when a newer
 * walk of the workspace superseded this one.
 */
export const historyGet = (workspaceId: WorkspaceId, repoId: RepoId | null, target: HistoryTarget, text: string) =>
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
/** Changes the group operations' settings; a parallelism outside 1 to 32 is refused. */
export const settingsSetGroups = (groups: GroupSettings) => unwrap(core.commands.settingsSetGroups(groups));

export const workspaceSetLayout = (workspaceId: WorkspaceId, layout: PaneLayout) =>
  unwrap(core.commands.workspaceSetLayout(workspaceId, layout));

/** Turns AI on or off for the workspace (AI-6); the core refuses every AI request for it while off. */
export const workspaceSetAi = (workspaceId: WorkspaceId, enabled: boolean) =>
  unwrap(core.commands.workspaceSetAi(workspaceId, enabled));

/** The AI settings and providers, each with whether it has a key, never the key (SEC-1). */
export const aiProvidersGet = () => unwrap(core.commands.aiProvidersGet());

/** Saves a provider, new without an id. `key`: `null` keeps the stored one, `""` removes it (SEC-1, D11). */
export const aiProviderSave = (draft: ProviderDraft, key: string | null) =>
  unwrap(core.commands.aiProviderSave(draft, key));

/** Removes a provider and its key (D11). */
export const aiProviderDelete = (id: ProviderId) => unwrap(core.commands.aiProviderDelete(id));

export const aiProviderSetDefault = (id: ProviderId) => unwrap(core.commands.aiProviderSetDefault(id));

/** AI features (AI-16), the exclude list (AI-4) and AI debug logging (AI-7). */
export const aiSettingsSet = (options: AiOptions) => unwrap(core.commands.aiSettingsSet(options));

/** Keeps the user's consent to sending to `target`, the host or program the dialog showed (AI-6). */
export const aiProviderConsent = (id: ProviderId, target: string) =>
  unwrap(core.commands.aiProviderConsent(id, target));

/** Starts listing a provider's models; they arrive as `ai_models` with the returned id (AI-1). */
export const aiModelsGet = (draft: ProviderDraft, key: string | null) => unwrap(core.commands.aiModelsGet(draft, key));

/** Starts testing a provider; the result arrives as `ai_self_test` with the returned id (AI-2). */
export const aiSelfTest = (draft: ProviderDraft, key: string | null) => unwrap(core.commands.aiSelfTest(draft, key));

/** `null` when a newer view of the workspace superseded this one. */
export const changesGet = (workspaceId: WorkspaceId, repoId: RepoId, target: ChangesTarget) =>
  unwrap(core.commands.changesGet(workspaceId, repoId, target));

/** "Uncommitted changes" of a group or several repos (UI-34); `null` when a newer view superseded this one. */
export const changesGetMerged = (workspaceId: WorkspaceId, target: TargetSet) =>
  unwrap(core.commands.changesGetMerged(workspaceId, target));

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

/**
 * Opens the folder dialog for a clone's parent folder; the pick arrives as `clone_parent_picked` with `key`, which
 * names a target of the missing repos dialog, or is `null` for the clone dialog (SEC-2).
 */
export const clonePickParent = (workspaceId: WorkspaceId, key: string | null = null) =>
  unwrap(core.commands.clonePickParent(workspaceId, key));

/** The folder a clone of `url` gets by default, or `null`. */
export const cloneDefaultName = (url: string) => unwrap(core.commands.cloneDefaultName(url));

/** Why a clone into folder `name` of `parent` would be refused, or `null`; `createsParent` for a missing repo's. */
export const cloneTargetCheck = (parent: string, name: string, createsParent = false) =>
  unwrap(core.commands.cloneTargetCheck(parent, name, createsParent));

export const cloneStart = (workspaceId: WorkspaceId, request: CloneRequest) =>
  unwrap(core.commands.cloneStart(workspaceId, request));

export const repoOpenFileManager = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.repoOpenFileManager(workspaceId, repoId));

export const repoOpenTerminal = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.repoOpenTerminal(workspaceId, repoId));

/** The refs the merge dialog offers for a repo, filtered in the core (UI-20). */
export const mergeTargets = (workspaceId: WorkspaceId, repoId: RepoId, filter: string) =>
  unwrap(core.commands.mergeTargets(workspaceId, repoId, filter));

/** What the conflict banner names for a repo (OPS-2). */
export const conflictDetails = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.conflictDetails(workspaceId, repoId));

/** Unmerged file `file` of changes view `handle` as the conflict editor shows it (OPS-8). */
export const conflictGet = (workspaceId: WorkspaceId, handle: number, file: number) =>
  unwrap(core.commands.conflictGet(workspaceId, handle, file));
export const aiEligibility = (workspaceId: WorkspaceId, handle: number, file: number) =>
  unwrap(core.commands.aiEligibility(workspaceId, handle, file));

/** Resolve with AI, refused or started as a job whose result `ai_proposal_ready` announces (AI-3, M4-08). */
export const aiResolve = (workspaceId: WorkspaceId, request: AiResolveRequest) =>
  unwrap(core.commands.aiResolve(workspaceId, request));

/** Resolve all with AI for a repo's unmerged files (AI-3, AI-13): refused, or a job with each file's progress. */
export const aiResolveAll = (workspaceId: WorkspaceId, request: AiResolveAllRequest) =>
  unwrap(core.commands.aiResolveAll(workspaceId, request));

/** The proposal kept for an unmerged file, why AI failed for it, or none (section 3 "Proposal"). */
export const aiProposalGet = (workspaceId: WorkspaceId, handle: number, file: number) =>
  unwrap(core.commands.aiProposalGet(workspaceId, handle, file));

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
