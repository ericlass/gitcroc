// The consent dialog (AI-6): before a provider's first use, and again when its host or program changed, it says
// what is sent for each file (AI-5) and to which host or program, in the core's words. Allow keeps the consent
// and goes on; Cancel sends nothing. Cancel has the focus and Escape cancels. Text only (SEC-3).
import { For, onMount, Show } from "solid-js";
import type { AiStore } from "../stores/ai";
import { consentTargetValue } from "../stores/ai";

export default function ConsentDialog(props: { ai: AiStore }) {
  return (
    <Show when={props.ai.state.consent}>
      {(open) => {
        let cancel: HTMLButtonElement | undefined;
        onMount(() => cancel?.focus());
        const request = () => open().request;
        const where = () => consentTargetValue(request());
        return (
          <div class="backdrop">
            <div
              role="alertdialog"
              aria-modal="true"
              aria-labelledby="consent-title"
              aria-describedby="consent-text"
              class="dialog op-dialog consent-dialog"
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  props.ai.cancel();
                }
              }}
            >
              <h2 id="consent-title">Send to {where()}?</h2>
              <div id="consent-text">
                <p>
                  {request().to.kind === "host" ? (
                    <>
                      The AI provider {request().provider_name} sends to the host <code>{where()}</code>
                    </>
                  ) : (
                    <>
                      The AI provider {request().provider_name} runs the program <code>{where()}</code> and passes it
                    </>
                  )}{" "}
                  this for each file it resolves, and nothing else:
                </p>
                <ul class="consent-sent">
                  <For each={request().sent}>{(item) => <li>{item}</li>}</For>
                </ul>
                <p class="note">You are asked again if the provider's host or program changes.</p>
              </div>
              <Show when={open().error}>{(error) => <p role="alert">{error()}</p>}</Show>
              <div class="dialog-actions">
                <button type="button" ref={(element) => (cancel = element)} onClick={() => props.ai.cancel()}>
                  Cancel
                </button>
                <button
                  type="button"
                  class="primary"
                  disabled={open().sending}
                  onClick={() => void props.ai.allow()}
                >
                  Allow
                </button>
              </div>
            </div>
          </div>
        );
      }}
    </Show>
  );
}
