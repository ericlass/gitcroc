// The tree pane (UI-1, UI-3, UI-7, UI-8): filter box, add button and the
// tree, virtualized (PERF-1), with a context menu on each row that has
// operations and double-click to check out a local branch. Everything
// shown comes from the stores, which take it from the core; this component
// only lays it out and forwards input.
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { type RepoId, type TreeItem, type TreeNode, type WorkspaceId, workspacePickAndAdd, workspaceRemoveRepo } from "../ipc";
import type { BranchStore } from "../stores/branch";
import { isBlocked, type SummaryStore } from "../stores/summaries";
import type { TreeStore } from "../stores/tree";
import type { WorkspaceStore } from "../stores/workspaces";
import { type MenuAction, menuFor } from "./menus";
import RepoRow, { AheadBehind } from "./RepoRow";
import { flatten, type Row } from "./rows";

/** Height of one row in CSS pixels; `.tree-row` in App.css sets it, as `.history-row` does. */
export const ROW_HEIGHT = 22;

/** Rows rendered beyond each edge of the visible ones. */
export const OVERSCAN = 8;

const INDENT = 14;

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The accessible name of each kind of row's menu. */
const MENU_NAMES: Partial<Record<TreeItem["kind"], string>> = {
  repo: "Repository",
  branch: "Branch",
  remote_branch: "Remote branch",
  tag: "Tag",
};

const asNode = (row: Row) => (row.kind === "node" ? row : undefined);
const asMessage = (row: Row) => (row.kind === "message" ? row : undefined);

/** What a branch, tag, stash or other non-repo row shows. */
function RefLabel(props: { node: TreeNode }) {
  const branch = () => {
    const item = props.node.item;
    return item.kind === "branch" ? item : undefined;
  };
  const stash = () => {
    const item = props.node.item;
    return item.kind === "stash" ? item : undefined;
  };
  return (
    <Show
      when={branch()}
      fallback={
        <span class="tree-label" title={stash() ? new Date((stash()?.seconds ?? 0) * 1000).toLocaleString() : undefined}>
          {props.node.label}
        </span>
      }
    >
      {(b) => (
        <>
          <span class="tree-label" classList={{ current: b().current }} title={b().upstream ?? undefined}>
            {props.node.label}
          </span>
          <Show when={b().current}>
            <span class="tree-current" role="img" aria-label="Current branch" title="Current branch">
              ✓
            </span>
          </Show>
          <AheadBehind ahead={b().ahead} behind={b().behind} />
          <Show when={b().gone}>
            <span class="tree-gone" title="The upstream is gone">
              gone
            </span>
          </Show>
        </>
      )}
    </Show>
  );
}

function AddReportView(props: { workspaces: WorkspaceStore; workspace: WorkspaceId }) {
  const report = () => props.workspaces.state.reports[props.workspace];
  return (
    <Show when={report()}>
      {(report) => (
        <ul class="tree-report" aria-label="Add result">
          <Show when={report().added.length > 0}>
            <li>Added: {report().added.map((repo) => repo.name).join(", ")}</li>
          </Show>
          <Show when={report().already_present.length > 0}>
            <li>Already present: {report().already_present.map((repo) => repo.name).join(", ")}</li>
          </Show>
          <For each={report().refused}>
            {(refusal) => (
              <li class="tree-refused">
                Refused {refusal.path}: {refusal.reason}
              </li>
            )}
          </For>
          <Show when={report().added.length + report().already_present.length + report().refused.length === 0}>
            <li>No repos found.</li>
          </Show>
        </ul>
      )}
    </Show>
  );
}

