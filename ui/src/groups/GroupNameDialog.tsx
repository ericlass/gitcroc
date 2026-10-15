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

const MISSING: { missing: MissingBranch; label: string }[] = [
  { missing: "skip", label: "Skip the repository" },
  { missing: "default_branch", label: "Switch to its default branch" },
  { missing: "create", label: "Create the branch from HEAD" },
];

const FAST_FORWARD: { fastForward: MergeFastForward; label: string }[] = [
  { fastForward: "config", label: "As git config says" },
  { fastForward: "ff_only", label: "Fast-forward only" },
  { fastForward: "no_ff", label: "Always create a merge commit" },
];

const TITLES: Record<Dialog["kind"], string> = {
  checkout: "Check out a branch in",
  create: "New branch in",
  delete: "Delete a branch in",
  merge: "Merge into the current branch in",
  rebase: "Rebase the current branch in",
  tag: "New tag in",
};

const SUBMIT: Record<Dialog["kind"], string> = {
  checkout: "Check out",
  create: "Create",
  delete: "Delete",
  merge: "Merge",
  rebase: "Rebase",
  tag: "Create",
};

/** Every branch name under `nodes`, a group's **Branches** node's children. */
function namesOf(nodes: TreeNode[]): string[] {
  return nodes.flatMap((node) => [
    ...(node.item.kind === "group_branch" ? [node.item.name] : []),
    ...namesOf(node.children ?? []),
  ]);
}

/** A ref to merge or rebase onto: anything git would not read as an option. */
const refCheck = (name: string) =>
  Promise.resolve(name.trim() === "" || name.startsWith("-") ? "Name a branch, tag or commit" : null);

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
  const label = () => (dialog.kind === "tag" ? "Tag" : integrating() ? "Branch, tag or commit" : "Branch");

  return (
    <Frame id="group-name" title={`${TITLES[kind()]} ${dialog.targetName}`} onClose={() => props.onClose()}>
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
            <legend>Operation</legend>
            <label class="choice">
              <input
                type="radio"
                name="integrate"
                checked={kind() === "merge"}
                onChange={() => setKind("merge")}
              />{" "}
              Merge it into the current branch
            </label>
            <label class="choice">
              <input
                type="radio"
                name="integrate"
                checked={kind() === "rebase"}
                onChange={() => setKind("rebase")}
              />{" "}
              Rebase the current branch onto it
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
              <legend>Where the branch does not exist</legend>
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
              <legend>Start point</legend>
              <label class="choice">
                <input type="radio" name="start" checked={start() === "head"} onChange={() => setStart("head")} />{" "}
                Current HEAD
              </label>
              <label class="choice">
                <input
                  type="radio"
                  name="start"
                  checked={start() === "default_branch"}
                  onChange={() => setStart("default_branch")}
                />{" "}
                Default branch
              </label>
              <label class="choice">
                <input
                  type="radio"
                  name="start"
                  checked={start() === "revision"}
                  onChange={() => setStart("revision")}
                />{" "}
                Branch, tag or commit:
              </label>
              <input
                type="text"
                class="start-revision"
                aria-label="Branch, tag or commit"
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
            <p class="hint">A repository where the branch exists already, or the start point does not, is skipped.</p>
            <label class="choice">
              <input
                type="checkbox"
                checked={checkout()}
                onChange={(event) => setCheckout(event.currentTarget.checked)}
              />{" "}
              Check out the new branch
            </label>
          </Match>
          <Match when={kind() === "delete"}>
            <label class="choice">
              <input
                type="checkbox"
                checked={force()}
                onChange={(event) => setForce(event.currentTarget.checked)}
              />{" "}
              Force: delete it even if it is not merged
            </label>
            <label class="choice">
              <input
                type="checkbox"
                checked={onRemote()}
                onChange={(event) => setOnRemote(event.currentTarget.checked)}
              />{" "}
              Also delete it on the remote of its upstream, else on origin
            </label>
          </Match>
          <Match when={kind() === "merge"}>
            <fieldset>
              <legend>Fast-forward</legend>
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
              <legend>Kind</legend>
              <label class="choice">
                <input type="radio" name="tag-kind" checked={annotated()} onChange={() => setAnnotated(true)} />{" "}
                Annotated, with a message
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
            <p class="hint">Each repository is tagged at its own HEAD; one where the tag exists already is skipped.</p>
            <label class="choice">
              <input type="checkbox" checked={push()} onChange={(event) => setPush(event.currentTarget.checked)} /> Push
              the tag after creating it
            </label>
          </Match>
        </Switch>
        <Show when={integrating()}>
          <p class="hint">
            The preflight predicts conflicts; a repository where git stops in conflicts is resolved on its own.
          </p>
        </Show>
        <Show when={names.error as unknown}>
          <p role="alert">Cannot read the group's branches</p>
        </Show>
        <Actions submit={SUBMIT[kind()]} disabled={!ready()} onCancel={() => props.onClose()} />
      </form>
    </Frame>
  );
}
