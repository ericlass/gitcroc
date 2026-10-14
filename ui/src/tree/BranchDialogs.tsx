// The branch dialogs (section 6 "Branches"): create, rename, delete, set
// upstream, and the confirmation the core asks for before a destructive
// one (GRP-9). Names are checked by the core as the user types (SEC-4) and
// its reason shown under the field. Lists to pick from come from the core.
// Repo and ref names are shown as text only (SEC-3).
import { createEffect, createResource, createSignal, For, type JSX, on, onMount, Show, untrack } from "solid-js";
import { branchChoices, branchNameCheck, type RepoId, type StartPoint, type WorkspaceId } from "../ipc";
import type { BranchDialog, BranchStore } from "../stores/branch";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The core's verdict on `name` as a branch name, the latest one only. */
function createNameCheck(name: () => string) {
  const [verdict, setVerdict] = createSignal<{ name: string; error: string | null } | null>(null);
  createEffect(
    on(name, (text) => {
      branchNameCheck(text)
        .then((error) => {
          if (name() === text) {
            setVerdict({ name: text, error });
          }
        })
        .catch((failure) => {
          if (name() === text) {
            setVerdict({ name: text, error: message(failure) });
          }
        });
    }),
  );
  const current = () => {
    const known = verdict();
    return known?.name === name() ? known : null;
  };
  return {
    valid: () => current()?.error === null,
    /** Shown once something was typed. */
    error: () => (name() === "" ? null : (current()?.error ?? null)),
  };
}

/** A dialog's frame: title, Escape closes it, the first field has the focus. */
function Frame(props: { id: string; title: string; class?: string; onClose: () => void; children: JSX.Element }) {
  let dialog: HTMLDivElement | undefined;
  onMount(() => dialog?.querySelector<HTMLElement>("input:not([type=radio]), select, button")?.focus());
  return (
    <div class="backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${props.id}-title`}
        class={`dialog branch-dialog ${props.class ?? ""}`}
        ref={(element) => (dialog = element)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            props.onClose();
          }
        }}
      >
        <h2 id={`${props.id}-title`}>{props.title}</h2>
        {props.children}
      </div>
    </div>
  );
}

function NameField(props: { label: string; value: string; error: string | null; onInput: (value: string) => void }) {
  return (
    <label class="field">
      <span>{props.label}</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck={false}
        value={props.value}
        aria-invalid={props.error !== null}
        aria-describedby={props.error === null ? undefined : "branch-name-error"}
        onInput={(event) => props.onInput(event.currentTarget.value)}
      />
      <Show when={props.error}>
        {(error) => (
          <span id="branch-name-error" class="field-error">
            {error()}
          </span>
        )}
      </Show>
    </label>
  );
}

function Actions(props: { submit: string; disabled: boolean; danger?: boolean; onCancel: () => void }) {
  return (
    <div class="dialog-actions">
      <button type="button" onClick={() => props.onCancel()}>
        Cancel
      </button>
      <button type="submit" class={props.danger ? "danger" : "primary"} disabled={props.disabled}>
        {props.submit}
      </button>
    </div>
  );
}

