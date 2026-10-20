// The consent dialog (AI-6): before a provider's first use, and again when its host or program changed, it says
// what is sent for each file (AI-5) and to which host or program, in the core's words. Allow keeps the consent
// and goes on; Cancel sends nothing. Cancel has the focus and Escape cancels. Text only (SEC-3).
import { For, onMount, Show } from "solid-js";
import type { AiStore } from "../stores/ai";
import { consentTargetValue } from "../stores/ai";
import { Modal } from "../dialogs/parts";
import { strings } from "../strings/en";

export default function ConsentDialog(props: { ai: AiStore }) {
  return (
    <Show when={props.ai.state.consent}>
      {(open) => {
        let cancel: HTMLButtonElement | undefined;
        onMount(() => cancel?.focus());
        const request = () => open().request;
        const where = () => consentTargetValue(request());
        return (
          <Modal
            role="alertdialog"
            labelledby="consent-title"
            describedby="consent-text"
            class="dialog op-dialog consent-dialog"
            onEscape={() => props.ai.cancel()}
          >
            <h2 id="consent-title">{strings.ai.consent.title(where())}</h2>
            <div id="consent-text">
              <p>
                {request().to.kind === "host" ? (
                  <>
                    {strings.ai.consent.sendsToHost(request().provider_name)}
                    <code>{where()}</code>
                  </>
                ) : (
                  <>
                    {strings.ai.consent.runsProgram(request().provider_name)}
                    <code>{where()}</code>
                    {strings.ai.consent.passesIt}
                  </>
                )}{" "}
                {strings.ai.consent.whatIsSent}
              </p>
              <ul class="consent-sent">
                <For each={request().sent}>{(item) => <li>{item}</li>}</For>
              </ul>
              <p class="note">{strings.ai.consent.askedAgain}</p>
            </div>
            <Show when={open().error}>{(error) => <p role="alert">{error()}</p>}</Show>
            <div class="dialog-actions">
              <button type="button" ref={(element) => (cancel = element)} onClick={() => props.ai.cancel()}>
                {strings.common.cancel}
              </button>
              <button type="button" class="primary" disabled={open().sending} onClick={() => void props.ai.allow()}>
                {strings.ai.consent.allow}
              </button>
            </div>
          </Modal>
        );
      }}
    </Show>
  );
}
