// The options of Pull and Push (section 5 "Pull" and "Push", UI-20), the
// confirmation the core asks for before a force push (GRP-9), and why the
// core refused a toolbar click. Remotes and the upstream come from the
// core. Repo, branch and remote names are shown as text only (SEC-3).
import { createResource, createSignal, For, Show } from "solid-js";
import { Actions, ConfirmDialog, Frame, Refusal } from "../dialogs/parts";
import { type PullStrategy, pushChoices, type RepoId, type WorkspaceId } from "../ipc";
import type { SyncDialog, SyncStore } from "../stores/sync";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

interface DialogProps<K extends SyncDialog["kind"]> {
  workspace: WorkspaceId;
  sync: SyncStore;
  dialog: Extract<SyncDialog, { kind: K }>;
  repoName: string;
}

export const STRATEGIES: { strategy: PullStrategy; label: string }[] = [
  {
    strategy: "config",
    label: "As git config says (fast-forward only when unset)",
  },
  { strategy: "ff_only", label: "Fast-forward only" },
  { strategy: "merge", label: "Merge" },
  { strategy: "rebase", label: "Rebase" },
];

function PullDialog(props: DialogProps<"pull">) {
  const [strategy, setStrategy] = createSignal<PullStrategy>("config");
  const close = () => props.sync.close(props.workspace);
  return (
    <Frame id="sync-pull" title={`Pull ${props.repoName}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.sync.run(props.workspace, props.dialog.repo, {
            kind: "pull",
            strategy: strategy(),
          });
        }}
      >
        <fieldset>
          <legend>Strategy</legend>
          <For each={STRATEGIES}>
            {(choice) => (
              <label class="choice">
                <input
                  type="radio"
                  name="strategy"
                  checked={strategy() === choice.strategy}
                  onChange={() => setStrategy(choice.strategy)}
                />{" "}
                {choice.label}
              </label>
            )}
          </For>
        </fieldset>
        <Refusal error={props.sync.error(props.workspace)} />
        <Actions submit="Pull" disabled={props.sync.sending(props.workspace)} onCancel={close} />
      </form>
    </Frame>
  );
}

function PushDialog(props: DialogProps<"push">) {
  const [choices] = createResource(() => pushChoices(props.workspace, props.dialog.repo));
  /** The remote picked; `null` until picked, then the core's default applies. */
  const [picked, setPicked] = createSignal<string | null>(null);
  const [setUpstream, setSetUpstream] = createSignal(true);
  const [force, setForce] = createSignal(false);
  const close = () => props.sync.close(props.workspace);
  const remotes = () => props.dialog.remotes ?? choices()?.remotes ?? [];
  /** The core must be told which remote: it asked, or has no default. */
  const mustPick = () => props.dialog.remotes !== null || (choices() !== undefined && choices()?.remote === null);
  const remote = () => picked() ?? (mustPick() ? null : (choices()?.remote ?? null));
  const upstream = () => choices()?.upstream ?? null;
  const ready = () =>
    choices() !== undefined && (upstream() !== null || remote() !== null) && !props.sync.sending(props.workspace);
  return (
    <Frame id="sync-push" title={`Push ${props.repoName}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.sync.run(props.workspace, props.dialog.repo, {
              kind: "push",
              set_upstream: setUpstream(),
              remote: upstream() === null ? remote() : null,
              force_with_lease: force(),
            });
          }
        }}
      >
        <Show when={choices()}>
          {(known) => (
            <Show
              when={known().upstream}
              fallback={
                <>
                  <p>{known().branch ?? "HEAD"} has no upstream.</p>
                  <label class="field">
                    <span>Remote</span>
                    <select value={remote() ?? ""} onChange={(event) => setPicked(event.currentTarget.value || null)}>
                      <Show when={remote() === null}>
                        <option value="">Choose a remote</option>
                      </Show>
                      <For each={remotes()}>{(name) => <option value={name}>{name}</option>}</For>
                    </select>
                  </label>
                  <label class="choice">
                    <input
                      type="checkbox"
                      checked={setUpstream()}
                      onChange={(event) => setSetUpstream(event.currentTarget.checked)}
                    />{" "}
                    Set upstream
                  </label>
                </>
              }
            >
              {(target) => (
                <p>
                  Pushes {known().branch} to {target()}.
                </p>
              )}
            </Show>
          )}
        </Show>
        <label class="choice">
          <input type="checkbox" checked={force()} onChange={(event) => setForce(event.currentTarget.checked)} /> Force
          with lease (replaces the remote branch)
        </label>
        <Show when={choices.error as unknown}>
          <p role="alert">Cannot read the remotes: {message(choices.error)}</p>
        </Show>
        <Refusal error={props.sync.error(props.workspace)} />
        <Actions submit="Push" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

/** GRP-9: what a force push cannot undo, before it runs. */
function SyncConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit="Force push"
      error={props.sync.error(props.workspace)}
      sending={props.sync.sending(props.workspace)}
      onConfirm={() => void props.sync.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.sync.close(props.workspace)}
    />
  );
}

/** Why the core refused the last toolbar click, until dismissed. */
export function SyncRefusal(props: { workspace: WorkspaceId; sync: SyncStore }) {
  return (
    <Show when={props.sync.dialog(props.workspace) === null && props.sync.error(props.workspace)}>
      {(error) => (
        <div role="alert" class="banner banner-dismissible">
          <span>{error()}</span>
          <button type="button" onClick={() => props.sync.dismissError(props.workspace)}>
            Dismiss
          </button>
        </div>
      )}
    </Show>
  );
}

/** The open sync dialog of `workspace`, if any. */
export default function SyncDialogs(props: {
  workspace: WorkspaceId;
  sync: SyncStore;
  repoName: (repo: RepoId) => string;
}) {
  const dialog = () => props.sync.dialog(props.workspace);
  const as = <K extends SyncDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<SyncDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({
    workspace: props.workspace,
    sync: props.sync,
    repoName: props.repoName(repo),
  });
  return (
    <>
      <Show when={as("pull")} keyed>
        {(open) => <PullDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("push")} keyed>
        {(open) => <PushDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <SyncConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
