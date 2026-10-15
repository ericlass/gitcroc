// Trees for the fake core (fake.ts): the skeleton, ref nodes built like the
// core builds them, the filter, and the scenarios Playwright picks with
// `?fake=<name>`. Test support only; the real tree comes from the core.
import type {
  ConflictDetails,
  GroupBranch,
  GroupId,
  GroupSummary,
  GroupView,
  RemoteUrlsView,
  RepoId,
  RepoView,
  SummaryView,
  TreeItem,
  TreeNode,
  WorkspaceView,
} from "./bindings";
import { upstreamOf } from "./fakeBranches";
import { defaultDetails, type FakeCommit, type FakeWorkingCopy, keep, textFile } from "./fakeChanges";
import { type FakeHistory, hashOf } from "./fakeHistory";

const CATEGORIES = [
  ["local", "Local"],
  ["remotes", "Remotes"],
  ["tags", "Tags"],
  ["stashes", "Stashes"],
] as const;

/** What node ids start with in a place: under group `group`, or under Ungrouped for `null`. */
const placeOf = (group: GroupId | null) => (group === null ? "" : `group:${group}:`);

/** Like the core's derived group branch (GRP-4). */
function groupBranch(views: SummaryView[]): GroupBranch {
  const values = new Set<string>();
  for (const view of views) {
    const head = view.missing ? undefined : view.summary?.head;
    if (head !== undefined) {
      values.add(head.kind === "branch" ? `branch:${head.name}` : "detached");
    }
  }
  const [only] = values;
  if (only === undefined) {
    return { kind: "none" };
  }
  if (values.size > 1) {
    return { kind: "mixed", count: values.size };
  }
  return only === "detached" ? { kind: "detached" } : { kind: "branch", name: only.slice("branch:".length) };
}

/** A group's row as the core derives it from its members' summaries (UI-4). */
export function groupSummary(workspaceId: string, group: GroupView, summaries: SummaryView[]): GroupSummary {
  const members = group.repos.flatMap((repo) =>
    summaries.filter((view) => view.workspace_id === workspaceId && view.repo_id === repo),
  );
  return {
    workspace_id: workspaceId,
    group_id: group.id,
    branch: groupBranch(members),
    members: group.repos.length,
    dirty: members.filter((view) => !view.missing && view.summary?.dirty === true).length,
    conflicts: members.filter((view) => view.conflict).length,
  };
}

function groupNode(
  workspace: WorkspaceView,
  group: GroupView,
  members: TreeNode[],
  summaries: SummaryView[],
): TreeNode {
  const branches: TreeNode = {
    id: `group:${group.id}:branches`,
    label: "Branches",
    item: { kind: "group_branches", group_id: group.id },
    children: null,
  };
  return {
    id: `group:${group.id}`,
    label: group.name,
    item: { kind: "group", group_id: group.id, summary: groupSummary(workspace.id, group, summaries) },
    children: [branches, ...members],
  };
}

const ungrouped = (repos: TreeNode[]): TreeNode => ({
  id: "ungrouped",
  label: "Ungrouped",
  item: { kind: "ungrouped" },
  children: repos,
});

/** The members of `group`, in its order. */
const membersOf = (workspace: WorkspaceView, group: GroupView) =>
  group.repos.flatMap((id) => workspace.repos.filter((repo) => repo.id === id));

/** The repos in no group, in workspace order. */
const ungroupedRepos = (workspace: WorkspaceView) =>
  workspace.repos.filter((repo) => !workspace.groups.some((group) => group.repos.includes(repo.id)));

/** Each group with its Branches node and members, then Ungrouped; each repo with its four ref nodes not loaded. */
export function skeleton(workspace: WorkspaceView, summaries: SummaryView[] = []): TreeNode[] {
  const groups = workspace.groups.map((group) =>
    groupNode(
      workspace,
      group,
      membersOf(workspace, group).map((repo) => repoNode(repo, group.id)),
      summaries,
    ),
  );
  return [...groups, ungrouped(ungroupedRepos(workspace).map((repo) => repoNode(repo, null)))];
}

function repoNode(repo: RepoView, group: GroupId | null): TreeNode {
  const base = `${placeOf(group)}repo:${repo.id}`;
  return {
    id: base,
    label: repo.name,
    item: { kind: "repo", repo_id: repo.id, group_id: group },
    children: CATEGORIES.map(([key, label]) => ({
      id: `${base}:${key}`,
      label,
      item: { kind: key, repo_id: repo.id, group_id: group },
      children: null,
    })),
  };
}

