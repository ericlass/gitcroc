// The only way into the core. Components import from here, never from
// `@tauri-apps/api` or `./bindings` directly (enforced by eslint.config.js).
import type { HistoryTarget, IpcError, RepoId, Theme, WorkspaceId } from "./bindings";
import type { Core } from "./core";
import { fake } from "./fake";
import { real } from "./core";

export type {
  AddReport,
  AppInfo,
  CommitGraphWrittenEvent,
  CommitRow,
  ConfigWarning,
  Edge,
  GitStatus,
  GraphRow,
  Head,
  HistoryTarget,
  HistoryView,
  IpcError,
  JobId,
  JobProgress,
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
  Theme,
  Time,
  TreeItem,
  TreeNode,
  Upstream,
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

export const commitGraphDecline = (workspaceId: WorkspaceId, repoId: RepoId) =>
  unwrap(core.commands.commitGraphDecline(workspaceId, repoId));

export const debugEmitProgress = (workspaceId: string) => unwrap(core.commands.debugEmitProgress(workspaceId));
