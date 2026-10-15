// The dialogs of the workspace tabs (UI-28): a new workspace's name, a
// rename, deleting a workspace file that is not open, and closing a tab that
// holds unsaved conflict edits (OPS-9). The core checks names and refuses
// what it does not allow; its refusal is shown inline. Names are text (SEC-3).
import { createSignal, For, Show } from "solid-js";
import { createSend, NameDialog, Question } from "../dialogs/parts";
import type { WorkspaceId } from "../ipc";
import type { WorkspaceStore } from "../stores/workspaces";

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

function DeleteDialog(props: { name: string; send: () => Promise<unknown>; onClose: () => void }) {
  const sender = createSend(() => props.onClose());
  return (
    <Question
      title={`Delete the workspace ${props.name}?`}
      submit="Delete"
      error={sender.error()}
      sending={sender.sending()}
      onConfirm={() => void sender.run(props.send)}
      onClose={props.onClose}
    >
      <p>Its workspace file is deleted. The repositories on disk are not touched.</p>
    </Question>
  );
}

function CloseDialog(props: { name: string; files: string[]; send: () => Promise<unknown>; onClose: () => void }) {
  const sender = createSend(() => props.onClose());
  return (
    <Question
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
    </Question>
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
