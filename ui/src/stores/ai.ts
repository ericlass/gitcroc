// The AI settings and providers as the core reports them (ARCH-6): the store mirrors `ai_providers_get` and the
// `ai_changed`, `ai_models` and `ai_self_test` events, and sends what the user changed. The core decides, saves
// and keeps keys; nothing here holds a key once it is sent (SEC-1). One store serves every tab, so a change of AI
// features reaches all of them at once (AI-16). It also asks the user's consent before a provider's first use
// (AI-6), with the text the core gives.
import { createStore } from "solid-js/store";
import {
  type AiOptions,
  type AiRequestId,
  type AiView,
  aiModelsGet,
  aiProviderConsent,
  aiProviderDelete,
  aiProviderSave,
  aiProvidersGet,
  aiProviderSetDefault,
  aiSelfTest,
  aiSettingsSet,
  type ConsentRequest,
  core,
  type ModelsResult,
  type ProviderDraft,
  type ProviderId,
  type SelfTest,
} from "../ipc";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** What `ConsentTarget` names: the host or the program's path. */
export const consentTargetValue = (request: ConsentRequest) =>
  request.to.kind === "host" ? request.to.host : request.to.path;

export interface AiState {
  /** `null` until the first read. */
  view: AiView | null;
  /** Why the last read or change failed. */
  error: string | null;
  /** The models each `ai_models_get` brought, by request id; kept from before its id is known. */
  models: Record<AiRequestId, ModelsResult>;
  /** The result of each `ai_self_test`, by request id. */
  tests: Record<AiRequestId, SelfTest>;
  /** The consent the user is asked for now (AI-6). */
  consent: { request: ConsentRequest; error: string | null; sending: boolean } | null;
}

export function createAiStore() {
  const [state, setState] = createStore<AiState>({ view: null, error: null, models: {}, tests: {}, consent: null });
  /** What answers the open consent question. */
  let answer: ((allowed: boolean) => void) | null = null;

  const finishConsent = (allowed: boolean) => {
    const done = answer;
    answer = null;
    setState("consent", null);
    done?.(allowed);
  };

  /** Keeps `view`; turning AI features off closes the consent question unanswered (AI-16). */
  const record = (view: AiView) => {
    setState({ view, error: null });
    if (!view.enabled && answer !== null) {
      finishConsent(false);
    }
  };

  /** Sends a change; the view it answers with is shown at once. A refusal is thrown for the form to show. */
  const change = async (request: Promise<AiView>) => {
    const view = await request;
    record(view);
    return view;
  };

  /**
   * Starts recording the AI events, then reads the view; resolves to the unsubscribe function. A view heard
   * before the read finishes is newer than the read, so it wins.
   */
  const subscribe = async () => {
    let heard: AiView | null = null;
    let loaded = false;
    const stops = await Promise.all([
      core.events.aiChanged.listen((event) => (loaded ? record(event.payload) : (heard = event.payload))),
      core.events.aiModels.listen((event) => setState("models", event.payload.request_id, event.payload.result)),
      core.events.aiSelfTest.listen((event) => setState("tests", event.payload.request_id, event.payload.result)),
    ]);
    const unlisten = () => stops.forEach((stop) => stop());
    try {
      const read = await aiProvidersGet();
      loaded = true;
      record(heard ?? read);
    } catch (error) {
      setState("error", message(error));
    }
    return unlisten;
  };

  /** The options as they are, with `change` applied. */
  const options = (change: Partial<AiOptions>): AiOptions => ({
    enabled: state.view?.enabled ?? true,
    exclude: state.view?.exclude ?? [],
    debug_log: state.view?.debug_log ?? false,
    ...change,
  });

  return {
    state,
    subscribe,
    /** AI features (AI-16): every AI element shows only while on, and not before the core said so. */
    enabled: () => state.view?.enabled === true,
    provider: (id: ProviderId) => state.view?.providers.find((provider) => provider.id === id),
    /** Saves a provider; `key` `null` keeps the stored key, `""` removes it. */
    save: (draft: ProviderDraft, key: string | null) => change(aiProviderSave(draft, key)),
    remove: (id: ProviderId) => change(aiProviderDelete(id)),
    setDefault: (id: ProviderId) => change(aiProviderSetDefault(id)),
    /** Changes some of AI features, the exclude list and AI debug logging; a refusal shows in `error`. */
    setOptions: async (change: Partial<AiOptions>) => {
      try {
        record(await aiSettingsSet(options(change)));
      } catch (error) {
        setState("error", message(error));
      }
    },
    /** Starts listing the models of `draft`; they arrive in `state.models` under the returned id. */
    fetchModels: (draft: ProviderDraft, key: string | null) => aiModelsGet(draft, key),
    /** Starts testing `draft`; the result arrives in `state.tests` under the returned id. */
    test: (draft: ProviderDraft, key: string | null) => aiSelfTest(draft, key),
    /**
     * Whether provider `id` may be used now: at once when the user consented to where it sends, otherwise after
     * the consent dialog. `false` while AI features are off, when no dialog opens (AI-16), and on Cancel, which
     * sends nothing.
     */
    askConsent: (id: ProviderId): Promise<boolean> => {
      const request = state.view?.enabled ? state.view.providers.find((known) => known.id === id) : undefined;
      if (request === undefined) {
        return Promise.resolve(false);
      }
      if (request.consent_needed === null) {
        return Promise.resolve(true);
      }
      finishConsent(false);
      setState("consent", { request: request.consent_needed, error: null, sending: false });
      return new Promise((resolve) => (answer = resolve));
    },
    /** Allow: the core keeps the consent to what the dialog showed, then the use goes on. */
    allow: async () => {
      const open = state.consent;
      if (open === null) {
        return;
      }
      setState("consent", { ...open, error: null, sending: true });
      try {
        record(await aiProviderConsent(open.request.provider_id, consentTargetValue(open.request)));
        finishConsent(true);
      } catch (error) {
        if (state.consent !== null) {
          setState("consent", { ...open, error: message(error), sending: false });
        }
      }
    },
    /** Cancel: nothing is sent. */
    cancel: () => finishConsent(false),
  };
}

export type AiStore = ReturnType<typeof createAiStore>;
