// AI providers for the fake core (fake.ts): the settings, providers, keys, models, self-tests and consent of the
// core's `ai_*` commands, simplified. Keys live in a map that stands in for the keychain and never reach a view
// (SEC-1). Test support only; the real rules are the core's.
import type {
  AiOptions,
  AiView,
  ConsentRequest,
  ConsentTarget,
  DetectedCli,
  IpcError,
  ModelsResult,
  ProviderDraft,
  ProviderId,
  ProviderView,
  SelfTest,
} from "./bindings";

/** A provider as the settings file keeps it: its view without what the core works out. */
export type FakeProvider = Omit<ProviderView, "has_key" | "check" | "consent_needed">;

/** What is sent for each file (AI-5), as the core words it. */
const SENT = [
  "The file's path in the repository",
  "The operation, such as merge, rebase, cherry-pick, revert or stash apply",
  "The labels of the two sides, such as the branch names",
  "Each conflicting part: its base, ours and theirs text, with the 40 lines around it",
  "The messages of up to 10 commits per side that changed the file, each cut at 2,000 characters",
];

export const fakeAi: {
  enabled: boolean;
  exclude: string[];
  debugLog: boolean;
  default: ProviderId | null;
  providers: FakeProvider[];
  /** The fake keychain, by provider id. */
  keys: Record<ProviderId, string>;
  detected: DetectedCli[];
  /** The CLI providers' self-tests by id; the others have none. */
  checks: Record<ProviderId, SelfTest>;
  /** What `ai_models_get` answers. */
  models: ModelsResult;
  /** What `ai_self_test` answers. */
  selfTest: SelfTest;
  /** Every draft and key sent to `ai_models_get` and `ai_self_test`, oldest first. */
  requests: { command: "models" | "self_test"; draft: ProviderDraft; key: string | null }[];
} = {
  enabled: true,
  exclude: [],
  debugLog: false,
  default: null,
  providers: [],
  keys: {},
  detected: [],
  checks: {},
  models: { kind: "not_offered" },
  selfTest: { kind: "passed", version: null },
  requests: [],
};

export const DEFAULT_EXCLUDE = [".env*", "*.pem", "*.key", "*.p12", "id_rsa*"];

export function resetFakeAi(): void {
  fakeAi.enabled = true;
  fakeAi.exclude = [...DEFAULT_EXCLUDE];
  fakeAi.debugLog = false;
  fakeAi.default = null;
  fakeAi.providers = [];
  fakeAi.keys = {};
  fakeAi.detected = [];
  fakeAi.checks = {};
  fakeAi.models = { kind: "not_offered" };
  fakeAi.selfTest = { kind: "passed", version: null };
  fakeAi.requests = [];
}
resetFakeAi();

/** `record` without `id`. */
const without = <T>(record: Record<ProviderId, T>, id: ProviderId) =>
  Object.fromEntries(Object.entries(record).filter(([known]) => known !== id));

const isCli = (kind: ProviderDraft["kind"]) => kind === "claude_code" || kind === "codex";

