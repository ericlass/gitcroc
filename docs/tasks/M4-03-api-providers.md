# M4-03 API providers: Anthropic and OpenAI-compatible

## Goal
`gitcroc-ai` can call the Anthropic Messages API and any OpenAI-compatible Chat Completions endpoint, list the models each offers, and report token usage. It is tested against a local fake server, never a real provider.

## Depends on
M4-01

## Spec
Implements: D4 (API half; M4-04 adds the CLI providers)
Respects: AI-1, AI-7, AI-14, D11, D12, SEC-1, section 4 "Stack" (`reqwest` with rustls), section 7 "Providers"

## In scope
- `AnthropicProvider { api_key, model, base_url }`:
  - The base URL defaults to `https://api.anthropic.com`; tests point it at the fake server.
  - `POST /v1/messages` with streaming (server-sent events), `temperature: 0`, and a `max_tokens` from a constant sized for the reply.
  - The streamed text is put together into the reply. Usage comes from the `message_start` and `message_delta` events.
  - Cancelling drops the request. It is checked in tests with a server that holds the stream open until released.
- `OpenAiProvider { base_url, api_key: Option, model }`:
  - `POST {base_url}/chat/completions`, not streamed, with `temperature: 0`. Usage comes from `usage` when present.
  - A 400 reply that names `temperature` is retried once without it, and the provider remembers that for the session. Some local models refuse it.
- Model lists (AI-1): `list_models()` on both, from `GET /v1/models` and `GET {base_url}/models`. An endpoint without the list returns "not offered", and the user types the name (M4-06). No model name appears in the code, the tests' fake server excepted.
- Errors:
  - HTTP status with the provider's error message, connection failure and timeout are `ProviderError`s.
  - The request timeout is 5 minutes. It is not a setting, and no test waits for it.
  - Keys are sent only in headers (`x-api-key`; `Authorization: Bearer`). They never appear in errors, `Debug` output or logs (SEC-1); `api_key` is a type whose `Debug` prints `***`.
- Dependencies:
  - `reqwest` with `rustls-tls` and `stream`, and no default features.
  - A small `tokio`-based fake HTTP server in the crate's tests, with no new dependency.
  - Licenses that `cargo deny` reports for the TLS stack are added to `deny.toml` with a comment, as `notify`'s was.
- Tests (against the fake server):
  - `ai_1_anthropic_and_openai_list_models_from_the_provider`, and "not offered" on a 404.
  - Anthropic: the streamed reply is put together and its usage reported (`ai_14_usage_is_reported_when_the_provider_gives_it`); the request has temperature 0 and the key header.
  - OpenAI-compatible: the reply and usage; no `Authorization` header without a key; the temperature fallback.
  - `sec_1_key_never_in_errors_or_debug`: a 401 whose body echoes the key is reported with the key masked.
  - Cancel ends a held stream without a reply.

## Out of scope
- Storing keys and the provider list (M4-05). The CLI providers (M4-04).

## Acceptance
- The tests above exist and pass on all three platforms with no network access.
- `cargo deny check` passes, and `just check` passes.
- In the pull request, one manual call against a real endpoint of each kind, run by Eric with his own key, is recorded as working. It is not a test.