/** The rows, virtualized (PERF-1), with mouse and keyboard selection. */
function TreeList(props: {
  workspace: WorkspaceId;
  tree: TreeStore;
  summaries: SummaryStore;
  onMenu: (item: TreeItem, x: number, y: number) => void;
  onCheckout: (repo: RepoId, name: string) => void;
}) {
  let scroller: HTMLDivElement | undefined;
  const tree = () => props.tree.tree(props.workspace);
  const rows = createMemo(() => flatten(tree()));
  const selectedIndex = () => rows().findIndex((row) => row.id === tree().selected);

  const virtualizer = createVirtualizer({
    get count() {
      return rows().length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });

  const select = (index: number) => {
    const row = rows()[index];
    if (row?.kind !== "node") {
      return;
    }
    props.tree.select(props.workspace, row.node);
    virtualizer.scrollToIndex(index, { align: "auto" });
  };

  const toggle = (row: Row) => {
    if (row.kind === "node" && row.expandable) {
      props.tree.setExpanded(props.workspace, row.node, !row.expanded);
    }
  };

  /** The next node row from `from` in direction `step`. */
  const nextNode = (from: number, step: 1 | -1) => {
    for (let index = from + step; index >= 0 && index < rows().length; index += step) {
      if (rows()[index]?.kind === "node") {
        return index;
      }
    }
    return -1;
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const index = selectedIndex();
    const row = rows()[index];
    switch (event.key) {
      case "ArrowDown": {
        const next = nextNode(index, 1);
        if (next >= 0) {
          select(next);
        }
        break;
      }
      case "ArrowUp": {
        const previous = index < 0 ? nextNode(-1, 1) : nextNode(index, -1);
        if (previous >= 0) {
          select(previous);
        }
        break;
      }
      case "ArrowRight":
        if (row?.kind === "node" && row.expandable && !row.expanded) {
          toggle(row);
        } else if (row?.kind === "node" && row.expanded) {
          const child = nextNode(index, 1);
          if (child >= 0 && rows()[child]?.depth === row.depth + 1) {
            select(child);
          }
        }
        break;
      case "ArrowLeft":
        if (row?.kind === "node" && row.expanded) {
          toggle(row);
        } else if (row?.parent) {
          const parent = row.parent;
          select(rows().findIndex((candidate) => candidate.id === parent));
        }
        break;
      case "Home":
        select(nextNode(-1, 1));
        break;
      case "End":
        select(nextNode(rows().length, -1));
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  const summary = (node: TreeNode) => {
    const repo = node.item.kind === "repo" ? node.item.repo_id : null;
    return repo === null ? undefined : props.summaries.state.byWorkspace[props.workspace]?.[repo];
  };

  return (
    <div
      class="tree-scroller"
      ref={(element) => (scroller = element)}
      role="tree"
      aria-label="Repositories"
      tabindex="0"
      aria-activedescendant={selectedIndex() >= 0 ? `tree-row-${selectedIndex()}` : undefined}
      onKeyDown={onKeyDown}
    >
      <div class="tree-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <For each={virtualizer.getVirtualItems()}>
          {(item) => (
            <Show when={rows()[item.index]}>
              {(row) => {
                const node = () => asNode(row());
                return (
                  <div
                    id={`tree-row-${item.index}`}
                    class="tree-row"
                    classList={{ selected: row().id === tree().selected }}
                    role={node() ? "treeitem" : "none"}
                    aria-level={row().depth + 1}
                    aria-expanded={node()?.expandable ? node()?.expanded : undefined}
                    aria-selected={node() ? row().id === tree().selected : undefined}
                    style={{
                      transform: `translateY(${item.start}px)`,
                      "padding-left": `${4 + row().depth * INDENT}px`,
                    }}
                    onClick={() => select(item.index)}
                    onDblClick={() => {
                      // UI-7: a local branch is checked out; other rows open or close.
                      const target = node()?.node.item;
                      if (target?.kind === "branch") {
                        props.onCheckout(target.repo_id, target.name);
                      } else {
                        toggle(row());
                      }
                    }}
                    onContextMenu={(event) => {
                      const target = node()?.node.item;
                      if (target !== undefined && menuFor(target, false).length > 0) {
                        event.preventDefault();
                        select(item.index);
                        props.onMenu(target, event.clientX, event.clientY);
                      }
                    }}
                  >
                    <Show
                      when={node()}
                      fallback={
                        <span class="tree-message" classList={{ error: asMessage(row())?.error }}>
                          {asMessage(row())?.text}
                        </span>
                      }
                    >
                      {(shown) => (
                        <>
                          <span
                            class="tree-chevron"
                            aria-hidden="true"
                            onClick={(event) => {
                              event.stopPropagation();
                              select(item.index);
                              toggle(shown());
                            }}
                          >
                            {shown().expandable ? (shown().expanded ? "▾" : "▸") : ""}
                          </span>
                          <Show when={shown().node.item.kind === "repo"} fallback={<RefLabel node={shown().node} />}>
                            <RepoRow name={shown().node.label} view={summary(shown().node)} />
                          </Show>
                        </>
                      )}
                    </Show>
                  </div>
                );
              }}
            </Show>
          )}
        </For>
      </div>
    </div>
  );
}

export default function TreePane(props: {
  workspace: WorkspaceId;
  tree: TreeStore;
  summaries: SummaryStore;
  workspaces: WorkspaceStore;
  branch: BranchStore;
}) {
  const [actionError, setActionError] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ item: TreeItem; x: number; y: number } | null>(null);

  const tree = () => props.tree.tree(props.workspace);
  /** Loaded, unfiltered and without repos. */
  const empty = () => tree().filtered === null && tree().roots?.[0]?.children?.length === 0;

  const add = () => {
    setActionError(null);
    workspacePickAndAdd(props.workspace).catch((error) => setActionError(message(error)));
  };

  const remove = (repo: RepoId) => {
    setMenu(null);
    setActionError(null);
    workspaceRemoveRepo(props.workspace, repo).catch((error) => setActionError(message(error)));
  };

  const blocked = (repo: RepoId) => isBlocked(props.summaries.view(props.workspace, repo));
  const itemBlocked = (item: TreeItem) => item.kind !== "ungrouped" && blocked(item.repo_id);

  const run = (repo: RepoId, op: Parameters<BranchStore["run"]>[2]) => void props.branch.run(props.workspace, repo, op);

  /** Does what `action` of the menu of `item` says. */
  const act = (action: MenuAction, item: TreeItem) => {
    setMenu(null);
    if (item.kind === "ungrouped") {
      return;
    }
    const repo = item.repo_id;
    const open = props.branch.open.bind(null, props.workspace);
    switch (action) {
      case "remove_repo":
        remove(repo);
        return;
      case "create":
        open({
          kind: "create",
          repo,
          start:
            item.kind === "branch" || item.kind === "remote_branch" || item.kind === "tag"
              ? { kind: "revision", revision: item.name }
              : { kind: "head" },
        });
        return;
      case "checkout":
        if (item.kind === "branch") {
          run(repo, { kind: "checkout", name: item.name });
        }
        return;
      case "checkout_remote":
        if (item.kind === "remote_branch") {
          run(repo, { kind: "checkout_remote", branch: { remote: item.remote, branch: item.branch } });
        }
        return;
      case "checkout_detached":
        if (item.kind === "tag") {
          run(repo, { kind: "checkout_detached", revision: item.name });
        }
        return;
      case "delete_remote":
        if (item.kind === "remote_branch") {
          run(repo, { kind: "delete_remote", branch: { remote: item.remote, branch: item.branch } });
        }
        return;
      case "rename":
      case "delete":
      case "set_upstream":
      case "unset_upstream":
        if (item.kind !== "branch") {
          return;
        }
        if (action === "rename") {
          open({ kind: "rename", repo, name: item.name });
        } else if (action === "delete") {
          open({ kind: "delete", repo, name: item.name, tracked: item.upstream !== null && !item.gone });
        } else if (action === "set_upstream") {
          open({ kind: "upstream", repo, name: item.name, upstream: item.upstream });
        } else {
          run(repo, { kind: "set_upstream", name: item.name, upstream: null });
        }
    }
  };

  const closeMenu = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      setMenu(null);
    }
  };
  document.addEventListener("keydown", closeMenu);
  onCleanup(() => document.removeEventListener("keydown", closeMenu));

  return (
    <div class="tree-pane">
      <div class="tree-header">
        <input
          class="tree-filter"
          type="search"
          placeholder="Filter repos and branches"
          aria-label="Filter repos and branches"
          value={tree().filter}
          onInput={(event) => void props.tree.setFilter(props.workspace, event.currentTarget.value)}
        />
        <button type="button" onClick={add}>
          Add repository
        </button>
      </div>
      <Show when={actionError()}>{(error) => <p role="alert">{error()}</p>}</Show>
      <Show when={props.branch.dialog(props.workspace) === null && props.branch.error(props.workspace)}>
        {(error) => (
          <div role="alert" class="tree-refusal">
            <span>{error()}</span>
            <button type="button" onClick={() => props.branch.dismissError(props.workspace)}>
              Dismiss
            </button>
          </div>
        )}
      </Show>
      <AddReportView workspaces={props.workspaces} workspace={props.workspace} />
      <Show when={tree().error}>{(error) => <p role="alert">Cannot read the tree: {error()}</p>}</Show>
      <Show
        when={!empty()}
        fallback={
          <div class="tree-empty">
            <p class="placeholder">This workspace has no repositories yet.</p>
            <button type="button" onClick={add}>
              Add repository
            </button>
          </div>
        }
      >
        <TreeList
          workspace={props.workspace}
          tree={props.tree}
          summaries={props.summaries}
          onMenu={(item, x, y) => setMenu({ item, x, y })}
          onCheckout={(repo, name) => {
            if (!blocked(repo)) {
              run(repo, { kind: "checkout", name });
            }
          }}
        />
      </Show>
      <Show when={menu()}>
        {(open) => (
          <div class="menu-backdrop" onClick={() => setMenu(null)} onContextMenu={(event) => event.preventDefault()}>
            <div
              class="menu"
              role="menu"
              aria-label={MENU_NAMES[open().item.kind] ?? "Actions"}
              style={{ left: `${open().x}px`, top: `${open().y}px` }}
              ref={(element) =>
                queueMicrotask(() => element.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus())
              }
              onClick={(event) => event.stopPropagation()}
            >
              <For each={menuFor(open().item, itemBlocked(open().item))}>
                {(entry) => (
                  <button
                    type="button"
                    role="menuitem"
                    disabled={entry.disabled}
                    onClick={() => act(entry.action, open().item)}
                  >
                    {entry.label}
                  </button>
                )}
              </For>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
