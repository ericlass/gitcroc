// The parts the dialogs share. Every dialog is a Modal (M5-10): it takes the
// focus, keeps Tab inside, closes on Escape where it may, and gives the focus
// back to what had it. The operation dialogs (branches, sync, tags, reset,
// remotes) also share a frame, a name field the core checks as the user
// types (SEC-4), the Cancel and submit buttons, the core's refusal, and the
// confirmation the core asks for before a destructive operation (GRP-9).
// Names are shown as text only (SEC-3).
import { createEffect, createSignal, For, type JSX, on, onCleanup, onMount, Show } from "solid-js";
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

const TABBABLE =
  "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], " +
  "[tabindex], [contenteditable=true]";

/** The elements of `root` that Tab stops at, in order. */
function tabbables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(TABBABLE)].filter(
    (element) => element.tabIndex >= 0 && element.getClientRects().length > 0,
  );
}

/** Where the focus goes back to from each open dialog, when what opened it is gone. */
const openers = new WeakMap<Element, Element | null>();

/**
 * A modal dialog over the window. It takes the focus unless its content already did, on its first field or
 * else its first button; Tab and Shift+Tab go around inside it; Escape calls `onEscape`, where the dialog may be
 * closed so; and when it closes the focus goes back to what had it when it opened, unless closing moved it on.
 */
export function Modal(props: {
  role?: "dialog" | "alertdialog";
  labelledby: string;
  describedby?: string;
  class: string;
  onEscape?: () => void;
  /** A click on the backdrop. */
  onBackdrop?: () => void;
  ref?: (element: HTMLDivElement) => void;
  children: JSX.Element;
}) {
  let dialog: HTMLDivElement | undefined;
  // Taken before the content's own onMount moves the focus in. A dialog opened from another that closes, as a
  // confirmation from its question, gives the focus back to what opened the first.
  const opener = document.activeElement;
  const from = opener?.closest("[aria-modal=true]");
  const fallback = from != null && openers.has(from) ? (openers.get(from) ?? null) : opener;
  onMount(() => {
    if (dialog !== undefined && !dialog.contains(document.activeElement)) {
      dialog.querySelector<HTMLElement>("input:not([type=radio]), select, textarea, button")?.focus();
    }
  });
  onCleanup(() => {
    const focused = document.activeElement;
    const lost = focused === null || focused === document.body || dialog?.contains(focused) === true;
    const back = opener?.isConnected === true ? opener : fallback;
    if (lost && back instanceof HTMLElement && back.isConnected) {
      back.focus();
    }
  });
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && props.onEscape !== undefined) {
      event.preventDefault();
      props.onEscape();
    } else if (event.key === "Tab" && dialog !== undefined) {
      const stops = tabbables(dialog);
      const [first, last] = [stops[0], stops[stops.length - 1]];
      const at = document.activeElement;
      if (first === undefined || last === undefined) {
        event.preventDefault();
      } else if (event.shiftKey && (at === first || !dialog.contains(at))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (at === last || !dialog.contains(at))) {
        event.preventDefault();
        first.focus();
      }
    }
  };
  return (
    <div class="backdrop" onClick={(event) => event.target === event.currentTarget && props.onBackdrop?.()}>
      <div
        role={props.role ?? "dialog"}
        aria-modal="true"
        aria-labelledby={props.labelledby}
        aria-describedby={props.describedby}
        class={props.class}
        ref={(element) => {
          dialog = element;
          openers.set(element, fallback);
          props.ref?.(element);
        }}
        onKeyDown={onKeyDown}
      >
        {props.children}
      </div>
    </div>
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
  return (
    <Modal labelledby={`${props.id}-title`} class={`dialog op-dialog ${props.class ?? ""}`} onEscape={props.onClose}>
      <h2 id={`${props.id}-title`}>{props.title}</h2>
      {props.children}
    </Modal>
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
    <Modal
      role="alertdialog"
      labelledby="op-confirm-title"
      describedby="op-confirm-text"
      class="dialog op-dialog"
      onEscape={props.onClose}
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
    </Modal>
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
    <Modal
      role="alertdialog"
      labelledby="question-title"
      describedby="question-text"
      class="dialog op-dialog"
      onEscape={props.onClose}
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
    </Modal>
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
