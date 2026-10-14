// The Merge dialog of the toolbar (UI-20, section 5 "Merge X into current"
// and "Rebase current onto X"), the choice of mainline the core asks for
// before a merge commit's revert, and why the core refused a request.
// The refs to pick from are filtered in the core as the user types. Names
// are shown as text only (SEC-3).
import { createEffect, createSignal, For, on, Show } from "solid-js";
import { Actions, Frame, Refusal } from "../dialogs/parts";
import { mergeTargets, type MergeFastForward, type RepoId, type WorkspaceId } from "../ipc";
import type { IntegrateDialog, IntegrateStore } from "../stores/integrate";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

interface DialogProps<K extends IntegrateDialog["kind"]> {
  workspace: WorkspaceId;
  integrate: IntegrateStore;
  dialog: Extract<IntegrateDialog, { kind: K }>;
  repoName: string;
}

const FAST_FORWARDS: { value: MergeFastForward; label: string }[] = [
  { value: "config", label: "As git config says (fast-forward when possible)" },
  { value: "ff_only", label: "Fast-forward only" },
  { value: "no_ff", label: "Always create a merge commit" },
];

function MergeDialog(props: DialogProps<"merge"> & { branch: string | null }) {
  const [text, setText] = createSignal("");
  const [targets, setTargets] = createSignal<string[]>([]);
  const [listError, setListError] = createSignal<string | null>(null);
  const [fastForward, setFastForward] = createSignal<MergeFastForward>("config");
  const [rebase, setRebase] = createSignal(false);
  let request = 0;
  const close = () => props.integrate.close(props.workspace);
  const branch = () => props.branch ?? "the current branch";

  // The refs matching what is typed, filtered by the core; the latest wins.
  createEffect(
    on(text, (filter) => {
      const mine = ++request;
      mergeTargets(props.workspace, props.dialog.repo, filter).then(
        (found) => {
          if (mine === request) {
            setTargets(found);
            setListError(null);
          }
        },
        (error: unknown) => {
          if (mine === request) {
            setListError(message(error));
          }
        },
      );
    }),
  );

  const ready = () => text().trim() !== "" && !props.integrate.sending(props.workspace);
  return (
    <Frame id="integrate-merge" title={`Merge into ${branch()} in ${props.repoName}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!ready()) {
            return;
          }
          const target = text().trim();
          void props.integrate.run(
            props.workspace,
            props.dialog.repo,
            rebase() ? { kind: "rebase", onto: target } : { kind: "merge", revision: target, fast_forward: fastForward() },
          );
        }}
      >
        <label class="field">
          <span>Branch, remote branch or tag</span>
          <input
            type="text"
            value={text()}
            placeholder="Type to filter"
            spellcheck={false}
            autocomplete="off"
            onInput={(event) => setText(event.currentTarget.value)}
          />
        </label>
        <div class="merge-targets" role="listbox" aria-label="Refs">
          <For each={targets()}>
            {(name) => (
              <button
                type="button"
                role="option"
                class="merge-target"
                aria-selected={name === text()}
                onClick={() => setText(name)}
              >
                {name}
              </button>
            )}
          </For>
          <Show when={targets().length === 0 && listError() === null}>
            <p class="placeholder">No ref matches.</p>
          </Show>
        </div>
        <Show when={listError()}>{(error) => <p role="alert">Cannot list the refs: {error()}</p>}</Show>
        <fieldset disabled={rebase()}>
          <legend>Fast-forward</legend>
          <For each={FAST_FORWARDS}>
            {(choice) => (
              <label class="choice">
                <input
                  type="radio"
                  name="fast-forward"
                  checked={fastForward() === choice.value}
                  onChange={() => setFastForward(choice.value)}
                />{" "}
                {choice.label}
              </label>
            )}
          </For>
        </fieldset>
        <label class="choice">
          <input type="checkbox" checked={rebase()} onChange={(event) => setRebase(event.currentTarget.checked)} />{" "}
          Rebase {branch()} onto it instead
        </label>
        <Refusal error={props.integrate.error(props.workspace)} />
        <Actions submit={rebase() ? "Rebase" : "Merge"} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

/** A revert of a merge commit keeps one parent's side: which, the first by default. */
function MainlineDialog(props: DialogProps<"mainline">) {
  const [mainline, setMainline] = createSignal(1);
  const close = () => props.integrate.close(props.workspace);
  return (
    <Frame id="integrate-mainline" title={`Revert a merge commit in ${props.repoName}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.integrate.run(props.workspace, props.dialog.repo, {
            kind: "revert",
            commit: props.dialog.commit,
            mainline: mainline(),
          });
        }}
      >
        <fieldset>
          <legend>Keep the side of parent</legend>
          <For each={props.dialog.parents}>
            {(parent, index) => (
              <label class="choice">
                <input
                  type="radio"
                  name="mainline"
                  checked={mainline() === index() + 1}
                  onChange={() => setMainline(index() + 1)}
                />{" "}
                {index() + 1}: {parent}
              </label>
            )}
          </For>
        </fieldset>
        <Refusal error={props.integrate.error(props.workspace)} />
        <Actions submit="Revert" disabled={props.integrate.sending(props.workspace)} onCancel={close} />
      </form>
    </Frame>
  );
}

/** Why the core refused a request made outside a dialog, until dismissed. */
export function IntegrateRefusal(props: { workspace: WorkspaceId; integrate: IntegrateStore }) {
  return (
    <Show when={props.integrate.dialog(props.workspace) === null && props.integrate.error(props.workspace)}>
      {(error) => (
        <div role="alert" class="history-offer error">
          <p>{error()}</p>
          <button type="button" onClick={() => props.integrate.dismissError(props.workspace)}>
            Dismiss
          </button>
        </div>
      )}
    </Show>
  );
}

/** The open integrate dialog of `workspace`, if any. */
export default function IntegrateDialogs(props: {
  workspace: WorkspaceId;
  integrate: IntegrateStore;
  repoName: (repo: RepoId) => string;
  /** The current branch of `repo`, or `null` when `HEAD` is detached or unknown. */
  branchOf: (repo: RepoId) => string | null;
}) {
  const dialog = () => props.integrate.dialog(props.workspace);
  const as = <K extends IntegrateDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<IntegrateDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({
    workspace: props.workspace,
    integrate: props.integrate,
    repoName: props.repoName(repo),
  });
  return (
    <>
      <Show when={as("merge")} keyed>
        {(open) => <MergeDialog {...common(open.repo)} dialog={open} branch={props.branchOf(open.repo)} />}
      </Show>
      <Show when={as("mainline")} keyed>
        {(open) => <MainlineDialog {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
