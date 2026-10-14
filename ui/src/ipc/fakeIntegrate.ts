// Merge, rebase, cherry-pick, revert, abort and continue for the fake core
// (fake.ts), on the ref model the fake trees are built from (`FakeRefs`):
// the checks and answers of the core's `op_integrate` and `merge_targets`,
// simplified. Test support only; the real rules are the core's.
import type { ConflictDetails, IntegrateOp, Operation } from "./bindings";
import type { FakeRefs } from "./fakeTree";

/** What `op_integrate` answers before a job starts. */
export type IntegratePlan =
  | { kind: "refused"; why: string }
  | { kind: "choose_mainline"; commit: string; parents: string[] }
  /** Cannot be undone: the core asks first. */
  | { kind: "confirm"; actions: string[] }
  /** Runs, with the job's label; `operation` is what it may stop in. */
  | { kind: "run"; label: string; operation: Operation | null };

/** What `merge_targets` answers: refs but the current branch, those containing `filter`. */
export function mergeTargetsOf(refs: FakeRefs, filter: string): string[] {
  const lower = filter.toLowerCase();
  const local = [...(refs.local ?? [])].filter((name) => name !== refs.current).sort();
  const remote = Object.entries(refs.remotes ?? {}).flatMap(([remote, branches]) =>
    [...branches].sort().map((branch) => `${remote}/${branch}`),
  );
  const tags = [...(refs.tags ?? [])].sort();
  return [...local, ...remote, ...tags].filter((name) => name.toLowerCase().includes(lower));
}

const OPERATION_NAMES: Record<Operation, string> = {
  merge: "merge",
  rebase: "rebase",
  apply_mailbox: "patch application",
  cherry_pick: "cherry-pick",
  revert: "revert",
  stash_apply: "stash apply",
};

const short = (hash: string) => hash.slice(0, 7);

/**
 * What the core would answer for `op` on `refs`, with the repo in
 * `conflict` (or not), `parents` giving each commit's parents, and whether
 * the user `confirmed` it.
 */
export function planIntegrate(
  refs: FakeRefs,
  op: IntegrateOp,
  conflict: ConflictDetails | undefined,
  parents: (hash: string) => string[],
  confirmed: boolean,
): IntegratePlan {
  const known = (name: string) => mergeTargetsOf(refs, "").includes(name) || name === refs.current;
  switch (op.kind) {
    case "merge":
    case "rebase": {
      const branch = refs.current;
      const target = op.kind === "merge" ? op.revision : op.onto;
      if (branch === undefined) {
        return {
          kind: "refused",
          why: `HEAD is detached; check out a branch to ${op.kind === "merge" ? "merge into it" : "rebase it"}`,
        };
      }
      if (!known(target)) {
        return { kind: "refused", why: `'${target}' names no commit` };
      }
      return op.kind === "merge"
        ? { kind: "run", label: `Merge ${target} into ${branch}`, operation: "merge" }
        : { kind: "run", label: `Rebase ${branch} onto ${target}`, operation: "rebase" };
    }
    case "cherry_pick": {
      const [first, ...rest] = op.commits;
      if (first === undefined) {
        return { kind: "refused", why: "no commit to cherry-pick" };
      }
      const named = rest.length === 0 ? short(first) : `${op.commits.length} commits`;
      return { kind: "run", label: `Cherry-pick ${named}`, operation: "cherry_pick" };
    }
    case "revert": {
      const of = parents(op.commit);
      if (of.length > 1 && op.mainline === null) {
        return { kind: "choose_mainline", commit: op.commit, parents: of.map(short) };
      }
      if (of.length <= 1 && op.mainline !== null) {
        return { kind: "refused", why: `${short(op.commit)} is not a merge commit` };
      }
      return { kind: "run", label: `Revert ${short(op.commit)}`, operation: "revert" };
    }
    case "abort":
    case "continue": {
      const operation = conflict?.operation ?? null;
      if (operation === null) {
        return { kind: "refused", why: `the repo has no operation to ${op.kind}` };
      }
      if (op.kind === "continue" && operation === "stash_apply") {
        return {
          kind: "refused",
          why: "a stash apply has nothing to continue: it ends when the last file is marked resolved",
        };
      }
      if (op.kind === "abort" && operation === "stash_apply" && !confirmed) {
        return {
          kind: "confirm",
          actions: [
            "Put the files the stash apply changed back to HEAD, resolutions included; the stash is kept and other local changes stay",
          ],
        };
      }
      if (op.kind === "continue" && (conflict?.unmerged_paths ?? 0) > 0) {
        return { kind: "refused", why: "paths are still unmerged; resolve and stage them first" };
      }
      const verb = op.kind === "abort" ? "Abort" : "Continue";
      return { kind: "run", label: `${verb} the ${OPERATION_NAMES[operation]}`, operation: null };
    }
  }
}