/** `refNodes`' nodes as `refs_get` returns them for the place under group `group`. */
export function inPlace(nodes: TreeNode[], group: GroupId | null): TreeNode[] {
  if (group === null) {
    return nodes;
  }
  const place = (node: TreeNode): TreeNode => {
    const item = node.item;
    const placed =
      item.kind === "local" || item.kind === "remotes" || item.kind === "tags" || item.kind === "stashes"
        ? { ...item, group_id: group }
        : item;
    return { ...node, id: `${placeOf(group)}${node.id}`, item: placed, children: node.children?.map(place) ?? null };
  };
  return nodes.map(place);
}

/** The local branch names of loaded ref nodes. */
function localBranches(nodes: TreeNode[]): string[] {
  const names: string[] = [];
  const visit = (node: TreeNode) => {
    if (node.item.kind === "branch") {
      names.push(node.item.name);
    }
    node.children?.forEach(visit);
  };
  nodes.filter((node) => node.item.kind === "local").forEach(visit);
  return names;
}

/** A group's Branches node's children as `group_branches` returns them, not folded. */
export function groupBranchNodes(group: GroupView, refs: Record<RepoId, TreeNode[]>): TreeNode[] {
  const present = new Map<string, number>();
  for (const repo of group.repos) {
    for (const name of new Set(localBranches(refs[repo] ?? []))) {
      present.set(name, (present.get(name) ?? 0) + 1);
    }
  }
  return [...present.keys()].sort().map((name) =>
    leaf(`group:${group.id}:branches:ref:${name}`, name, {
      kind: "group_branch",
      group_id: group.id,
      name,
      present: present.get(name) ?? 0,
      members: group.repos.length,
    }),
  );
}

export interface FakeRefs {
  local?: string[];
  current?: string;
  /** Ahead and behind of local branches, by name; such a branch tracks `origin/<name>`. */
  counts?: Record<string, [number, number]>;
  /** What local branches track, by name, over what `counts` implies; `null` is nothing. */
  upstreams?: Record<string, string | null>;
  remotes?: Record<string, string[]>;
  /** Remote URLs by remote; a remote without an entry has none set. */
  urls?: Record<string, RemoteUrlsView>;
  tags?: string[];
  stashes?: string[];
}

const leaf = (id: string, label: string, item: TreeItem): TreeNode => ({ id, label, item, children: [] });

/** A stash entry's commit hash in the fake: made from its message, so it stays with the entry when the list changes. */
export function stashHash(message: string): string {
  let hash = 0;
  for (const char of message) {
    hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
  }
  return hash.toString(16).padStart(8, "0").repeat(5);
}

/** Folds `/`-separated names into folders: folders first, each sorted. */
function fold(parent: string, repoId: RepoId, names: string[], item: (name: string) => TreeItem): TreeNode[] {
  const build = (entries: string[], path: string): TreeNode[] => {
    const folders = new Map<string, string[]>();
    const refs: string[] = [];
    for (const entry of entries) {
      const slash = entry.indexOf("/");
      if (slash < 0) {
        refs.push(entry);
      } else {
        const folder = entry.slice(0, slash);
        folders.set(folder, [...(folders.get(folder) ?? []), entry.slice(slash + 1)]);
      }
    }
    const folderNodes = [...folders.keys()].sort().map((name) => ({
      id: `${parent}:dir:${path}${name}/`,
      label: name,
      item: { kind: "folder" as const, repo_id: repoId },
      children: build(folders.get(name) ?? [], `${path}${name}/`),
    }));
    const refNodes = refs.sort().map((name) => leaf(`${parent}:ref:${path}${name}`, name, item(`${path}${name}`)));
    return [...folderNodes, ...refNodes];
  };
  return build(names, "");
}

/** The four loaded ref nodes of `repoId`, as `refs_get` returns them. */
export function refNodes(repoId: RepoId, refs: FakeRefs): TreeNode[] {
  const base = `repo:${repoId}`;
  const local = fold(`${base}:local`, repoId, refs.local ?? [], (name) => ({
    kind: "branch",
    repo_id: repoId,
    name,
    current: name === refs.current,
    upstream: upstreamOf(refs, name),
    gone: false,
    ahead: refs.counts?.[name]?.[0] ?? null,
    behind: refs.counts?.[name]?.[1] ?? null,
  }));
  const remotes = Object.entries(refs.remotes ?? {}).map(([remote, branches]) => ({
    id: `${base}:remotes:${remote}`,
    label: remote,
    item: { kind: "remote" as const, repo_id: repoId, remote },
    children: fold(`${base}:remotes:${remote}`, repoId, branches, (name) => ({
      kind: "remote_branch",
      repo_id: repoId,
      name: `${remote}/${name}`,
      remote,
      branch: name,
    })),
  }));
  const tags = (refs.tags ?? []).map((name) =>
    leaf(`${base}:tags:ref:${name}`, name, { kind: "tag", repo_id: repoId, name, annotated: false }),
  );
  const stashes = (refs.stashes ?? []).map((message, index) =>
    leaf(`${base}:stashes:ref:${index}`, message, {
      kind: "stash",
      repo_id: repoId,
      index,
      hash: stashHash(message),
      message,
      seconds: 1_700_000_000 - index * 60,
    }),
  );
  const children = [local, remotes, tags, stashes];
  return CATEGORIES.map(([key, label], index) => ({
    id: `${base}:${key}`,
    label,
    item: { kind: key, repo_id: repoId, group_id: null },
    children: children[index] ?? [],
  }));
}

