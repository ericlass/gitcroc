// The tag dialogs (section 6 "Tags", section 5 "Create tag X"): create at
// `HEAD` or at a commit of the history, annotated with a message (default)
// or lightweight, pushed after (default off); delete, also on the remote;
// the remote to push to or delete on when the core asks which; and the
// confirmation it asks for before a delete on the remote. The name is
// checked by the core as the user types (SEC-4). Names and messages are
// shown as text only (SEC-3).
import { createSignal, For, Show, untrack } from "solid-js";
import { Actions, ConfirmDialog, createNameCheck, Frame, NameField, Refusal } from "../dialogs/parts";
import { type RepoId, tagNameCheck, type WorkspaceId } from "../ipc";
import type { TagDialog, TagStore } from "../stores/tag";

interface DialogProps<K extends TagDialog["kind"]> {
  workspace: WorkspaceId;
  tag: TagStore;
  dialog: Extract<TagDialog, { kind: K }>;
  repoName: string;
}

function CreateDialog(props: DialogProps<"create">) {
  const [name, setName] = createSignal("");
  const [annotated, setAnnotated] = createSignal(true);
  const [message, setMessage] = createSignal("");
  const [push, setPush] = createSignal(false);
  const check = createNameCheck(name, tagNameCheck);
  const close = () => props.tag.close(props.workspace);
  const ready = () =>
    check.valid() && (!annotated() || message().trim() !== "") && !props.tag.sending(props.workspace);
  return (
    <Frame id="tag-create" title={`New tag at ${props.dialog.at} in ${props.repoName}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.tag.run(props.workspace, props.dialog.repo, {
              kind: "create",
              name: name(),
              commit: props.dialog.commit,
              annotated: annotated(),
              message: annotated() ? message() : "",
              push: push(),
              remote: null,
            });
          }
        }}
      >
        <NameField label="Name" value={name()} error={check.error()} onInput={setName} />
        <fieldset>
          <legend>Kind</legend>
          <label class="choice">
            <input type="radio" name="tag-kind" checked={annotated()} onChange={() => setAnnotated(true)} /> Annotated,
            with a message
          </label>
          <label class="choice">
            <input type="radio" name="tag-kind" checked={!annotated()} onChange={() => setAnnotated(false)} />{" "}
            Lightweight
          </label>
        </fieldset>
        <label class="field">
          <span>Message</span>
          <textarea
            rows={4}
            spellcheck={false}
            disabled={!annotated()}
            value={message()}
            onInput={(event) => setMessage(event.currentTarget.value)}
          />
        </label>
        <label class="choice">
          <input type="checkbox" checked={push()} onChange={(event) => setPush(event.currentTarget.checked)} /> Push
          the tag after creating it
        </label>
        <Refusal error={props.tag.error(props.workspace)} />
        <Actions submit="Create" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function DeleteDialog(props: DialogProps<"delete">) {
  const [onRemote, setOnRemote] = createSignal(false);
  const close = () => props.tag.close(props.workspace);
  return (
    <Frame id="tag-delete" title={`Delete tag ${props.dialog.name} in ${props.repoName}?`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.tag.run(props.workspace, props.dialog.repo, {
            kind: "delete",
            name: props.dialog.name,
            on_remote: onRemote(),
            remote: null,
          });
        }}
      >
        <label class="choice">
          <input type="checkbox" checked={onRemote()} onChange={(event) => setOnRemote(event.currentTarget.checked)} />{" "}
          Also delete it on the remote
        </label>
        <Refusal error={props.tag.error(props.workspace)} />
        <Actions submit="Delete" danger disabled={props.tag.sending(props.workspace)} onCancel={close} />
      </form>
    </Frame>
  );
}

/** The repo has several remotes and no `origin`: the one the operation goes to. */
function RemoteDialog(props: DialogProps<"remote">) {
  const [remote, setRemote] = createSignal(untrack(() => props.dialog.remotes[0]) ?? "");
  const close = () => props.tag.close(props.workspace);
  const op = () => props.dialog.op;
  const title = () => {
    const current = op();
    return current.kind === "delete"
      ? `Delete tag ${current.name} on which remote?`
      : `Push tag ${current.name} to which remote?`;
  };
  return (
    <Frame id="tag-remote" title={title()} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.tag.run(props.workspace, props.dialog.repo, { ...op(), remote: remote() });
        }}
      >
        <label class="field">
          <span>Remote</span>
          <select value={remote()} onChange={(event) => setRemote(event.currentTarget.value)}>
            <For each={props.dialog.remotes}>{(name) => <option value={name}>{name}</option>}</For>
          </select>
        </label>
        <Refusal error={props.tag.error(props.workspace)} />
        <Actions
          submit={op().kind === "delete" ? "Delete" : op().kind === "create" ? "Create" : "Push"}
          danger={op().kind === "delete"}
          disabled={props.tag.sending(props.workspace) || remote() === ""}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

function TagConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit="Delete"
      error={props.tag.error(props.workspace)}
      sending={props.tag.sending(props.workspace)}
      onConfirm={() => void props.tag.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.tag.close(props.workspace)}
    />
  );
}

/** The open tag dialog of `workspace`, if any. */
export default function TagDialogs(props: { workspace: WorkspaceId; tag: TagStore; repoName: (repo: RepoId) => string }) {
  const dialog = () => props.tag.dialog(props.workspace);
  const as = <K extends TagDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<TagDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({ workspace: props.workspace, tag: props.tag, repoName: props.repoName(repo) });
  return (
    <>
      <Show when={as("create")} keyed>
        {(open) => <CreateDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("delete")} keyed>
        {(open) => <DeleteDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("remote")} keyed>
        {(open) => <RemoteDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <TagConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
