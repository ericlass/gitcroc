// The conflict banner of OPS-2 above the history list (UI-10): names the
// operation the repo is in, what it applies onto what and, for a rebase,
// the step, with Abort and Continue. Whether the repo is in conflict state
// and how many paths are unmerged come from the repo's summary, so a
// conflict started in a terminal shows the same (OPS-1). Names are text
// only (SEC-3).
import { createEffect, on, Show } from "solid-js";
import type { ConflictDetails, Operation, RepoId, WorkspaceId } from "../ipc";
import type { IntegrateStore } from "../stores/integrate";
import type { SummaryStore } from "../stores/summaries";

const NAMES: Record<Operation, string> = {
  merge: "Merge",
  rebase: "Rebase",
  apply_mailbox: "Patch application",
  cherry_pick: "Cherry-pick",
  revert: "Revert",
};

/** What the banner says of `operation`, with what the core read of it. */
export function conflictText(operation: Operation, details: ConflictDetails | undefined): string {
  const known = details?.operation === operation ? details : undefined;
  const applying = known?.applying ?? null;
  const onto = known?.onto ?? null;
  const step = known?.step ? `, step ${known.step.current} of ${known.step.total}` : "";
  const what = (verb: string, joint: string) =>
    applying === null ? "" : `: ${verb} ${applying}${onto === null ? "" : ` ${joint} ${onto}`}`;
  switch (operation) {
    case "merge":
      return `Merge in progress${what("merging", "into")}`;
    case "rebase":
      return `Rebase in progress${what("rebasing", "onto")}${step}`;
    case "cherry_pick":
      return `Cherry-pick in progress${what("applying", "onto")}`;
    case "revert":
      return `Revert in progress${what("reverting", "on")}`;
    case "apply_mailbox":
      return `Patch application in progress${known?.step ? `: patch ${known.step.current} of ${known.step.total}` : ""}`;
  }
}

/** How many paths still need resolving, or that none do. */
export function unmergedText(paths: number): string {
  if (paths === 0) {
    return "All conflicts are resolved and staged.";
  }
  return paths === 1
    ? "1 path is unmerged: resolve and stage it, then Continue."
    : `${paths} paths are unmerged: resolve and stage them, then Continue.`;
}

export default function ConflictBanner(props: {
  workspace: WorkspaceId;
  repo: RepoId;
  summaries: SummaryStore;
  integrate: IntegrateStore;
}) {
  const view = () => props.summaries.view(props.workspace, props.repo);
  const state = () => view()?.summary?.state;
  const operation = () => state()?.operation ?? null;
  const unmerged = () => state()?.unmerged_paths ?? 0;
  const sending = () => props.integrate.sending(props.workspace);

  // Read again whenever the repo is read again: a step went by, a path was
  // staged, or the operation changed.
  createEffect(
    on(view, () => {
      if (operation() !== null) {
        void props.integrate.loadDetails(props.workspace, props.repo);
      }
    }),
  );

  return (
    <Show when={operation()}>
      {(current) => (
        <section class="conflict-banner" aria-label="Conflict state">
          <div class="conflict-text">
            <strong>{conflictText(current(), props.integrate.details(props.workspace, props.repo))}</strong>
            <span>{unmergedText(unmerged())}</span>
            <Show when={props.integrate.detailError(props.workspace, props.repo)}>
              {(error) => <span role="alert">Cannot read the operation: {error()}</span>}
            </Show>
          </div>
          <div class="conflict-actions">
            <button
              type="button"
              disabled={sending()}
              title={`Abort the ${NAMES[current()].toLowerCase()} and go back to where it started`}
              onClick={() => void props.integrate.run(props.workspace, props.repo, { kind: "abort" })}
            >
              Abort
            </button>
            <button
              type="button"
              class="primary"
              disabled={sending() || unmerged() > 0}
              title={unmerged() > 0 ? "Resolve and stage every unmerged path first" : undefined}
              onClick={() => void props.integrate.run(props.workspace, props.repo, { kind: "continue" })}
            >
              Continue
            </button>
          </div>
        </section>
      )}
    </Show>
  );
}
