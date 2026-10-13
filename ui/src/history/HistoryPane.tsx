// The history pane (UI-9, UI-10, UI-12): search box, column header and the
// commit list, virtualized and paged (PERF-1, PERF-2). Rows, lanes and
// search results come from the core through the history store; this
// component lays them out and forwards input.
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, For, Index, Match, on, onMount, Show, Switch } from "solid-js";
import type { CommitRow, RefLabel, RepoId, WorkspaceId } from "../ipc";
import Splitter from "../layout/Splitter";
import type { HistoryStore } from "../stores/history";
import { type HistoryColumn, type LayoutStore, MIN_COLUMN_WIDTH } from "../stores/layout";
import type { SummaryStore } from "../stores/summaries";
import Graph from "./Graph";
import { formatDate, formatFullDate, type HistoryRow, historyRows } from "./rows";

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

function CommitCells(props: { commit: CommitRow }) {
  const dateTitle = () =>
    `Authored ${formatFullDate(props.commit.author_time.seconds)}\n` +
    `Committed ${formatFullDate(props.commit.commit_time.seconds)}`;
  return (
    <>
      <span class="history-cell history-graph">
        <Graph graph={props.commit.graph} height={ROW_HEIGHT} />
      </span>
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
function RowView(props: { row: HistoryRow }) {
  return (
    <Switch>
      <Match when={props.row.kind === "uncommitted" && props.row}>
        {(uncommitted) => (
          <span class="history-cell history-uncommitted">Uncommitted changes ({uncommitted().count})</span>
        )}
      </Match>
      <Match when={props.row.kind === "commit" && props.row}>{(commit) => <CommitCells commit={commit().commit} />}</Match>
      <Match when={props.row.kind === "placeholder"}>
        <span class="history-cell history-skeleton" aria-label="Loading" />
      </Match>
    </Switch>
  );
}

/** The commit list, virtualized (PERF-1) and paged as it scrolls (PERF-2). */
function HistoryList(props: { workspace: WorkspaceId; history: HistoryStore; summaries: SummaryStore }) {
  let scroller: HTMLDivElement | undefined;
  const history = () => props.history.history(props.workspace);
  const changes = () => {
    const repo = history().source?.repo;
    const view = repo === undefined ? undefined : props.summaries.state.byWorkspace[props.workspace]?.[repo];
    return view?.summary?.changes ?? 0;
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
    props.history.select(props.workspace, row.id);
    virtualizer.scrollToIndex(index, { align: "auto" });
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
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <div
      class="history-scroller"
      ref={(element) => (scroller = element)}
      role="listbox"
      aria-label="History"
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
                        placeholder: row().kind === "placeholder",
                      }}
                      role="option"
                      aria-selected={row().id === history().selected}
                      aria-busy={row().kind === "placeholder" ? true : undefined}
                      aria-posinset={item().index + 1}
                      aria-setsize={rows().count}
                      data-index={item().index}
                      style={{ transform: `translateY(${item().start}px)` }}
                      onClick={() => select(item().index)}
                    >
                      <RowView row={row()} />
                    </div>
                  )}
                </Show>
              )}
            </Show>
          )}
        </Index>
      </div>
    </div>
  );
}

export default function HistoryPane(props: {
  workspace: WorkspaceId;
  history: HistoryStore;
  summaries: SummaryStore;
  layout: LayoutStore;
}) {
  const history = () => props.history.history(props.workspace);
  const repo = (): RepoId | undefined => history().source?.repo;
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
        <div class="history-columns" role="presentation">
          <For each={COLUMNS}>
            {(column) => (
              <span class={`history-cell ${column.class}`}>
                <span class="history-column-label">{column.label}</span>
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
        <HistoryList workspace={props.workspace} history={props.history} summaries={props.summaries} />
      </Show>
    </div>
  );
}