function prune(node: TreeNode, text: string): TreeNode | null {
  const item = node.item;
  const key = item.kind === "branch" || item.kind === "remote_branch" || item.kind === "tag" ? item.name : null;
  if (key?.toLowerCase().includes(text)) {
    return node;
  }
  const kept = (node.children ?? []).map((child) => prune(child, text)).filter((child) => child !== null);
  return kept.length > 0 ? { ...node, children: kept } : null;
}

/** Like the core's filter: matching repos and refs with their ancestors. */
export function filterTree(
  workspace: WorkspaceView,
  refs: Record<RepoId, TreeNode[]>,
  text: string,
  summaries: SummaryView[] = [],
): TreeNode[] {
  const lower = text.toLowerCase();
  if (lower === "") {
    return skeleton(workspace, summaries);
  }
  const filtered = (repo: RepoView, group: GroupId | null): TreeNode[] => {
    const loaded = inPlace(refs[repo.id] ?? [], group);
    const kept = loaded.map((node) => prune(node, lower)).filter((node) => node !== null);
    if (kept.length > 0) {
      return [{ ...repoNode(repo, group), children: kept }];
    }
    return repo.name.toLowerCase().includes(lower) ? [repoNode(repo, group)] : [];
  };
  const groups: TreeNode[] = [];
  for (const group of workspace.groups) {
    // A group matched by name shows all its members, not loaded.
    const named = group.name.toLowerCase().includes(lower);
    const members = membersOf(workspace, group).flatMap((repo) =>
      named ? [repoNode(repo, group.id)] : filtered(repo, group.id),
    );
    if (named || members.length > 0) {
      groups.push(groupNode(workspace, group, members, summaries));
    }
  }
  return [...groups, ungrouped(ungroupedRepos(workspace).flatMap((repo) => filtered(repo, null)))];
}

export function summary(workspaceId: string, repoId: RepoId, branch: string, change: Partial<SummaryView> = {}) {
  const view: SummaryView = {
    workspace_id: workspaceId,
    repo_id: repoId,
    summary: {
      head: { kind: "branch", name: branch, hash: "0123456789abcdef0123456789abcdef01234567" },
      upstream: { name: `origin/${branch}`, ahead: 0, behind: 0, gone: false },
      dirty: false,
      changes: 0,
      state: { operation: null, unmerged_paths: 0 },
    },
    stale: false,
    missing: false,
    error: null,
    refreshing: false,
    conflict: false,
    refs_version: 0,
    busy: false,
  };
  return { ...view, ...change };
}

export interface Scenario {
  repos: RepoView[];
  groups?: GroupView[];
  /** What the folder dialog returns. */
  pickedFolder?: string;
  /** What the clone's folder dialog returns. */
  cloneParent?: string;
  refs: Record<RepoId, TreeNode[]>;
  summaries: SummaryView[];
  /** See `fakeState.histories`. */
  histories?: Record<string, FakeHistory>;
  /** See `fakeState.historyDelay`. */
  historyDelay?: number;
  /** See `fakeState.slowRepos`. */
  slowRepos?: Record<RepoId, boolean>;
  /** See `fakeState.commits`. */
  commits?: Record<string, FakeCommit>;
  /** See `fakeState.workingCopies`. */
  workingCopies?: Record<RepoId, FakeWorkingCopy>;
  /** See `fakeState.refModels`. */
  refModels?: Record<RepoId, FakeRefs>;
  /** See `fakeState.conflicts`. */
  conflicts?: Record<RepoId, ConflictDetails>;
}

const repo = (id: string, name: string): RepoView => ({ id, name, path: `/src/${name}`, remote: null });

