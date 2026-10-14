// The only way into the core. Components import from here, never from
// `@tauri-apps/api` or `./bindings` directly (enforced by eslint.config.js).
import type {
  Appearance,
  ChangeRequest,
  ChangesSettings,
  ChangesTarget,
  DiffRequest,
  FileRef,
  HistoryTarget,
  ImageSide,
  IpcError,
  JobId,
  PaneLayout,
  PerfTrace,
  RepoId,
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
  CancelResult,
  ChangeAction,
  ChangeRequest,
  ChangeStart,
  ChangeTarget,
  CodeTheme,
  ChangedFile,
  ChangesContent,
  ChangesSettings,
  ChangesTarget,
  ChangesView,
  CommitGraphWrittenEvent,
  CommitDetails,
  CommitRow,
  Conflict,
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
  IpcError,
  JobId,
  JobState,
  JobView,
  Line,
  LineKind,
  ModeChange,
  Outcome,
  PaneLayout,
  PerfSample,
  PerfTrace,
  Person,
  PromptKind,
  RefKind,
  RefLabel,
  Refusal,
  RepoId,
  RepoRef,
  RepoState,
  RepoSummary,
  RepoView,
  Settings,
  SettingsView,
  SummaryView,
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

/** One side of the image diff last read for `file`, as base64; `null` when there is none. */
export const changesApply = (workspaceId: WorkspaceId, request: ChangeRequest) =>
  unwrap(core.commands.changesApply(workspaceId, request));

export const diffImage = (workspaceId: WorkspaceId, handle: number, file: FileRef, side: ImageSide) =>
  unwrap(core.commands.diffImage(workspaceId, handle, file, side));

/** `perf-trace` builds only (`just bench-ui`): writes what the UI measured. */
export const perfTraceWrite = (trace: PerfTrace) => unwrap(core.commands.perfTraceWrite(trace));
