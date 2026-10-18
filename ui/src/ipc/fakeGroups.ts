// Group operations for the fake core (fake.ts): the preflight classes each
// repo as the test sets it, and the job ends each repo with the outcome the
// test sets. Test support only; the rules are the core's (section 5).
import type {
  Class,
  GroupOp,
  JobId,
  JobView,
  Outcome,
  PreflightId,
  PreflightReady,
  RepoId,
  RepoResult,
  TargetSet,
  WorkspaceView,
} from "./bindings";

/** What the fake's group operations do; tests change it and `reset` puts it back. */
export interface FakeGroups {
  /** Each repo's class in a preflight; others will run. */
  classes: Record<RepoId, Class>;
  /** Each repo's outcome in a run; others succeed. */
  outcomes: Record<RepoId, Outcome>;
  /** What git printed in each repo; others print nothing. */
  outputs: Record<RepoId, string>;
  /** Every `group_preflight`, oldest first. */
  preflights: { workspaceId: string; target: TargetSet; op: GroupOp }[];
  /** Every `group_start`, oldest first. */
  starts: { preflightId: PreflightId; confirmed: boolean }[];
  /** Every `group_drop`, oldest first. */
  drops: PreflightId[];
  /** Every `group_retry_failed`, oldest first. */
  retries: JobId[];
  /** Group jobs stay running, with their sub-jobs, until `finishGroup`. */
  hold: boolean;
}

const empty = (): FakeGroups => ({
  classes: {},
  outcomes: {},
  outputs: {},
  preflights: [],
  starts: [],
  drops: [],
  retries: [],
  hold: false,
});

export const fakeGroups: FakeGroups = empty();

/** What the fake core lends the group operations. */
export interface GroupDeps {
  emitReady: (ready: PreflightReady) => void;
  sendJob: (view: JobView) => void;
  newJobId: () => JobId;
  /** The always-show setting (section 5, step 2). */
  alwaysShow: () => boolean;
  /**
   * The number of files a group commit takes from `repo`, which its
   * confirmation lists (GRP-15): staged ones, and with `stageAll` changed
   * tracked ones.
   */
  stagedFiles: (repo: RepoId, stageAll: boolean) => number;
}

/** Preflights not used yet, and the operation of each group job started. */
const kept = new Map<PreflightId, { ready: PreflightReady | null; op: GroupOp }>();
const ran = new Map<JobId, { workspaceId: string; op: GroupOp }>();
const held: (() => void)[] = [];
let nextPreflight = 1;

export function resetGroups(): void {
  Object.assign(fakeGroups, empty());
  kept.clear();
  ran.clear();
  held.length = 0;
  nextPreflight = 1;
}

/** Ends the oldest group job held by `fakeGroups.hold`. */
export function finishGroup(): void {
  held.shift()?.();
}

/** The operation as the core names it. */
export function actionOf(op: GroupOp): string {
  switch (op.kind) {
    case "fetch":
      return "Fetch";
    case "pull":
      return "Pull";
    case "push":
      return op.force_with_lease ? "Force push" : "Push";
    case "checkout":
      return `Checkout ${op.name}`;
    case "create_branch":
      return `Create branch ${op.name}`;
    case "delete_branch":
      return `Delete branch ${op.name}`;
    case "merge":
      return `Merge ${op.name}`;
    case "rebase":
      return `Rebase onto ${op.onto}`;
    case "create_tag":
      return `Create tag ${op.name}`;
    case "commit":
      return "Commit";
  }
}

/** What cannot be undone, as the core says it (GRP-9). */
export function groupDestructiveOf(op: GroupOp): string[] {
  if (op.kind === "push" && op.force_with_lease) {
    return ["Force push the current branch; commits only on the remote branch may be lost"];
  }
  if (op.kind === "delete_branch") {
    return [
      ...(op.force ? [`Force-delete branch ${op.name} even if it is not merged; its commits may be lost`] : []),
      ...(op.on_remote ? [`Delete branch ${op.name} on the remote`] : []),
    ];
  }
  return [];
}

