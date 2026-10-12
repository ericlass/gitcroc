// A fake core for Vitest and Playwright, selected by `__FAKE_CORE__` (see
// vite.config.ts). It implements the generated interface, so a change to the
// Rust commands breaks it at compile time like the real one.
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo, JobProgress } from "./bindings";
import type { Core } from "./core";

const defaultAppInfo: AppInfo = { name: "gitcroc", version: "0.0.0-fake", os: "fake-os", arch: "fake-arch" };

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

/** What the fake returns; tests change it and call `reset` afterwards. */
export const fakeState = { appInfo: defaultAppInfo };

/** Delivers a `job_progress` event to every listener, as the core would. */
export function emitJobProgress(payload: JobProgress): void {
  const event: Event<JobProgress> = { event: "job_progress", id: nextEventId++, payload };
  for (const listener of [...jobProgressListeners]) {
    listener(event);
  }
}

export function reset(): void {
  fakeState.appInfo = defaultAppInfo;
  jobProgressListeners.clear();
}

export const fake: Core = {
  commands: {
    appInfo: () => Promise.resolve({ status: "ok", data: fakeState.appInfo }),
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
    jobProgress: fakeEvent(jobProgressListeners),
  },
};