/** Named data sets for Playwright, picked with `?fake=<name>`. */
export function scenario(name: string, workspaceId: string): Scenario | null {
  if (name === "tree") {
    const refModels: Record<RepoId, FakeRefs> = {
      r1: {
        local: ["main", "feature/login", "bugfix/crash"],
        current: "main",
        counts: { main: [1, 2] },
        remotes: { origin: ["main", "feature/login"] },
        tags: ["v1.0"],
        stashes: ["On main: work"],
      },
      r2: { local: ["main", "release"], current: "main" },
      r3: { local: ["main"], current: "main" },
    };
    return {
      repos: [repo("r1", "api"), repo("r2", "web"), repo("r3", "login-service")],
      pickedFolder: "/src/new-repo",
      cloneParent: "/src",
      refModels,
      refs: Object.fromEntries(Object.entries(refModels).map(([id, refs]) => [id, refNodes(id, refs)])),
      summaries: [
        summary(workspaceId, "r1", "main", {
          summary: {
            head: { kind: "branch", name: "main", hash: "0123456789abcdef0123456789abcdef01234567" },
            upstream: { name: "origin/main", ahead: 1, behind: 2, gone: false },
            dirty: true,
            changes: 3,
            state: { operation: null, unmerged_paths: 0 },
          },
        }),
        summary(workspaceId, "r2", "main"),
        summary(workspaceId, "r3", "main", { stale: true }),
      ],
      // api's fifth commit merges a side branch (UI-16).
      commits: {
        [hashOf(4)]: {
          details: defaultDetails(hashOf(4), [hashOf(5), "51de000000000000000000000000000000000000"]),
          files: [[textFile("side.txt", [{ add: "from the side branch" }], "added")], [textFile("main.txt", [...keep(1, 2), { add: "on main" }])]],
        },
      },
      workingCopies: {
        r1: {
          staged: [textFile("src/app.ts", [...keep(1, 3), { add: "staged line" }])],
          unstaged: [
            textFile("src/app.ts", [...keep(1, 3), { add: "staged line" }, { add: "unstaged line" }]),
            textFile("notes.txt", [{ add: "untracked" }], "untracked"),
            // Two hunks, to stage one (UI-15).
            textFile("src/main.rs", [
              ...keep(1, 2),
              { remove: "line 3" },
              { add: "line three" },
              ...keep(4, 20),
              { add: "added near the end" },
              ...keep(24, 2),
            ]),
          ],
          unmerged: [],
        },
      },
    };
  }
  if (name === "groups") {
    // The tree data set with api and web in Backend and web and login-service in Services (UI-4, UI-7).
    const base = scenario("tree", workspaceId);
    return base === null
      ? null
      : {
          ...base,
          groups: [
            { id: "g1", name: "Backend", repos: ["r1", "r2"] },
            { id: "g2", name: "Services", repos: ["r2", "r3"] },
          ],
        };
  }
  if (name === "conflict") {
    // api is merging feature/login and stopped on two files (OPS-3).
    const base = scenario("tree", workspaceId);
    if (base === null) {
      return null;
    }
    const state = { operation: "merge" as const, unmerged_paths: 2 };
    return {
      ...base,
      summaries: base.summaries.map((known) =>
        known.repo_id === "r1" && known.summary !== null
          ? { ...known, conflict: true, summary: { ...known.summary, state } }
          : known,
      ),
      workingCopies: {
        r1: {
          staged: [],
          unstaged: [],
          unmerged: [
            { path: "README.md", conflict: "both_modified" },
            { path: "src/app.ts", conflict: "both_modified" },
          ],
          inConflict: true,
        },
      },
      conflicts: { r1: { ...state, applying: "feature/login", onto: "main", step: null } },
    };
  }
  if (name === "big-history") {
    // PERF-1, PERF-2: 100,000 commits, paged with a 20 ms delay.
    return {
      repos: [repo("r1", "huge")],
      refs: { r1: refNodes("r1", { local: ["main"], current: "main" }) },
      summaries: [summary(workspaceId, "r1", "main")],
      histories: {
        r1: {
          length: 100_000,
          prefix: "huge",
          refs: {
            0: [
              { kind: "current_branch", name: "main" },
              { kind: "remote_branch", name: "origin/main" },
            ],
          },
        },
      },
      historyDelay: 20,
    };
  }
  if (name === "big-tree") {
    const local = Array.from({ length: 5000 }, (_, index) => `branch-${String(index).padStart(4, "0")}`);
    return {
      repos: [repo("r1", "big")],
      refs: { r1: refNodes("r1", { local, current: "branch-0000" }) },
      summaries: [summary(workspaceId, "r1", "branch-0000")],
    };
  }
  return null;
}
