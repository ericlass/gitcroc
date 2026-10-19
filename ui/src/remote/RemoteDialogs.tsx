// The remote dialogs (section 6 "Remotes"): add, with a fetch after it as
// an option (default off); rename; edit the URL, and the push URL when one
// is set; and the confirmation the core asks for before a remove, naming
// the upstreams it unsets. Names are checked by the core as the user types
// (SEC-4). Names and URLs are shown as text only (SEC-3).
import { createSignal, Show, untrack } from "solid-js";
import { Actions, ConfirmDialog, createNameCheck, Frame, NameField, Refusal } from "../dialogs/parts";
import { type RemoteUrlsView, remoteNameCheck, type RepoId, type WorkspaceId } from "../ipc";
import type { RemoteDialog, RemoteStore } from "../stores/remote";
import { strings } from "../strings/en";

interface DialogProps<K extends RemoteDialog["kind"]> {
  workspace: WorkspaceId;
  remote: RemoteStore;
  dialog: Extract<RemoteDialog, { kind: K }>;
  repoName: string;
}

function UrlField(props: { label: string; value: string; onInput: (value: string) => void }) {
  return (
    <label class="field">
      <span>{props.label}</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck={false}
        value={props.value}
        onInput={(event) => props.onInput(event.currentTarget.value)}
      />
    </label>
  );
}

function AddDialog(props: DialogProps<"add">) {
  const [name, setName] = createSignal("");
  const [url, setUrl] = createSignal("");
  const [fetch, setFetch] = createSignal(false);
  const check = createNameCheck(name, remoteNameCheck);
  const close = () => props.remote.close(props.workspace);
  const ready = () => check.valid() && url().trim() !== "" && !props.remote.sending(props.workspace);
  return (
    <Frame id="remote-add" title={strings.remote.addTitle(props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.remote.run(props.workspace, props.dialog.repo, {
              kind: "add",
              name: name(),
              url: url(),
              fetch: fetch(),
            });
          }
        }}
      >
        <NameField label={strings.remote.name} value={name()} error={check.error()} onInput={setName} />
        <UrlField label={strings.remote.url} value={url()} onInput={setUrl} />
        <label class="choice">
          <input type="checkbox" checked={fetch()} onChange={(event) => setFetch(event.currentTarget.checked)} />{" "}
          {strings.remote.fetchAfter}
        </label>
        <Refusal error={props.remote.error(props.workspace)} />
        <Actions submit={strings.remote.add} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function RenameDialog(props: DialogProps<"rename">) {
  const [name, setName] = createSignal(untrack(() => props.dialog.name));
  const check = createNameCheck(name, remoteNameCheck);
  const close = () => props.remote.close(props.workspace);
  const ready = () => check.valid() && name() !== props.dialog.name && !props.remote.sending(props.workspace);
  return (
    <Frame id="remote-rename" title={strings.remote.renameTitle(props.dialog.name, props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.remote.run(props.workspace, props.dialog.repo, {
              kind: "rename",
              name: props.dialog.name,
              new_name: name(),
            });
          }
        }}
      >
        <NameField label={strings.remote.newName} value={name()} error={check.error()} onInput={setName} />
        <p>{strings.remote.renameMoves}</p>
        <Refusal error={props.remote.error(props.workspace)} />
        <Actions submit={strings.remote.rename} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

/** The URL fields, filled with what the core read. */
function UrlForm(props: DialogProps<"url"> & { urls: RemoteUrlsView }) {
  const [url, setUrl] = createSignal(untrack(() => props.urls.url) ?? "");
  const [pushUrl, setPushUrl] = createSignal(untrack(() => props.urls.push_url));
  const close = () => props.remote.close(props.workspace);
  const pushReady = () => {
    const push = pushUrl();
    return push === null || push.trim() !== "";
  };
  const ready = () => url().trim() !== "" && pushReady() && !props.remote.sending(props.workspace);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (ready()) {
          void props.remote.run(props.workspace, props.dialog.repo, {
            kind: "set_url",
            name: props.dialog.name,
            url: url(),
            push_url: pushUrl(),
          });
        }
      }}
    >
      <UrlField
        label={pushUrl() === null ? strings.remote.url : strings.remote.fetchUrl}
        value={url()}
        onInput={setUrl}
      />
      <Show when={pushUrl() !== null}>
        <UrlField label={strings.remote.pushUrl} value={pushUrl() ?? ""} onInput={setPushUrl} />
      </Show>
      <Refusal error={props.remote.error(props.workspace)} />
      <Actions submit={strings.remote.save} disabled={!ready()} onCancel={close} />
    </form>
  );
}

function UrlDialog(props: DialogProps<"url">) {
  const close = () => props.remote.close(props.workspace);
  return (
    <Frame id="remote-url" title={strings.remote.urlTitle(props.dialog.name, props.repoName)} onClose={close}>
      <Show
        when={props.dialog.urls}
        keyed
        fallback={
          <Show
            when={props.remote.error(props.workspace)}
            fallback={<p class="placeholder">{strings.remote.readingUrls}</p>}
          >
            <Refusal error={props.remote.error(props.workspace)} />
            <div class="dialog-actions">
              <button type="button" onClick={close}>
                {strings.common.close}
              </button>
            </div>
          </Show>
        }
      >
        {(urls) => <UrlForm {...props} urls={urls} />}
      </Show>
    </Frame>
  );
}

function RemoteConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit={strings.remote.remove}
      error={props.remote.error(props.workspace)}
      sending={props.remote.sending(props.workspace)}
      onConfirm={() => void props.remote.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.remote.close(props.workspace)}
    />
  );
}

/** The open remote dialog of `workspace`, if any. */
export default function RemoteDialogs(props: {
  workspace: WorkspaceId;
  remote: RemoteStore;
  repoName: (repo: RepoId) => string;
}) {
  const dialog = () => props.remote.dialog(props.workspace);
  const as = <K extends RemoteDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<RemoteDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({
    workspace: props.workspace,
    remote: props.remote,
    repoName: props.repoName(repo),
  });
  return (
    <>
      <Show when={as("add")} keyed>
        {(open) => <AddDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("rename")} keyed>
        {(open) => <RenameDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("url")} keyed>
        {(open) => <UrlDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <RemoteConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
