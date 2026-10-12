// One tab for the open workspace until workspace tabs exist (UI-27, UI-28
// arrive in M3).
export default function TabBar(props: { name?: string }) {
  return (
    <div class="tab-bar">
      <div role="tablist" aria-label="Workspaces" class="tabs">
        <button type="button" role="tab" aria-selected="true" class="tab">
          {props.name ?? "No workspace"}
        </button>
      </div>
      <button type="button" class="tab-add" aria-label="Open workspace" title="Open workspace" disabled>
        +
      </button>
    </div>
  );
}
