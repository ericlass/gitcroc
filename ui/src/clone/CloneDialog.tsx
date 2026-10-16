// The clone dialog (section 6 "Repos"): the URL, the parent folder picked
// with the native dialog the core opens (SEC-2), and the folder name, which
// follows the core's default for the URL until the user edits it. The core
// checks the target as the user types: a folder that exists and is not
// empty is refused before git runs. Add to group picks none, a group, or a
// new group's name, which the core checks (GRP-2) before git runs. Paths
// and names are shown as text only (SEC-3).
import { createEffect, createSignal, For, on, Show } from "solid-js";
import { Actions, Frame, NameField, Refusal } from "../dialogs/parts";
import { type CloneGroup, cloneDefaultName, cloneTargetCheck, type GroupView, type WorkspaceId } from "../ipc";
import type { CloneStore } from "../stores/clone";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The Add to group choice of a new group; group ids are `g` and a number. */
const NEW_GROUP = "+new";

export default function CloneDialog(props: { workspace: WorkspaceId; clone: CloneStore; groups: GroupView[] }) {
  const [url, setUrl] = createSignal("");
  const [name, setName] = createSignal("");
  /** The user typed a name: the URL no longer sets it. */
  const [edited, setEdited] = createSignal(false);
  const [check, setCheck] = createSignal<{ key: string; error: string | null } | null>(null);
  /** `""` for none, a group's id, or {@link NEW_GROUP}. */
  const [groupChoice, setGroupChoice] = createSignal("");
  const [groupName, setGroupName] = createSignal("");
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
  const group = (): CloneGroup | null => {
    const choice = groupChoice();
    if (choice === "") {
      return null;
    }
    return choice === NEW_GROUP ? { kind: "new", name: groupName() } : { kind: "existing", group_id: choice };
  };
  const ready = () =>
    url().trim() !== "" &&
    parent() !== null &&
    verdict()?.error === null &&
    (groupChoice() !== NEW_GROUP || groupName().trim() !== "") &&
    !props.clone.sending(props.workspace);

  return (
    <Frame id="clone" title="Clone a repository" onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.clone.start(props.workspace, url(), name(), group());
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
        <label class="field">
          <span>Add to group</span>
          <select value={groupChoice()} onChange={(event) => setGroupChoice(event.currentTarget.value)}>
            <option value="">None</option>
            <For each={props.groups}>{(known) => <option value={known.id}>{known.name}</option>}</For>
            <option value={NEW_GROUP}>New group…</option>
          </select>
        </label>
        <Show when={groupChoice() === NEW_GROUP}>
          <label class="field">
            <span>Group name</span>
            <input
              type="text"
              autocomplete="off"
              spellcheck={false}
              value={groupName()}
              onInput={(event) => setGroupName(event.currentTarget.value)}
            />
          </label>
        </Show>
        <Refusal error={props.clone.error(props.workspace)} />
        <Actions submit="Clone" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}
