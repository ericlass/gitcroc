// The three panes of section 8: the tree, then the history with the changes
// pane docked right of it or below it (UI-19). For a group or several repos
// the history pane shows the target set's summary strip (UI-11) above their
// merged history (PERF-5). Sizes and the dock position come from the layout
// store under the given workspace id (ARCH-9).
import ChangesPane from "../changes/ChangesPane";
import { Show } from "solid-js";
import HistoryPane from "../history/HistoryPane";
import TargetView from "../history/TargetView";
import type { RepoId, WorkspaceId } from "../ipc";
import type { AiStore } from "../stores/ai";
import type { BranchStore } from "../stores/branch";
import type { StashStore } from "../stores/stash";
import type { CloneStore } from "../stores/clone";
import type { ShareStore } from "../stores/share";
import type { RemoteStore } from "../stores/remote";
import type { TagStore } from "../stores/tag";
import type { ResetStore } from "../stores/reset";
import type { IntegrateStore } from "../stores/integrate";
import type { ChangesStore } from "../stores/changes";
import type { ConflictStore } from "../stores/conflicts";
import type { CommitStore } from "../stores/commit";
import type { GroupOpStore } from "../stores/groupOps";
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
  conflicts: ConflictStore;
  commit: CommitStore;
  branch: BranchStore;
  stash: StashStore;
  tag: TagStore;
  remote: RemoteStore;
  clone: CloneStore;
  share: ShareStore;
  groupOps: GroupOpStore;
  reset: ResetStore;
  integrate: IntegrateStore;
  settings: SettingsStore;
  ai: AiStore;
  /** Opens Settings at its AI section (AI-3). */
  onAiSettings: () => void;
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

  /** The target set when it is a group or several repos. */
  const several = () => {
    const target = props.tree.target(props.workspace);
    return target?.kind === "group" || target?.kind === "repos" ? target : null;
  };
  const name = (repo: RepoId) =>
    props.workspaces.state.views?.find((view) => view.id === props.workspace)?.repos.find((known) => known.id === repo)
      ?.name ?? repo;

  const historyPane = (members?: () => RepoId[] | null) => (
    <HistoryPane
      workspace={props.workspace}
      history={props.history}
      summaries={props.summaries}
      name={name}
      members={members}
      layout={props.layout}
      branch={props.branch}
      integrate={props.integrate}
      tag={props.tag}
      reset={props.reset}
    />
  );

  const history = () => (
    <Show when={several()} fallback={historyPane()}>
      {(target) => (
        <TargetView
          workspace={props.workspace}
          target={target()}
          tree={props.tree}
          summaries={props.summaries}
          name={name}
        >
          {(members) => historyPane(members)}
        </TargetView>
      )}
    </Show>
  );

  const changes = () => (
    <ChangesPane
      workspace={props.workspace}
      changes={props.changes}
      commit={props.commit}
      conflicts={props.conflicts}
      settings={props.settings}
      layout={props.layout}
      onSelectCommit={(hash) => props.history.selectCommit(props.workspace, hash)}
      ai={{
        store: props.ai,
        workspaceOn: () => props.workspaces.state.views?.find((view) => view.id === props.workspace)?.ai ?? true,
        openSettings: props.onAiSettings,
      }}
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
          branch={props.branch}
          stash={props.stash}
          tag={props.tag}
          remote={props.remote}
          clone={props.clone}
          share={props.share}
          groupOps={props.groupOps}
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