function Refusal(props: { error: string | null }) {
  return <Show when={props.error}>{(error) => <p role="alert">{error()}</p>}</Show>;
}

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
  const check = createNameCheck(name);
  const close = () => props.branch.close(props.workspace);

  const startPoint = (): StartPoint =>
    start() === "revision" ? { kind: "revision", revision: revision().trim() } : { kind: start() as "head" | "default_branch" };
  const ready = () => check.valid() && !props.branch.sending(props.workspace) && (start() !== "revision" || revision().trim() !== "");

  return (
    <Frame id="branch-create" title={`New branch in ${props.repoName}`} onClose={close}>
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
        <NameField label="Name" value={name()} error={check.error()} onInput={setName} />
        <fieldset>
          <legend>Start point</legend>
          <label class="choice">
            <input type="radio" name="start" checked={start() === "head"} onChange={() => setStart("head")} /> Current
            HEAD
          </label>
          <label class="choice">
            <input
              type="radio"
              name="start"
              checked={start() === "default_branch"}
              disabled={!choices()?.default_branch}
              onChange={() => setStart("default_branch")}
            />{" "}
            Default branch
            <Show when={choices()?.default_branch}>{(found) => <> ({found()})</>}</Show>
          </label>
          <label class="choice">
            <input type="radio" name="start" checked={start() === "revision"} onChange={() => setStart("revision")} />{" "}
            Branch, tag or commit:
          </label>
          <input
            type="text"
            class="start-revision"
            aria-label="Branch, tag or commit"
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
          Check out the new branch
        </label>
        <Show when={choices.error as unknown}>
          <p role="alert">Cannot read the branches: {message(choices.error)}</p>
        </Show>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions submit="Create" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function RenameDialog(props: DialogProps<"rename">) {
  const [name, setName] = createSignal(untrack(() => props.dialog.name));
  const check = createNameCheck(name);
  const close = () => props.branch.close(props.workspace);
  const ready = () => check.valid() && name() !== props.dialog.name && !props.branch.sending(props.workspace);
  return (
    <Frame id="branch-rename" title={`Rename ${props.dialog.name} in ${props.repoName}`} onClose={close}>
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
        <NameField label="New name" value={name()} error={check.error()} onInput={setName} />
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions submit="Rename" disabled={!ready()} onCancel={close} />
      </form>
    </Frame>
  );
}

function DeleteDialog(props: DialogProps<"delete">) {
  const [force, setForce] = createSignal(false);
  const [onRemote, setOnRemote] = createSignal(false);
  const close = () => props.branch.close(props.workspace);
  return (
    <Frame id="branch-delete" title={`Delete branch ${props.dialog.name} in ${props.repoName}?`} onClose={close}>
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
          <input type="checkbox" checked={force()} onChange={(event) => setForce(event.currentTarget.checked)} /> Delete
          it even if it is not merged
        </label>
        <label class="choice">
          <input
            type="checkbox"
            checked={onRemote()}
            disabled={!props.dialog.tracked}
            onChange={(event) => setOnRemote(event.currentTarget.checked)}
          />{" "}
          Also delete the branch it tracks on the remote
        </label>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions submit="Delete" danger disabled={props.branch.sending(props.workspace)} onCancel={close} />
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
    <Frame id="branch-upstream" title={`Upstream of ${props.dialog.name} in ${props.repoName}`} onClose={close}>
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
          <span>Remote branch</span>
          <select value={picked()} onChange={(event) => setPicked(event.currentTarget.value)}>
            <option value="" disabled>
              {choices.loading ? "Loading…" : "Pick a remote branch"}
            </option>
            <For each={choices()?.remote_branches ?? []}>
              {(candidate) => (
                <option value={key(candidate.remote, candidate.branch)}>{key(candidate.remote, candidate.branch)}</option>
              )}
            </For>
          </select>
        </label>
        <Show when={choices()?.remote_branches.length === 0}>
          <p>The repo has no remote branches.</p>
        </Show>
        <Show when={choices.error as unknown}>
          <p role="alert">Cannot read the branches: {message(choices.error)}</p>
        </Show>
        <Refusal error={props.branch.error(props.workspace)} />
        <Actions
          submit="Set upstream"
          disabled={chosen() === null || props.branch.sending(props.workspace)}
          onCancel={close}
        />
      </form>
    </Frame>
  );
}

/** GRP-9: names the repo and what cannot be undone; Cancel has the focus. */
function ConfirmDialog(props: DialogProps<"confirm">) {
  let cancel: HTMLButtonElement | undefined;
  onMount(() => cancel?.focus());
  const close = () => props.branch.close(props.workspace);
  return (
    <div class="backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="branch-confirm-title"
        aria-describedby="branch-confirm-text"
        class="dialog branch-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            close();
          }
        }}
      >
        <h2 id="branch-confirm-title">Cannot be undone in {props.dialog.repoName}</h2>
        <p id="branch-confirm-text">This will:</p>
        <ul class="branch-actions">
          <For each={props.dialog.actions}>{(action) => <li>{action}</li>}</For>
        </ul>
        <Refusal error={props.branch.error(props.workspace)} />
        <div class="dialog-actions">
          <button type="button" ref={(element) => (cancel = element)} onClick={close}>
            Cancel
          </button>
          <button
            type="button"
            class="danger"
            disabled={props.branch.sending(props.workspace)}
            onClick={() => void props.branch.run(props.workspace, props.dialog.repo, props.dialog.op, true)}
          >
            Delete
          </button>
        </div>
      </div>
    </div>
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
        {(open) => <ConfirmDialog {...common(open.repo)} dialog={open} />}
      </Show>
    </>
  );
}
