// The clone dialog (section 6 "Repos"): the URL, the parent folder picked
// with the native dialog the core opens (SEC-2), and the folder name, which
// follows the core's default for the URL until the user edits it. The core
// checks the target as the user types: a folder that exists and is not
// empty is refused before git runs. Paths and names are shown as text only
// (SEC-3).
import { createEffect, createSignal, on, Show } from "solid-js";
import { Actions, Frame, NameField, Refusal } from "../dialogs/parts";
import { cloneDefaultName, cloneTargetCheck, type WorkspaceId } from "../ipc";
import type { CloneStore } from "../stores/clone";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export default function CloneDialog(props: { workspace: WorkspaceId; clone: CloneStore }) {
  const [url, setUrl] = createSignal("");
  const [name, setName] = createSignal("");
  /** The user typed a name: the URL no longer sets it. */
  const [edited, setEdited] = createSignal(false);
  const [check, setCheck] = createSignal<{ key: string; error: string | null } | null>(null);
  const parent = () => props.clone.parent(props.workspace);
  const close = () => props.clone.close(props.workspace);

  createEffect(
    on(url, (text) => {
      if (edited()) {
        return;
      }
      cloneDefaultName(text)
        .then((named) => {
          if (url() === text && !edited()) {
            setName(named ?? "");
          }
        })
        .catch(() => {
          // The user can still type a name.
        });
    }),
  );

  const key = () => `${parent() ?? ""}\n${name()}`;
  createEffect(
    on(key, (current) => {
      const into = parent();
      if (into === null || name() === "") {
        return;
      }
      cloneTargetCheck(into, name())
        .then((error) => key() === current && setCheck({ key: current, error }))
        .catch((failure) => key() === current && setCheck({ key: current, error: message(failure) }));
    }),
  );
  const verdict = () => {
    const known = check();
    return known?.key === key() ? known : null;
  };
  const ready = () =>
    url().trim() !== "" && parent() !== null && verdict()?.error === null && !props.clone.sending(props.workspace);

  return (
    <Frame id="clone" title="Clone a repository" onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.clone.start(props.workspace, url(), name());
          }
        }}
      >
        <label class="field">
          <span>URL</span>
          <input
            type="text"
            autocomplete="off"
            spellcheck={false}
            value={url()}
            onInput={(event) => setUrl(event.currentTarget.value)}
          />
        </label>
        <div class="field">
          <span id="clone-parent-label">Into folder</span>
          <div class="clone-parent">
            <span class="clone-path" aria-labelledby="clone-parent-label">
              {parent() ?? "No folder chosen"}
            </span>
            <button type="button" onClick={() => props.clone.pickParent(props.workspace)}>
              Choose…
            </button>
          </div>
        </div>
        <NameField
          label="Folder name"
          value={name()}
          error={name() === "" ? null : (verdict()?.error ?? null)}
          onInput={(value) => {
            setEdited(true);
            setName(value);
          }}
        />
        <Show when={parent() !== null && name() !== ""}>
          <p class="clone-target">Clones into {`${parent()}/${name()}`}</p>
        </Show>
        <Refusal error={props.clone.error(props.workspace)} />
        <Actions submit="Clone" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}
