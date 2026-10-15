// The history pane (UI-9, UI-10, UI-12, UI-13): search box, the conflict
// banner (OPS-2), column header and the commit list, virtualized and paged
// (PERF-1, PERF-2), with a context menu on each commit and a
// multi-selection for cherry-pick. The merged history of a group or several
// repos (PERF-5) shows each row's repo where the graph would be, and a row
// acts on its own repo. Rows, lanes and search results come from the core
// through the history store; this component lays them out and forwards
// input.
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, createSignal, For, Index, Match, on, onMount, Show, Switch } from "solid-js";
import ConflictBanner from "../integrate/ConflictBanner";
import { IntegrateRefusal } from "../integrate/IntegrateDialogs";
import type { HistoryRow, RefLabel, RepoId, WorkspaceId } from "../ipc";
import Splitter from "../layout/Splitter";
import type { BranchStore } from "../stores/branch";
import { commitOf, type HistoryStore } from "../stores/history";
import type { IntegrateStore } from "../stores/integrate";
import type { ResetStore } from "../stores/reset";
import type { TagStore } from "../stores/tag";
import { type HistoryColumn, type LayoutStore, MIN_COLUMN_WIDTH } from "../stores/layout";
import { isBlocked, type SummaryStore } from "../stores/summaries";
import CommitMenu, { type CommitAction, commitMenu } from "./CommitMenu";
import Graph from "./Graph";
import { formatDate, formatFullDate, historyRows, type ListRow } from "./rows";

/**
 * Height of one row in CSS pixels; every row has it. `.history-row` in
 * App.css sets it: a constant inline style would become a `style`
 * attribute in Solid's template, which the CSP blocks (SEC-2).
 */
export const ROW_HEIGHT = 24;

/** Rows rendered beyond each edge of the visible ones. */
export const OVERSCAN = 8;

/** The next page is requested when this few loaded rows are left below the view. */
export const PREFETCH_ROWS = 100;

const REF_TITLES: Record<RefLabel["kind"], string> = {
  head: "Detached HEAD",
  current_branch: "Current branch",
  local_branch: "Local branch",
  remote_branch: "Remote branch",
  tag: "Tag",
};

function RefLabels(props: { refs: RefLabel[] }) {
  return (
    <For each={props.refs}>
      {(label) => (
        <span
          class={`ref-label ref-${label.kind}`}
          title={`${REF_TITLES[label.kind]}: ${label.name}`}
          aria-label={`${REF_TITLES[label.kind]} ${label.name}`}
        >
          {label.kind === "current_branch" ? `✓ ${label.name}` : label.name}
        </span>
      )}
    </For>
  );
}

/** A commit's cells; `repo` names its repo in a merged history, which has it in place of the graph (UI-9). */
function CommitCells(props: { commit: HistoryRow; repo: string | null }) {
  const dateTitle = () =>
    `Authored ${formatFullDate(props.commit.author_time.seconds)}\n` +
    `Committed ${formatFullDate(props.commit.commit_time.seconds)}`;
  return (
    <>
      <Show
        when={props.repo}
        fallback={
          <span class="history-cell history-graph">
            <Graph graph={props.commit.graph} height={ROW_HEIGHT} />
          </span>
        }
      >
        {(repo) => (
          <span class="history-cell history-graph history-repo" title={repo()}>
            {repo()}
          </span>
        )}
      </Show>
      <span class="history-cell history-subject">
        <RefLabels refs={props.commit.refs} />
        <span class="history-subject-text">{props.commit.subject}</span>
      </span>
      <span class="history-cell history-author" title={props.commit.author_email}>
        {props.commit.author_name}
      </span>
      <span class="history-cell history-date" title={dateTitle()}>
        {formatDate(props.commit.author_time.seconds)}
      </span>
      <span class="history-cell history-hash">{props.commit.short_hash}</span>
    </>
  );
}

