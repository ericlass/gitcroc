// The conflict banner of OPS-2 above the history list (UI-10): names the
// operation the repo is in, what it applies onto what and, for a rebase,
// the step, with Abort and Continue. Whether the repo is in conflict state
// and how many paths are unmerged come from the repo's summary, so a
// conflict started in a terminal shows the same (OPS-1). Names are text
// only (SEC-3). With AI features on (AI-16) it has Resolve all with AI
// (AI-3), and under it the files of the repo's last run and where each is
// (AI-13); a file's name opens it in the conflict editor.
import { createEffect, on, Show } from "solid-js";
import AiFileList from "../conflict/AiFileList";
import { aiAsk, type ConflictAi } from "../conflict/ConflictEditor";
import type { ConflictDetails, Operation, RepoId, WorkspaceId } from "../ipc";
import type { IntegrateStore } from "../stores/integrate";
import type { ResolveAllStore } from "../stores/resolveAll";
import type { SummaryStore } from "../stores/summaries";
import { strings } from "../strings/en";

/** What the banner needs for Resolve all with AI; without it the banner has none. */
export interface BannerAi {
  store: ResolveAllStore;
  ai: ConflictAi;
}

const NAMES: Record<Operation, string> = strings.integrate.operations;

/** What the banner says of `operation`, with what the core read of it. */
export function conflictText(operation: Operation, details: ConflictDetails | undefined): string {
  const known = details?.operation === operation ? details : undefined;
  const applying = known?.applying ?? null;
  const onto = known?.onto ?? null;
  const step = known?.step ?? null;
  const text = strings.integrate.inProgress;
  switch (operation) {
    case "merge":
      return text.merge(applying, onto);
    case "rebase":
      return text.rebase(applying, onto, step);
    case "cherry_pick":
      return text.cherryPick(applying, onto);
    case "revert":
      return text.revert(applying, onto);
    case "apply_mailbox":
      return text.applyMailbox(step);
    case "stash_apply":
      return text.stashApply(onto);
  }
}

/**
 * How many paths still need resolving, or that none do. A stash apply has
 * nothing to continue: it ends with the last path marked resolved.
 */
export function unmergedText(paths: number, operation: Operation | null = null): string {
  if (paths === 0) {
    return strings.integrate.allResolved;
  }
  return operation === "stash_apply" ? strings.integrate.markResolved(paths) : strings.integrate.resolveAndStage(paths);
}

/** What Abort does, for its tooltip. */
function abortTitle(operation: Operation): string {
  return operation === "stash_apply"
    ? strings.integrate.abortStashApply
    : strings.integrate.abortTitle(NAMES[operation].toLowerCase());
}

export default function ConflictBanner(props: {
  workspace: WorkspaceId;
  repo: RepoId;
  summaries: SummaryStore;
  integrate: IntegrateStore;
  resolveAll?: BannerAi;
}) {
  const view = () => props.summaries.view(props.workspace, props.repo);
  const state = () => view()?.summary?.state;
  const operation = () => state()?.operation ?? null;
  const unmerged = () => state()?.unmerged_paths ?? 0;
  const sending = () => props.integrate.sending(props.workspace);
  const aiOn = () => props.resolveAll?.ai.store.enabled() === true;
  const run = () => props.resolveAll?.store.run(props.workspace, props.repo);
  const runJob = () => props.resolveAll?.store.view(props.workspace, props.repo);
  const running = () => runJob()?.state === "running";
  const workspaceOff = () => props.resolveAll?.ai.workspaceOn() === false;
  const resolveAll = () => {
    const banner = props.resolveAll;
    if (banner !== undefined) {
      void banner.store.start(props.workspace, props.repo, aiAsk(banner.ai));
    }
  };

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
        <section class="conflict-banner" aria-label={strings.integrate.conflictState}>
          <div class="conflict-text">
            <strong>{conflictText(current(), props.integrate.details(props.workspace, props.repo))}</strong>
            <span>{unmergedText(unmerged(), current())}</span>
            <Show when={props.integrate.detailError(props.workspace, props.repo)}>
              {(error) => <span role="alert">{strings.integrate.cannotRead(error())}</span>}
            </Show>
            <Show when={aiOn() && workspaceOff()}>
              <span class="conflict-ai-reason">{strings.conflict.aiOffHere}</span>
            </Show>
            <Show when={aiOn() && run()?.refusal}>{(refusal) => <span role="alert">{refusal()}</span>}</Show>
          </div>
          <div class="conflict-actions">
            <Show when={aiOn()}>
              <button
                type="button"
                disabled={workspaceOff() || run()?.starting === true || running()}
                title={workspaceOff() ? strings.integrate.aiOffTitle : strings.integrate.resolveAllTitle}
                onClick={resolveAll}
              >
                {strings.integrate.resolveAll}
              </button>
            </Show>
            <button
              type="button"
              disabled={sending()}
              title={abortTitle(current())}
              onClick={() => void props.integrate.run(props.workspace, props.repo, { kind: "abort" })}
            >
              {strings.integrate.abort}
            </button>
            <button
              type="button"
              class="primary"
              disabled={sending() || unmerged() > 0}
              title={
                current() === "stash_apply"
                  ? strings.integrate.stashApplyEnds
                  : unmerged() > 0
                    ? strings.integrate.resolveFirst
                    : undefined
              }
              onClick={() => void props.integrate.run(props.workspace, props.repo, { kind: "continue" })}
            >
              {strings.integrate.continue}
            </button>
          </div>
          <Show when={aiOn() && runJob()}>
            {(job) => (
              <section class="conflict-run" aria-label={strings.integrate.resolveAll}>
                <div class="conflict-run-header">
                  <strong>{job().label}</strong>
                  <Show when={running()}>
                    <button type="button" onClick={() => props.resolveAll?.store.cancel(props.workspace, props.repo)}>
                      {strings.common.cancel}
                    </button>
                  </Show>
                </div>
                <AiFileList
                  job={job()}
                  onOpen={(path) => void props.resolveAll?.store.open(props.workspace, props.repo, path)}
                />
              </section>
            )}
          </Show>
        </section>
      )}
    </Show>
  );
}
