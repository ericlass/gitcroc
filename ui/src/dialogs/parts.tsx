// The parts the operation dialogs share (branches, sync, tags, reset,
// remotes): a modal frame, a name field the core checks as the user types
// (SEC-4), the Cancel and submit buttons, the core's refusal, and the
// confirmation the core asks for before a destructive operation (GRP-9).
// Names are shown as text only (SEC-3).
import { createEffect, createSignal, For, type JSX, on, onMount, Show } from "solid-js";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The core's verdict on `name` by `check`, such as a branch name check, the latest one only. */
export function createNameCheck(name: () => string, check: (name: string) => Promise<string | null>) {
  const [verdict, setVerdict] = createSignal<{ name: string; error: string | null } | null>(null);
  createEffect(
    on(name, (text) => {
      check(text)
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

export function NameField(props: {
  label: string;
  value: string;
  error: string | null;
  onInput: (value: string) => void;
  /** The id of a `datalist` of names to pick from. */
  list?: string;
}) {
  return (
    <label class="field">
      <span>{props.label}</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck={false}
        list={props.list}
        value={props.value}
        aria-invalid={props.error !== null}
        aria-describedby={props.error === null ? undefined : "name-error"}
        onInput={(event) => props.onInput(event.currentTarget.value)}
      />
      <Show when={props.error}>
        {(error) => (
          <span id="name-error" class="field-error">
            {error()}
          </span>
        )}
      </Show>
    </label>
  );
}

/** A dialog's frame: title, Escape closes it, the first field has the focus. */
export function Frame(props: {
  id: string;
  title: string;
  class?: string;
  onClose: () => void;
  children: JSX.Element;
}) {
  let dialog: HTMLDivElement | undefined;
  onMount(() => dialog?.querySelector<HTMLElement>("input:not([type=radio]), select, button")?.focus());
  return (
    <div class="backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${props.id}-title`}
        class={`dialog op-dialog ${props.class ?? ""}`}
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

export function Actions(props: { submit: string; disabled: boolean; danger?: boolean; onCancel: () => void }) {
  return (
    <div class="dialog-actions">
      <button type="button" onClick={() => props.onCancel()}>
        {strings.common.cancel}
      </button>
      <button type="submit" class={props.danger ? "danger" : "primary"} disabled={props.disabled}>
        {props.submit}
      </button>
    </div>
  );
}

/** Why the core refused the request. */
export function Refusal(props: { error: string | null }) {
  return <Show when={props.error}>{(error) => <p role="alert">{error()}</p>}</Show>;
}

/** GRP-9: names the repo and what cannot be undone; Cancel has the focus. */
export function ConfirmDialog(props: {
  repoName: string;
  actions: string[];
  /** The button that goes ahead, such as `Delete`. */
  submit: string;
  error: string | null;
  sending: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  let cancel: HTMLButtonElement | undefined;
  onMount(() => cancel?.focus());
  return (
    <div class="backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="op-confirm-title"
        aria-describedby="op-confirm-text"
        class="dialog op-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            props.onClose();
          }
        }}
      >
        <h2 id="op-confirm-title">{strings.dialogs.cannotBeUndoneIn(props.repoName)}</h2>
        <p id="op-confirm-text">{strings.dialogs.thisWill}</p>
        <ul class="op-actions">
          <For each={props.actions}>{(action) => <li>{action}</li>}</For>
        </ul>
        <Refusal error={props.error} />
        <div class="dialog-actions">
          <button type="button" ref={(element) => (cancel = element)} onClick={() => props.onClose()}>
            {strings.common.cancel}
          </button>
          <button type="button" class="danger" disabled={props.sending} onClick={() => props.onConfirm()}>
            {props.submit}
          </button>
        </div>
      </div>
    </div>
  );
}

/** A name for something new, or a new name; the core's refusal shows under the field. */
export function NameDialog(props: {
  title: string;
  submit: string;
  initial: string;
  send: (name: string) => Promise<unknown>;
  onClose: () => void;
}) {
  const [name, setName] = createSignal(props.initial);
  const [error, setError] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);
  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    setSending(true);
    try {
      await props.send(name());
      props.onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setSending(false);
    }
  };
  return (
    <Frame id="name-dialog" title={props.title} onClose={props.onClose}>
      <form onSubmit={(event) => void submit(event)}>
        <NameField
          label={strings.dialogs.name}
          value={name()}
          error={error()}
          onInput={(value) => {
            setName(value);
            setError(null);
          }}
        />
        <Actions submit={props.submit} disabled={sending()} onCancel={props.onClose} />
      </form>
    </Frame>
  );
}

/** A question that cannot be undone: Cancel has the focus, Escape cancels. */
export function Question(props: {
  title: string;
  submit: string;
  error: string | null;
  sending: boolean;
  onConfirm: () => void;
  onClose: () => void;
  children: JSX.Element;
}) {
  let cancel: HTMLButtonElement | undefined;
  onMount(() => cancel?.focus());
  return (
    <div class="backdrop">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="question-title"
        aria-describedby="question-text"
        class="dialog op-dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            props.onClose();
          }
        }}
      >
        <h2 id="question-title">{props.title}</h2>
        <div id="question-text">{props.children}</div>
        <Refusal error={props.error} />
        <div class="dialog-actions">
          <button type="button" ref={(element) => (cancel = element)} onClick={() => props.onClose()}>
            {strings.common.cancel}
          </button>
          <button type="button" class="danger" disabled={props.sending} onClick={() => props.onConfirm()}>
            {props.submit}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Runs `send`; closes the dialog when it succeeds and shows the refusal when it does not. */
export function createSend(onClose: () => void) {
  const [error, setError] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);
  const run = async (send: () => Promise<unknown>) => {
    setSending(true);
    setError(null);
    try {
      await send();
      onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setSending(false);
    }
  };
  return { error, sending, run };
}
