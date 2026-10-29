# M6-15b Askpass dialog and update notice

## Goal
When git asks for something it cannot get silently, the GPUI app asks the
user in a dialog, and the answer goes to git and nowhere else, as rc.1's
does. When a newer version is on offer, the status bar says so; its dialog
shows the release notes and installs and restarts, or opens the download
page for the packages that only notify.

## Depends on
- M6-15a: the status bar the notice sits in.
- M6-05, PR 4: `askpass_answer`, the `AskpassPrompt` and `AskpassClosed`
  events, and `Updates` behind `App`.

M6-27, the update source, is not needed: the tests offer a version through
a fake `UpdateSource`, as the core's update tests do.

## Spec
Implements: D10, PLAT-4
Respects: SEC-1, SEC-3, D12, OPS-9, ARCH-6, ARCH-7, UI-22, UI-23, D15, QA-3, QA-4

This task is the UI side of PLAT-4. Its tests take PLAT-4 off
`just coverage M6`; the update itself is still proven by M6-27 and M6-28.

## In scope
- **The askpass prompts** (D10), a port of `ui/src/stores/askpass.ts`, one
  queue for the window:
  - each `AskpassPrompt` joins the queue, and `AskpassClosed` takes its
    prompt out, as when its job ended or was cancelled;
  - the dialog shows the oldest prompt; the next one follows when it is
    answered.
- **The askpass dialog** (D10), a port of `ui/src/AskpassDialog.tsx`, in
  the window's dialog layer:
  - its title is the job's label and where it runs: its repos and the
    workspace, as "api, web in My workspace", or the workspace alone;
  - git's prompt text, shown as text (SEC-3);
  - a `Secret` prompt has a masked field and a `Text` prompt a plain one,
    each with OK and Cancel. Enter answers;
  - a `YesNo` prompt (an SSH host key) has Yes and No;
  - Escape and Cancel send no answer (`askpass_answer` with `None`);
  - the field has the focus when the dialog opens, or Yes for a question;
  - the answer is sent and not kept: no field, model or log holds it after
    `askpass_answer` (D10, SEC-1). The field and its undo history are
    emptied as it goes, and the dialog is dropped.
- **The update notice** (PLAT-4), a port of `ui/src/update/UpdateNotice.tsx`
  and `ui/src/stores/update.ts`:
  - the window reads `update_status` at start and mirrors
    `UpdateStatusChanged`. An event that arrives before the read answers
    wins, as in rc.1;
  - while a version is on offer and Later has not put it off, the status
    bar shows "gitcroc <version> is available" at its right. A click, or
    Enter while it has the focus, opens the update dialog;
  - debug builds never check (D12), so the notice shows only in a release
    build or a test.
- **The update dialog** (PLAT-4), a port of `ui/src/update/UpdateDialog.tsx`:
  - the running and the new version, the date it was published, and the
    release notes as text (SEC-3), or "This release has no notes.";
  - a package that installs itself (the Windows installer, the macOS app,
    the AppImage) has "Install and restart" (`update_install`);
  - the others have "Open download page" (`update_open_download_page`) and
    say they do not update themselves;
  - Later (`update_later`) hides the notice until the next start or a newer
    version; Escape closes the dialog without it;
  - a refusal, such as a running job, is shown in the dialog;
  - when `UpdateStatusChanged` says `ReadyToRestart`, the window restarts
    the app into the new version, as M6-27 expects of the UI.
- **Restarting with unsaved conflict edits** (OPS-9): rc.1 asks first. The
  GPUI app has no conflict editor until M6-19, so there is nothing to lose
  yet; M6-19 adds the question to this dialog.
- **Test seam:** `AppSetup::update_test` sets the package kind and lets a
  debug build check, for the tests' fake source; the binary leaves it
  unset, so it detects its package and checks in release builds only, as
  now (D12). The restart needs no seam: GPUI's test platform reports it to
  `expect_restart`.
- **Strings (D15):** the askpass and update text moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
The askpass tests fetch from a local HTTP host that asks for a password, as
`d10_askpass_prompt_round_trips_through_app_events` does, with
`gitcroc-next` as git's askpass helper.
- `d10_askpass_dialog_answers_git`:
  - the dialog names the job, the repo and the workspace;
  - the password field is masked;
  - the typed user name and password reach the host, and the fetch goes on;
  - after the answer, the dialog is dropped and its field is empty.
- `d10_cancelled_askpass_dialog_fails_the_job`: Escape sends no answer, and
  the job ends *failed* with git's message.
- `d10_askpass_dialog_closes_when_its_job_ends`: cancelling the job in the
  job list closes its dialog.
- `d10_askpass_prompts_queue`: two prompts are shown one after the other.
- `d10_host_key_question_is_yes_or_no`: a `YesNo` prompt, given to the
  dialog directly, has Yes and No and sends "yes" or "no".
- `sec_3_askpass_prompt_renders_as_text`: a prompt with markup shows as
  written.
- `plat_4_update_notice_opens_the_dialog`:
  - a fake source offers a version, and the notice names it;
  - the dialog shows both versions and the notes;
  - Later hides the notice.
- `plat_4_install_and_restart`: with an installing package, Install starts
  the job, and `ReadyToRestart` asks for the restart.
- `plat_4_notify_only_packages_offer_the_download_page`: a `.deb` has "Open
  download page" and no Install.
- `sec_3_release_notes_render_as_text`.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **The update source** (PLAT-4): M6-27. Packaging and the update tests on
  the runners: M6-28.
- **Settings' Updates section**, with the automatic check's switch and
  Check now: M6-26.
- **The question about unsaved conflict edits before a restart** (OPS-9):
  M6-19.
- **Announcing the notice to screen readers**, which rc.1's live region
  did (M5-10): M6-25.
- **Fetch, pull, push and clone** in the GPUI app: M6-18 and M6-20. The
  tests start a fetch through the app.

## Acceptance
- The askpass tests run with a real git and `gitcroc-next` as its helper.
  The check in a real window, with an HTTPS remote that needs a password,
  waits for M6-18's Fetch button.
- `just coverage M6` no longer lists D10 or PLAT-4.
- `just check` passes.
