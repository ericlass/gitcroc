// What a group row shows (UI-4): name, member count, the branch the core
// derives (GRP-4) and the dirty and conflict totals, each only when not
// zero. Names go in as text only (SEC-3).
import { Show } from "solid-js";
import type { GroupSummary } from "../ipc";
import { strings } from "../strings/en";

export default function GroupRow(props: { name: string; summary: GroupSummary }) {
  const branch = () => props.summary.branch;
  const head = () => {
    const current = branch();
    switch (current.kind) {
      case "branch":
        return current.name;
      case "detached":
        return strings.tree.detached;
      case "mixed":
        return strings.tree.mixed(current.count);
      default:
        return null;
    }
  };
  return (
    <span class="tree-group">
      <span class="tree-label">{props.name}</span>
      <span class="tree-count" aria-label={strings.tree.members(props.summary.members)}>
        {props.summary.members}
      </span>
      <Show when={head()}>
        {(head) => (
          <span
            class="tree-head"
            classList={{ mixed: branch().kind === "mixed" }}
            title={branch().kind === "mixed" ? strings.tree.mixedTitle : undefined}
          >
            {head()}
          </span>
        )}
      </Show>
      <Show when={props.summary.dirty}>
        {(dirty) => (
          <span
            class="tree-dirty"
            role="img"
            aria-label={strings.tree.membersWithChanges(dirty())}
            title={strings.tree.membersWithUncommitted(dirty())}
          >
            ●{dirty()}
          </span>
        )}
      </Show>
      <Show when={props.summary.conflicts}>
        {(conflicts) => (
          <span
            class="tree-conflict"
            role="img"
            aria-label={strings.tree.membersInConflict(conflicts())}
            title={strings.tree.membersInConflict(conflicts())}
          >
            ⚔{conflicts()}
          </span>
        )}
      </Show>
    </span>
  );
}
