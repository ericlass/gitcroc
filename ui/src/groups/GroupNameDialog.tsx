// The group operations on a name X typed or picked once (section 5
// "Checkout branch X", "Create branch X", "Delete branch X", "Merge X into
// current", "Rebase current onto X" and "Create tag X" rows). X is picked
// from the group's branch names (UI-5's list) or typed. A branch or tag
// name is checked by the core as the user types (SEC-4); a ref to merge or
// rebase onto may be any ref. Each repo resolves X on its own (GRP-14).
// Running goes through the preflight like any group operation, which
// predicts conflicts (GRP-13) and asks before a destructive one (GRP-9):
// this dialog only picks the name and the options. Names are shown as text
// (SEC-3).
import { createResource, createSignal, For, Match, Show, Switch, untrack } from "solid-js";
import {
  branchNameCheck,
  groupBranches,
  type GroupOp,
  type MergeFastForward,
  type MissingBranch,
  type StartPoint,
  tagNameCheck,
  type TreeNode,
} from "../ipc";
import { Actions, createNameCheck, Frame, NameField } from "../dialogs/parts";
import type { GroupNameDialog as Dialog } from "../stores/groupOps";
import { strings } from "../strings/en";

const MISSING: { missing: MissingBranch; label: string }[] = [
  { missing: "skip", label: strings.groupOps.missing.skip },
  { missing: "default_branch", label: strings.groupOps.missing.defaultBranch },
  { missing: "create", label: strings.groupOps.missing.create },
];

const FAST_FORWARD: { fastForward: MergeFastForward; label: string }[] = [
  { fastForward: "config", label: strings.groupOps.fastForwards.config },
  { fastForward: "ff_only", label: strings.groupOps.fastForwards.ffOnly },
  { fastForward: "no_ff", label: strings.groupOps.fastForwards.noFf },
];

const TITLES = strings.groupOps.titles;

const SUBMIT = strings.groupOps.submit;

/** Every branch name under `nodes`, a group's **Branches** node's children. */
function namesOf(nodes: TreeNode[]): string[] {
  return nodes.flatMap((node) => [
    ...(node.item.kind === "group_branch" ? [node.item.name] : []),
    ...namesOf(node.children ?? []),
  ]);
}

/** A ref to merge or rebase onto: anything git would not read as an option. */
const refCheck = (name: string) =>
  Promise.resolve(name.trim() === "" || name.startsWith("-") ? strings.groupOps.nameARef : null);

