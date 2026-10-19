// The Stash dialog of the toolbar (section 6 "Stash", UI-20): an optional
// message and whether untracked files go too. Also the confirmation the
// core asks for before a drop, and the offer to drop a stash git kept
// because its pop stopped in conflicts, made once the repo is out of
// conflict state. Names and messages are shown as text only (SEC-3).
import { createSignal, For, Show } from "solid-js";
import { Actions, ConfirmDialog, Frame, Refusal } from "../dialogs/parts";
import type { RepoId, SummaryView, WorkspaceId } from "../ipc";
import type { StashDialog, StashStore } from "../stores/stash";
import { strings } from "../strings/en";

interface DialogProps<K extends StashDialog["kind"]> {
  workspace: WorkspaceId;
  stash: StashStore;
  dialog: Extract<StashDialog, { kind: K }>;
  repoName: string;
}

function PushDialog(props: DialogProps<"push">) {
  const [message, setMessage] = createSignal("");
  const [untracked, setUntracked] = createSignal(false);
  const close = () => props.stash.close(props.workspace);
  return (
    <Frame id="stash-push" title={strings.stash.pushTitle(props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (props.stash.sending(props.workspace)) {
            return;
          }
          void props.stash.run(props.workspace, props.dialog.repo, {
            kind: "push",
            message: message().trim() === "" ? null : message(),
            include_untracked: untracked(),
          });
        }}
      >
        <label class="field">
          <span>{strings.stash.message}</span>
          <input
            type="text"
            value={message()}
            spellcheck={false}
            autocomplete="off"
            onInput={(event) => setMessage(event.currentTarget.value)}
          />
        </label>
        <label class="choice">
          <input type="checkbox" checked={untracked()} onChange={(event) => setUntracked(event.currentTarget.checked)} />{" "}
          {strings.stash.includeUntracked}
        </label>
        <Refusal error={props.stash.error(props.workspace)} />
        <Actions submit={strings.stash.stash} disabled={props.stash.sending(props.workspace)} onCancel={close} />
      </form>
    </Frame>
  );
}

/** A drop cannot be undone from the app: what it loses, before it runs. */
function StashConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit={strings.stash.drop}
      error={props.stash.error(props.workspace)}
      sending={props.stash.sending(props.workspace)}
      onConfirm={() => void props.stash.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.stash.close(props.workspace)}
    />
  );
}

/**
 * After a pop stopped in conflicts and they were resolved, git still has
 * the stash: the offer to drop it, until answered. Not shown while the
 * repo is in conflict state or being read.
 */
export function StashOffer(props: {
  workspace: WorkspaceId;
  stash: StashStore;
  view: (repo: RepoId) => SummaryView | undefined;
  repoName: (repo: RepoId) => string;
}) {
  const shown = () =>
    props.stash.kept(props.workspace).filter((kept) => {
      const view = props.view(kept.repo);
      return view?.summary !== null && view?.summary !== undefined && !view.conflict && !view.refreshing;
    });
  return (
    <For each={shown()}>
      {(kept) => (
        <div role="status" class="banner banner-dismissible" aria-label={strings.stash.kept}>
          <span>{strings.stash.popKept(`stash@{${kept.stash.index}}`, kept.message, props.repoName(kept.repo))}</span>
          <button
            type="button"
            disabled={props.stash.sending(props.workspace)}
            onClick={() => void props.stash.run(props.workspace, kept.repo, { kind: "drop", stash: kept.stash })}
          >
            {strings.stash.dropStash}
          </button>
          <button type="button" onClick={() => props.stash.forget(props.workspace, kept.repo)}>
            {strings.stash.keepIt}
          </button>
        </div>
      )}
    </For>
  );
}

/** The open stash dialog of `workspace`, if any. */
export default function StashDialogs(props: {
  workspace: WorkspaceId;
  stash: StashStore;
  repoName: (repo: RepoId) => string;
}) {
  const dialog = () => props.stash.dialog(props.workspace);
  const as = <K extends StashDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<StashDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({
    workspace: props.workspace,
    stash: props.stash,
    repoName: props.repoName(repo),
  });
  return (
    <>
      <Show when={as("push")} keyed>
        {(open) => <PushDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <StashConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
