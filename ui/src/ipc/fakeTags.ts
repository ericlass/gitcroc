// Tag create, delete and push, and reset, for the fake core (fake.ts), on
// the ref model the fake trees are built from (`FakeRefs`): the checks and
// answers of the core's `op_tag` and `op_reset`, simplified. Test support
// only; the real rules are the core's.
import type { ResetMode, TagOp } from "./bindings";
import { fakeNameCheck } from "./fakeBranches";
import type { FakeRefs } from "./fakeTree";

/** What `op_tag` and `op_reset` answer before a job starts. */
export type TagPlan =
  | { kind: "refused"; why: string }
  | { kind: "choose_remote"; remotes: string[] }
  /** Cannot be undone: the core asks first. */
  | { kind: "confirm"; actions: string[] }
  /** Runs, with the job's label; `refs` is the model after it. */
  | { kind: "run"; label: string; refs: FakeRefs };

const remotesOf = (refs: FakeRefs) => Object.keys(refs.remotes ?? {}).sort();

/** `picked`, or `origin`, or the only remote; else what to ask or why not. */
function remoteOf(refs: FakeRefs, picked: string | null): string | TagPlan {
  const remotes = remotesOf(refs);
  if (picked !== null) {
    return remotes.includes(picked) ? picked : { kind: "refused", why: `there is no remote ${picked}` };
  }
  if (remotes.includes("origin")) {
    return "origin";
  }
  if (remotes.length === 1) {
    return remotes[0] ?? "";
  }
  return remotes.length === 0 ? { kind: "refused", why: "the repo has no remote" } : { kind: "choose_remote", remotes };
}

/** What the core would answer for `op` on `refs`, `confirmed` or not. */
export function planTag(refs: FakeRefs, op: TagOp, confirmed: boolean): TagPlan {
  const tags = refs.tags ?? [];
  const invalid = fakeNameCheck(op.name);
  if (invalid !== null) {
    return { kind: "refused", why: invalid };
  }
  if (op.kind === "create") {
    if (tags.includes(op.name)) {
      return { kind: "refused", why: `a tag named ${op.name} exists already` };
    }
    if (op.annotated && op.message.trim() === "") {
      return { kind: "refused", why: "an annotated tag needs a message" };
    }
    const after = { ...refs, tags: [...tags, op.name].sort() };
    if (!op.push) {
      return { kind: "run", label: `Create tag ${op.name}`, refs: after };
    }
    const remote = remoteOf(refs, op.remote);
    return typeof remote === "string"
      ? { kind: "run", label: `Create tag ${op.name} and push it to ${remote}`, refs: after }
      : remote;
  }
  if (!tags.includes(op.name)) {
    return { kind: "refused", why: `there is no tag ${op.name}` };
  }
  if (op.kind === "push") {
    const remote = remoteOf(refs, op.remote);
    return typeof remote === "string" ? { kind: "run", label: `Push tag ${op.name} to ${remote}`, refs } : remote;
  }
  const after = { ...refs, tags: tags.filter((tag) => tag !== op.name) };
  if (!op.on_remote) {
    return { kind: "run", label: `Delete tag ${op.name}`, refs: after };
  }
  const remote = remoteOf(refs, op.remote);
  if (typeof remote !== "string") {
    return remote;
  }
  if (!confirmed) {
    return {
      kind: "confirm",
      actions: [`Delete tag ${op.name} on the remote ${remote}, for everyone who fetches from it`],
    };
  }
  return { kind: "run", label: `Delete tag ${op.name} here and on ${remote}`, refs: after };
}

/**
 * What the core would answer for a reset of `branch` (`null`: a detached
 * `HEAD`) to the commit `text` names (`1a2b3c4 Subject`), with `lost`
 * files' uncommitted changes.
 */
export function planReset(
  branch: string | null,
  text: string,
  mode: ResetMode,
  lost: number,
  confirmed: boolean,
): TagPlan {
  if (mode === "hard" && !confirmed) {
    const moved =
      branch === null ? `HEAD is detached: reset HEAD, not a branch, to ${text}` : `Reset branch ${branch} to ${text}`;
    const files =
      lost === 0
        ? "No uncommitted changes are lost; untracked files stay"
        : `Discard the uncommitted changes to ${lost} ${lost === 1 ? "file" : "files"}; untracked files stay`;
    return { kind: "confirm", actions: [moved, files] };
  }
  const short = text.split(" ")[0] ?? text;
  return { kind: "run", label: `Reset ${branch ?? "HEAD (detached)"} to ${short} (${mode})`, refs: {} };
}
