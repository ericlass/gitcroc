# M4-12 AI fixture runs, AI smoke test and M4 sign-off

## Goal
Milestone M4 is verified: every AI requirement has a test against the mock provider, and the fixture set has run against one API provider and one CLI provider.

## Depends on
M4-01 to M4-11

## Spec
Respects: QA-1, QA-3, QA-5, AI-15, D9, D12, SEC-1, section 12 "AI" and "End to end smoke" rows, section 13 M4 row

## In scope
- Fixture runs (AI-15, section 13), by hand on the developer's machine with Eric's accounts:
  - `just ai-eval` against one API provider: Anthropic, or an OpenAI-compatible endpoint, which may be a local model. The key comes from an environment variable Eric sets; it is never typed by an agent or stored in the repo.
  - `just ai-eval` against one CLI provider: Claude Code or Codex.
  - Each run's summary goes into the sign-off: matches, correct unresolved, differs, failed, token usage, provider, model and prompt version.
  - A run with many *differs* or *failed* is reported, not hidden. Whether to add a prompt version is Eric's call, as a follow-up task.
- End-to-end smoke (`just smoke`, Linux) gains an AI flow after the M3 steps, on the merge conflict M2's flow makes:
  - The `e2e` build has a mock provider kind, set by `GITCROC_E2E_AI`, that answers from a scripted reply. The in-memory keychain is used.
  1. Settings, AI: add the mock provider and make it the default.
  2. **Resolve with AI** on the conflicted file; the consent dialog appears; **Allow**.
  3. The proposal shows; **Accept all high-confidence**.
  4. **Mark resolved**, then **Continue**; git says the merge is done, and the file holds the proposed text.
  - Each step waits for its result in the DOM, never for a time.
- `docs/tasks/M4-signoff.md`, filled in by the person verifying:
  - the coverage result;
  - both fixture runs with date, commit, provider, model and prompt version;
  - the smoke run;
  - the section 9 budgets re-run with `just bench-ui` and `just bench` to show M4 did not regress them, compared with M3 in a table;
  - Windows and macOS deferred, as before;
  - the keychain checked by hand on Linux.
- `just coverage M4` joins `just check`, so every M4 requirement has a test (QA-3).
- `CONTRIBUTING.md`:
  - how the AI parts fit together: core context, the `Proposer` seam, `gitcroc-ai`'s ask loop, and the providers;
  - how to add a provider kind;
  - how to add a prompt version and compare it with `just ai-eval --compare`;
  - that no test may call a real provider.

## Out of scope
- M5 work. Windows and macOS runs (deferred).

## Acceptance
- `docs/tasks/M4-signoff.md` is filled in for Linux with both fixture runs. Any budget missed since M3 is turned into a follow-up task.
- The smoke test passes locally and in CI, and `just check` enforces M4 coverage and passes.
