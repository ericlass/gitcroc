// The reset dialog (section 6 "Reset", UI-13): the current branch, or a
// detached `HEAD`, to the commit picked in the history, soft, mixed
// (default, as in git) or hard; and the confirmation the core asks for
// before a hard reset, naming the branch, the commit and how many files'
// changes are lost (GRP-9). Names and subjects are shown as text only
// (SEC-3).
import { createSignal, For, Show } from "solid-js";
import { Actions, ConfirmDialog, Frame, Refusal } from "../dialogs/parts";
import type { RepoId, ResetMode, WorkspaceId } from "../ipc";
import type { ResetDialog, ResetStore } from "../stores/reset";

interface DialogProps<K extends ResetDialog["kind"]> {
  workspace: WorkspaceId;
  reset: ResetStore;
  dialog: Extract<ResetDialog, { kind: K }>;
  repoName: string;
}

const MODES: { mode: ResetMode; label: string }[] = [
  { mode: "soft", label: "Soft: keep the index and the working tree" },
  { mode: "mixed", label: "Mixed: reset the index, keep the working tree" },
  { mode: "hard", label: "Hard: reset the index and the working tree; uncommitted changes are lost" },
];

function ModeDialog(props: DialogProps<"reset">) {
  const [mode, setMode] = createSignal<ResetMode>("mixed");
  const close = () => props.reset.close(props.workspace);
  return (
    <Frame
      id="reset-mode"
      title={`Reset ${props.dialog.branch ?? "HEAD"} in ${props.repoName} to ${props.dialog.text}`}
      onClose={close}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.reset.run(props.workspace, props.dialog.repo, props.dialog.commit, mode());
        }}
      >
        <Show when={props.dialog.branch === null}>
          <p>HEAD is detached: HEAD is reset, not a branch.</p>
        </Show>
        <fieldset>
          <legend>Mode</legend>
          <For each={MODES}>
            {(choice) => (
              <label class="choice">
                <input
                  type="radio"
                  name="reset-mode"
                  checked={mode() === choice.mode}
                  onChange={() => setMode(choice.mode)}
                />{" "}
                {choice.label}
              </label>
            )}
          </For>
        </fieldset>
        <Refusal error={props.reset.error(props.workspace)} />
        <Actions
          submit={mode() === "hard" ? "Reset hard…" : "Reset"}
          danger={mode() === "hard"}
          disabled={props.reset.sending(props.workspace)}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

function ResetConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit="Reset hard"
      error={props.reset.error(props.workspace)}
      sending={props.reset.sending(props.workspace)}
      onConfirm={() =>
        void props.reset.run(props.workspace, props.dialog.repo, props.dialog.commit, props.dialog.mode, true)
      }
      onClose={() => props.reset.close(props.workspace)}
    />
  );
}

/** The open reset dialog of `workspace`, if any. */
export default function ResetDialogs(props: {
  workspace: WorkspaceId;
  reset: ResetStore;
  repoName: (repo: RepoId) => string;
}) {
  const dialog = () => props.reset.dialog(props.workspace);
  const as = <K extends ResetDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<ResetDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({ workspace: props.workspace, reset: props.reset, repoName: props.repoName(repo) });
  return (
    <>
      <Show when={as("reset")} keyed>
        {(open) => <ModeDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <ResetConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
