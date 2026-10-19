// The dialogs of the group menus (UI-7): a new group's name, a rename, and
// deleting a group, which says the repos are not touched (GRP-3). The core
// checks names (GRP-2); its refusal is shown inline. Names are text (SEC-3).
import { Show } from "solid-js";
import { createSend, NameDialog, Question } from "../dialogs/parts";
import { groupCreate, groupDelete, groupRename, type GroupId, type RepoId, type WorkspaceId } from "../ipc";
import { strings } from "../strings/en";

export type GroupDialog =
  /** `repos` become the new group's members. */
  | { kind: "new"; repos: RepoId[] }
  | { kind: "rename"; group: GroupId; name: string }
  | { kind: "delete"; group: GroupId; name: string };

function DeleteDialog(props: { name: string; send: () => Promise<unknown>; onClose: () => void }) {
  const sender = createSend(() => props.onClose());
  return (
    <Question
      title={strings.groups.deleteDialog(props.name)}
      submit={strings.groups.delete}
      error={sender.error()}
      sending={sender.sending()}
      onConfirm={() => void sender.run(props.send)}
      onClose={props.onClose}
    >
      <p>{strings.groups.deleteWhat}</p>
    </Question>
  );
}

export default function GroupDialogs(props: {
  workspace: WorkspaceId;
  dialog: GroupDialog | null;
  onClose: () => void;
}) {
  /** The open dialog when it is of `kind`; each opens as a new instance. */
  const of = <K extends GroupDialog["kind"]>(kind: K) => {
    const dialog = props.dialog;
    return dialog?.kind === kind ? (dialog as Extract<GroupDialog, { kind: K }>) : null;
  };
  return (
    <>
      <Show when={of("new")} keyed>
        {(dialog) => (
          <NameDialog
            title={strings.groups.newDialog}
            submit={strings.groups.create}
            initial=""
            send={(name) => groupCreate(props.workspace, name, dialog.repos)}
            onClose={props.onClose}
          />
        )}
      </Show>
      <Show when={of("rename")} keyed>
        {(dialog) => (
          <NameDialog
            title={strings.groups.renameDialog(dialog.name)}
            submit={strings.groups.rename}
            initial={dialog.name}
            send={(name) => groupRename(props.workspace, dialog.group, name)}
            onClose={props.onClose}
          />
        )}
      </Show>
      <Show when={of("delete")} keyed>
        {(dialog) => (
          <DeleteDialog
            name={dialog.name}
            send={() => groupDelete(props.workspace, dialog.group)}
            onClose={props.onClose}
          />
        )}
      </Show>
    </>
  );
}