/** The host of an http or https URL, with its port and without a user name, as the core reads it. */
function urlHost(url: string): string | null {
  const match = /^https?:\/\/([^/?#]*)/i.exec(url.trim());
  const host = match?.[1]?.split("@").pop()?.toLowerCase() ?? "";
  return host === "" ? null : host;
}

function consentTarget(provider: FakeProvider): ConsentTarget | null {
  if (isCli(provider.kind)) {
    return provider.path === null ? null : { kind: "program", path: provider.path };
  }
  const host = urlHost(provider.kind === "anthropic" ? "https://api.anthropic.com" : (provider.base_url ?? ""));
  return host === null ? null : { kind: "host", host };
}

const targetValue = (target: ConsentTarget) => (target.kind === "host" ? target.host : target.path);

function consentNeeded(provider: FakeProvider): ConsentRequest | null {
  const to = consentTarget(provider);
  if (to === null || provider.consent === targetValue(to)) {
    return null;
  }
  return { provider_id: provider.id, provider_name: provider.name, to, sent: [...SENT] };
}

/** The view `ai_providers_get` returns: never a key, only whether one is stored. */
export function fakeAiView(): AiView {
  const configured = new Set(fakeAi.providers.map((provider) => provider.path));
  return {
    enabled: fakeAi.enabled,
    exclude: [...fakeAi.exclude],
    debug_log: fakeAi.debugLog,
    default: fakeAi.default,
    providers: fakeAi.providers.map((provider) => ({
      ...provider,
      has_key: provider.id in fakeAi.keys,
      check: fakeAi.enabled ? (fakeAi.checks[provider.id] ?? null) : null,
      consent_needed: consentNeeded(provider),
    })),
    detected: fakeAi.enabled ? [...fakeAi.detected] : [],
    offer: fakeAi.enabled ? (fakeAi.detected.find((cli) => !configured.has(cli.path)) ?? null) : null,
    keychain_error: null,
  };
}

type Answer<T> = { status: "ok"; data: T } | { status: "error"; error: IpcError };

const ok = <T>(data: T): Answer<T> => ({ status: "ok", data });
const refused = <T>(message: string): Answer<T> => ({
  status: "error",
  error: { code: "refused", message, git_stderr: null },
});
const aiOff = <T>(): Answer<T> => ({
  status: "error",
  error: { code: "ai_off", message: "AI features are off", git_stderr: null },
});

/** `text` trimmed, or `null` when that leaves nothing. */
const filled = (text: string | null) => (text?.trim() ? text.trim() : null);

/** Why the core refuses `draft`, or `null`. */
function draftRefusal(draft: ProviderDraft): string | null {
  if (draft.name.trim() === "") {
    return "a provider needs a name";
  }
  if (!isCli(draft.kind) && filled(draft.model) === null) {
    return `an ${draft.kind === "anthropic" ? "Anthropic" : "OpenAI-compatible"} provider needs a model`;
  }
  return connectionRefusal(draft);
}

/** Why the provider cannot be reached as `draft` says, or `null`. */
function connectionRefusal(draft: ProviderDraft): string | null {
  if (draft.kind === "openai" && urlHost(draft.base_url ?? "") === null) {
    return "the base URL must start with http:// or https:// and name a host";
  }
  if (isCli(draft.kind) && filled(draft.path) === null) {
    return "a CLI provider needs the path of its program";
  }
  const budget = draft.context_budget;
  if (budget !== null && (budget < 1_000 || budget > 10_000_000)) {
    return "the context budget is 1000 to 10000000 tokens";
  }
  return null;
}

/** The fake's `ai_*` commands; each change calls `changed` with the new view, as the core sends `ai_changed`. */
export function fakeAiCommands(events: {
  changed: (view: AiView) => void;
  models: (requestId: number, result: ModelsResult) => void;
  selfTest: (requestId: number, result: SelfTest) => void;
}) {
  let lastRequest = 0;
  const changed = () => {
    const view = fakeAiView();
    events.changed(view);
    return ok(view);
  };
  /** Answers after the request id has gone back, as the core's background task does. */
  const later = (send: () => void) => void Promise.resolve().then(send);

  return {
    aiProvidersGet: () => Promise.resolve(ok(fakeAiView())),
    aiProviderSave: (draft: ProviderDraft, key: string | null) => {
      const refusal = draftRefusal(draft);
      if (refusal !== null) {
        return Promise.resolve(refused<AiView>(refusal));
      }
      const old = fakeAi.providers.find((provider) => provider.id === draft.id);
      if (draft.id !== null && old === undefined) {
        return Promise.resolve(refused<AiView>(`there is no provider ${draft.id}`));
      }
      if (isCli(draft.kind) && filled(key) !== null) {
        return Promise.resolve(refused<AiView>("CLI providers use their own login and take no API key"));
      }
      const used = fakeAi.providers.map((provider) => Number.parseInt(provider.id.slice(1), 10));
      const id = draft.id ?? `p${Math.max(0, ...used) + 1}`;
      const keeps = old !== undefined && key === null && !isCli(draft.kind) && id in fakeAi.keys;
      const hasKey = key === null ? keeps : key.trim() !== "";
      if (draft.kind === "anthropic" && !hasKey) {
        return Promise.resolve(refused<AiView>("an Anthropic provider needs an API key"));
      }
      if (key !== null && key.trim() !== "") {
        fakeAi.keys[id] = key;
      } else if (!hasKey) {
        fakeAi.keys = without(fakeAi.keys, id);
      }
      const entry: FakeProvider = {
        id,
        name: draft.name.trim(),
        kind: draft.kind,
        model: filled(draft.model),
        base_url: draft.kind === "openai" ? filled(draft.base_url) : null,
        path: isCli(draft.kind) ? filled(draft.path) : null,
        context_budget: draft.context_budget ?? 100_000,
        consent: old?.consent ?? null,
      };
      fakeAi.providers = old
        ? fakeAi.providers.map((provider) => (provider.id === id ? entry : provider))
        : [...fakeAi.providers, entry];
      if (isCli(entry.kind) && fakeAi.enabled) {
        fakeAi.checks[id] = fakeAi.selfTest;
      } else {
        fakeAi.checks = without(fakeAi.checks, id);
      }
      return Promise.resolve(changed());
    },
    aiProviderDelete: (id: ProviderId) => {
      if (!fakeAi.providers.some((provider) => provider.id === id)) {
        return Promise.resolve(refused<AiView>(`there is no provider ${id}`));
      }
      fakeAi.providers = fakeAi.providers.filter((provider) => provider.id !== id);
      fakeAi.keys = without(fakeAi.keys, id);
      fakeAi.checks = without(fakeAi.checks, id);
      fakeAi.default = fakeAi.default === id ? null : fakeAi.default;
      return Promise.resolve(changed());
    },
    aiProviderSetDefault: (id: ProviderId) => {
      if (!fakeAi.providers.some((provider) => provider.id === id)) {
        return Promise.resolve(refused<AiView>(`there is no provider ${id}`));
      }
      fakeAi.default = id;
      return Promise.resolve(changed());
    },
    aiSettingsSet: (options: AiOptions) => {
      fakeAi.enabled = options.enabled;
      fakeAi.exclude = options.exclude.map((pattern) => pattern.trim()).filter((pattern) => pattern !== "");
      fakeAi.debugLog = options.debug_log;
      return Promise.resolve(changed());
    },
    aiProviderConsent: (id: ProviderId, target: string) => {
      if (!fakeAi.enabled) {
        return Promise.resolve(aiOff<AiView>());
      }
      const provider = fakeAi.providers.find((known) => known.id === id);
      if (provider === undefined) {
        return Promise.resolve(refused<AiView>(`there is no provider ${id}`));
      }
      const to = consentTarget(provider);
      if (to === null || targetValue(to) !== target) {
        return Promise.resolve(
          refused<AiView>(`the provider no longer sends to ${target}; look at what it sends again`),
        );
      }
      fakeAi.providers = fakeAi.providers.map((known) => (known.id === id ? { ...known, consent: target } : known));
      return Promise.resolve(changed());
    },
    aiModelsGet: (draft: ProviderDraft, key: string | null) => {
      if (!fakeAi.enabled) {
        return Promise.resolve(aiOff<number>());
      }
      const refusal = connectionRefusal(draft);
      if (refusal !== null) {
        return Promise.resolve(refused<number>(refusal));
      }
      fakeAi.requests.push({ command: "models", draft, key });
      const requestId = ++lastRequest;
      const result: ModelsResult = isCli(draft.kind) ? { kind: "not_offered" } : fakeAi.models;
      later(() => events.models(requestId, result));
      return Promise.resolve(ok(requestId));
    },
    aiSelfTest: (draft: ProviderDraft, key: string | null) => {
      if (!fakeAi.enabled) {
        return Promise.resolve(aiOff<number>());
      }
      const refusal = connectionRefusal(draft);
      if (refusal !== null) {
        return Promise.resolve(refused<number>(refusal));
      }
      fakeAi.requests.push({ command: "self_test", draft, key });
      const requestId = ++lastRequest;
      const result = fakeAi.selfTest;
      later(() => events.selfTest(requestId, result));
      return Promise.resolve(ok(requestId));
    },
  };
}
