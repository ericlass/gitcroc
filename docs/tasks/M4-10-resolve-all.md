# M4-10 Resolve all with AI

## Goal
From the conflict banner the user runs **Resolve all with AI** for every eligible file of the repo in conflict. Up to 4 files are processed at a time, with progress per file. The run can be cancelled, and finished proposals stay.

## Depends on
M4-09

## Spec
Implements: AI-3, AI-13
Respects: AI-4, AI-6, AI-16, AI-7, AI-9, AI-11, AI-12, D9, OPS-2, OPS-9, ARCH-7, UI-21, UI-22, UI-30

## In scope
- The banner of OPS-2 gets **Resolve all with AI** next to Abort and Continue:
  - With no provider configured it opens the provider settings (AI-3).
  - With the workspace switch off it is disabled with the reason.
  - With AI features off it is hidden, and so is the progress view (AI-16).
  - Consent is asked once, before the run, as in M4-09.
- `ai_resolve_all(workspace, repo, files)`:
  - `files` lists, for each unmerged file the UI has an editor buffer for, its untouched hunks (OPS-9). Every other file is sent with all its hunks.
  - Ineligible files are not sent. They are listed with their reason (AI-4).
  - It starts one job, "Resolve all with AI · <repo> (n files)", with one sub-job per eligible file. At most 4 run at once, inside M4-08's limit of 4 requests.
  - Each file's proposal is stored as it finishes, exactly as `ai_resolve` stores it, and `ai_proposal_ready` is sent for it.
- Progress (AI-13): each file is shown as waiting, running, done (the hunk count by confidence), *AI failed* with the reason, ineligible with the reason, or cancelled.
  - The view sits under the banner and in the job list (UI-21). Clicking a file opens it in the conflict editor with its proposal.
- Cancel stops waiting files and drops running requests. Finished proposals stay, and unfinished files keep no proposal and stay untouched (AI-13).
- The job keeps running in a background tab (UI-30). Its proposals are there when the tab is active again.
- Nothing is accepted for the user: every file still needs Accept, Mark resolved and Continue (AI-11, AI-12, D9).
- Tests:
  - Core, with a mock provider that holds each request until released:
    - `ai_13_at_most_4_files_at_a_time`: 6 eligible files, asserted on the most requests held at once.
    - `ai_13_cancel_keeps_finished_proposals_and_leaves_the_rest_untouched`.
    - Per-file progress events in order. Ineligible files are reported and never sent.
    - The untouched hunks of a file with a buffer are the only ones sent.
    - `ai_16_resolve_all_refused_when_off`, and a running Resolve all cancelled when AI is turned off, keeping no proposal.
  - Vitest:
    - `ai_3_banner_resolve_all_opens_settings_without_a_provider`.
    - The progress list, and opening a file from it.
    - Cancel shows cancelled files.
    - `ai_16_off_hides_resolve_all`.
  - Playwright on the fake core: Resolve all on three fake files, one ineligible.

## Out of scope
- Accepting across files (not allowed, AI-11).

## Acceptance
- The tests above exist and pass with no timing, and `just check` passes.