function labelOf(op: GroupOp, target: TargetSet, workspace: WorkspaceView, names: string[]): string {
  const count = names.length === 1 ? "1 repo" : `${names.length} repos`;
  const group = target.kind === "group" ? workspace.groups.find((known) => known.id === target.group_id) : undefined;
  const what =
    target.kind === "repo" ? (names[0] ?? count) : group === undefined ? count : `${group.name} (${count})`;
  return `${actionOf(op)} · ${what}`;
}

/** Like the core: classes `repos` and sends the result later; resolves to the preflight's id. */
export function fakePreflight(
  deps: GroupDeps,
  workspace: WorkspaceView,
  target: TargetSet,
  repos: RepoId[],
  op: GroupOp,
): PreflightId {
  fakeGroups.preflights.push({ workspaceId: workspace.id, target, op });
  const id = nextPreflight++;
  kept.set(id, { ready: null, op });
  const names = repos.map((repo) => workspace.repos.find((known) => known.id === repo)?.name ?? repo);
  // Like the core: a commit leaves out a repo with nothing staged and lists the others' staged files.
  const staged = (repo: RepoId) => (op.kind === "commit" ? deps.stagedFiles(repo, op.stage_all) : null);
  const classed = repos.map((repo, at) => ({
    repo_id: repo,
    repo_name: names[at] ?? repo,
    class:
      fakeGroups.classes[repo] ??
      (staged(repo) === 0 ? { kind: "will_skip" as const, reason: "nothing staged" } : ({ kind: "will_run" } as const)),
    staged_files: staged(repo) || null,
  }));
  const destructive = groupDestructiveOf(op);
  const ready: PreflightReady = {
    workspace_id: workspace.id,
    preflight_id: id,
    label: labelOf(op, target, workspace, names),
    action: actionOf(op),
    destructive,
    repos: classed,
    needs_confirmation:
      deps.alwaysShow() ||
      op.kind === "commit" ||
      destructive.length > 0 ||
      classed.some((repo) => repo.class.kind !== "will_run"),
  };
  setTimeout(() => {
    const preflight = kept.get(id);
    if (preflight !== undefined) {
      preflight.ready = ready;
      deps.emitReady(ready);
    }
  }, 0);
  return id;
}

/** The repo's skip, as the core turns a preflight class into an outcome. */
function skipOf(type: Class): Outcome | null {
  switch (type.kind) {
    case "will_skip":
      return { kind: "skipped", reason: { kind: "preflight", reason: type.reason } };
    case "blocked":
      return { kind: "skipped", reason: { kind: "blocked", reason: type.reason } };
    default:
      return null;
  }
}

/** The job's outcome from its report, as the core's `overall` decides it. */
function overall(report: RepoResult[]): { outcome: Outcome; message: string | null } {
  const count = (kind: Outcome["kind"]) => report.filter((line) => line.outcome.kind === kind).length;
  const repos = report.length === 1 ? "repo" : "repos";
  if (count("failed") > 0) {
    return { outcome: { kind: "failed" }, message: `${count("failed")} of ${report.length} ${repos} failed` };
  }
  if (count("conflict") > 0) {
    return { outcome: { kind: "conflict" }, message: `${count("conflict")} of ${report.length} ${repos} in conflict` };
  }
  if (count("succeeded") > 0) {
    return { outcome: { kind: "succeeded" }, message: null };
  }
  if (count("nothing_to_do") > 0) {
    return { outcome: { kind: "nothing_to_do" }, message: null };
  }
  const first = report[0];
  return { outcome: first?.outcome ?? { kind: "skipped", reason: { kind: "cancelled" } }, message: first?.message ?? null };
}

type Answer<T> = { status: "ok"; data: T } | { status: "error"; error: { code: "refused"; message: string; git_stderr: null } };

const refused = <T>(message: string): Answer<T> => ({
  status: "error",
  error: { code: "refused", message, git_stderr: null },
});

