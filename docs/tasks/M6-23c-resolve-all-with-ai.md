# M6-23c Resolve all with AI

## Goal
The conflict banner has **Resolve all with AI** for every unmerged file of
the repo in conflict. The core runs up to 4 files at a time; the UI shows
each file's progress under the banner and in the job list, can cancel the
run, and keeps the proposals that finished. Nothing is accepted for the
user.

## Depends on
- M6-23b: Resolve with AI, the consent dialog, the workspace switch and
  live proposals in the cards.
- M6-19c1: the conflict banner.
- M6-15a: the job list, which already shows a job's `ai_files`.

## Spec
Implements: AI-3 (Resolve all), AI-13, D9 (the UI)
Respects: AI-4, AI-6, AI-9, AI-11, AI-12, AI-16, OPS-2, OPS-9, UI-21, UI-22, UI-30, SEC-3, ARCH-6, ARCH-7, ARCH-9, D15, QA-2, QA-3, QA-4

`ai_resolve_all` refuses as `ai_resolve` does, lists ineligible files
without sending them, runs at most 4 files at once and carries each file's
`AiFileState` in the job's view; each file's result arrives as
`AiProposalReady` (M4-10). The UI sends, for each file with a kept editor,
its untouched hunks, and shows the job.

## In scope
- **The button**, a port of the AI parts of
  `ui/src/integrate/ConflictBanner.tsx` and `ui/src/stores/resolveAll.ts`:
  - **Resolve all with AI** beside Abort and Continue sends
    `ai_resolve_all` with the untouched hunks of every file that has a
    kept editor (OPS-9); other files go with all their hunks;
  - `no_provider` opens the Settings dialog at the AI tab (AI-3);
    `consent_needed` opens M6-23b's dialog once, before the run;
    `ai_disabled` keeps it disabled with the reason (AI-6);
  - while the run goes it is **Cancel**, which cancels the job.
- **The progress**, a port of `ui/src/conflict/AiFileList.tsx` (AI-13):
  - under the banner, each file as waiting, running, done with its hunks
    by confidence, *AI failed* with the reason, ineligible with the
    reason, every hunk already edited, or cancelled; from the job's view
    only;
  - clicking a file opens it in the conflict editor with its proposal;
  - each `AiProposalReady` reaches the file's editor as in M6-23b, and a
    file opened later reads its proposal;
  - Cancel keeps the proposals that finished; the other files keep none
    and their buffers are unchanged (AI-13).
- **In a background tab** the run goes on (UI-30), and its proposals are
  there when the tab is shown again.
- **Nothing is accepted for the user** (D9, AI-11, AI-12): every file still
  needs Accept, Mark resolved and Continue.
- **AI features off** (AI-16): the button and the progress are hidden, and
  turning it off during a run ends it (the core cancels the job) and drops
  its proposals.
- **Strings (D15):** the banner's AI text and the file states move from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
Real git conflicts on `repo-gen` fixtures (QA-2) with several unmerged
files, one on the exclude list, and the fake AI backend's mock provider,
held where a test needs files still running. No test waits on timing.
- `ai_3_banner_resolve_all_opens_settings_without_a_provider`.
- `ai_13_progress_lists_each_file_and_opens_one_with_its_proposal`: the
  ineligible file shows its reason and is never sent.
- `ai_13_cancel_shows_cancelled_files_and_keeps_finished_proposals`: one
  reply released, then Cancel; the cancelled file's buffer is unchanged.
- `ai_13_buffers_send_only_their_untouched_hunks`.
- `ai_13_the_run_goes_on_in_a_background_tab`.
- `ai_6_resolve_all_disabled_with_the_reason_when_the_workspace_switch_is_off`.
- `d9_resolve_all_accepts_nothing`: after the run no file on disk or in
  the index changed and every buffer is as before.
- `ai_16_off_hides_resolve_all`, and turning it off during a run ends it
  with no proposal kept.
- `sec_3_file_paths_and_reasons_render_as_text`.
- `ui_22_resolve_all_and_the_progress_list_are_keyboard_reachable`.

## Out of scope
- **Accepting across files:** not allowed (AI-11).
- **The palette's Resolve all command** (UI-22): M6-24.

## Acceptance
- **On the reference machine,** `just dev-next` on a merge with several
  conflicting files, one of them a `.env`: Resolve all with AI with a real
  provider, open two files from the progress, accept, Mark resolved and
  Continue; another run, cancelled halfway, keeps what finished.
- `just coverage M6` no longer lists AI-3, AI-13 or D9.
- `just check` passes.
