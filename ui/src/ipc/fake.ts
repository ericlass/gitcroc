// A fake core for Vitest and Playwright, selected by `__FAKE_CORE__` (see
// vite.config.ts). It implements the generated interface, so a change to the
// Rust commands breaks it at compile time like the real one.
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo, GitStatus, JobProgress } from "./bindings";
import type { Core } from "./core";

const defaultAppInfo: AppInfo = { name: "gitcroc", version: "0.0.0-fake", os: "fake-os", arch: "fake-arch" };
const defaultGitStatus: GitStatus = { kind: "usable", path: "/fake/bin/git", version: "2.47.0" };

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

/** What the fake returns; tests change it and call `reset` afterwards. */
export const fakeState: { appInfo: AppInfo; gitStatus: GitStatus } = {
  appInfo: defaultAppInfo,
  gitStatus: defaultGitStatus,
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

export function reset(): void {
  fakeState.appInfo = defaultAppInfo;
  fakeState.gitStatus = defaultGitStatus;
  jobProgressListeners.clear();
  gitStatusListeners.clear();
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
  },
};
