// The parts the operation dialogs share (branches, sync): a modal frame,
// the Cancel and submit buttons, the core's refusal, and the confirmation
// the core asks for before a destructive operation (GRP-9). Names are shown
// as text only (SEC-3).
import { For, type JSX, onMount, Show } from "solid-js";

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
        Cancel
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
        <h2 id="op-confirm-title">Cannot be undone in {props.repoName}</h2>
        <p id="op-confirm-text">This will:</p>
        <ul class="op-actions">
          <For each={props.actions}>{(action) => <li>{action}</li>}</For>
        </ul>
        <Refusal error={props.error} />
        <div class="dialog-actions">
          <button type="button" ref={(element) => (cancel = element)} onClick={() => props.onClose()}>
            Cancel
          </button>
          <button type="button" class="danger" disabled={props.sending} onClick={() => props.onConfirm()}>
            {props.submit}
          </button>
        </div>
      </div>
    </div>
  );
}
