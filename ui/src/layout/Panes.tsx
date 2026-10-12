// The three panes of section 8. Sizes come from the layout store under the
// given workspace id (ARCH-9).
import HistoryPane from "../history/HistoryPane";
import type { WorkspaceId } from "../ipc";
import type { HistoryStore } from "../stores/history";
import { type LayoutStore, MIN_PANE_WIDTH, type ResizablePane } from "../stores/layout";
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
}) {
  let changes: HTMLElement | undefined;
  const sizes = () => props.layout.paneSizes(props.workspace);

  // A left pane can grow only by what the changes pane has above its minimum.
  const maxWidth = (pane: ResizablePane) => () => {
    const spare = changes?.clientWidth;
    return spare ? sizes()[pane] + spare - MIN_PANE_WIDTH : undefined;
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
      <section class="pane pane-history" aria-label="History" style={{ width: `${sizes().history}px` }}>
        <HistoryPane
          workspace={props.workspace}
          history={props.history}
          summaries={props.summaries}
          layout={props.layout}
        />
      </section>
      {splitter("history", "Resize History")}
      <section class="pane pane-fill" aria-label="Changes" ref={(element) => (changes = element)}>
        <p class="placeholder">Changes</p>
      </section>
    </div>
  );
}
