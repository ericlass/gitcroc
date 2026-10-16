# M4-11 Prompt fixture set and the eval command

## Goal
The repo holds at least 30 real-world style conflicts, each with reviewed acceptable resolutions. One command runs them against a chosen provider and prompt version and reports how each hunk did, so prompt versions can be compared.

## Depends on
M4-02, M4-03, M4-04, M4-07

## Spec
Implements: AI-15
Respects: AI-5, AI-8, AI-9, AI-10, AI-14, QA-2, SEC-1, section 12 "AI" row (no CI test calls a real provider)

## In scope
- Fixtures in `crates/gitcroc-ai/fixtures/conflicts/<nn>-<name>/`, plain text only (QA-2):
  - `meta.toml` holds the path, operation, side labels and commit messages per side.
  - `base`, `ours` and `theirs` hold the whole file text of each side.
  - `accept/1.txt`, `accept/2.txt`, … each hold one acceptable resolution of the whole file.
  - The hunks come from `chunks_from_texts` (M4-07), as the editor's would.
- At least 30 fixtures, real-world style and written for this repo, none copied from a project:
  - languages: Rust, TypeScript, Python, Java, C#, Go, and also JSON, YAML, TOML, Markdown and a lock-file-like list;
  - kinds: both sides add to a list, a rename against an edit, an import block, a version bump, a function moved and edited, a whitespace-only change against an edit, a changelog entry each, a deleted line against an edited line, and conflicts where keeping one side is the only right answer;
  - at least 5 fixtures where the right answer is *unresolved*, such as contradictory logic changes. Their `accept/` holds `UNRESOLVED`.
  - Eric reviews every fixture and its accepted resolutions in the pull request. That review is what "reviewed" means in AI-15.
- `gitcroc-cli ai eval --provider <id> --key-env NAME [--prompt v1] [--only <nn>] [--json]`:
  - The provider is one from the settings, or given with `--kind`, `--model`, `--base-url` and `--path`.
  - It builds each fixture's request (AI-5), asks (M4-01, M4-02), and scores each hunk. *match* means the whole resolved file, with the proposals applied, equals an accepted resolution after line endings are normalized and trailing whitespace is trimmed. The other scores are *differs*, *unresolved* (correct when expected), and *AI failed* with the reason.
  - It prints a table and a summary: matches, correct unresolved, differs, failed, confidence against score, token usage. `--json` writes the same as JSON.
  - `--compare v1,v2` runs both prompt versions and lists the fixtures whose score changed.
- `just ai-eval *args` wraps it. It never runs in `just check` or CI.
- Tests (no real provider):
  - `ai_15_at_least_30_fixtures_each_with_an_accepted_resolution`: each fixture loads, has at least one conflict hunk, and has at least one accepted resolution or `UNRESOLVED`.
  - `ai_15_eval_scores_a_matching_and_a_differing_reply`, with the mock provider answering from a fixture's accepted resolution.
  - The eval's JSON report has the fields above.

## Out of scope
- Running against real providers (M4-12, by hand). A second prompt version, unless the first run shows a need.

## Acceptance
- The tests above exist and pass. Eric has reviewed the fixtures in the pull request.
- One eval run against a real provider on this machine is attached to the pull request as a first reading. It is not a gate.
- `just check` passes.