/** Like the core: starts preflight `id`'s job, refused without `confirmed` when it must be confirmed. */
export function fakeStart(
  deps: GroupDeps,
  workspaceName: (id: string) => string,
  id: PreflightId,
  confirmed: boolean,
): Answer<JobId> {
  fakeGroups.starts.push({ preflightId: id, confirmed });
  const preflight = kept.get(id);
  if (preflight === undefined) {
    return refused("no such preflight, or it was used already");
  }
  const ready = preflight.ready;
  if (ready === null) {
    return refused("the preflight has not finished");
  }
  if (ready.needs_confirmation && !confirmed) {
    return refused("the operation needs the user's confirmation");
  }
  kept.delete(id);
  const jobId = deps.newJobId();
  ran.set(jobId, { workspaceId: ready.workspace_id, op: preflight.op });
  const runs = ready.repos.filter((repo) => skipOf(repo.class) === null);
  const running: JobView = {
    job_id: jobId,
    workspace_id: ready.workspace_id,
    workspace_name: workspaceName(ready.workspace_id),
    repos: ready.repos.map((repo) => repo.repo_id),
    repo_names: ready.repos.map((repo) => repo.repo_name),
    label: ready.label,
    state: "running",
    done: 0,
    total: runs.length,
    step: null,
    started_at: Date.now(),
    duration_ms: null,
    outcome: null,
    output: "",
    message: null,
    sub_jobs: runs.map((repo, at) => ({
      repo_id: repo.repo_id,
      repo_name: repo.repo_name,
      state: at === 0 ? "running" : "queued",
      outcome: null,
    })),
    report: null,
    ai_files: [],
  };
  const end = () => {
    const report: RepoResult[] = ready.repos.map((repo) => ({
      repo_id: repo.repo_id,
      repo_name: repo.repo_name,
      outcome: skipOf(repo.class) ?? fakeGroups.outcomes[repo.repo_id] ?? { kind: "succeeded" },
      output: fakeGroups.outputs[repo.repo_id] ?? "",
      message: repo.class.kind === "will_skip" || repo.class.kind === "blocked" ? repo.class.reason : null,
    }));
    const { outcome, message } = overall(report);
    deps.sendJob({
      ...running,
      state: "ended",
      done: runs.length,
      duration_ms: 1200,
      outcome,
      message,
      sub_jobs: running.sub_jobs.map((sub) => ({
        ...sub,
        state: "ended",
        outcome: report.find((line) => line.repo_id === sub.repo_id)?.outcome ?? null,
      })),
      report,
    });
  };
  deps.sendJob(running);
  if (fakeGroups.hold) {
    held.push(end);
  } else {
    setTimeout(end, 0);
  }
  return { status: "ok", data: jobId };
}

/** Like the core: drops a preflight whose dialog was cancelled. */
export function fakeDrop(id: PreflightId): void {
  fakeGroups.drops.push(id);
  kept.delete(id);
}

/**
 * Like the core's Retry failed (GRP-11): a new preflight of the job's
 * operation on the repos its report has as failed.
 */
export function fakeRetry(
  deps: GroupDeps,
  jobs: JobView[],
  workspaceOf: (id: string) => WorkspaceView | undefined,
  jobId: JobId,
): Answer<PreflightId> {
  fakeGroups.retries.push(jobId);
  const started = ran.get(jobId);
  const job = jobs.find((known) => known.job_id === jobId);
  const workspace = started === undefined ? undefined : workspaceOf(started.workspaceId);
  if (started === undefined || job === undefined || workspace === undefined) {
    return refused("no such group job");
  }
  if (job.state !== "ended" || job.report === null) {
    return refused("the job has not ended");
  }
  const failed = job.report.filter((line) => line.outcome.kind === "failed").map((line) => line.repo_id);
  if (failed.length === 0) {
    return refused("no repo failed in that job");
  }
  const target: TargetSet = { kind: "repos", repo_ids: failed };
  return { status: "ok", data: fakePreflight(deps, workspace, target, failed, started.op) };
}
