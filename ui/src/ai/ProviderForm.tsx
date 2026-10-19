// One AI provider's form in Settings (AI-1, AI-2, AI-10): the fields its kind needs (section 7 "Providers"), the
// context budget, models fetched from the provider or typed, and Test. The key field is write-only: it says
// "Key stored" and never shows a key, and can replace or remove it (SEC-1). The core checks and saves; this shows
// what it says.
import { createSignal, For, Match, Show, Switch, untrack } from "solid-js";
import type {
  AiRequestId,
  DetectedCli,
  ModelsResult,
  ProviderDraft,
  ProviderKind,
  ProviderView,
  SelfTest,
} from "../ipc";
import type { AiStore } from "../stores/ai";
import { strings } from "../strings/en";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export const KIND_LABELS: Record<ProviderKind, string> = strings.ai.kinds;

export const isCli = (kind: ProviderKind) => kind === "claude_code" || kind === "codex";

/** The kinds that call an API, which may have a key and need a model; the CLI kinds and the mock take neither. */
const isApi = (kind: ProviderKind) => kind === "anthropic" || kind === "openai";

/** A self-test's state in words (AI-2). */
export function selfTestText(test: SelfTest): string {
  switch (test.kind) {
    case "running":
      return strings.ai.testing;
    case "passed":
      return test.version === null ? strings.ai.works : strings.ai.worksWith(test.version);
    case "missing_flags":
      return strings.ai.lacks(test.version, test.missing.join(", "));
    case "failed":
      return strings.ai.failed(test.reason);
  }
}

