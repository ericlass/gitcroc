// Trees for the fake core (fake.ts): the skeleton, ref nodes built like the
// core builds them, the filter, and the scenarios Playwright picks with
// `?fake=<name>`. Test support only; the real tree comes from the core.
import type {
  ConflictDetails,
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

/** Ungrouped with every repo, each with its four ref nodes not loaded. */
export function skeleton(workspace: WorkspaceView): TreeNode[] {
  const repos = workspace.repos.map(repoNode);
  return [{ id: "ungrouped", label: "Ungrouped", item: { kind: "ungrouped" }, children: repos }];
}

function repoNode(repo: RepoView): TreeNode {
  return {
    id: `repo:${repo.id}`,
    label: repo.name,
    item: { kind: "repo", repo_id: repo.id },
    children: CATEGORIES.map(([key, label]) => ({
      id: `repo:${repo.id}:${key}`,
      label,
      item: { kind: key, repo_id: repo.id },
      children: null,
    })),
  };
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
    item: { kind: key, repo_id: repoId },
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
export function filterTree(workspace: WorkspaceView, refs: Record<RepoId, TreeNode[]>, text: string): TreeNode[] {
  const lower = text.toLowerCase();
  if (lower === "") {
    return skeleton(workspace);
  }
  const repos: TreeNode[] = [];
  for (const repo of workspace.repos) {
    const kept = (refs[repo.id] ?? []).map((node) => prune(node, lower)).filter((node) => node !== null);
    if (kept.length > 0) {
      repos.push({ ...repoNode(repo), children: kept });
    } else if (repo.name.toLowerCase().includes(lower)) {
      repos.push(repoNode(repo));
    }
  }
  return [{ id: "ungrouped", label: "Ungrouped", item: { kind: "ungrouped" }, children: repos }];
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
  /** What the folder dialog returns. */
  pickedFolder?: string;
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
