// Remote add, rename, remove and URL edits for the fake core (fake.ts), on
// the ref model the fake trees are built from (`FakeRefs`): the checks and
// answers of the core's `op_remote`, simplified. Test support only; the
// real rules are the core's.
import type { RemoteOp } from "./bindings";
import { fakeNameCheck, upstreamOf } from "./fakeBranches";
import type { TagPlan } from "./fakeTags";
import type { FakeRefs } from "./fakeTree";

/** Why `name` cannot name a new remote, or `null`. */
export function fakeRemoteNameCheck(name: string): string | null {
  return fakeNameCheck(name) ?? (name.includes("/") ? "a remote name cannot contain '/'" : null);
}

function urlRefusal(url: string): string | null {
  const trimmed = url.trim();
  if (trimmed === "") {
    return "the URL is empty";
  }
  return trimmed.startsWith("-") ? "the URL cannot start with '-'" : null;
}

const without = <T>(record: Record<string, T> | undefined, key: string) =>
  Object.fromEntries(Object.entries(record ?? {}).filter(([known]) => known !== key));

/** What the core would answer for `op` on `refs`, `confirmed` or not. */
export function planRemote(refs: FakeRefs, op: RemoteOp, confirmed: boolean): TagPlan {
  const remotes = refs.remotes ?? {};
  const exists = (name: string) => name in remotes;
  const refused = (why: string): TagPlan => ({ kind: "refused", why });
  const newName = (name: string) =>
    fakeRemoteNameCheck(name) ?? (exists(name) ? `a remote named ${name} exists already` : null);
  if (op.kind === "add") {
    const why = newName(op.name) ?? urlRefusal(op.url);
    if (why !== null) {
      return refused(why);
    }
    const after: FakeRefs = {
      ...refs,
      remotes: { ...remotes, [op.name]: [] },
      urls: { ...refs.urls, [op.name]: { url: op.url.trim(), push_url: null } },
    };
    const label = op.fetch ? `Add remote ${op.name} and fetch it` : `Add remote ${op.name}`;
    return { kind: "run", label, refs: after };
  }
  if (!exists(op.name)) {
    return refused(`there is no remote ${op.name}`);
  }
  const tracking = (refs.local ?? []).filter((branch) => upstreamOf(refs, branch)?.startsWith(`${op.name}/`));
  switch (op.kind) {
    case "rename": {
      const why = op.new_name === op.name ? `the remote is named ${op.name} already` : newName(op.new_name);
      if (why !== null) {
        return refused(why);
      }
      const upstreams = { ...refs.upstreams };
      for (const branch of tracking) {
        upstreams[branch] = `${op.new_name}/${upstreamOf(refs, branch)?.slice(op.name.length + 1) ?? ""}`;
      }
      const urls = refs.urls?.[op.name];
      const after: FakeRefs = {
        ...refs,
        upstreams,
        remotes: { ...without(remotes, op.name), [op.new_name]: remotes[op.name] ?? [] },
        urls: urls === undefined ? refs.urls : { ...without(refs.urls, op.name), [op.new_name]: urls },
      };
      return { kind: "run", label: `Rename remote ${op.name} to ${op.new_name}`, refs: after };
    }
    case "remove": {
      if (!confirmed) {
        const unset = tracking.map((branch) => `Unset the upstream of ${branch} (${upstreamOf(refs, branch) ?? ""})`);
        return {
          kind: "confirm",
          actions: [
            `Remove remote ${op.name} and its remote-tracking branches`,
            ...(unset.length === 0 ? [`No branch has its upstream on ${op.name}`] : unset),
          ],
        };
      }
      const upstreams = { ...refs.upstreams };
      for (const branch of tracking) {
        upstreams[branch] = null;
      }
      const after = { ...refs, upstreams, remotes: without(remotes, op.name), urls: without(refs.urls, op.name) };
      return { kind: "run", label: `Remove remote ${op.name}`, refs: after };
    }
    case "set_url": {
      const why = urlRefusal(op.url) ?? (op.push_url === null ? null : urlRefusal(op.push_url));
      if (why !== null) {
        return refused(why);
      }
      // Like git: without a push URL given, the one set stays.
      const pushUrl = op.push_url?.trim() ?? refs.urls?.[op.name]?.push_url ?? null;
      const urls = { url: op.url.trim(), push_url: pushUrl };
      return {
        kind: "run",
        label: `Set the URL of remote ${op.name}`,
        refs: { ...refs, urls: { ...refs.urls, [op.name]: urls } },
      };
    }
  }
}
