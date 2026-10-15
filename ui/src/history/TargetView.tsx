// What the history pane shows for a group or several repos (section 8
// "History pane"): the summary strip (UI-11), one line per repo of the
// target set in the core's order (GRP-5), then the merged history. A line
// shows the summary the UI already has, stale as in the tree (PERF-3), and
// a click selects its repo in the tree. Repo text is text only (SEC-3).
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from "solid-js";
import { type RepoId, type SummaryView, type TargetSet, targetRepos, type WorkspaceId } from "../ipc";
import type { SummaryStore } from "../stores/summaries";
import type { TreeStore } from "../stores/tree";
import { AheadBehind } from "../tree/RepoRow";

/** Height of one strip line in CSS pixels; `.strip-line` in App.css sets it. */
export const LINE_HEIGHT = 22;

/** Lines shown before the strip scrolls. */
export const STRIP_LINES = 8;

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

function StripLine(props: { name: string; view: SummaryView | undefined; onSelect: () => void }) {
  const summary = () => props.view?.summary ?? null;
  const head = () => {
    const current = summary()?.head;
    if (!current) {
      return null;
    }
    return current.kind === "branch" ? current.name : current.hash.slice(0, 7);
  };
  const stale = () => props.view?.stale ?? true;
  return (
    <button
      type="button"
      class="strip-line"
      classList={{ stale: stale() }}
      data-stale={stale()}
      onClick={() => props.onSelect()}
    >
      <span class="tree-label">{props.name}</span>
      <Show when={head()}>
        {(head) => (
          <span class="tree-head" classList={{ detached: summary()?.head.kind === "detached" }}>
            {head()}
          </span>
        )}
      </Show>
      <Show when={summary()?.upstream}>{(up) => <AheadBehind ahead={up().ahead} behind={up().behind} />}</Show>
      <Show when={summary()?.dirty && summary()?.changes}>
        {(changes) => (
          <span
            class="tree-dirty"
            role="img"
            aria-label={plural(changes(), "changed file", "changed files")}
            title={plural(changes(), "file with uncommitted changes", "files with uncommitted changes")}
          >
            ●{changes()}
          </span>
        )}
      </Show>
      <Show when={props.view?.conflict}>
        <span class="tree-conflict" role="img" aria-label="Conflict state" title="Conflict state">
          ⚔
        </span>
      </Show>
      <Show when={props.view?.missing}>
        <span class="tree-warning" role="img" aria-label="Folder is missing" title="Folder is missing">
          ⚠
        </span>
      </Show>
    </button>
  );
}

/** The strip's lines, virtualized beyond {@link STRIP_LINES} in a box of fixed height. */
function SummaryStrip(props: {
  workspace: WorkspaceId;
  repos: RepoId[];
  summaries: SummaryStore;
  name: (repo: RepoId) => string;
  onSelect: (repo: RepoId) => void;
}) {
  let scroller: HTMLDivElement | undefined;
  const virtualizer = createVirtualizer({
    get count() {
      return props.repos.length;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => LINE_HEIGHT,
    overscan: STRIP_LINES,
  });
  return (
    <div
      class="summary-strip"
      ref={(element) => (scroller = element)}
      role="list"
      aria-label="Repositories of the selection"
      style={{ height: `${Math.min(props.repos.length, STRIP_LINES) * LINE_HEIGHT}px` }}
    >
      <div class="strip-lines" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <For each={virtualizer.getVirtualItems()}>
          {(item) => (
            <Show when={props.repos[item.index]}>
              {(repo) => (
                <div role="listitem" class="strip-slot" style={{ transform: `translateY(${item.start}px)` }}>
                  <StripLine
                    name={props.name(repo())}
                    view={props.summaries.view(props.workspace, repo())}
                    onSelect={() => props.onSelect(repo())}
                  />
                </div>
              )}
            </Show>
          )}
        </For>
      </div>
    </div>
  );
}

export default function TargetView(props: {
  workspace: WorkspaceId;
  target: TargetSet;
  tree: TreeStore;
  summaries: SummaryStore;
  name: (repo: RepoId) => string;
}) {
  const [repos, setRepos] = createSignal<RepoId[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  /** The target set as text, so the same set is not resolved twice. */
  const key = createMemo(() => JSON.stringify(props.target));

  // The core resolves the target set (GRP-5), again whenever the tree is
  // read again, as after a group changed.
  createEffect(
    on(
      () => [key(), props.tree.tree(props.workspace).roots] as const,
      () => {
        let current = true;
        onCleanup(() => (current = false));
        targetRepos(props.workspace, props.target).then(
          (resolved) => {
            if (current) {
              setRepos(resolved);
              setError(null);
            }
          },
          (failed: unknown) => {
            if (current) {
              setRepos(null);
              setError(message(failed));
            }
          },
        );
      },
    ),
  );

  /** The row of `repo` to select: under the group being viewed, or the row that was picked. */
  const rowOf = (repo: RepoId): string | null => {
    const target = props.target;
    const tree = props.tree.tree(props.workspace);
    if (target.kind === "group") {
      const group = tree.roots?.find((node) => node.item.kind === "group" && node.item.group_id === target.group_id);
      const member = group?.children?.find((node) => node.item.kind === "repo" && node.item.repo_id === repo);
      return member?.id ?? null;
    }
    return tree.multi?.find((row) => row.item.repo_id === repo)?.row ?? null;
  };

  const select = (repo: RepoId) => {
    const row = rowOf(repo);
    if (row !== null) {
      props.tree.selectRow(props.workspace, row);
    }
  };

  return (
    <div class="history-pane target-view">
      <Show when={error()}>{(text) => <p role="alert">Cannot show the selection: {text()}</p>}</Show>
      <Show when={repos()}>
        {(shown) => (
          <Show when={shown().length > 0} fallback={<p class="placeholder">This group has no repositories.</p>}>
            <SummaryStrip
              workspace={props.workspace}
              repos={shown()}
              summaries={props.summaries}
              name={props.name}
              onSelect={select}
            />
          </Show>
        )}
      </Show>
      <p class="placeholder">Merged history arrives in M3-07</p>
    </div>
  );
}
