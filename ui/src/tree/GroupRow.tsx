// What a group row shows (UI-4): name, member count, the branch the core
// derives (GRP-4) and the dirty and conflict totals, each only when not
// zero. Names go in as text only (SEC-3).
import { Show } from "solid-js";
import type { GroupSummary } from "../ipc";

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

export default function GroupRow(props: { name: string; summary: GroupSummary }) {
  const branch = () => props.summary.branch;
  const head = () => {
    const current = branch();
    switch (current.kind) {
      case "branch":
        return current.name;
      case "detached":
        return "detached";
      case "mixed":
        return `mixed (${current.count})`;
      default:
        return null;
    }
  };
  return (
    <span class="tree-group">
      <span class="tree-label">{props.name}</span>
      <span class="tree-count" aria-label={plural(props.summary.members, "member", "members")}>
        {props.summary.members}
      </span>
      <Show when={head()}>
        {(head) => (
          <span
            class="tree-head"
            classList={{ mixed: branch().kind === "mixed" }}
            title={branch().kind === "mixed" ? "The members are on different branches" : undefined}
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
            aria-label={plural(dirty(), "member with changes", "members with changes")}
            title={plural(dirty(), "member with uncommitted changes", "members with uncommitted changes")}
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
            aria-label={plural(conflicts(), "member in conflict state", "members in conflict state")}
            title={plural(conflicts(), "member in conflict state", "members in conflict state")}
          >
            ⚔{conflicts()}
          </span>
        )}
      </Show>
    </span>
  );
}
