# M6-23a The Settings dialog's AI tab

## Goal
The toolbar's Settings opens the Settings dialog, which for now has one tab,
AI. In it the user turns AI features on and off, adds, edits, tests and
removes providers, picks the default and a model from the provider's list,
and edits the exclude list and AI debug logging. M6-26 adds the other three
tabs to the same dialog.

## Depends on
- M6-15a: dialogs, the job list and the app events in the window.
- M6-06: the toolbar, whose Settings button is disabled until now.

## Spec
Implements: AI-1, AI-16 (the Settings part), SEC-1 (the key field)
Respects: AI-2, AI-4, AI-7, AI-10, UI-20, UI-22, UI-23, SEC-3, ARCH-6, ARCH-7, D11, D15, QA-2, QA-3, QA-4, section 10 "Files"

The services are done (M6-05, PR 4): `ai_providers_get`, `ai_provider_save`,
`ai_provider_delete`, `ai_provider_set_default`, `ai_settings_set`,
`ai_models_get` and `ai_self_test`, with `AiChanged`, `AiModels` and
`AiSelfTest` on the event channel. The core lists the kinds, the detected
CLIs, each provider's `has_key` and self-test result; the dialog shows them
and keeps no copy of its own (ARCH-6).

## In scope
- **The Settings dialog**, a port of the frame of `ui/src/SettingsDialog.tsx`:
  - opened by the toolbar's Settings, which is enabled now, and by a
    caller that names the tab, as AI-3's "open the provider settings" will
    (M6-23b);
  - a tab strip with only **AI** until M6-26, keyboard-reachable as rc.1's
    (arrow keys between tabs, focus to the tab's first control);
  - which gitcroc is running below the tabs, as rc.1 shows it;
  - one dialog per window; opening it again focuses it.
- **The AI tab**, a port of `ui/src/ai/AiSection.tsx`:
  - first the **AI features** switch (AI-16). Off, it is the only control,
    with rc.1's note that no AI request is sent and the AI buttons are
    hidden;
  - the provider list, the default marked, with Add, Edit, Remove and Set
    as default;
  - the exclude list (AI-4), one pattern per line, and AI debug logging
    (AI-7), whose note says prompts and replies are then written to the log.
    Both save through `ai_settings_set`.
- **The provider form**, a port of `ui/src/ai/ProviderForm.tsx`:
  - the kind, from the core's list only (AI-1), then the fields the section
    7 table gives that kind, and the context budget (AI-10);
  - **the key field is write-only** (SEC-1): it says "key stored" when
    `has_key` is set, never shows a key, and can replace or remove it. The
    typed key goes only to `ai_provider_save`, `ai_models_get` and
    `ai_self_test`, and is cleared from the field when the form closes;
  - **Fetch models** fills a list from `AiModels`; with none offered the
    model is a text field. No model is preselected (AI-1);
  - for Claude Code and Codex: the detected paths to pick from, a path
    field and the self-test result, the version or the missing flags
    (AI-2);
  - **Test** runs `ai_self_test` and shows its result; while a test or a
    model list is asked for, its button is busy and a second press does
    nothing;
  - a refusal from the core shows in the form; nothing is saved then.
- **AI features off** (AI-16): the window reads the switch from
  `AiChanged`, and every open tab shows the change at once. M6-23b and
  M6-23c hide their buttons from the same value.
- **The tests' fake AI backend:** `support::Options` takes an `AiBackend`
  that answers from a script: detected CLIs, a self-test result, a model
  list, and the replies of the mock provider, which can be held until the
  test releases them (`MockProvider::held`). The default stays
  `ProviderCalls`, so no other test changes. No test reaches a network or a
  real CLI.
- **Strings (D15):** `settings.tabs`, `ai` and the provider form's text move
  from `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
GPUI tests on the fake AI backend, with the in-memory keychain.
- `ai_1_the_kind_list_is_the_cores`: the kind choices are the backend's,
  in its order, and the mock kind shows only when it offers it.
- `ai_1_add_a_provider_pick_a_fetched_model_and_keep_it_through_ai_off_and_on`:
  add an OpenAI-compatible provider, fetch models, pick one, set it as the
  default; turn AI features off and on, and it is still there and still
  the default. The port of `ui/e2e/ai.spec.ts`.
- `ai_1_no_model_is_preselected`, and a provider offering no list gets a
  text field.
- `ai_2_cli_providers_show_detected_paths_and_the_self_test`: a missing
  flag is named.
- `ai_16_off_hides_the_ai_section_except_the_switch`.
- `ai_16_off_reaches_every_open_tab`.
- `sec_1_key_field_never_shows_the_key`: after a save the field is empty
  and says "key stored"; no rendered text and no view the dialog keeps
  holds the key.
- `sec_3_provider_names_and_models_render_as_text`.
- `ui_22_the_settings_dialog_is_keyboard_reachable`: tabs, list and form.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The consent dialog and the workspace AI switch** (AI-6): M6-23b.
- **The Appearance, Git and Updates tabs:** M6-26.
- **The palette's Settings commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next`: add an Anthropic provider
  with a real key, fetch its models, pick one, test it; add Claude Code from
  the detected path and see its self-test. Restart, and the providers and
  the default are kept and the key is still only "key stored".
- `just coverage M6` no longer lists AI-1.
- `just check` passes.
