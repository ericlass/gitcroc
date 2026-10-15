// The dialogs of the workspace tabs (UI-28): a new workspace's name, a
// rename, deleting a workspace file that is not open, and closing a tab that
// holds unsaved conflict edits (OPS-9). The core checks names and refuses
// what it does not allow; its refusal is shown inline. Names are text (SEC-3).
import { createSignal, For, type JSX, onMount, Show } from "solid-js";
import { Actions, Frame, NameField, Refusal } from "../dialogs/parts";
import type { WorkspaceId } from "../ipc";
import type { WorkspaceStore } from "../stores/workspaces";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export type WorkspaceDialog =
  | { kind: "new" }
  | { kind: "rename"; id: WorkspaceId; name: string }
  | { kind: "delete"; id: WorkspaceId; name: string }
  /** `files` names each file with unsaved edits as "repo: path". */
  | { kind: "close"; id: WorkspaceId; name: string; files: string[] };

/** Which workspace dialog is open; there is at most one. */
export function createWorkspaceDialogs() {
  const [dialog, setDialog] = createSignal<WorkspaceDialog | null>(null);
  return { dialog, open: (next: WorkspaceDialog) => setDialog(next), close: () => setDialog(null) };
}

export type WorkspaceDialogs = ReturnType<typeof createWorkspaceDialogs>;

/** A name for a new workspace, or a new name for one. */
function NameDialog(props: {
  title: string;
  submit: string;
  initial: string;
  send: (name: string) => Promise<unknown>;
  onClose: () => void;
}) {
  const [name, setName] = createSignal(props.initial);
  const [error, setError] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);
  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    setSending(true);
    try {
      await props.send(name());
      props.onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setSending(false);
    }
  };
  return (
    <Frame id="workspace-name" title={props.title} onClose={props.onClose}>
      <form onSubmit={(event) => void submit(event)}>
        <NameField
          label="Name"
          value={name()}
          error={error()}
          onInput={(value) => {
            setName(value);
            setError(null);
          }}
        />
        <Actions submit={props.submit} disabled={sending()} onCancel={props.onClose} />
      </form>
    </Frame>
  );
}

/** A question that cannot be undone: Cancel has the focus, Escape cancels. */
function Confirm(props: {
  title: string;
  submit: string;
  error: string | null;
  sending: boolean;
  onConfirm: () => void;
  onClose: () => void;
  children: JSX.Element;
}) {
  let cancel: HTMLButtonElement | undefined;
  onMount(() => cancel?.focus());
  return (
    <div class="backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="workspace-confirm-title"
        aria-describedby="workspace-confirm-text"
        class="dialog op-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            props.onClose();
          }
        }}
      >
        <h2 id="workspace-confirm-title">{props.title}</h2>
        <div id="workspace-confirm-text">{props.children}</div>
        <Refusal error={props.error} />
        <div class="dialog-actions">
          <button type="button" ref={(element) => (cancel = element)} onClick={() => props.onClose()}>
            Cancel
          </button>
          <button type="button" class="danger" disabled={props.sending} onClick={() => props.onConfirm()}>
            {props.submit}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Runs `send`; closes the dialog when it succeeds and shows the refusal when it does not. */
function createSend(onClose: () => void) {
  const [error, setError] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);
  const run = async (send: () => Promise<unknown>) => {
    setSending(true);
    setError(null);
    try {
      await send();
      onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setSending(false);
    }
  };
  return { error, sending, run };
}

function DeleteDialog(props: { name: string; send: () => Promise<unknown>; onClose: () => void }) {
  const sender = createSend(() => props.onClose());
  return (
    <Confirm
      title={`Delete the workspace ${props.name}?`}
      submit="Delete"
      error={sender.error()}
      sending={sender.sending()}
      onConfirm={() => void sender.run(props.send)}
      onClose={props.onClose}
    >
      <p>Its workspace file is deleted. The repositories on disk are not touched.</p>
    </Confirm>
  );
}

function CloseDialog(props: { name: string; files: string[]; send: () => Promise<unknown>; onClose: () => void }) {
  const sender = createSend(() => props.onClose());
  return (
    <Confirm
      title={`Close ${props.name}?`}
      submit="Close tab"
      error={sender.error()}
      sending={sender.sending()}
      onConfirm={() => void sender.run(props.send)}
      onClose={props.onClose}
    >
      <p>The unsaved conflict edits of these files are lost:</p>
      <ul class="op-actions">
        <For each={props.files}>{(file) => <li>{file}</li>}</For>
      </ul>
    </Confirm>
  );
}

export default function WorkspaceDialogsView(props: {
  dialogs: WorkspaceDialogs;
  workspaces: WorkspaceStore;
  /** Closes a tab whose unsaved conflict edits the user agreed to lose. */
  onCloseTab: (id: WorkspaceId) => Promise<unknown>;
}) {
  const close = () => props.dialogs.close();
  /** The open dialog when it is of `kind`; each opens as a new instance. */
  const of = <K extends WorkspaceDialog["kind"]>(kind: K) => {
    const dialog = props.dialogs.dialog();
    return dialog?.kind === kind ? (dialog as Extract<WorkspaceDialog, { kind: K }>) : null;
  };
  return (
    <>
      <Show when={of("new")} keyed>
        <NameDialog
          title="New workspace"
          submit="Create"
          initial=""
          send={(name) => props.workspaces.create(name)}
          onClose={close}
        />
      </Show>
      <Show when={of("rename")} keyed>
        {(dialog) => (
          <NameDialog
            title={`Rename ${dialog.name}`}
            submit="Rename"
            initial={dialog.name}
            send={(name) => props.workspaces.rename(dialog.id, name)}
            onClose={close}
          />
        )}
      </Show>
      <Show when={of("delete")} keyed>
        {(dialog) => (
          <DeleteDialog name={dialog.name} send={() => props.workspaces.remove(dialog.id)} onClose={close} />
        )}
      </Show>
      <Show when={of("close")} keyed>
        {(dialog) => (
          <CloseDialog
            name={dialog.name}
            files={dialog.files}
            send={() => props.onCloseTab(dialog.id)}
            onClose={close}
          />
        )}
      </Show>
    </>
  );
}
