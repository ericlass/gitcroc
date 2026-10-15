// Checkout, create and delete of branch X on a target set (section 5
// "Checkout branch X", "Create branch X" and "Delete branch X" rows). X is
// typed or picked once from the group's branch names (UI-5's list) and
// checked by the core as the user types (SEC-4); each repo resolves it on
// its own (GRP-14). Running goes through the preflight like any group
// operation, which asks before a destructive one (GRP-9): this dialog only
// picks the name and the options. Names are shown as text (SEC-3).
import { createResource, createSignal, For, Match, Show, Switch, untrack } from "solid-js";
import {
  branchNameCheck,
  groupBranches,
  type GroupOp,
  type MissingBranch,
  type StartPoint,
  type TreeNode,
} from "../ipc";
import { Actions, createNameCheck, Frame, NameField } from "../dialogs/parts";
import type { GroupBranchDialog as Dialog } from "../stores/groupOps";

const MISSING: { missing: MissingBranch; label: string }[] = [
  { missing: "skip", label: "Skip the repository" },
  { missing: "default_branch", label: "Switch to its default branch" },
  { missing: "create", label: "Create the branch from HEAD" },
];

const TITLES: Record<Dialog["kind"], string> = {
  checkout: "Check out a branch in",
  create: "New branch in",
  delete: "Delete a branch in",
};

const SUBMIT: Record<Dialog["kind"], string> = { checkout: "Check out", create: "Create", delete: "Delete" };

/** Every branch name under `nodes`, a group's **Branches** node's children. */
function namesOf(nodes: TreeNode[]): string[] {
  return nodes.flatMap((node) => [
    ...(node.item.kind === "group_branch" ? [node.item.name] : []),
    ...namesOf(node.children ?? []),
  ]);
}

export default function GroupBranchDialog(props: {
  dialog: Dialog;
  onRun: (op: GroupOp) => void;
  onClose: () => void;
}) {
  const dialog = untrack(() => props.dialog);
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
  const check = createNameCheck(name, branchNameCheck);

  const startPoint = (): StartPoint =>
    start() === "revision"
      ? { kind: "revision", revision: revision().trim() }
      : { kind: start() as "head" | "default_branch" };
  const ready = () => check.valid() && (dialog.kind !== "create" || start() !== "revision" || revision().trim() !== "");
  const op = (): GroupOp => {
    switch (dialog.kind) {
      case "checkout":
        return { kind: "checkout", name: name(), missing: missing() };
      case "create":
        return { kind: "create_branch", name: name(), start: startPoint(), switch: checkout() };
      case "delete":
        return { kind: "delete_branch", name: name(), force: force(), on_remote: onRemote() };
    }
  };

  return (
    <Frame id="group-branch" title={`${TITLES[dialog.kind]} ${dialog.targetName}`} onClose={() => props.onClose()}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            props.onRun(op());
          }
        }}
      >
        <NameField
          label="Branch"
          value={name()}
          error={check.error()}
          onInput={setName}
          list={dialog.kind === "create" ? undefined : "group-branch-names"}
        />
        <datalist id="group-branch-names">
          <For each={names() ?? []}>{(known) => <option value={known} />}</For>
        </datalist>
        <Switch>
          <Match when={dialog.kind === "checkout"}>
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
          <Match when={dialog.kind === "create"}>
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
          <Match when={dialog.kind === "delete"}>
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
        </Switch>
        <Show when={names.error as unknown}>
          <p role="alert">Cannot read the group's branches</p>
        </Show>
        <Actions submit={SUBMIT[dialog.kind]} disabled={!ready()} onCancel={() => props.onClose()} />
      </form>
    </Frame>
  );
}
