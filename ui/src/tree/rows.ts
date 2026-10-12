// The tree as the rows on screen: depth-first, children only below expanded
// nodes (PERF-1 renders just the visible slice of these). Which nodes exist
// and match a filter is the core's answer; this only lays it out.
import type { TreeNode } from "../ipc";
import { isRefsNode, repoOf, type WorkspaceTree } from "../stores/tree";

export type Row =
  | {
      kind: "node";
      id: string;
      node: TreeNode;
      depth: number;
      /** The id of the parent row's node; `null` at the top. */
      parent: string | null;
      expandable: boolean;
      expanded: boolean;
    }
  | { kind: "message"; id: string; depth: number; parent: string; text: string; error: boolean };

/** `node`'s children: its own, or its repo's loaded ones; `null` if not loaded. */
function childrenOf(node: TreeNode, tree: WorkspaceTree): TreeNode[] | null {
  if (node.children !== null) {
    return node.children;
  }
  const repo = repoOf(node);
  if (repo === null || !isRefsNode(node)) {
    return null;
  }
  return tree.refs[repo]?.find((loaded) => loaded.id === node.id)?.children ?? null;
}

/**
 * The visible rows. Ungrouped starts expanded, everything else collapsed.
 * While a filter is shown, every node the core sent with children is
 * expanded, so the matches are in view.
 */
export function flatten(tree: WorkspaceTree): Row[] {
  const filtering = tree.filtered !== null;
  const roots = tree.filtered ?? tree.roots ?? [];
  const toggled = filtering ? tree.filterExpanded : tree.expanded;
  const rows: Row[] = [];
  const visit = (nodes: TreeNode[], depth: number, parent: string | null) => {
    for (const node of nodes) {
      const children = childrenOf(node, tree);
      const expandable = children === null || children.length > 0;
      const opened = filtering && node.children !== null ? true : node.item.kind === "ungrouped";
      const expanded = expandable && (toggled[node.id] ?? opened);
      rows.push({ kind: "node", id: node.id, node, depth, parent, expandable, expanded });
      if (!expanded) {
        continue;
      }
      if (children !== null) {
        visit(children, depth + 1, node.id);
        continue;
      }
      const repo = repoOf(node);
      const error = repo === null ? undefined : tree.refsErrors[repo];
      rows.push({
        kind: "message",
        id: `${node.id}:message`,
        depth: depth + 1,
        parent: node.id,
        text: error ?? "Loading…",
        error: error !== undefined,
      });
    }
  };
  visit(roots, 0, null);
  return rows;
}
