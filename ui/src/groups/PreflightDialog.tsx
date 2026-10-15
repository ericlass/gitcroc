// The preflight dialog of section 5, step 2: a progress line while the core
// classes the repos, then one row per repo with its class and reason,
// *needs attention* and *blocked* first, and the counts. A destructive
// operation names what cannot be undone and every repo it runs in (GRP-9).
// Repos cannot be left out here: the target set is built only in the tree
// (GRP-5). Escape cancels; Tab and Enter reach and press the buttons.
import { createEffect, createMemo, For, on, Show } from "solid-js";
import type { Class, RepoClass } from "../ipc";
import type { GroupOpStore, PendingPreflight } from "../stores/groupOps";

const CLASS_TEXT: Record<Class["kind"], string> = {
  will_run: "Will run",
  needs_attention: "Needs attention",
  will_skip: "Will skip",
  blocked: "Blocked",
};

/** Rows first by how much they need a look; within a class, target-set order. */
const CLASS_ORDER: Class["kind"][] = ["needs_attention", "blocked", "will_skip", "will_run"];

const repos = (n: number) => (n === 1 ? "1 repo" : `${n} repos`);

const reasonOf = (type: Class) => (type.kind === "will_run" ? null : type.reason);

/** Whether the repo runs: *will run* and *needs attention* do. */
export const runs = (repo: RepoClass) => repo.class.kind === "will_run" || repo.class.kind === "needs_attention";

export default function PreflightDialog(props: { pending: PendingPreflight; groupOps: GroupOpStore }) {
  let dialog: HTMLDivElement | undefined;
  const ready = () => props.pending.ready;
  const rows = createMemo(() => {
    const classed = ready()?.repos ?? [];
    return CLASS_ORDER.flatMap((kind) => classed.filter((repo) => repo.class.kind === kind));
  });
  const running = () => rows().filter(runs);
  const counts = () =>
    ["will_run", "needs_attention", "will_skip", "blocked"]
      .map((kind) => ({ kind, n: rows().filter((repo) => repo.class.kind === kind).length }))
      .filter((count) => count.n > 0)
      .map((count) => `${count.n} ${CLASS_TEXT[count.kind as Class["kind"]].toLowerCase()}`)
      .join(", ");
  const destructive = () => (ready()?.destructive.length ?? 0) > 0;
  const submit = () => {
    const shown = ready();
    const n = running().length;
    return shown !== null && destructive() ? `${shown.action} in ${repos(n)}` : `Run on ${repos(n)}`;
  };

  // Cancel has the focus while the preflight runs and where something cannot
  // be undone, Run elsewhere.
  createEffect(
    on(ready, () => {
      const run = destructive() ? null : dialog?.querySelector<HTMLElement>("button.run:enabled");
      (run ?? dialog?.querySelector<HTMLElement>("button.cancel"))?.focus();
    }),
  );

  return (
    <div class="backdrop">
      <div
        role={destructive() ? "alertdialog" : "dialog"}
        aria-modal="true"
        aria-labelledby="preflight-title"
        class="dialog op-dialog preflight"
        ref={(element) => {
          dialog = element;
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            props.groupOps.cancel();
          }
        }}
      >
        <h2 id="preflight-title">{ready()?.label ?? "Group operation"}</h2>
        <Show
          when={ready()}
          fallback={
            <Show when={props.pending.error === null}>
              <p role="status" class="preflight-progress">
                <span class="spinner" aria-hidden="true" /> Checking the repos…
              </p>
            </Show>
          }
        >
          {(shown) => (
            <>
              <p class="preflight-counts">{counts()}</p>
              <Show when={destructive()}>
                <div class="preflight-destructive">
                  <p>Cannot be undone in {repos(running().length)}:</p>
                  <ul class="op-actions">
                    <For each={shown().destructive}>{(action) => <li>{action}</li>}</For>
                  </ul>
                  <p>In these repos:</p>
                  <ul class="preflight-affected">
                    <For each={running()}>{(repo) => <li>{repo.repo_name}</li>}</For>
                  </ul>
                </div>
              </Show>
              <ul class="preflight-repos" aria-label="Repos">
                <For each={rows()}>
                  {(repo) => (
                    <li class={`preflight-repo preflight-${repo.class.kind.replaceAll("_", "-")}`}>
                      <span class="preflight-name">{repo.repo_name}</span>
                      <span class="preflight-class">{CLASS_TEXT[repo.class.kind]}</span>
                      <Show when={reasonOf(repo.class)}>
                        {(reason) => <span class="preflight-reason">{reason()}</span>}
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
              <p class="hint">To leave a repo out, change the selection in the tree and start again.</p>
            </>
          )}
        </Show>
        <Show when={props.pending.error}>{(error) => <p role="alert">{error()}</p>}</Show>
        <div class="dialog-actions">
          <button type="button" class="cancel" onClick={() => props.groupOps.cancel()}>
            Cancel
          </button>
          <Show when={ready()}>
            <button
              type="button"
              class={destructive() ? "run danger" : "run primary"}
              disabled={running().length === 0 || props.pending.starting}
              onClick={() => void props.groupOps.confirm()}
            >
              {submit()}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
