// The fake core's palette (UI-22): the catalog the core exports, matched and
// ranked as `gitcroc_core::palette` does, so the UI tests see the core's
// answers. Test support only; the UI itself filters nothing (ARCH-6).
import {
  PALETTE_CATALOG,
  type PaletteItem,
  type PaletteResult,
  type SummaryView,
  type WorkspaceView,
} from "./bindings";
import type { FakeRefs } from "./fakeTree";

/** The most items one answer holds, as `MAX_PALETTE_ITEMS` in the core. */
export const MAX_PALETTE_ITEMS = 100;

/** How a name matches the query, best first. */
const Rank = { Whole: 0, Prefix: 1, WordPrefix: 2, Anywhere: 3 } as const;
type Rank = (typeof Rank)[keyof typeof Rank];

const isAlphanumeric = (char: string) => /[\p{L}\p{N}]/u.test(char);

/** How `name` matches the query's `words` and `whole`, when every word occurs in `text`; `null` when one does not. */
function rank(words: string[], whole: string, name: string, text: string): Rank | null {
  const lower = text.toLowerCase();
  if (!words.every((word) => lower.includes(word))) {
    return null;
  }
  const shown = name.toLowerCase();
  if (shown === whole) {
    return Rank.Whole;
  }
  if (shown.startsWith(whole)) {
    return Rank.Prefix;
  }
  for (let at = shown.indexOf(whole); at > 0; at = shown.indexOf(whole, at + 1)) {
    const before = shown[at - 1];
    if (before !== undefined && !isAlphanumeric(before)) {
      return Rank.WordPrefix;
    }
  }
  return Rank.Anywhere;
}

/**
 * The answer to `text` in `workspace`: operations, then repos in tree order
 * (group members first, then the ungrouped repos, each once), groups, and
 * each repo's local and remote branches, ranked by name with ties in that
 * order. The AI entries are left out unless `ai` (AI-16).
 */
export function fakePaletteQuery(
  workspace: WorkspaceView,
  summaries: SummaryView[],
  refModels: Record<string, FakeRefs>,
  ai: boolean,
  text: string,
): PaletteResult {
  const words = text.split(/\s+/).filter((word) => word !== "").map((word) => word.toLowerCase());
  const whole = words.join(" ");
  const found: { rank: Rank; item: PaletteItem }[] = [];
  const add = (name: string, text: string, item: () => PaletteItem) => {
    const matched = words.length === 0 ? Rank.Anywhere : rank(words, whole, name, text);
    if (matched !== null) {
      found.push({ rank: matched, item: item() });
    }
  };
  for (const known of PALETTE_CATALOG) {
    if (ai || !known.ai) {
      add(known.label, [known.label, ...known.keywords].join(" "), () => ({
        kind: "operation",
        op: known.op,
        label: known.label,
        subject: known.subject,
      }));
    }
  }
  if (words.length > 0) {
    const members = workspace.groups.flatMap((group) => group.repos);
    const order = [...new Set([...members, ...workspace.repos.map((repo) => repo.id)])];
    const nameOf = (repo: string) => workspace.repos.find((known) => known.id === repo)?.name ?? repo;
    for (const repo of order) {
      const head = summaries.find((known) => known.repo_id === repo)?.summary?.head;
      add(nameOf(repo), nameOf(repo), () => ({
        kind: "repo",
        repo_id: repo,
        name: nameOf(repo),
        branch: refModels[repo]?.current ?? (head?.kind === "branch" ? head.name : null),
      }));
    }
    for (const group of workspace.groups) {
      add(group.name, group.name, () => ({
        kind: "group",
        group_id: group.id,
        name: group.name,
        members: group.repos.length,
      }));
    }
    for (const repo of order) {
      const refs = refModels[repo];
      const repoName = nameOf(repo);
      const branch = (name: string, fullName: string, remote: string | null) =>
        add(name, `${name} ${repoName}`, () => ({
          kind: "branch",
          repo_id: repo,
          repo_name: repoName,
          name,
          full_name: fullName,
          remote,
        }));
      for (const name of refs?.local ?? []) {
        branch(name, `refs/heads/${name}`, null);
      }
      for (const [remote, names] of Object.entries(refs?.remotes ?? {})) {
        for (const name of names) {
          branch(`${remote}/${name}`, `refs/remotes/${remote}/${name}`, remote);
        }
      }
    }
  }
  // A stable sort: ties keep their kind and catalog or tree order.
  found.sort((a, b) => a.rank - b.rank);
  return { items: found.slice(0, MAX_PALETTE_ITEMS).map((known) => known.item), total: found.length };
}
