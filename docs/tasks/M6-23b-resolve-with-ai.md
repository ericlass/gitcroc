# M6-23b Resolve with AI, consent and the workspace switch

## Goal
The conflict editor's header has **Resolve with AI** for the file shown. It
asks the core for a proposal of the hunks still untouched in the buffer,
after the consent dialog when the core asks for it, and M6-19b3's cards
then show the proposal for review. Each workspace's tab menu has **AI for
this workspace**.

## Depends on
- M6-23a: the Settings dialog at its AI tab, the AI features value in the
  window and the tests' fake AI backend.
- M6-19c: the conflict view, its kept editors per file and Mark resolved.
- M6-19b3: the cards' review of a supplied proposal.
- M6-09: the tab menu.

## Spec
Implements: AI-3 (Resolve with AI), AI-4 (the reason shown), AI-5 (the hunks sent), AI-6, AI-9 (the UI), AI-14, AI-16 (the editor and the tab menu)
Respects: AI-8, AI-11, AI-12, D9, OPS-3, OPS-5, OPS-9, UI-22, UI-25, UI-29, UI-30, SEC-3, ARCH-6, ARCH-7, ARCH-9, D15, QA-2, QA-3, QA-4

The core decides everything (ARCH-6): `ai_resolve` refuses, in its order,
with `ai_off`, `ai_disabled`, `no_provider`, `provider_unusable`,
`ineligible` or `consent_needed`, and starts the job otherwise;
`ai_eligibility` gives a file's reason up front; `ai_proposal_get` reads
the result after `AiProposalReady`. The UI only sends the untouched hunks'
numbers and shows the answers.

## In scope
- **The button**, a port of the AI parts of `ui/src/conflict/ConflictView.tsx`
  and `ui/src/stores/ai.ts`:
  - **Resolve with AI** in the editor's header sends `ai_resolve` with the
    hunks that are still untouched in the buffer (AI-5); with none left it
    is disabled;
  - `no_provider` opens the Settings dialog at the AI tab (AI-3);
  - `consent_needed` opens the consent dialog, then asks again;
  - `ineligible`, `ai_disabled` and `provider_unusable` keep it disabled
    with the reason beside it; the editor asks `ai_eligibility` when it
    shows a file, so the reason is there before a press (AI-4);
  - while the job runs it is **Cancel**, with the provider's name, and
    Cancel cancels the job;
  - `already_resolved` reads the lists again, as Mark resolved does.
- **The result:**
  - on `AiProposalReady` for a file with a kept editor, `ai_proposal_get`
    reads it and the editor's cards get it (`set_proposal`); a file opened
    later reads its proposal when it is shown;
  - *AI failed* (AI-9) shows the reason in the header with **Try again**;
  - the token usage of each request shows in the header when the provider
    reported it, and nothing when it did not (AI-14);
  - proposals and their review stay with the file's editor across files,
    repos and tabs (OPS-9, UI-29); a job keeps running in a background tab
    (UI-30). They go with the editor on Abort.
- **The consent dialog**, a port of `ui/src/ai/ConsentDialog.tsx` (AI-6): it
  lists in plain words what AI-5 sends, and names the host or program the
  core's `consent` request gives. **Allow** stores it with
  `ai_provider_consent` and goes on; **Cancel** sends nothing. It never
  opens while AI features is off.
- **AI for this workspace** (AI-6) in the tab menu, a checked item that
  calls `workspace_set_ai`; hidden while AI features is off (AI-16). With it
  off the editor's button is disabled with the reason.
- **AI features off** (AI-16): the button and every proposal go at once;
  the buffer and its edits stay.
- **Strings (D15):** the consent dialog's and the editor's AI text move
  from `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real git conflicts on `repo-gen` fixtures (QA-2), with the fake AI backend's
mock provider; a held reply where a test needs the job running.
- `ai_3_no_provider_opens_the_settings`, at the AI tab.
- `ai_3_while_the_job_runs_the_button_is_cancel_with_the_providers_name`:
  Cancel ends it with no proposal.
- `ai_4_the_ineligible_reason_shows_beside_the_disabled_button`: a file on
  the exclude list.
- `ai_5_only_the_untouched_hunks_are_sent`: with one hunk taken, the mock
  provider's request lists the others only.
- `ai_6_consent_dialog_names_what_is_sent_and_where`, for an API host and a
  CLI path.
- `ai_6_consent_first_then_asked_again_and_cancel_sends_nothing`: the mock
  provider has no request until Allow.
- `ai_6_consent_asked_again_when_the_host_changes`.
- `ai_6_the_workspace_switch_disables_the_button_with_the_reason`.
- `ai_9_ai_failed_shows_the_reason_with_try_again`: two invalid replies.
- `ai_14_token_usage_shown_per_request`, and nothing without usage.
- `ai_11_a_live_proposal_reaches_the_cards`, and it survives a switch to
  another file and back.
- `ai_12_resolve_accept_both_hunks_mark_resolved_and_continue`: the file on
  disk has the accepted text, and Continue is enabled. The port of
  `ui/e2e/conflict.spec.ts`'s AI-12 flow.
- `ai_16_off_hides_resolve_with_ai_and_proposals`, including a proposal
  shown when the switch is turned off.
- `ai_16_off_hides_the_workspace_ai_switch` and
  `ai_16_off_never_opens_the_consent_dialog`.
- `sec_3_the_reason_and_consent_target_render_as_text`.
- `ui_22_resolve_with_ai_and_the_consent_dialog_are_keyboard_reachable`.

## Out of scope
- **Resolve all with AI** from the banner: M6-23c.
- **The palette's AI commands** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on a real merge conflict:
  Resolve with AI with Claude Code and with an API provider, allow the
  consent once, accept a hunk, edit another, Mark resolved and Continue.
- `just coverage M6` no longer lists AI-4, AI-5, AI-6, AI-9 or AI-14.
- `just check` passes.
