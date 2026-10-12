// A fake core for Vitest and Playwright, selected by `__FAKE_CORE__` (see
// vite.config.ts). It implements the generated interface, so a change to the
// Rust commands breaks it at compile time like the real one.
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddReport,
  AppInfo,
  GitStatus,
  JobProgress,
  RepoId,
  SettingsView,
  SummaryView,
  WorkspaceView,
} from "./bindings";
import type { Core } from "./core";

const defaultAppInfo: AppInfo = { name: "gitcroc", version: "0.0.0-fake", os: "fake-os", arch: "fake-arch" };
const defaultGitStatus: GitStatus = { kind: "usable", path: "/fake/bin/git", version: "2.47.0" };
const defaultSettings: SettingsView = { settings: { theme: "system" }, read_only: false, warning: null };
const defaultWorkspace: WorkspaceView = {
  id: "fake-workspace",
  name: "My workspace",
  repos: [],
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
const jobProgressListeners: Listeners<JobProgress> = new Set();
const gitStatusListeners: Listeners<GitStatus> = new Set();
const workspaceChangedListeners: Listeners<WorkspaceView> = new Set();
const workspaceAddReportListeners: Listeners<AddReport> = new Set();
const repoSummaryListeners: Listeners<SummaryView> = new Set();

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
} = {
  appInfo: defaultAppInfo,
  gitStatus: defaultGitStatus,
  settings: defaultSettings,
  workspace: defaultWorkspace,
  pickedFolder: null,
  summaries: [],
  selected: {},
};

function emit<T>(listeners: Listeners<T>, name: string, payload: T): void {
  const event: Event<T> = { event: name, id: nextEventId++, payload };
  for (const listener of [...listeners]) {
    listener(event);
  }
}

/** Delivers a `job_progress` event to every listener, as the core would. */
export function emitJobProgress(payload: JobProgress): void {
  emit(jobProgressListeners, "job_progress", payload);
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

export function reset(): void {
  fakeState.appInfo = defaultAppInfo;
  fakeState.gitStatus = defaultGitStatus;
  fakeState.settings = defaultSettings;
  fakeState.workspace = defaultWorkspace;
  fakeState.pickedFolder = null;
  fakeState.summaries = [];
  fakeState.selected = {};
  jobProgressListeners.clear();
  gitStatusListeners.clear();
  workspaceChangedListeners.clear();
  workspaceAddReportListeners.clear();
  repoSummaryListeners.clear();
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
  setTimeout(() => {
    emitJobProgress({ job_id: jobId, workspace_id: workspaceId, label: "Checking repos", done: 0, total: 2 });
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
    emitJobProgress({ job_id: jobId, workspace_id: workspaceId, label: "Adding repos", done: 2, total: 2 });
    emitWorkspaceChanged(fakeState.workspace);
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
    debugEmitProgress: (workspaceId) => {
      const jobId = nextJobId++;
      const total = 3;
      for (let done = 1; done <= total; done++) {
        setTimeout(() => {
          emitJobProgress({ job_id: jobId, workspace_id: workspaceId, label: "Debug job", done, total });
        }, done * 100);
      }
      return Promise.resolve({ status: "ok", data: jobId });
    },
  },
  events: {
    gitStatus: fakeEvent(gitStatusListeners),
    jobProgress: fakeEvent(jobProgressListeners),
    repoSummary: fakeEvent(repoSummaryListeners),
    workspaceAddReport: fakeEvent(workspaceAddReportListeners),
    workspaceChanged: fakeEvent(workspaceChangedListeners),
  },
};
