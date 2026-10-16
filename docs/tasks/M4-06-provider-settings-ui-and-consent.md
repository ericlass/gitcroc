# M4-06 Provider settings UI, consent and the workspace AI switch

## Goal
The user can add, edit, test and remove AI providers in Settings, pick the default and pick models from the provider's list. Before a provider is first used, a consent dialog says what will be sent and where. Each workspace has a switch that turns AI off entirely.

## Depends on
M4-05

## Spec
Implements: AI-6
Respects: AI-1, AI-2, AI-4, AI-7, AI-10, SEC-1, UI-22, UI-23, ARCH-6, section 10 "Files"

## In scope
- Settings dialog, a new **AI** section:
  - The provider list, with the default marked, and Add, Edit, Remove and Set as default.
  - Each provider's form shows the fields its kind needs (section 7 table) and the context budget.
  - The key field is write-only. It shows "key stored" when `has_key` is set and never shows the key; it can replace or remove the key.
  - **Fetch models** fills a list from `ai_models_get`. When the provider offers none, the model is a text field. No model is preselected.
  - CLI providers show the detected paths to pick from, a path field, and the self-test result (version, or the missing flags).
  - **Test** runs `ai_self_test` and shows the result.
  - The exclude list (AI-4) is edited as one pattern per line. AI debug logging (AI-7) is a switch whose note says prompts and replies are then written to the log.
- Consent (AI-6):
  - The core decides: using a provider whose `consent` is absent, or differs from its current host (API) or program path (CLI), returns `consent_needed` with the text to show instead of running.
  - The dialog lists what AI-5 sends, in plain words: the file path, the operation, the two side labels, the conflicting parts with 40 lines around each, and up to 10 commit messages per side. It also names the host, for example `api.anthropic.com`, or the program path.
  - **Allow** stores the consent through `ai_provider_consent` and goes on. **Cancel** sends nothing.
- Workspace AI switch (AI-6):
  - An optional `ai = false` in the workspace file, schema unchanged as with `[layout]`, set from the workspace's tab menu under **AI for this workspace**.
  - With AI off, the core refuses every AI request for that workspace, and the AI buttons of M4-09 and M4-10 are disabled with the reason.
  - Export (DATA-2) leaves the switch out.
- Everything in the section is reachable by keyboard (UI-22) and uses the theme variables (UI-23).
- Tests:
  - Vitest: `ai_6_consent_dialog_names_what_is_sent_and_where`, for an API host and a CLI path.
  - Vitest: `ai_6_consent_asked_again_when_the_host_changes`.
  - Core: `ai_6_no_request_without_consent`: the mock provider records nothing until consent is stored.
  - Core: `ai_6_workspace_switch_disables_ai`.
  - Vitest: `sec_1_key_field_never_shows_the_key`, and that the provider form's IPC payloads to the UI have no key.
  - Playwright on the fake core: add an OpenAI-compatible provider, fetch models, pick one, set it as default.

## Out of scope
- The buttons that use a provider (M4-09, M4-10).

## Acceptance
- The tests above exist and pass, and `just check` passes.
