// The Merge dialog of the toolbar (UI-20, section 5 "Merge X into current"
// and "Rebase current onto X"), the choice of mainline the core asks for
// before a merge commit's revert, the confirmation it asks for before an
// abort that cannot be undone, and why the core refused a request.
// The refs to pick from are filtered in the core as the user types. Names
// are shown as text only (SEC-3).
import { createEffect, createSignal, For, on, Show, untrack } from "solid-js";
import { Actions, ConfirmDialog, Frame, Refusal } from "../dialogs/parts";
import { mergeTargets, type MergeFastForward, type RepoId, type WorkspaceId } from "../ipc";
import type { IntegrateDialog, IntegrateStore } from "../stores/integrate";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

interface DialogProps<K extends IntegrateDialog["kind"]> {
  workspace: WorkspaceId;
  integrate: IntegrateStore;
  dialog: Extract<IntegrateDialog, { kind: K }>;
  repoName: string;
}

const FAST_FORWARDS: { value: MergeFastForward; label: string }[] = [
  { value: "config", label: strings.integrate.fastForwards.config },
  { value: "ff_only", label: strings.integrate.fastForwards.ffOnly },
  { value: "no_ff", label: strings.integrate.fastForwards.noFf },
];

function MergeDialog(props: DialogProps<"merge"> & { branch: string | null }) {
  const [text, setText] = createSignal("");
  const [targets, setTargets] = createSignal<string[]>([]);
  const [listError, setListError] = createSignal<string | null>(null);
  const [fastForward, setFastForward] = createSignal<MergeFastForward>("config");
  // The dialog is made anew for each opening: what it opens with is read once.
  const [rebase, setRebase] = createSignal(untrack(() => props.dialog.rebase === true));
  let request = 0;
  const close = () => props.integrate.close(props.workspace);
  const branch = () => props.branch ?? strings.integrate.theCurrentBranch;

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
    <Frame id="integrate-merge" title={strings.integrate.mergeTitle(branch(), props.repoName)} onClose={close}>
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
          <span>{strings.integrate.mergeRef}</span>
          <input
            type="text"
            value={text()}
            placeholder={strings.integrate.typeToFilter}
            spellcheck={false}
            autocomplete="off"
            onInput={(event) => setText(event.currentTarget.value)}
          />
        </label>
        <div class="merge-targets" role="listbox" aria-label={strings.integrate.refs}>
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
            <p class="placeholder">{strings.integrate.noRefMatches}</p>
          </Show>
        </div>
        <Show when={listError()}>{(error) => <p role="alert">{strings.integrate.cannotListRefs(error())}</p>}</Show>
        <fieldset disabled={rebase()}>
          <legend>{strings.integrate.fastForward}</legend>
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
          {strings.integrate.rebaseInstead(branch())}
        </label>
        <Refusal error={props.integrate.error(props.workspace)} />
        <Actions
          submit={rebase() ? strings.integrate.rebase : strings.integrate.merge}
          disabled={!ready()}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

/** A revert of a merge commit keeps one parent's side: which, the first by default. */
function MainlineDialog(props: DialogProps<"mainline">) {
  const [mainline, setMainline] = createSignal(1);
  const close = () => props.integrate.close(props.workspace);
  return (
    <Frame id="integrate-mainline" title={strings.integrate.revertMergeTitle(props.repoName)} onClose={close}>
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
          <legend>{strings.integrate.keepParent}</legend>
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
        <Actions
          submit={strings.integrate.revert}
          disabled={props.integrate.sending(props.workspace)}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

/** The abort of a stash apply cannot be undone: what it does, before it runs. */
function IntegrateConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit={strings.integrate.abort}
      error={props.integrate.error(props.workspace)}
      sending={props.integrate.sending(props.workspace)}
      onConfirm={() => void props.integrate.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.integrate.close(props.workspace)}
    />
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
            {strings.common.dismiss}
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
      <Show when={as("confirm")} keyed>
        {(open) => <IntegrateConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