export default function GroupNameDialog(props: { dialog: Dialog; onRun: (op: GroupOp) => void; onClose: () => void }) {
  const dialog = untrack(() => props.dialog);
  // Merge and rebase are one dialog, either picked.
  const [kind, setKind] = createSignal(dialog.kind);
  const integrating = () => kind() === "merge" || kind() === "rebase";
  // Only a group has a Branches node; several repos get no list.
  const [names] = createResource(async () => {
    const target = dialog.target;
    return target.kind === "group" ? namesOf(await groupBranches(dialog.workspaceId, target.group_id)) : [];
  });
  const [name, setName] = createSignal(dialog.name);
  const [missing, setMissing] = createSignal<MissingBranch>("skip");
  const [start, setStart] = createSignal<StartPoint["kind"]>("head");
  const [revision, setRevision] = createSignal("");
  const [checkout, setCheckout] = createSignal(true);
  const [force, setForce] = createSignal(false);
  const [onRemote, setOnRemote] = createSignal(false);
  const [fastForward, setFastForward] = createSignal<MergeFastForward>("config");
  const [annotated, setAnnotated] = createSignal(true);
  const [message, setMessage] = createSignal("");
  const [push, setPush] = createSignal(false);
  // Merge and rebase switch between each other, never to a branch or tag name.
  const ref = dialog.kind === "merge" || dialog.kind === "rebase";
  const nameCheck = dialog.kind === "tag" ? tagNameCheck : ref ? refCheck : branchNameCheck;
  const check = createNameCheck(name, nameCheck);

  const startPoint = (): StartPoint =>
    start() === "revision"
      ? { kind: "revision", revision: revision().trim() }
      : { kind: start() as "head" | "default_branch" };
  const ready = () =>
    check.valid() &&
    (kind() !== "create" || start() !== "revision" || revision().trim() !== "") &&
    (kind() !== "tag" || !annotated() || message().trim() !== "");
  const op = (): GroupOp => {
    switch (kind()) {
      case "checkout":
        return { kind: "checkout", name: name(), missing: missing() };
      case "create":
        return { kind: "create_branch", name: name(), start: startPoint(), switch: checkout() };
      case "delete":
        return { kind: "delete_branch", name: name(), force: force(), on_remote: onRemote() };
      case "merge":
        return { kind: "merge", name: name().trim(), fast_forward: fastForward() };
      case "rebase":
        return { kind: "rebase", onto: name().trim() };
      case "tag":
        return {
          kind: "create_tag",
          name: name(),
          annotated: annotated(),
          message: annotated() ? message() : "",
          push: push(),
        };
    }
  };
  const label = () =>
    dialog.kind === "tag" ? strings.groupOps.tag : integrating() ? strings.groupOps.ref : strings.groupOps.branch;

  return (
    <Frame id="group-name" title={TITLES[kind()](dialog.targetName)} onClose={() => props.onClose()}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            props.onRun(op());
          }
        }}
      >
        <Show when={integrating()}>
          <fieldset>
            <legend>{strings.groupOps.operation}</legend>
            <label class="choice">
              <input
                type="radio"
                name="integrate"
                checked={kind() === "merge"}
                onChange={() => setKind("merge")}
              />{" "}
              {strings.groupOps.mergeIt}
            </label>
            <label class="choice">
              <input
                type="radio"
                name="integrate"
                checked={kind() === "rebase"}
                onChange={() => setKind("rebase")}
              />{" "}
              {strings.groupOps.rebaseOnto}
            </label>
          </fieldset>
        </Show>
        <NameField
          label={label()}
          value={name()}
          error={check.error()}
          onInput={setName}
          list={kind() === "create" || kind() === "tag" ? undefined : "group-branch-names"}
        />
        <datalist id="group-branch-names">
          <For each={names() ?? []}>{(known) => <option value={known} />}</For>
        </datalist>
        <Switch>
          <Match when={kind() === "checkout"}>
            <fieldset>
              <legend>{strings.groupOps.whereMissing}</legend>
              <For each={MISSING}>
                {(choice) => (
                  <label class="choice">
                    <input
                      type="radio"
                      name="missing"
                      checked={missing() === choice.missing}
                      onChange={() => setMissing(choice.missing)}
                    />{" "}
                    {choice.label}
                  </label>
                )}
              </For>
            </fieldset>
          </Match>
          <Match when={kind() === "create"}>
            <fieldset>
              <legend>{strings.branches.startPoint}</legend>
              <label class="choice">
                <input type="radio" name="start" checked={start() === "head"} onChange={() => setStart("head")} />{" "}
                {strings.branches.currentHead}
              </label>
              <label class="choice">
                <input
                  type="radio"
                  name="start"
                  checked={start() === "default_branch"}
                  onChange={() => setStart("default_branch")}
                />{" "}
                {strings.branches.defaultBranch}
              </label>
              <label class="choice">
                <input
                  type="radio"
                  name="start"
                  checked={start() === "revision"}
                  onChange={() => setStart("revision")}
                />{" "}
                {strings.branches.revision}
              </label>
              <input
                type="text"
                class="start-revision"
                aria-label={strings.branches.revisionLabel}
                autocomplete="off"
                spellcheck={false}
                value={revision()}
                onFocus={() => setStart("revision")}
                onInput={(event) => {
                  setStart("revision");
                  setRevision(event.currentTarget.value);
                }}
              />
            </fieldset>
            <p class="hint">{strings.groupOps.createSkips}</p>
            <label class="choice">
              <input
                type="checkbox"
                checked={checkout()}
                onChange={(event) => setCheckout(event.currentTarget.checked)}
              />{" "}
              {strings.branches.checkOutNew}
            </label>
          </Match>
          <Match when={kind() === "delete"}>
            <label class="choice">
              <input
                type="checkbox"
                checked={force()}
                onChange={(event) => setForce(event.currentTarget.checked)}
              />{" "}
              {strings.groupOps.force}
            </label>
            <label class="choice">
              <input
                type="checkbox"
                checked={onRemote()}
                onChange={(event) => setOnRemote(event.currentTarget.checked)}
              />{" "}
              {strings.groupOps.onRemote}
            </label>
          </Match>
          <Match when={kind() === "merge"}>
            <fieldset>
              <legend>{strings.groupOps.fastForward}</legend>
              <For each={FAST_FORWARD}>
                {(choice) => (
                  <label class="choice">
                    <input
                      type="radio"
                      name="fast-forward"
                      checked={fastForward() === choice.fastForward}
                      onChange={() => setFastForward(choice.fastForward)}
                    />{" "}
                    {choice.label}
                  </label>
                )}
              </For>
            </fieldset>
          </Match>
          <Match when={kind() === "tag"}>
            <fieldset>
              <legend>{strings.tag.kind}</legend>
              <label class="choice">
                <input type="radio" name="tag-kind" checked={annotated()} onChange={() => setAnnotated(true)} />{" "}
                {strings.tag.annotated}
              </label>
              <label class="choice">
                <input type="radio" name="tag-kind" checked={!annotated()} onChange={() => setAnnotated(false)} />{" "}
                {strings.tag.lightweight}
              </label>
            </fieldset>
            <label class="field">
              <span>{strings.tag.message}</span>
              <textarea
                rows={4}
                spellcheck={false}
                disabled={!annotated()}
                value={message()}
                onInput={(event) => setMessage(event.currentTarget.value)}
              />
            </label>
            <p class="hint">{strings.groupOps.tagSkips}</p>
            <label class="choice">
              <input type="checkbox" checked={push()} onChange={(event) => setPush(event.currentTarget.checked)} />{" "}
              {strings.tag.push}
            </label>
          </Match>
        </Switch>
        <Show when={integrating()}>
          <p class="hint">{strings.groupOps.predictsConflicts}</p>
        </Show>
        <Show when={names.error as unknown}>
          <p role="alert">{strings.groupOps.cannotReadBranches}</p>
        </Show>
        <Actions submit={SUBMIT[kind()]} disabled={!ready()} onCancel={() => props.onClose()} />
      </form>
    </Frame>
  );
}
