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
import { strings } from "../strings/en";

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
    <Frame id="clone" title={strings.clone.title} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.clone.start(props.workspace, url(), name(), group());
          }
        }}
      >
        <label class="field">
          <span>{strings.clone.url}</span>
          <input
            type="text"
            autocomplete="off"
            spellcheck={false}
            value={url()}
            onInput={(event) => setUrl(event.currentTarget.value)}
          />
        </label>
        <div class="field">
          <span id="clone-parent-label">{strings.clone.intoFolder}</span>
          <div class="clone-parent">
            <span class="clone-path" aria-labelledby="clone-parent-label">
              {parent() ?? strings.clone.noFolder}
            </span>
            <button type="button" onClick={() => props.clone.pickParent(props.workspace)}>
              {strings.clone.choose}
            </button>
          </div>
        </div>
        <NameField
          label={strings.clone.folderName}
          value={name()}
          error={name() === "" ? null : (verdict()?.error ?? null)}
          onInput={(value) => {
            setEdited(true);
            setName(value);
          }}
        />
        <Show when={parent() !== null && name() !== ""}>
          <p class="clone-target">{strings.clone.clonesInto(`${parent()}/${name()}`)}</p>
        </Show>
        <label class="field">
          <span>{strings.clone.addToGroup}</span>
          <select value={groupChoice()} onChange={(event) => setGroupChoice(event.currentTarget.value)}>
            <option value="">{strings.clone.noGroup}</option>
            <For each={props.groups}>{(known) => <option value={known.id}>{known.name}</option>}</For>
            <option value={NEW_GROUP}>{strings.clone.newGroup}</option>
          </select>
        </label>
        <Show when={groupChoice() === NEW_GROUP}>
          <label class="field">
            <span>{strings.clone.groupName}</span>
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
        <Actions submit={strings.clone.clone} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}
