# M4-09 Proposals in the conflict editor

## Goal
In the conflict editor the user clicks **Resolve with AI** for a file, then sees each hunk's proposed text, explanation and confidence, and accepts, rejects or edits each one. Accepting changes only the editor buffer; writing still happens only through Mark resolved.

## Depends on
M4-06, M4-08

## Spec
Implements: AI-11, AI-12, AI-14
Respects: AI-3, AI-4, AI-6, AI-16, AI-8, AI-9, D9, OPS-3, OPS-5, OPS-9, UI-22, UI-25, UI-29, SEC-3, ARCH-6

## In scope
- The editor header gets **Resolve with AI**, which calls `ai_resolve` with the hunks that are still untouched in the buffer:
  - `no_provider` opens Settings at the AI section (AI-3).
  - `consent_needed` opens M4-06's dialog, then asks again.
  - `ineligible` and `ai_disabled` keep the button disabled with the reason shown beside it (AI-4).
  - While the job runs, the button becomes **Cancel**, with the provider's name.
- While AI features is off (AI-16), the editor shows neither the button nor any proposal. Turning it off while a proposal is shown removes the proposal at once; the buffer and its edits stay.
- Showing a proposal (AI-11), under each hunk it covers:
  - the proposed text, read-only, in the code theme;
  - the explanation, rendered as text, never HTML (SEC-3);
  - the confidence as a word and a marker that does not rely on color alone;
  - **Accept**, **Reject** and **Edit**.
  - An *unresolved* hunk shows its explanation and no Accept.
  - "AI failed" shows the reason in the header, with **Try again**.
- Accept (AI-12):
  - Replaces the hunk's whole range, markers included, with the proposed text in the buffer and nowhere else, exactly as take ours does (OPS-3). Nothing is sent to the core.
  - Offered only while the hunk is untouched, the same rule as the take buttons. A hunk edited after the proposal arrived shows the proposal as "hunk changed", with Accept disabled.
- Reject removes the proposal from that hunk in the view. Edit opens the proposed text in a small inline editor; its Accept puts the edited text in the buffer, and its Cancel goes back.
- **Accept all high-confidence** for the file accepts every untouched hunk proposed with *high* confidence. There is no accept-everything across files (AI-11).
- Token usage (AI-14): the header shows the input and output tokens of each request when the provider reported them, and nothing when it did not.
- Proposals and their per-hunk view state survive switching files, repos and tabs as the buffer does (OPS-9, UI-29). They are dropped with the buffer on Abort.
- The buttons are reachable by keyboard (UI-22). Confidence and proposal colors come from the code theme and keep 4.5:1 contrast (UI-25).
- Tests (Vitest on mocked IPC, Playwright on the fake core):
  - `ai_11_each_hunk_shows_text_explanation_and_confidence`.
  - `ai_11_accept_all_high_confidence_only_in_the_file`: medium and low stay, and other files are untouched.
  - `ai_12_accept_changes_only_the_buffer`: no IPC call except reads, and Mark resolved then sends the accepted text.
  - `ai_12_accept_refused_on_an_edited_hunk`.
  - `ai_14_token_usage_shown_per_request`, and nothing without usage.
  - `ai_3_no_provider_opens_the_settings`, for this task's button.
  - `ai_16_off_hides_resolve_with_ai_and_proposals`, including a proposal shown when the switch is turned off.
  - The ineligible reason shows; Edit then Accept puts the edited text; an unresolved hunk has no Accept; the explanation renders as text.
  - Playwright: Resolve with AI on a fake conflict, Accept two hunks, Mark resolved, Continue enabled.

## Out of scope
- Resolve all with AI (M4-10).

## Acceptance
- The tests above exist and pass, and `just check` passes.
