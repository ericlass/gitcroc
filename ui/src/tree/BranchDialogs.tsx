// The branch dialogs (section 6 "Branches"): create, rename, delete, set
// upstream, and the confirmation the core asks for before a destructive
// one (GRP-9). Names are checked by the core as the user types (SEC-4) and
// its reason shown under the field. Lists to pick from come from the core.
// Repo and ref names are shown as text only (SEC-3).
import { createResource, createSignal, For, Show, untrack } from "solid-js";
import { branchChoices, branchNameCheck, type RepoId, type StartPoint, type WorkspaceId } from "../ipc";
import type { BranchDialog, BranchStore } from "../stores/branch";
import { Actions, ConfirmDialog, createNameCheck, Frame, NameField, Refusal } from "../dialogs/parts";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

interface DialogProps<K extends BranchDialog["kind"]> {
  workspace: WorkspaceId;
  branch: BranchStore;
  dialog: Extract<BranchDialog, { kind: K }>;
  repoName: string;
}

function CreateDialog(props: DialogProps<"create">) {
  const [choices] = createResource(() => branchChoices(props.workspace, props.dialog.repo));
  const [name, setName] = createSignal("");
  const initial = untrack(() => props.dialog.start);
  const [start, setStart] = createSignal<StartPoint["kind"]>(initial.kind);
  const [revision, setRevision] = createSignal(initial.kind === "revision" ? initial.revision : "");
  const [checkout, setCheckout] = createSignal(true);
  const check = createNameCheck(name, branchNameCheck);
  const close = () => props.branch.close(props.workspace);

  const startPoint = (): StartPoint =>
    start() === "revision" ? { kind: "revision", revision: revision().trim() } : { kind: start() as "head" | "default_branch" };
  const ready = () => check.valid() && !props.branch.sending(props.workspace) && (start() !== "revision" || revision().trim() !== "");

  return (
    <Frame id="branch-create" title={strings.branches.createDialog(props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.branch.run(props.workspace, props.dialog.repo, {
              kind: "create",
              name: name(),
              start: startPoint(),
              switch: checkout(),
            });
          }
        }}
      >
        <NameField label={strings.branches.name} value={name()} error={check.error()} onInput={setName} />
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
              disabled={!choices()?.default_branch}
              onChange={() => setStart("default_branch")}
            />{" "}
            {strings.branches.defaultBranch}
            <Show when={choices()?.default_branch}>{(found) => <> ({found()})</>}</Show>
          </label>
          <label class="choice">
            <input type="radio" name="start" checked={start() === "revision"} onChange={() => setStart("revision")} />{" "}
            {strings.branches.revision}
          </label>
          <input
            type="text"
            class="start-revision"
            aria-label={strings.branches.revisionLabel}
            list="branch-start-points"
            autocomplete="off"
            spellcheck={false}
            value={revision()}
            onFocus={() => setStart("revision")}
            onInput={(event) => {
              setStart("revision");
              setRevision(event.currentTarget.value);
            }}
          />
          <datalist id="branch-start-points">
            <For each={choices()?.start_points ?? []}>{(point) => <option value={point} />}</For>
          </datalist>
        </fieldset>
        <label class="choice">
          <input type="checkbox" checked={checkout()} onChange={(event) => setCheckout(event.currentTarget.checked)} />{" "}
          {strings.branches.checkOutNew}
        </label>
        <Show when={choices.error as unknown}>
          <p role="alert">{strings.branches.cannotRead(message(choices.error))}</p>
        </Show>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions submit={strings.branches.create} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function RenameDialog(props: DialogProps<"rename">) {
  const [name, setName] = createSignal(untrack(() => props.dialog.name));
  const check = createNameCheck(name, branchNameCheck);
  const close = () => props.branch.close(props.workspace);
  const ready = () => check.valid() && name() !== props.dialog.name && !props.branch.sending(props.workspace);
  return (
    <Frame id="branch-rename" title={strings.branches.renameDialog(props.dialog.name, props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (ready()) {
            void props.branch.run(props.workspace, props.dialog.repo, {
              kind: "rename",
              name: props.dialog.name,
              new_name: name(),
            });
          }
        }}
      >
        <NameField label={strings.branches.newName} value={name()} error={check.error()} onInput={setName} />
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions submit={strings.branches.rename} disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function DeleteDialog(props: DialogProps<"delete">) {
  const [force, setForce] = createSignal(false);
  const [onRemote, setOnRemote] = createSignal(false);
  const close = () => props.branch.close(props.workspace);
  return (
    <Frame id="branch-delete" title={strings.branches.deleteDialog(props.dialog.name, props.repoName)} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void props.branch.run(props.workspace, props.dialog.repo, {
            kind: "delete",
            name: props.dialog.name,
            force: force(),
            on_remote: onRemote(),
          });
        }}
      >
        <label class="choice">
          <input type="checkbox" checked={force()} onChange={(event) => setForce(event.currentTarget.checked)} />{" "}
          {strings.branches.force}
        </label>
        <label class="choice">
          <input
            type="checkbox"
            checked={onRemote()}
            disabled={!props.dialog.tracked}
            onChange={(event) => setOnRemote(event.currentTarget.checked)}
          />{" "}
          {strings.branches.onRemote}
        </label>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions
          submit={strings.branches.delete}
          danger
          disabled={props.branch.sending(props.workspace)}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

function UpstreamDialog(props: DialogProps<"upstream">) {
  const [choices] = createResource(() => branchChoices(props.workspace, props.dialog.repo));
  const [picked, setPicked] = createSignal<string>(untrack(() => props.dialog.upstream) ?? "");
  const close = () => props.branch.close(props.workspace);
  const key = (remote: string, branch: string) => `${remote}/${branch}`;
  const chosen = () =>
    choices()?.remote_branches.find((candidate) => key(candidate.remote, candidate.branch) === picked()) ?? null;
  return (
    <Frame
      id="branch-upstream"
      title={strings.branches.upstreamDialog(props.dialog.name, props.repoName)}
      onClose={close}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const upstream = chosen();
          if (upstream !== null) {
            void props.branch.run(props.workspace, props.dialog.repo, {
              kind: "set_upstream",
              name: props.dialog.name,
              upstream,
            });
          }
        }}
      >
        <label class="field">
          <span>{strings.branches.remoteBranch}</span>
          <select value={picked()} onChange={(event) => setPicked(event.currentTarget.value)}>
            <option value="" disabled>
              {choices.loading ? strings.branches.loading : strings.branches.pickRemoteBranch}
            </option>
            <For each={choices()?.remote_branches ?? []}>
              {(candidate) => (
                <option value={key(candidate.remote, candidate.branch)}>{key(candidate.remote, candidate.branch)}</option>
              )}
            </For>
          </select>
        </label>
        <Show when={choices()?.remote_branches.length === 0}>
          <p>{strings.branches.noRemoteBranches}</p>
        </Show>
        <Show when={choices.error as unknown}>
          <p role="alert">{strings.branches.cannotRead(message(choices.error))}</p>
        </Show>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions
          submit={strings.branches.setUpstream}
          disabled={chosen() === null || props.branch.sending(props.workspace)}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

/** GRP-9: what cannot be undone, before the delete runs. */
function BranchConfirm(props: DialogProps<"confirm">) {
  return (
    <ConfirmDialog
      repoName={props.dialog.repoName}
      actions={props.dialog.actions}
      submit={strings.branches.delete}
      error={props.branch.error(props.workspace)}
      sending={props.branch.sending(props.workspace)}
      onConfirm={() => void props.branch.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
      onClose={() => props.branch.close(props.workspace)}
    />
  );
}

/** The open branch dialog of `workspace`, if any. */
export default function BranchDialogs(props: {
  workspace: WorkspaceId;
  branch: BranchStore;
  repoName: (repo: RepoId) => string;
}) {
  const dialog = () => props.branch.dialog(props.workspace);
  const as = <K extends BranchDialog["kind"]>(kind: K) => {
    const open = dialog();
    return open?.kind === kind ? (open as Extract<BranchDialog, { kind: K }>) : undefined;
  };
  const common = (repo: RepoId) => ({
    workspace: props.workspace,
    branch: props.branch,
    repoName: props.repoName(repo),
  });
  return (
    <>
      <Show when={as("create")} keyed>
        {(open) => <CreateDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("rename")} keyed>
        {(open) => <RenameDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("delete")} keyed>
        {(open) => <DeleteDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("upstream")} keyed>
        {(open) => <UpstreamDialog {...common(open.repo)} dialog={open} />}
      </Show>
      <Show when={as("confirm")} keyed>
        {(open) => <BranchConfirm {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
