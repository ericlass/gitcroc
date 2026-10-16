// The AI section of Settings. First the AI features switch (AI-16): off, it is all the section shows. On, the
// providers with the default marked (AI-1), Add, Edit, Remove and Set as default, the CLI the core found and
// offers (AI-2), the exclude list (AI-4) and AI debug logging (AI-7). Everything comes from the core's view.
import { createSignal, For, Show } from "solid-js";
import type { DetectedCli, ProviderView } from "../ipc";
import type { AiStore } from "../stores/ai";
import ProviderForm, { isCli, KIND_LABELS, selfTestText } from "./ProviderForm";

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
          <legend>AI</legend>
          <label class="choice">
            <input
              type="checkbox"
              checked={ai().enabled}
              onChange={(event) => void props.ai.setOptions({ enabled: event.currentTarget.checked })}
            />
            AI features
          </label>
          <Show
            when={ai().enabled}
            fallback={
              <p class="note">
                AI features are off: no AI request is sent and the AI buttons are hidden. Providers, keys and consent
                are kept for when you turn them on.
              </p>
            }
          >
            <Show when={ai().keychain_error}>
              {(reason) => <p role="alert">The keychain cannot be used, so keys cannot be stored: {reason()}</p>}
            </Show>
            <Show
              when={editing()}
              keyed
              fallback={
                <>
                  <h3 class="ai-heading">Providers</h3>
                  <Show when={ai().providers.length === 0}>
                    <p class="note">No AI provider yet.</p>
                  </Show>
                  <ul class="ai-providers" aria-label="AI providers">
                    <For each={ai().providers}>
                      {(provider) => (
                        <li class="ai-provider">
                          <div class="ai-provider-text">
                            <span class="ai-provider-name">{provider.name}</span>
                            <Show when={ai().default === provider.id}>
                              <span class="ai-provider-default"> (default)</span>
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
                                  aria-label={`Edit ${provider.name}`}
                                  onClick={() => setEditing({ provider })}
                                >
                                  Edit…
                                </button>
                                <button
                                  type="button"
                                  aria-label={`Set ${provider.name} as default`}
                                  disabled={ai().default === provider.id}
                                  onClick={() => send(props.ai.setDefault(provider.id))}
                                >
                                  Set as default
                                </button>
                                <button
                                  type="button"
                                  aria-label={`Remove ${provider.name}`}
                                  onClick={() => setRemoving(provider)}
                                >
                                  Remove…
                                </button>
                              </div>
                            }
                          >
                            <div class="ai-provider-actions" role="group" aria-label={`Remove ${provider.name}?`}>
                              <span>Remove {provider.name} and its key?</span>
                              <button
                                type="button"
                                ref={(element) => queueMicrotask(() => element.focus())}
                                onClick={() => setRemoving(null)}
                              >
                                Cancel
                              </button>
                              <button
                                type="button"
                                class="danger"
                                onClick={() => {
                                  setRemoving(null);
                                  send(props.ai.remove(provider.id));
                                }}
                              >
                                Remove
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
                        Found {KIND_LABELS[offer().kind]} at <code>{offer().path}</code>.{" "}
                        <button type="button" onClick={() => setEditing({ provider: null, start: offer() })}>
                          Add it…
                        </button>
                      </p>
                    )}
                  </Show>
                  <Show when={error()}>{(text) => <p role="alert">{text()}</p>}</Show>
                  <button type="button" onClick={() => setEditing({ provider: null })}>
                    Add provider…
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
              <span>Never send files matching (one pattern per line)</span>
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
              AI debug logging
            </label>
            <p class="note">
              While on, every prompt and reply is written to the log, which then holds repo content. Keys are never
              written.
            </p>
          </Show>
          <Show when={props.ai.state.error}>{(text) => <p role="alert">{text()}</p>}</Show>
        </fieldset>
      )}
    </Show>
  );
}
