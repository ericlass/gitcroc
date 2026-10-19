// The AI section of Settings. First the AI features switch (AI-16): off, it is all the section shows. On, the
// providers with the default marked (AI-1), Add, Edit, Remove and Set as default, the CLI the core found and
// offers (AI-2), the exclude list (AI-4) and AI debug logging (AI-7). Everything comes from the core's view.
import { createSignal, For, Show } from "solid-js";
import type { DetectedCli, ProviderView } from "../ipc";
import type { AiStore } from "../stores/ai";
import ProviderForm, { isCli, KIND_LABELS, selfTestText } from "./ProviderForm";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export default function AiSection(props: { ai: AiStore; readOnly: boolean }) {
  /** The form shown in place of the list: a provider to edit, or a new one. */
  const [editing, setEditing] = createSignal<{ provider: ProviderView | null; start?: DetectedCli } | null>(null);
  const [removing, setRemoving] = createSignal<ProviderView | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const view = () => props.ai.state.view;
  const send = (request: Promise<unknown>) => {
    setError(null);
    request.catch((failure) => setError(message(failure)));
  };

  return (
    <Show when={view()}>
      {(ai) => (
        <fieldset class="ai-settings" disabled={props.readOnly}>
          <legend>{strings.ai.legend}</legend>
          <label class="choice">
            <input
              type="checkbox"
              checked={ai().enabled}
              onChange={(event) => void props.ai.setOptions({ enabled: event.currentTarget.checked })}
            />
            {strings.ai.features}
          </label>
          <Show
            when={ai().enabled}
            fallback={
              <p class="note">{strings.ai.off}</p>
            }
          >
            <Show when={ai().keychain_error}>
              {(reason) => <p role="alert">{strings.ai.noKeychain(reason())}</p>}
            </Show>
            <Show
              when={editing()}
              keyed
              fallback={
                <>
                  <h3 class="ai-heading">{strings.ai.providers}</h3>
                  <Show when={ai().providers.length === 0}>
                    <p class="note">{strings.ai.noProvider}</p>
                  </Show>
                  <ul class="ai-providers" aria-label={strings.ai.providersLabel}>
                    <For each={ai().providers}>
                      {(provider) => (
                        <li class="ai-provider">
                          <div class="ai-provider-text">
                            <span class="ai-provider-name">{provider.name}</span>
                            <Show when={ai().default === provider.id}>
                              <span class="ai-provider-default"> {strings.ai.isDefault}</span>
                            </Show>
                            <span class="about">
                              {" "}
                              {KIND_LABELS[provider.kind]}
                              {provider.model === null ? "" : ` · ${provider.model}`}
                            </span>
                            <Show when={isCli(provider.kind) && provider.check}>
                              {(check) => <span class="about"> · {selfTestText(check())}</span>}
                            </Show>
                          </div>
                          <Show
                            when={removing()?.id === provider.id}
                            fallback={
                              <div class="ai-provider-actions">
                                <button
                                  type="button"
                                  aria-label={strings.ai.editLabel(provider.name)}
                                  onClick={() => setEditing({ provider })}
                                >
                                  {strings.ai.edit}
                                </button>
                                <button
                                  type="button"
                                  aria-label={strings.ai.setDefaultLabel(provider.name)}
                                  disabled={ai().default === provider.id}
                                  onClick={() => send(props.ai.setDefault(provider.id))}
                                >
                                  {strings.ai.setDefault}
                                </button>
                                <button
                                  type="button"
                                  aria-label={strings.ai.removeLabel(provider.name)}
                                  onClick={() => setRemoving(provider)}
                                >
                                  {strings.ai.remove}
                                </button>
                              </div>
                            }
                          >
                            <div
                              class="ai-provider-actions"
                              role="group"
                              aria-label={strings.ai.removeQuestion(provider.name)}
                            >
                              <span>{strings.ai.removeWithKey(provider.name)}</span>
                              <button
                                type="button"
                                ref={(element) => queueMicrotask(() => element.focus())}
                                onClick={() => setRemoving(null)}
                              >
                                {strings.common.cancel}
                              </button>
                              <button
                                type="button"
                                class="danger"
                                onClick={() => {
                                  setRemoving(null);
                                  send(props.ai.remove(provider.id));
                                }}
                              >
                                {strings.ai.removeConfirm}
                              </button>
                            </div>
                          </Show>
                        </li>
                      )}
                    </For>
                  </ul>
                  <Show when={ai().offer}>
                    {(offer) => (
                      <p class="ai-offer">
                        {strings.ai.found(KIND_LABELS[offer().kind])}
                        <code>{offer().path}</code>.{" "}
                        <button type="button" onClick={() => setEditing({ provider: null, start: offer() })}>
                          {strings.ai.addIt}
                        </button>
                      </p>
                    )}
                  </Show>
                  <Show when={error()}>{(text) => <p role="alert">{text()}</p>}</Show>
                  <button type="button" onClick={() => setEditing({ provider: null })}>
                    {strings.ai.addProvider}
                  </button>
                </>
              }
            >
              {(open) => (
                <ProviderForm
                  ai={props.ai}
                  provider={open.provider}
                  start={open.start}
                  readOnly={props.readOnly}
                  onDone={() => setEditing(null)}
                />
              )}
            </Show>
            <label class="field ai-exclude">
              <span>{strings.ai.exclude}</span>
              <textarea
                rows={4}
                spellcheck={false}
                value={ai().exclude.join("\n")}
                onChange={(event) => void props.ai.setOptions({ exclude: event.currentTarget.value.split("\n") })}
              />
            </label>
            <label class="choice">
              <input
                type="checkbox"
                checked={ai().debug_log}
                onChange={(event) => void props.ai.setOptions({ debug_log: event.currentTarget.checked })}
              />
              {strings.ai.debugLog}
            </label>
            <p class="note">{strings.ai.debugLogNote}</p>
          </Show>
          <Show when={props.ai.state.error}>{(text) => <p role="alert">{text()}</p>}</Show>
        </fieldset>
      )}
    </Show>
  );
}
