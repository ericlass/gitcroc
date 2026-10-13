// The three panes of section 8: the tree, then the history with the changes
// pane docked right of it or below it (UI-19). Sizes and the dock position
// come from the layout store under the given workspace id (ARCH-9).
import ChangesPane from "../changes/ChangesPane";
import HistoryPane from "../history/HistoryPane";
import type { WorkspaceId } from "../ipc";
import type { ChangesStore } from "../stores/changes";
import type { HistoryStore } from "../stores/history";
import { type LayoutStore, MIN_PANE_HEIGHT, MIN_PANE_WIDTH, type ResizablePane } from "../stores/layout";
import type { SettingsStore } from "../stores/settings";
import type { SummaryStore } from "../stores/summaries";
import type { TreeStore } from "../stores/tree";
import type { WorkspaceStore } from "../stores/workspaces";
import TreePane from "../tree/TreePane";
import Splitter from "./Splitter";

export default function Panes(props: {
  workspace: WorkspaceId;
  layout: LayoutStore;
  tree: TreeStore;
  summaries: SummaryStore;
  workspaces: WorkspaceStore;
  history: HistoryStore;
  changes: ChangesStore;
  settings: SettingsStore;
}) {
  let fill: HTMLElement | undefined;
  let stack: HTMLElement | undefined;
  const sizes = () => props.layout.paneSizes(props.workspace);
  const bottom = () => props.layout.dock(props.workspace) === "bottom";

  // A pane can grow only by what the pane that fills the rest has above its minimum.
  const maxWidth = (pane: ResizablePane) => () => {
    const spare = fill?.clientWidth;
    return spare ? sizes()[pane] + spare - MIN_PANE_WIDTH : undefined;
  };
  const maxHeight = () => {
    const height = stack?.clientHeight;
    return height ? height - MIN_PANE_HEIGHT : undefined;
  };

  const splitter = (pane: ResizablePane, label: string) => (
    <Splitter
      pane={pane}
      label={label}
      width={sizes()[pane]}
      min={MIN_PANE_WIDTH}
      max={maxWidth(pane)}
      onResize={(width, max) => props.layout.setPaneWidth(props.workspace, pane, width, max)}
      onReset={() => props.layout.resetPane(props.workspace, pane)}
      onCommit={() => props.layout.save(props.workspace)}
    />
  );

  const history = () => (
    <HistoryPane workspace={props.workspace} history={props.history} summaries={props.summaries} layout={props.layout} />
  );

  const changes = () => (
    <ChangesPane
      workspace={props.workspace}
      changes={props.changes}
      settings={props.settings}
      layout={props.layout}
      onSelectCommit={(hash) => props.history.select(props.workspace, hash)}
    />
  );

  return (
    <div class="panes">
      <section class="pane pane-tree" aria-label="Tree" style={{ width: `${sizes().tree}px` }}>
        <TreePane
          workspace={props.workspace}
          tree={props.tree}
          summaries={props.summaries}
          workspaces={props.workspaces}
        />
      </section>
      {splitter("tree", "Resize Tree")}
      {bottom() ? (
        <div class="pane-stack pane-fill" ref={(element) => (stack = fill = element)}>
          <section class="pane pane-history pane-fill" aria-label="History">
            {history()}
          </section>
          <Splitter
            pane="changes"
            orientation="horizontal"
            label="Resize Changes"
            width={sizes().changes}
            min={MIN_PANE_HEIGHT}
            max={maxHeight}
            onResize={(height, max) => props.layout.setPaneWidth(props.workspace, "changes", height, max)}
            onReset={() => props.layout.resetPane(props.workspace, "changes")}
            onCommit={() => props.layout.save(props.workspace)}
          />
          <section class="pane pane-changes" aria-label="Changes" style={{ height: `${sizes().changes}px` }}>
            {changes()}
          </section>
        </div>
      ) : (
        <>
          <section class="pane pane-history" aria-label="History" style={{ width: `${sizes().history}px` }}>
            {history()}
          </section>
          {splitter("history", "Resize History")}
          <section class="pane pane-fill pane-changes" aria-label="Changes" ref={(element) => (fill = element)}>
            {changes()}
          </section>
        </>
      )}
    </div>
  );
}
