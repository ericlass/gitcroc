# M4-08 Proposal jobs in the core

## Goal
The core resolves one conflicted file with the default provider as a job. The job keeps the proposal in memory, keyed by workspace and file, and never writes, stages, commits or continues anything. Prompts and replies stay in memory unless the user turned on AI debug logging.

## Depends on
M4-02, M4-05, M4-07

## Spec
Implements: D9, AI-7
Respects: AI-3, AI-4, AI-5, AI-6, AI-16, AI-9, AI-10, AI-12, AI-14, ARCH-1, ARCH-5, ARCH-6, ARCH-7, ARCH-9, OPS-5, OPS-9, SEC-1, section 3 "Proposal"

## In scope
- The seam between the core and `gitcroc-ai`:
  - `gitcroc-core::ai::Proposer`, an async trait: `propose(AiFileContext, cancel) -> Result<FileProposal, String>`. `FileProposal` is the core's own type: per-hunk answers and per-request usage.
  - `gitcroc-app` and `gitcroc-cli` each implement it with a newtype over `gitcroc-ai`'s ask loop. The newtype maps `AiFileContext` onto `FileRequest` and the answers back, field by field.
  - The mapping is the only code the two copies share. If it grows past that, stop and ask (DEV-2).
- `ai_resolve(workspace, handle, file, hunks)`, which first checks, in order, and returns at once:
  - `ai_off` when the AI features setting is off (AI-16);
  - `ai_disabled` when the workspace switch is off;
  - `no_provider`, which tells M4-09 to open the provider settings (AI-3);
  - `provider_unusable` with the self-test's reason;
  - `ineligible` with M4-07's reason;
  - `consent_needed` (M4-06).
  - Otherwise it starts a job labelled "Resolve with AI · <file name>" and returns its id.
- The job:
  - reads the context (M4-07), splits and asks (M4-02, M4-01) with the default provider, and stores the result;
  - ends *succeeded* with a proposal, or *failed* with the "AI failed" reason (AI-9);
  - can be cancelled, which drops the request or kills the CLI and keeps nothing.
  - AI jobs do not take a worker of the git pool. They share a limit of 4 running requests, which AI-13 uses for Resolve all.
- Proposals (section 3, D9):
  - Kept in core memory per workspace (ARCH-9), keyed by repo, path and the conflict's stage object ids. A proposal whose stages no longer match is dropped when read, so a file changed meanwhile never shows a stale one.
  - Dropped on Abort, on Continue, and when the file is marked resolved or chosen.
  - Turning AI features off cancels every running AI job in every workspace and drops every proposal (AI-16). Nothing is kept for when it is turned on again.
  - `ai_proposal_get(handle, file)` returns the proposal, the failure reason, or nothing. `ai_proposal_ready` tells the UI one is ready.
- D9:
  - The job only reads the repo and calls the provider. It has no code path to the write runner, the file system or the index; the runner's read-only check runs on every AI job.
  - Accepting, Mark resolved and Continue stay the user's actions (AI-12, OPS-5).
- AI-7: prompts and replies are never written to the log, the cache or the job list's output, unless the `debug_log` setting is on. Then the log gets the prompt and the reply of each request, never a key (SEC-1). The job list shows the job, its outcome and its token usage, not the prompt.
- `gitcroc-cli conflict ai-resolve <repo> <path>` runs the same job, either with `--mock <replies.json>` or with a provider from the settings and `--key-env`. It gives the same refusals as `ai_resolve`.
- Tests (with the mock provider and fixture repos):
  - `d9_proposal_job_writes_nothing`: after a successful job, the working file, the index, `HEAD`, the refs and the conflict state are byte for byte as before.
  - `d9_ai_jobs_never_reach_the_write_runner`: a test runner that fails on any write.
  - `ai_7_prompts_and_replies_not_logged_by_default`, and logged with `debug_log` on, never with the key.
  - The order of the refusals above, one test each; the first is `ai_16_off_refuses_before_any_other_check`.
  - `ai_16_turning_off_cancels_jobs_and_drops_proposals`: a job held by the mock is cancelled, and a stored proposal is gone.
  - A proposal is dropped on Abort, on Mark resolved, and when the stages change.
  - Cancel keeps no proposal and leaves the file untouched.
  - The CLI runs a mock resolution on a fixture conflict and prints the proposal.

## Out of scope
- Showing and accepting proposals (M4-09). Several files at once (M4-10).

## Acceptance
- The tests above exist and pass on all three platforms, and `just check` passes.
