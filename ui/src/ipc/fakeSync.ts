// Fetch, pull and push for the fake core (fake.ts), on the ref model the
// fake trees are built from (`FakeRefs`): the checks and answers of the
// core's `op_sync` and `push_choices`, simplified. A pull takes the
// upstream's commits, a push sends the branch's; ahead and behind follow.
// Test support only; the real rules are the core's.
import type { Outcome, PushChoices, SyncOp } from "./bindings";
import { upstreamOf } from "./fakeBranches";
import type { FakeRefs } from "./fakeTree";

/** What `op_sync` answers before a job starts. */
export type SyncPlan =
  | { kind: "refused"; why: string }
  | { kind: "choose_remote"; remotes: string[] }
  | { kind: "confirm"; actions: string[] }
  /** Runs, with the job's label; `upstream` is where a pull or push goes. */
  | {
      kind: "run";
      label: string;
      branch: string | null;
      upstream: string | null;
      setUpstream: boolean;
    };

const remotesOf = (refs: FakeRefs) => Object.keys(refs.remotes ?? {}).sort();

/** Where a branch without an upstream pushes to, as the core picks it. */
const defaultRemote = (remotes: string[]) =>
  remotes.includes("origin") ? "origin" : remotes.length === 1 ? (remotes[0] ?? null) : null;

/** What the core would answer for `op` on `refs`, unconfirmed or `confirmed`. */
export function planSync(refs: FakeRefs, op: SyncOp, confirmed: boolean): SyncPlan {
  const remotes = remotesOf(refs);
  if (op.kind === "fetch") {
    return remotes.length === 0
      ? { kind: "refused", why: "the repo has no remote" }
      : {
          kind: "run",
          label: "Fetch all remotes",
          branch: null,
          upstream: null,
          setUpstream: false,
        };
  }
  const branch = refs.current;
  if (branch === undefined) {
    return {
      kind: "refused",
      why: `HEAD is detached; check out a branch to ${op.kind}`,
    };
  }
  const upstream = upstreamOf(refs, branch);
  if (op.kind === "pull") {
    return upstream === null
      ? { kind: "refused", why: `${branch} has no upstream; set one to pull` }
      : {
          kind: "run",
          label: `Pull ${upstream} into ${branch}`,
          branch,
          upstream,
          setUpstream: false,
        };
  }
  let target = upstream;
  if (target === null) {
    if (remotes.length === 0) {
      return { kind: "refused", why: "the repo has no remote" };
    }
    if (op.remote !== null && !remotes.includes(op.remote)) {
      return { kind: "refused", why: `there is no remote ${op.remote}` };
    }
    const remote = op.remote ?? defaultRemote(remotes);
    if (remote === null) {
      return { kind: "choose_remote", remotes };
    }
    target = `${remote}/${branch}`;
  }
  const [remote = "", ...rest] = target.split("/");
  if (op.force_with_lease && !confirmed) {
    return {
      kind: "confirm",
      actions: [
        `Replace ${rest.join("/")} on the remote ${remote} with ${branch}, even if it is not a fast-forward; ` +
          "commits only on the remote branch may be lost",
      ],
    };
  }
  return {
    kind: "run",
    label: `${op.force_with_lease ? "Force push" : "Push"} ${branch} to ${target}`,
    branch,
    upstream: target,
    setUpstream: upstream === null && op.set_upstream,
  };
}

/** `refs` after a planned `op` ran, its outcome and what git printed. */
export function applySync(
  refs: FakeRefs,
  op: SyncOp,
  plan: Extract<SyncPlan, { kind: "run" }>,
): { refs: FakeRefs; outcome: Outcome; output: string } {
  const branch = plan.branch;
  if (op.kind === "fetch" || branch === null || plan.upstream === null) {
    return { refs, outcome: { kind: "nothing_to_do" }, output: "" };
  }
  const [ahead, behind] = refs.counts?.[branch] ?? [0, 0];
  const counts = (next: [number, number]) => ({
    ...refs.counts,
    [branch]: next,
  });
  if (op.kind === "pull") {
    if (behind === 0) {
      return {
        refs,
        outcome: { kind: "nothing_to_do" },
        output: "Already up to date.\n",
      };
    }
    const fastForwardOnly = op.strategy === "ff_only" || op.strategy === "config";
    if (ahead > 0 && fastForwardOnly) {
      return {
        refs,
        outcome: { kind: "failed" },
        output: "fatal: Not possible to fast-forward, aborting.\n",
      };
    }
    return {
      refs: { ...refs, counts: counts([ahead > 0 ? ahead + 1 : 0, 0]) },
      outcome: { kind: "succeeded" },
      output: "",
    };
  }
  if (op.kind !== "push") {
    return { refs, outcome: { kind: "nothing_to_do" }, output: "" };
  }
  if (behind > 0 && !op.force_with_lease) {
    return {
      refs,
      outcome: { kind: "failed" },
      output: ` ! [rejected]        ${branch} -> ${branch} (non-fast-forward)\n`,
    };
  }
  const [remote = "", ...rest] = plan.upstream.split("/");
  const remoteBranches = refs.remotes?.[remote] ?? [];
  const known = remoteBranches.includes(rest.join("/"));
  if (known && ahead === 0 && behind === 0 && !plan.setUpstream) {
    return {
      refs,
      outcome: { kind: "nothing_to_do" },
      output: "Everything up-to-date\n",
    };
  }
  return {
    refs: {
      ...refs,
      counts: counts([0, 0]),
      remotes: {
        ...refs.remotes,
        [remote]: known ? remoteBranches : [...remoteBranches, rest.join("/")],
      },
      upstreams: plan.setUpstream ? { ...refs.upstreams, [branch]: plan.upstream } : refs.upstreams,
    },
    outcome: { kind: "succeeded" },
    output: "",
  };
}

/** What `push_choices` answers for `refs`. */
export function pushChoicesOf(refs: FakeRefs): PushChoices {
  const remotes = remotesOf(refs);
  const branch = refs.current ?? null;
  return {
    branch,
    upstream: branch === null ? null : upstreamOf(refs, branch),
    remotes,
    remote: defaultRemote(remotes),
  };
}
