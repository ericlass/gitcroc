// The updater for the fake core (fake.ts), like the core's (PLAT-4, D12): a check finds what `fakeUpdate.check`
// says, Later hides the notice of that version, and an install runs as a job "Update to <version>" with download
// progress, refused while another job runs. Test support only; nothing here reaches the network.
import type { IpcError, JobId, JobView, UpdateView } from "./bindings";

/** When the fake's checks run: ten past noon on a fixed day, so times read the same in every run. */
export const CHECKED_AT = Date.UTC(2026, 10, 2, 12, 10, 0);

export const defaultUpdateView: UpdateView = {
  status: { kind: "idle" },
  current_version: "0.1.0",
  package: "app_image",
  installs: true,
  checks: true,
  later: false,
};

/** A version a check finds. */
export interface FakeFound {
  version: string;
  date: string | null;
  notes: string;
}

export const fakeUpdate: {
  view: UpdateView;
  /** What a check finds: a version, none, or why it failed. */
  check: FakeFound | null | { error: string };
  /** What the plugin says when the package does not verify; `null` lets the install succeed. */
  installFailure: string | null;
  /** How often each command ran. */
  checks: number;
  installs: number;
  pagesOpened: number;
} = {
  view: defaultUpdateView,
  check: null,
  installFailure: null,
  checks: 0,
  installs: 0,
  pagesOpened: 0,
};

/** Version 0.2.0 on offer, as the `update` data sets start. */
export const offered: FakeFound = {
  version: "0.2.0",
  date: "2026-11-02T10:00:00Z",
  notes: "Faster history.\n<b>Shown as text</b>",
};

export function resetUpdate(): void {
  fakeUpdate.view = defaultUpdateView;
  fakeUpdate.check = null;
  fakeUpdate.installFailure = null;
  fakeUpdate.checks = 0;
  fakeUpdate.installs = 0;
  fakeUpdate.pagesOpened = 0;
}

/** Starts with `found` on offer, for a package that installs or only notifies. */
export function offerUpdate(found: FakeFound, installs: boolean): void {
  fakeUpdate.check = found;
  fakeUpdate.view = {
    ...defaultUpdateView,
    package: installs ? "app_image" : "deb",
    installs,
    status: { kind: "available", ...found, checked_at: CHECKED_AT },
  };
}

/** What the update commands need of the rest of the fake core. */
export interface UpdateDeps {
  sendJob: (view: JobView) => void;
  newJobId: () => JobId;
  /** The active workspace, which the install job belongs to. */
  workspace: () => { id: string; name: string } | null;
  jobsRunning: () => boolean;
  /** Sends `update_status_changed`. */
  changed: (view: UpdateView) => void;
}

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

const refused = (message: string) =>
  Promise.resolve({ status: "error" as const, error: { code: "refused", message, git_stderr: null } as IpcError });

export function updateCommands(deps: UpdateDeps) {
  const set = (view: UpdateView) => {
    fakeUpdate.view = view;
    deps.changed(view);
  };

  const runCheck = () => {
    fakeUpdate.checks += 1;
    const found = fakeUpdate.check;
    const keptLater =
      fakeUpdate.view.later &&
      fakeUpdate.view.status.kind === "available" &&
      found !== null &&
      !("error" in found) &&
      found.version === fakeUpdate.view.status.version;
    set({ ...fakeUpdate.view, status: { kind: "checking" }, later: false });
    setTimeout(() => {
      if (found === null) {
        set({ ...fakeUpdate.view, status: { kind: "up_to_date", checked_at: CHECKED_AT } });
      } else if ("error" in found) {
        set({ ...fakeUpdate.view, status: { kind: "failed", reason: found.error, checked_at: CHECKED_AT } });
      } else {
        set({ ...fakeUpdate.view, status: { kind: "available", ...found, checked_at: CHECKED_AT }, later: keptLater });
      }
    }, 0);
  };

  const install = () => {
    const view = fakeUpdate.view;
    const workspace = deps.workspace();
    if (!view.installs) {
      return refused("this package does not install updates; download the new version instead");
    }
    if (deps.jobsRunning()) {
      return refused("wait for the running jobs to end before updating");
    }
    if (view.status.kind !== "available" || workspace === null) {
      return refused("no update is on offer");
    }
    fakeUpdate.installs += 1;
    const { version } = view.status;
    const previous = view.status;
    const job: JobView = {
      job_id: deps.newJobId(),
      workspace_id: workspace.id,
      workspace_name: workspace.name,
      label: `Update to ${version}`,
      repos: [],
      repo_names: [],
      state: "running",
      done: 0,
      total: 0,
      step: null,
      started_at: CHECKED_AT,
      duration_ms: null,
      outcome: null,
      output: "",
      message: null,
      sub_jobs: [],
      report: null,
      ai_files: [],
    };
    deps.sendJob(job);
    set({ ...view, status: { kind: "downloading", version, done: 0, total: 2_097_152 } });
    setTimeout(() => {
      deps.sendJob({ ...job, done: 1024, total: 2048, step: "Downloading (KiB)" });
      set({ ...fakeUpdate.view, status: { kind: "downloading", version, done: 1_048_576, total: 2_097_152 } });
      setTimeout(() => {
        const failure = fakeUpdate.installFailure;
        const ended = { ...job, state: "ended" as const, done: 2048, total: 2048, duration_ms: 900 };
        if (failure === null) {
          set({ ...fakeUpdate.view, status: { kind: "ready_to_restart", version } });
          deps.sendJob({ ...ended, outcome: { kind: "succeeded" } });
        } else {
          set({ ...fakeUpdate.view, status: previous });
          deps.sendJob({ ...ended, outcome: { kind: "failed" }, message: `the update was not installed: ${failure}` });
        }
      }, 0);
    }, 0);
    return ok(job.job_id);
  };

  return {
    updateStatus: () => ok(fakeUpdate.view),
    updateCheckNow: () => {
      if (fakeUpdate.view.checks) {
        runCheck();
      }
      return ok(null);
    },
    updateLater: () => {
      if (fakeUpdate.view.status.kind === "available") {
        set({ ...fakeUpdate.view, later: true });
      }
      return ok(null);
    },
    updateInstall: install,
    updateOpenDownloadPage: () => {
      fakeUpdate.pagesOpened += 1;
      return ok(null);
    },
  };
}