const COLUMNS: { column: HistoryColumn | null; label: string; class: string }[] = [
  // "Repository" in a merged history, at the graph's width.
  { column: "graph", label: "Graph", class: "history-graph" },
  { column: null, label: "Subject", class: "history-subject" },
  { column: "author", label: "Author", class: "history-author" },
  { column: "date", label: "Date", class: "history-date" },
  { column: "hash", label: "Hash", class: "history-hash" },
];

function CommitGraphOffer(props: { onWrite: () => void; onDecline: () => void }) {
  return (
    <div class="history-offer" role="group" aria-label="Commit-graph offer">
      <p>
        This repo has no commit-graph file, so its history loads slowly. Git can write one now in the background;
        nothing else changes.
      </p>
      <div class="history-offer-actions">
        <button type="button" onClick={() => props.onWrite()}>
          Write commit-graph file
        </button>
        <button type="button" onClick={() => props.onDecline()}>
          Don't ask again
        </button>
      </div>
    </div>
  );
}

/** One row's content; it updates in place when the row shows another commit. */
function RowView(props: { row: ListRow; name: ((repo: RepoId) => string) | null }) {
  return (
    <Switch>
      <Match when={props.row.kind === "uncommitted" && props.row}>
        {(uncommitted) => (
          <span class="history-cell history-uncommitted">Uncommitted changes ({uncommitted().count})</span>
        )}
      </Match>
      <Match when={props.row.kind === "commit" && props.row}>
        {(commit) => <CommitCells commit={commit().commit} repo={props.name?.(commit().commit.repo_id) ?? null} />}
      </Match>
      <Match when={props.row.kind === "placeholder"}>
        <span class="history-cell history-skeleton" aria-label="Loading" />
      </Match>
    </Switch>
  );
}