export default function ProviderForm(props: {
  ai: AiStore;
  /** The provider to edit; `null` adds one. */
  provider: ProviderView | null;
  /** A detected CLI to add, from the core's offer. */
  start?: DetectedCli;
  readOnly: boolean;
  onDone: () => void;
}) {
  // The form starts from the provider as it was when opened; the core's view may change meanwhile.
  const { saved, start } = untrack(() => ({ saved: props.provider, start: props.start }));
  const [kind, setKind] = createSignal<ProviderKind>(saved?.kind ?? start?.kind ?? "anthropic");
  const [name, setName] = createSignal(saved?.name ?? (start ? KIND_LABELS[start.kind] : ""));
  const [model, setModel] = createSignal(saved?.model ?? "");
  const [baseUrl, setBaseUrl] = createSignal(saved?.base_url ?? "");
  const [path, setPath] = createSignal(saved?.path ?? start?.path ?? "");
  const [budget, setBudget] = createSignal(saved === null ? "" : String(saved.context_budget));
  /** A key typed now; it is sent once and not kept after the form closes. */
  const [key, setKey] = createSignal("");
  const [replacing, setReplacing] = createSignal(false);
  const [removeKey, setRemoveKey] = createSignal(false);
  const [modelsRequest, setModelsRequest] = createSignal<AiRequestId | null>(null);
  const [testRequest, setTestRequest] = createSignal<AiRequestId | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);

  const hasKey = () => saved?.has_key === true && saved.kind === kind() && !removeKey();

  const draft = (): ProviderDraft => {
    const budgetValue = Number.parseInt(budget(), 10);
    return {
      id: saved?.id ?? null,
      name: name(),
      kind: kind(),
      model: model().trim() === "" ? null : model(),
      base_url: kind() === "openai" ? baseUrl() : null,
      path: isCli(kind()) ? path() : null,
      context_budget: Number.isNaN(budgetValue) ? null : budgetValue,
    };
  };

  /** What the core gets for the key: `null` keeps the stored one, `""` removes it. */
  const keyToSend = (): string | null => {
    if (!isApi(kind())) {
      return null;
    }
    if (removeKey()) {
      return "";
    }
    return key().trim() === "" ? null : key();
  };

  const models = (): ModelsResult | undefined => {
    const id = modelsRequest();
    return id === null ? undefined : props.ai.state.models[id];
  };
  const modelsFailure = () => {
    const result = models();
    return result?.kind === "failed" ? result.reason : null;
  };
  /** The test run here, else a CLI provider's check from the core. */
  const testResult = (): SelfTest | null => {
    const id = testRequest();
    if (id !== null) {
      return props.ai.state.tests[id] ?? { kind: "running" };
    }
    return saved !== null && saved.kind === kind() ? saved.check : null;
  };
  /** The offered models, with the chosen one when the list lacks it; none is preselected (AI-1). */
  const offered = () => {
    const result = models();
    if (result?.kind !== "offered") {
      return null;
    }
    const current = model().trim();
    return current === "" || result.models.includes(current) ? result.models : [current, ...result.models];
  };
  const detected = () => (props.ai.state.view?.detected ?? []).filter((cli) => cli.kind === kind());

  /** Waits for `work`; its refusal shows below the form. */
  const run = async (work: Promise<unknown>) => {
    setSending(true);
    try {
      await work;
    } catch (failure) {
      setError(message(failure));
    } finally {
      setSending(false);
    }
  };

  const onFetchModels = () => {
    setError(null);
    setModelsRequest(null);
    return run(props.ai.fetchModels(draft(), keyToSend()).then(setModelsRequest));
  };
  const onTest = () => {
    setError(null);
    setTestRequest(null);
    return run(props.ai.test(draft(), keyToSend()).then(setTestRequest));
  };
  const onSave = (event: SubmitEvent) => {
    event.preventDefault();
    setError(null);
    void run(
      props.ai.save(draft(), keyToSend()).then(() => {
        setKey("");
        props.onDone();
      }),
    );
  };

  const modelField = () => (
    <label class="field">
      <span>{isApi(kind()) ? strings.ai.model : strings.ai.modelOptional}</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck={false}
        value={model()}
        onInput={(event) => setModel(event.currentTarget.value)}
      />
    </label>
  );

  return (
    <form
      class="provider-form"
      aria-label={saved === null ? strings.ai.newProvider : strings.ai.provider(saved.name)}
      onSubmit={onSave}
    >
      <Show when={saved === null}>
        <label class="field">
          <span>{strings.ai.kind}</span>
          <select
            value={kind()}
            onChange={(event) => {
              setKind(event.currentTarget.value as ProviderKind);
              setModelsRequest(null);
              setTestRequest(null);
            }}
          >
            <For each={props.ai.state.view?.kinds ?? []}>
              {(option) => <option value={option}>{KIND_LABELS[option]}</option>}
            </For>
          </select>
        </label>
      </Show>
      <Show when={saved !== null}>
        <p class="about">{KIND_LABELS[kind()]}</p>
      </Show>
      <label class="field">
        <span>{strings.ai.name}</span>
        <input type="text" autocomplete="off" value={name()} onInput={(event) => setName(event.currentTarget.value)} />
      </label>
      <Show when={kind() === "openai"}>
        <label class="field">
          <span>{strings.ai.baseUrl}</span>
          <input
            type="url"
            autocomplete="off"
            spellcheck={false}
            placeholder={strings.ai.baseUrlPlaceholder}
            value={baseUrl()}
            onInput={(event) => setBaseUrl(event.currentTarget.value)}
          />
        </label>
      </Show>
      <Show when={isApi(kind())}>
        <div class="provider-key" role="group" aria-label={strings.ai.apiKey}>
          <Show
            when={hasKey() && !replacing()}
            fallback={
              <label class="field">
                <span>{kind() === "openai" ? strings.ai.apiKeyOptional : strings.ai.apiKey}</span>
                <input
                  type="password"
                  autocomplete="off"
                  spellcheck={false}
                  value={key()}
                  onInput={(event) => {
                    setKey(event.currentTarget.value);
                    setRemoveKey(false);
                  }}
                />
              </label>
            }
          >
            <span class="provider-key-state">{strings.ai.keyStored}</span>
            <button type="button" onClick={() => setReplacing(true)}>
              {strings.ai.replaceKey}
            </button>
            <button
              type="button"
              onClick={() => {
                setRemoveKey(true);
                setKey("");
              }}
            >
              {strings.ai.removeKey}
            </button>
          </Show>
          <Show when={removeKey()}>
            <p class="note">{strings.ai.keyRemovedOnSave}</p>
          </Show>
        </div>
      </Show>
      <Show when={isCli(kind())}>
        <Show when={detected().length > 0}>
          <fieldset class="provider-detected">
            <legend>{strings.ai.programsFound}</legend>
            <For each={detected()}>
              {(cli) => (
                <label class="choice">
                  <input
                    type="radio"
                    name="provider-detected"
                    checked={path() === cli.path}
                    onChange={() => setPath(cli.path)}
                  />
                  <code>{cli.path}</code>
                </label>
              )}
            </For>
          </fieldset>
        </Show>
        <label class="field">
          <span>{strings.ai.program}</span>
          <input
            type="text"
            autocomplete="off"
            spellcheck={false}
            value={path()}
            onInput={(event) => setPath(event.currentTarget.value)}
          />
        </label>
      </Show>
      <Switch fallback={modelField()}>
        <Match when={offered()}>
          {(list) => (
            <label class="field">
              <span>{strings.ai.model}</span>
              <select value={model()} onChange={(event) => setModel(event.currentTarget.value)}>
                <option value="" disabled>
                  {strings.ai.pickModel}
                </option>
                <For each={list()}>{(option) => <option value={option}>{option}</option>}</For>
              </select>
            </label>
          )}
        </Match>
        <Match when={modelsRequest() !== null && models() === undefined}>
          {modelField()}
          <p class="note" role="status">
            {strings.ai.fetchingModels}
          </p>
        </Match>
        <Match when={models()?.kind === "not_offered"}>
          {modelField()}
          <p class="note">{strings.ai.noModelList}</p>
        </Match>
        <Match when={modelsFailure()}>
          {(reason) => (
            <>
              {modelField()}
              <p role="alert">{strings.ai.cannotFetchModels(reason())}</p>
            </>
          )}
        </Match>
      </Switch>
      <label class="field">
        <span>{strings.ai.contextBudget}</span>
        <input
          type="number"
          min="1000"
          max="10000000"
          placeholder="100000"
          value={budget()}
          onInput={(event) => setBudget(event.currentTarget.value)}
        />
      </label>
      <Show when={testResult()}>
        {(result) => (
          <p class="provider-test" role="status">
            {strings.ai.testResult(selfTestText(result()))}
          </p>
        )}
      </Show>
      <Show when={error()}>{(text) => <p role="alert">{text()}</p>}</Show>
      <div class="dialog-actions provider-actions">
        <Show when={isApi(kind())}>
          <button type="button" disabled={sending()} onClick={() => void onFetchModels()}>
            {strings.ai.fetchModels}
          </button>
        </Show>
        <button type="button" disabled={sending()} onClick={() => void onTest()}>
          {strings.ai.test}
        </button>
        <span class="spacer" />
        <button type="button" onClick={() => props.onDone()}>
          {strings.common.cancel}
        </button>
        <button type="submit" class="primary" disabled={sending() || props.readOnly}>
          {strings.ai.save}
        </button>
      </div>
    </form>
  );
}
