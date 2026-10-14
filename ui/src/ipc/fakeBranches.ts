// Branch operations for the fake core (fake.ts), on the ref model the fake
// trees are built from (`FakeRefs`): the checks and answers of the core's
// `op_branch`, `branch_choices` and `branch_name_check`, simplified. Test
// support only; the real rules are the core's.
import type { BranchChoices, BranchOp } from "./bindings";
import type { FakeRefs } from "./fakeTree";

/** The name rules the fake knows; the core checks all of git's (SEC-4). */
export function fakeNameCheck(name: string): string | null {
  if (name === "") {
    return "the name is empty";
  }
  if (name.startsWith("-")) {
    return "the name cannot start with '-'";
  }
  if (name.includes(" ")) {
    return "the name cannot contain spaces";
  }
  if (name.includes("..")) {
    return "the name cannot contain '..'";
  }
  const forbidden = /[~^:?*[\\]/.exec(name);
  return forbidden ? `the name cannot contain '${forbidden[0]}'` : null;
}

/** The branch `name` tracks on a remote, as `origin/x`, or `null`. */
export function upstreamOf(refs: FakeRefs, name: string): string | null {
  const upstream = refs.upstreams?.[name];
  if (upstream !== undefined) {
    return upstream;
  }
  return refs.counts?.[name] ? `origin/${name}` : null;
}

/** Why the core would refuse `op` on `refs`, or `null`. */
export function refusalOf(refs: FakeRefs, op: BranchOp): string | null {
  const local = refs.local ?? [];
  const hasLocal = (name: string) => local.includes(name);
  const noLocal = (name: string) => (hasLocal(name) ? null : `there is no local branch ${name}`);
  switch (op.kind) {
    case "checkout":
      return fakeNameCheck(op.name) ?? noLocal(op.name);
    case "checkout_remote":
      return refs.remotes?.[op.branch.remote]?.includes(op.branch.branch)
        ? null
        : `there is no remote branch ${op.branch.remote}/${op.branch.branch}`;
    case "checkout_detached":
      return op.revision === "" ? "'' names no commit" : null;
    case "create":
      return fakeNameCheck(op.name) ?? (hasLocal(op.name) ? `a branch named ${op.name} exists already` : null);
    case "rename":
      return noLocal(op.name) ?? fakeNameCheck(op.new_name);
    case "delete":
      return (
        noLocal(op.name) ??
        (op.name === refs.current
          ? "the current branch cannot be deleted"
          : op.on_remote && upstreamOf(refs, op.name) === null
            ? `${op.name} tracks no remote branch`
            : null)
      );
    case "delete_remote":
      return refs.remotes?.[op.branch.remote] ? null : `there is no remote ${op.branch.remote}`;
    case "set_upstream":
      return noLocal(op.name);
  }
}

/** What cannot be undone, as the core names it (GRP-9); empty when nothing. */
export function destructiveOf(refs: FakeRefs, op: BranchOp): string[] {
  if (op.kind === "delete_remote") {
    return [`Delete ${op.branch.branch} on the remote ${op.branch.remote}`];
  }
  if (op.kind !== "delete") {
    return [];
  }
  const actions = op.force ? [`Delete branch ${op.name} even if it is not merged; its commits may be lost`] : [];
  const upstream = upstreamOf(refs, op.name);
  if (op.on_remote && upstream !== null) {
    const [remote = "", ...branch] = upstream.split("/");
    actions.push(`Delete ${branch.join("/")} on the remote ${remote}`);
  }
  return actions;
}

/** The job's label, as the core words it. */
export function labelOf(op: BranchOp): string {
  switch (op.kind) {
    case "checkout":
      return `Check out ${op.name}`;
    case "checkout_remote":
      return `Check out ${op.branch.remote}/${op.branch.branch}`;
    case "checkout_detached":
      return `Check out ${op.revision} (detached)`;
    case "create":
      return `Create branch ${op.name}`;
    case "rename":
      return `Rename ${op.name} to ${op.new_name}`;
    case "delete":
      return `${op.force ? "Force delete" : "Delete"} branch ${op.name}`;
    case "delete_remote":
      return `Delete ${op.branch.remote}/${op.branch.branch} on the remote`;
    case "set_upstream":
      return op.upstream === null
        ? `Unset the upstream of ${op.name}`
        : `Set the upstream of ${op.name} to ${op.upstream.remote}/${op.upstream.branch}`;
  }
}

const without = (names: string[] | undefined, name: string) => (names ?? []).filter((known) => known !== name);

/** `refs` after `op` ran, and whether it changed nothing (*nothing to do*). */
export function applyBranchOp(refs: FakeRefs, op: BranchOp): { refs: FakeRefs; nothingToDo: boolean } {
  const local = refs.local ?? [];
  switch (op.kind) {
    case "checkout":
      return { refs: { ...refs, current: op.name }, nothingToDo: refs.current === op.name };
    case "checkout_remote": {
      const name = op.branch.branch;
      const added = local.includes(name) ? local : [...local, name];
      const upstreams = local.includes(name)
        ? refs.upstreams
        : { ...refs.upstreams, [name]: `${op.branch.remote}/${name}` };
      return { refs: { ...refs, local: added, current: name, upstreams }, nothingToDo: refs.current === name };
    }
    case "checkout_detached": {
      const detached = { ...refs };
      delete detached.current;
      return { refs: detached, nothingToDo: refs.current === undefined };
    }
    case "create":
      return {
        refs: { ...refs, local: [...local, op.name], current: op.switch ? op.name : refs.current },
        nothingToDo: false,
      };
    case "rename":
      return {
        refs: {
          ...refs,
          local: [...without(local, op.name), op.new_name],
          current: refs.current === op.name ? op.new_name : refs.current,
          upstreams: { ...refs.upstreams, [op.new_name]: upstreamOf(refs, op.name) },
        },
        nothingToDo: false,
      };
    case "delete": {
      const upstream = upstreamOf(refs, op.name);
      const remotes = { ...refs.remotes };
      if (op.on_remote && upstream !== null) {
        const [remote = "", ...branch] = upstream.split("/");
        remotes[remote] = without(remotes[remote], branch.join("/"));
      }
      return { refs: { ...refs, local: without(local, op.name), remotes }, nothingToDo: false };
    }
    case "delete_remote":
      return {
        refs: {
          ...refs,
          remotes: { ...refs.remotes, [op.branch.remote]: without(refs.remotes?.[op.branch.remote], op.branch.branch) },
        },
        nothingToDo: false,
      };
    case "set_upstream":
      return {
        refs: {
          ...refs,
          upstreams: {
            ...refs.upstreams,
            [op.name]: op.upstream === null ? null : `${op.upstream.remote}/${op.upstream.branch}`,
          },
        },
        nothingToDo: false,
      };
  }
}

/** What `branch_choices` answers for `refs`. */
export function choicesOf(refs: FakeRefs): BranchChoices {
  const remoteBranches = Object.entries(refs.remotes ?? {}).flatMap(([remote, branches]) =>
    [...branches].sort().map((branch) => ({ remote, branch })),
  );
  const local = [...(refs.local ?? [])].sort();
  const defaultBranch = refs.remotes?.origin?.includes("main") ? "origin/main" : local.includes("main") ? "main" : null;
  return {
    start_points: [
      ...local,
      ...remoteBranches.map((known) => `${known.remote}/${known.branch}`),
      ...[...(refs.tags ?? [])].sort(),
    ],
    default_branch: defaultBranch,
    remote_branches: remoteBranches,
  };
}
