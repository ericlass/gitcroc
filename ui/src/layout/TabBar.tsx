// One static tab until workspaces exist (UI-27, UI-28 arrive in M3).
export default function TabBar() {
  return (
    <div class="tab-bar">
      <div role="tablist" aria-label="Workspaces" class="tabs">
        <button type="button" role="tab" aria-selected="true" class="tab">
          No workspace
        </button>
      </div>
      <button type="button" class="tab-add" aria-label="Open workspace" title="Open workspace" disabled>
        +
      </button>
    </div>
  );
}
