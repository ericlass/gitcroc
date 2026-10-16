# M4-01 AI response model, validation and the mock provider

## Goal
`gitcroc-ai` can ask a provider to resolve a file's hunks and accept only a valid answer: it validates the JSON the model returns, retries an invalid one once with the error appended, and marks the file "AI failed" after a second failure. A mock provider drives every test.

## Depends on
M3

## Spec
Implements: AI-8, AI-9
Respects: D9, ARCH-5, QA-4, SEC-5, section 3 "Provider" and "Proposal", section 7 "What comes back", section 12 "AI" row

## In scope
- Types (`gitcroc-ai`), with serde and specta derives, so the core and the UI can show them later:
  - `Confidence` (`high`, `medium`, `low`), `HunkStatus` (`resolved`, `unresolved`), `HunkAnswer { id, status, resolution, explanation, confidence }`, `FileAnswer { hunks }`.
  - `Usage { input_tokens, output_tokens }`, optional per request.
- The `Provider` trait: one async call `complete(&ProviderRequest) -> Result<ProviderReply, ProviderError>`.
  - `ProviderRequest` holds the system prompt, the user message, the follow-up messages of a retry, and `temperature: Option<f32>`.
  - `ProviderReply` holds the raw text and an optional `Usage`.
  - `ProviderError` is typed (`thiserror`): transport, HTTP status with the provider's message, process exit with its stderr, cancelled. It never carries a key (SEC-1).
- Validation (AI-8, AI-9):
  - The whole reply, after trimming whitespace and at most one surrounding Markdown code fence, must parse as one JSON object of the shape in section 7. Unknown keys are refused.
  - Every requested hunk id appears exactly once, and no other id appears.
  - A *resolved* hunk has a resolution and an explanation. An *unresolved* hunk has an explanation and no resolution.
  - No resolution contains a conflict marker line. The marker rule is the one M2-09 uses (seven `<`, `=`, `|` or `>` followed by a space or the line end). It is copied here, since `gitcroc-ai` depends on no other crate (ARCH-5), with a test that both copies agree on the same cases.
  - Each failure names its rule and the hunk id, for example "hunk 3 is listed twice".
- The ask loop: `ask(provider, request, hunk_ids) -> FileResult`:
  - A valid reply returns the answers.
  - An invalid reply is retried once. The retry sends the reply back as the assistant's message, then the validation error as a user message asking for corrected JSON only.
  - A second invalid reply ends as `AiFailed { reason }`.
  - A `ProviderError` ends the file at once as `AiFailed` with the provider's message; it is not retried.
  - The usage of each request is kept, so AI-14 can show it per request.
- `MockProvider`, public behind a `mock` feature that tests and the `e2e` build turn on:
  - It answers from a queue of scripted replies and records every request it got.
  - It can also wait until the test releases it, so cancellation tests need no timing.
- Recorded responses: `crates/gitcroc-ai/tests/responses/*.json`, real-looking replies the tests feed through the mock, valid and invalid. They include a fenced reply, prose before the JSON, a missing hunk, a duplicate id, a marker in a resolution, and an unresolved hunk with a resolution.
- Tests:
  - `ai_8_statuses_and_confidences`: the three confidences and two statuses parse; any other value is invalid.
  - `ai_8_unresolved_hunk_has_explanation_and_no_resolution`.
  - `ai_9_every_requested_hunk_exactly_once`: a missing, a duplicate and an extra id are each invalid.
  - `ai_9_no_resolution_contains_a_marker_line`.
  - `ai_9_invalid_reply_is_retried_once_with_the_error`: the mock sees the error in the second request.
  - `ai_9_second_invalid_reply_marks_the_file_ai_failed`.
  - `ai_9_provider_errors_are_not_retried`.

## Out of scope
- What the prompt says, and splitting (M4-02). Real providers (M4-03, M4-04). Jobs, eligibility and the UI (M4-07 onward).

## Acceptance
- The tests above exist and pass on all three platforms. No test calls a real provider.
- `cargo deny check` passes with the new dependencies, and `just check` passes.
