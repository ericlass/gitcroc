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
  stash_apply: "Stash apply",
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
    case "stash_apply":
      return `Stash apply stopped with conflicts${onto === null ? "" : ` on ${onto}`}`;
  }
}

/**
 * How many paths still need resolving, or that none do. A stash apply has
 * nothing to continue: it ends with the last path marked resolved.
 */
export function unmergedText(paths: number, operation: Operation | null = null): string {
  if (paths === 0) {
    return "All conflicts are resolved and staged.";
  }
  const unmergedCount = paths === 1 ? "1 path is unmerged" : `${paths} paths are unmerged`;
  const them = paths === 1 ? "it" : "them";
  return operation === "stash_apply"
    ? `${unmergedCount}: mark ${them} resolved, and the stash apply ends.`
    : `${unmergedCount}: resolve and stage ${them}, then Continue.`;
}

/** What Abort does, for its tooltip. */
function abortTitle(operation: Operation): string {
  return operation === "stash_apply"
    ? "Put the files the stash apply changed back to HEAD; the stash and other local changes are kept"
    : `Abort the ${NAMES[operation].toLowerCase()} and go back to where it started`;
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
            <span>{unmergedText(unmerged(), current())}</span>
            <Show when={props.integrate.detailError(props.workspace, props.repo)}>
              {(error) => <span role="alert">Cannot read the operation: {error()}</span>}
            </Show>
          </div>
          <div class="conflict-actions">
            <button
              type="button"
              disabled={sending()}
              title={abortTitle(current())}
              onClick={() => void props.integrate.run(props.workspace, props.repo, { kind: "abort" })}
            >
              Abort
            </button>
            <button
              type="button"
              class="primary"
              disabled={sending() || unmerged() > 0}
              title={
                current() === "stash_apply"
                  ? "A stash apply ends when its last path is marked resolved"
                  : unmerged() > 0
                    ? "Resolve and stage every unmerged path first"
                    : undefined
              }
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
