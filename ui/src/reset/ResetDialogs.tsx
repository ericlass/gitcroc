// The reset dialog (section 6 "Reset", UI-13): the current branch, or a
// detached `HEAD`, to the commit picked in the history, soft, mixed
// (default, as in git) or hard; and the confirmation the core asks for
// before a hard reset, naming the branch, the commit and how many files'
// changes are lost (GRP-9). Names and subjects are shown as text only
// (SEC-3).
import { createSignal, For, Show, untrack } from "solid-js";
import { Actions, ConfirmDialog, Frame, Refusal } from "../dialogs/parts";
import type { RepoId, ResetMode, WorkspaceId } from "../ipc";
import type { ResetDialog, ResetStore } from "../stores/reset";
import { strings } from "../strings/en";

interface DialogProps<K extends ResetDialog["kind"]> {
  workspace: WorkspaceId;
  reset: ResetStore;
  dialog: Extract<ResetDialog, { kind: K }>;
  repoName: string;
}

const MODES: { mode: ResetMode; label: string }[] = [
  { mode: "soft", label: strings.reset.modes.soft },
  { mode: "mixed", label: strings.reset.modes.mixed },
  { mode: "hard", label: strings.reset.modes.hard },
];

function ModeDialog(props: DialogProps<"reset">) {
  // The dialog is made anew for each opening: the mode it opens with is read once.
  const [mode, setMode] = createSignal<ResetMode>(untrack(() => props.dialog.mode ?? "mixed"));
  const close = () => props.reset.close(props.workspace);
  return (
    <Frame
      id="reset-mode"
      title={strings.reset.title(props.dialog.branch ?? "HEAD", props.repoName, props.dialog.text)}
      onClose={close}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.reset.run(props.workspace, props.dialog.repo, props.dialog.commit, mode());
        }}
      >
        <Show when={props.dialog.branch === null}>
          <p>{strings.reset.detached}</p>
        </Show>
        <fieldset>
          <legend>{strings.reset.mode}</legend>
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
          submit={mode() === "hard" ? strings.reset.resetHardAsk : strings.reset.reset}
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
      submit={strings.reset.resetHard}
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