/** The commit list, virtualized (PERF-1) and paged as it scrolls (PERF-2). */
function HistoryList(props: {
  workspace: WorkspaceId;
  history: HistoryStore;
  summaries: SummaryStore;
  name: (repo: RepoId) => string;
  branch: BranchStore;
  integrate: IntegrateStore;
  tag: TagStore;
  reset: ResetStore;
}) {
  let scroller: HTMLDivElement | undefined;
  const history = () => props.history.history(props.workspace);
  /** A merged history: rows of several repos. */
  const merged = () => history().source?.repo === null;
  const viewOf = (repo: RepoId) => props.summaries.view(props.workspace, repo);
  const changes = () => {
    const shown = history().source?.repo;
    return shown ? (viewOf(shown)?.summary?.changes ?? 0) : 0;
  };
  const rows = createMemo(() => historyRows(history(), changes()));
  const selectedIndex = () => {
    const selected = history().selected;
    return selected === null ? -1 : rows().indexOf(selected);
  };

  const virtualizer = createVirtualizer({
    get count() {
      return rows().count;
    },
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });

  // Each row element keeps a slot, its index modulo the number of slots, and
  // shows the row of that slot. Scrolling gives new content only to the rows
  // that come into view, and no element is made anew: new ones would leave
  // the web process the memory of every row ever shown (M1-14). Rows are
  // positioned, so their order in the DOM does not matter; `aria-posinset`
  // tells assistive technology where each one is.
  let slotCount = 0;
  const slots = createMemo(() => {
    const items = virtualizer.getVirtualItems();
    slotCount = Math.max(slotCount, items.length);
    const slots = new Array<(typeof items)[number] | undefined>(slotCount).fill(undefined);
    for (const item of items) {
      slots[item.index % slotCount] = item;
    }
    return slots;
  });

  // Near the end of the loaded rows, ask for the next page.
  createEffect(() => {
    const items = virtualizer.getVirtualItems();
    const last = items[items.length - 1];
    if (last && last.index >= rows().loaded - PREFETCH_ROWS && !history().done) {
      void props.history.more(props.workspace);
    }
  });

  // Back in a selection the user saw before: where they were.
  createEffect(
    on(
      () => history().restore,
      (restore) => {
        if (restore !== null && scroller) {
          scroller.scrollTop = restore;
          props.history.setScrollTop(props.workspace, scroller.scrollTop);
        }
      },
    ),
  );

  // Moving the pane to the other dock builds the list anew: where it was.
  onMount(() => {
    if (scroller) {
      scroller.scrollTop = history().scrollTop;
    }
  });

  // A commit selected elsewhere, such as a parent in the changes pane,
  // scrolls into view when it is loaded; nothing is read to find it.
  createEffect(
    on(
      () => history().selected,
      () => {
        const index = selectedIndex();
        if (index >= 0) {
          virtualizer.scrollToIndex(index, { align: "auto" });
        }
      },
      { defer: true },
    ),
  );

  const select = (index: number) => {
    const row = rows().at(index);
    if (row === undefined || row.kind === "placeholder") {
      return;
    }
    anchor = row.id;
    props.history.select(props.workspace, row.id);
    virtualizer.scrollToIndex(index, { align: "auto" });
  };

  /** Where a Shift-click's range starts: the row last clicked without Shift. */
  let anchor: string | null = null;

  /** A click: Ctrl/Cmd adds or removes a commit, Shift selects the commits from the anchor (UI-13). */
  const click = (index: number, event: MouseEvent) => {
    const row = rows().at(index);
    if (row?.kind !== "commit") {
      select(index);
      return;
    }
    if (event.ctrlKey || event.metaKey) {
      const marked = history().marked;
      anchor = row.id;
      props.history.mark(
        props.workspace,
        row.id,
        marked.includes(row.id) ? marked.filter((hash) => hash !== row.id) : [...marked, row.id],
      );
      return;
    }
    const from = anchor === null ? -1 : rows().indexOf(anchor);
    if (event.shiftKey && from >= 0) {
      const hashes: string[] = [];
      for (let at = Math.min(from, index); at <= Math.max(from, index); at++) {
        const inRange = rows().at(at);
        if (inRange?.kind === "commit") {
          hashes.push(inRange.id);
        }
      }
      props.history.mark(props.workspace, row.id, hashes);
      return;
    }
    select(index);
  };

  /** `text` names the row the menu was opened on, as `1a2b3c4 Subject`. */
  const [menu, setMenu] = createSignal<{ x: number; y: number; commits: string[]; text: string } | null>(null);

  /** Opens the menu of commit row `index`: for the multi-selection when the row is in it, else for the row alone. */
  const openMenu = (index: number, x: number, y: number) => {
    const row = rows().at(index);
    if (row?.kind !== "commit") {
      return;
    }
    let commits = history().marked;
    if (!commits.includes(row.id)) {
      select(index);
      commits = [row.id];
    }
    setMenu({ x, y, commits, text: `${row.commit.short_hash} ${row.commit.subject}` });
  };

  /** The repo and hashes of rows `rowIds`; the repo is `null` when they are of several repos. */
  const commitsOf = (rowIds: string[]) => {
    const source = history().source;
    const commits = source === null ? [] : rowIds.map((rowId) => commitOf(source, rowId));
    const repos = new Set(commits.map((commit) => commit.repo));
    const [repo] = repos;
    return { repo: repos.size === 1 && repo !== undefined ? repo : null, hashes: commits.map((commit) => commit.hash) };
  };

  /** The menu of rows `rowIds` (UI-13): a cherry-pick takes commits of one repo only. */
  const entries = (rowIds: string[]) => {
    const { repo } = commitsOf(rowIds);
    const entries = commitMenu(rowIds.length, repo !== null && isBlocked(viewOf(repo)));
    return repo !== null
      ? entries
      : entries.map((entry) =>
          entry.action === "cherry_pick"
            ? { ...entry, disabled: true, title: "Cherry-pick takes commits of one repository only" }
            : entry,
        );
  };

  const act = (action: CommitAction, rowIds: string[], text: string) => {
    setMenu(null);
    const { repo: shown, hashes: commits } = commitsOf(rowIds);
    const [hash] = commits;
    if (shown === null || hash === undefined) {
      return;
    }
    switch (action) {
      case "checkout":
        void props.branch.run(props.workspace, shown, { kind: "checkout_detached", revision: hash });
        return;
      case "create_branch":
        props.branch.open(props.workspace, { kind: "create", repo: shown, start: { kind: "revision", revision: hash } });
        return;
      case "cherry_pick":
        void props.integrate.run(props.workspace, shown, { kind: "cherry_pick", commits });
        return;
      case "revert":
        void props.integrate.run(props.workspace, shown, { kind: "revert", commit: hash, mainline: null });
        return;
      case "copy_hash":
        void navigator.clipboard.writeText(hash).catch(() => {
          // Nowhere to copy to: the hash stays in the Hash column and the changes pane.
        });
        return;
      case "create_tag":
        props.tag.open(props.workspace, { kind: "create", repo: shown, commit: hash, at: text });
        return;
      case "reset": {
        const head = viewOf(shown)?.summary?.head;
        const branch = head?.kind === "branch" ? head.name : null;
        props.reset.open(props.workspace, { kind: "reset", repo: shown, commit: hash, text, branch });
        return;
      }
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const index = selectedIndex();
    const page = Math.max(1, Math.floor((scroller?.clientHeight ?? ROW_HEIGHT) / ROW_HEIGHT) - 1);
    const go = (target: number) => select(Math.max(0, Math.min(rows().loaded - 1, target)));
    switch (event.key) {
      case "ArrowDown":
        go(index + 1);
        break;
      case "ArrowUp":
        go(index < 0 ? 0 : index - 1);
        break;
      case "PageDown":
        go(index + page);
        break;
      case "PageUp":
        go(index - page);
        break;
      case "Home":
        go(0);
        break;
      case "End":
        go(rows().loaded - 1);
        break;
      case "F10":
        if (!event.shiftKey) {
          return;
        }
        openMenuAtSelected();
        break;
      case "ContextMenu":
        openMenuAtSelected();
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  /** The keyboard's context menu: the selected row's, at the row. */
  const openMenuAtSelected = () => {
    const index = selectedIndex();
    const box = document.getElementById(`history-row-${index}`)?.getBoundingClientRect();
    if (index >= 0 && box !== undefined) {
      openMenu(index, box.left + 16, box.bottom);
    }
  };

  return (
    <>
    <div
      class="history-scroller"
      ref={(element) => (scroller = element)}
      role="listbox"
      aria-label="History"
      aria-multiselectable="true"
      tabindex="0"
      aria-activedescendant={selectedIndex() >= 0 ? `history-row-${selectedIndex()}` : undefined}
      onKeyDown={onKeyDown}
      onScroll={(event) => props.history.setScrollTop(props.workspace, event.currentTarget.scrollTop)}
    >
      <div class="history-rows" style={{ height: `${virtualizer.getTotalSize()}px` }}>
        <Index each={slots()}>
          {(slot) => (
            <Show when={slot()}>
              {(item) => (
                <Show when={rows().at(item().index)}>
                  {(row) => (
                    <div
                      id={`history-row-${item().index}`}
                      class="history-row"
                      classList={{
                        selected: row().id === history().selected,
                        marked: history().marked.length > 1 && history().marked.includes(row().id),
                        placeholder: row().kind === "placeholder",
                      }}
                      role="option"
                      aria-selected={row().id === history().selected || history().marked.includes(row().id)}
                      aria-busy={row().kind === "placeholder" ? true : undefined}
                      aria-posinset={item().index + 1}
                      aria-setsize={rows().count}
                      data-index={item().index}
                      style={{ transform: `translateY(${item().start}px)` }}
                      onClick={(event) => click(item().index, event)}
                      onContextMenu={(event) => {
                        if (row().kind === "commit") {
                          event.preventDefault();
                          openMenu(item().index, event.clientX, event.clientY);
                        }
                      }}
                    >
                      <RowView row={row()} name={merged() ? props.name : null} />
                    </div>
                  )}
                </Show>
              )}
            </Show>
          )}
        </Index>
      </div>
    </div>
    <Show when={menu()}>
      {(open) => (
        <CommitMenu
          x={open().x}
          y={open().y}
          entries={entries(open().commits)}
          onAction={(action) => act(action, open().commits, open().text)}
          onClose={() => setMenu(null)}
        />
      )}
    </Show>
    </>
  );
}

export default function HistoryPane(props: {
  workspace: WorkspaceId;
  history: HistoryStore;
  summaries: SummaryStore;
  /** A repo's name, for the repo column of a merged history. */
  name: (repo: RepoId) => string;
  layout: LayoutStore;
  branch: BranchStore;
  integrate: IntegrateStore;
  tag: TagStore;
  reset: ResetStore;
}) {
  const history = () => props.history.history(props.workspace);
  /** The repo of a single-repo history; a merged one has none. */
  const repo = (): RepoId | undefined => history().source?.repo ?? undefined;
  const columns = () => props.layout.historyColumns(props.workspace);
  const widths = () => {
    const c = columns();
    return {
      "--col-graph": `${c.graph}px`,
      "--col-author": `${c.author}px`,
      "--col-date": `${c.date}px`,
      "--col-hash": `${c.hash}px`,
    };
  };
  const empty = () => history().rows.length === 0 && history().done && history().error === null;

  return (
    <div class="history-pane" style={widths()}>
      <Show
        when={history().source}
        fallback={<p class="placeholder">Select a repo, branch, tag or stash to see its history.</p>}
      >
        <div class="history-header">
          <input
            class="history-search"
            type="search"
            placeholder="Search subject, author or hash"
            aria-label="Search history"
            value={history().text}
            onInput={(event) => void props.history.setText(props.workspace, event.currentTarget.value)}
          />
        </div>
        <Show when={repo() !== undefined && history().offers[repo() ?? ""] && repo()}>
          {(shown) => (
            <CommitGraphOffer
              onWrite={() => void props.history.writeCommitGraph(props.workspace, shown())}
              onDecline={() => void props.history.declineCommitGraph(props.workspace, shown())}
            />
          )}
        </Show>
        <Show when={repo() !== undefined && history().offerErrors[repo() ?? ""]}>
          {(error) => (
            <div class="history-offer error" role="alert">
              <p>{error()}</p>
              <button type="button" onClick={() => props.history.dismissOfferError(props.workspace, repo() ?? "")}>
                Dismiss
              </button>
            </div>
          )}
        </Show>
        <Show when={history().error}>{(error) => <p role="alert">Cannot read the history: {error()}</p>}</Show>
        <Show when={repo()}>
          {(shown) => (
            <ConflictBanner
              workspace={props.workspace}
              repo={shown()}
              summaries={props.summaries}
              integrate={props.integrate}
            />
          )}
        </Show>
        <IntegrateRefusal workspace={props.workspace} integrate={props.integrate} />
        <div class="history-columns" role="presentation">
          <For each={COLUMNS}>
            {(column) => (
              <span class={`history-cell ${column.class}`}>
                <span class="history-column-label">
                  {column.column === "graph" && repo() === undefined ? "Repository" : column.label}
                </span>
                <Show when={column.column}>
                  {(resizable) => (
                    <Splitter
                      pane={resizable()}
                      class="column-splitter"
                      label={`Resize ${column.label}`}
                      width={columns()[resizable()]}
                      min={MIN_COLUMN_WIDTH}
                      max={() => undefined}
                      onResize={(width) => props.layout.setColumnWidth(props.workspace, resizable(), width)}
                      onReset={() => props.layout.resetColumn(props.workspace, resizable())}
                    />
                  )}
                </Show>
              </span>
            )}
          </For>
        </div>
        <Show when={empty()}>
          <p class="placeholder">{history().text === "" ? "No commits yet." : "No commits match."}</p>
        </Show>
        <HistoryList
          workspace={props.workspace}
          history={props.history}
          summaries={props.summaries}
          name={props.name}
          branch={props.branch}
          integrate={props.integrate}
          tag={props.tag}
          reset={props.reset}
        />
      </Show>
    </div>
  );
}
