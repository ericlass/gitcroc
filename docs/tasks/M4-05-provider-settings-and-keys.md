# M4-05 Provider settings and keys in the keychain

## Goal
The core and the shell keep a list of configured AI providers with one default, plus the AI settings, in `settings.toml`. API keys go only to the OS keychain. The UI can list, save, delete and test providers and fetch their models over IPC, and never receives a key back.

## Depends on
M4-03, M4-04

## Spec
Implements: AI-1, SEC-1, D11
Respects: AI-2, AI-4, AI-6, AI-7, AI-10, PLAT-1, DATA-1, ARCH-7, D12, section 10 "Files"

## In scope
- Settings (`gitcroc-core::settings`): an optional `[ai]` table, so the schema stays the same, as the `[session]` table did:
  - `default`: a provider id.
  - `exclude`: AI-4's list, defaulting to `.env*`, `*.pem`, `*.key`, `*.p12`, `id_rsa*`.
  - `debug_log`: AI-7's switch, default off.
  - `[[ai.provider]]` entries, each with:
    - `id` (`p1`, `p2`, …), `name`, and `kind` (`anthropic`, `openai`, `claude_code`, `codex`);
    - `model`, `base_url` (OpenAI-compatible only), `path` (CLI only);
    - `context_budget`: AI-10's setting, default 100,000 tokens;
    - `consent`: AI-6, the host or program the user consented to, or absent.
  - The core validates every entry: a known kind, the fields that kind needs, an http or https base URL, and a budget from 1,000 to 10,000,000.
- Keys (SEC-1, D11):
  - A `Keychain` trait in `gitcroc-app`'s platform module (PLAT-1), implemented with the `keyring` crate on all three platforms.
  - Entries use the service `gitcroc` and the account `ai-provider-<id>`.
  - An in-memory implementation serves the tests and the `e2e` build. A keychain that cannot be reached, such as Linux without a Secret Service, is reported in the provider settings and never falls back to a file.
- IPC (generated types):
  - `ai_providers_get` returns the providers with `has_key: bool` and never the key.
  - `ai_provider_save` takes an entry and an optional new key: absent keeps the stored key, an empty string removes it.
  - `ai_provider_delete` removes the entry and its key; deleting the default clears `default`.
  - `ai_provider_set_default`, and `ai_settings_set` for the exclude list and the debug log switch.
  - `ai_models_get` and `ai_self_test` return a request id at once (ARCH-7), and their result arrives as an event. The models come from M4-03; the self-test from M4-04, for CLI providers; API providers get a short test call through `list_models`.
- At app start, every configured CLI provider is self-tested in the background (AI-2). A provider whose self-test failed carries the reason. M4-09 refuses to use it and shows why.
- The first CLI provider found by M4-04's detection is offered in the settings, never added on its own.
- `gitcroc-cli ai providers` lists the configured providers as JSON, for tests and for M4-11. It reads keys from an environment variable named on the command line (`--key-env NAME`), never from arguments or the keychain.
- Tests:
  - `ai_1_several_providers_one_default`: save three, set the default, delete it.
  - `ai_1_model_names_come_from_the_provider_or_the_user`: a provider without a list keeps the typed name. Plus a source test that no model name literal sits in the code outside tests.
  - `sec_1_keys_only_in_the_keychain`: after saving with a key, `settings.toml`, the log and every IPC payload sent to the UI hold no trace of it.
  - `d11_key_removed_with_its_provider`.
  - Validation refuses each bad field. An `[ai]` table with an unknown key is read as M3's tables are.

## Out of scope
- The settings UI and the consent dialog (M4-06). The workspace switch (M4-06). Using a provider on a conflict (M4-08).

## Acceptance
- The tests above exist and pass on all three platforms. CI uses the in-memory keychain.
- On this machine, a key saved through the IPC lands in the Secret Service and is read back by a manual check recorded in the pull request.
- `just check` passes.
