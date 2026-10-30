# M6-15c Askpass prompts end when the app quits

## Goal
Quitting the app while git waits on an askpass prompt ends that prompt
with no answer, on every platform. Git fails the way it does after Cancel,
the helper exits, and nothing is left running: no app, no git and no helper.

## Why
CI on PR #127 hung for 45 minutes on Windows. `d10_askpass_prompts_queue`
left a fetch waiting on git's prompt, and the test binary never exited.
b5608cf fixed the test by ending both fetches before the app goes. The app
itself has the same case: a fetch can be waiting on its prompt when the
window closes.

What is known:
- On quit, `gitcroc-ui`'s `main` calls `runtime.shutdown_background()`,
  which does not wait for jobs ("Jobs still running are abandoned").
- The helper (`askpass::helper`) waits on the app's reply in a blocking
  read with no time limit.
- The app's end of the prompt (`AskpassServer::answer`) waits for the
  user's answer or for the job to end. Nothing ends it when the app quits.
- `kill_on_drop` kills git, but on Windows killing a process does not kill
  its children. The child here is `git-remote-https` or `ssh`, and the
  helper under it.

What is not known: whether the real app hangs, or leaves git and the helper
running after its window has gone. The first step is a test that finds out.
The cause of the 45-minute test hang is also not known. Find it, because it
decides the fix.

## Depends on
M6-15b

## Spec
Respects: D10, ARCH-7, PLAT-1, SEC-1, QA-3, QA-4

The spec does not say what quitting does to an open prompt. This task
treats it as a Cancel: no answer, and the job is abandoned as other running
jobs are. If the work shows that this does not hold, stop and ask (DEV-2).

## In scope
- **The cause:** reproduce the hang from before b5608cf on the Windows
  runner, or locally on Windows, and write down in the PR what kept the
  process alive.
- **Ending prompts on quit,** in `gitcroc-app`:
  - `App` ends the askpass server before the runtime goes. Every open
    prompt gets no answer, every helper waiting on a reply gets one, and
    no new prompt is asked;
  - `main` calls it before `shutdown_background()`;
  - the UI closes any open askpass dialog, as it does for `AskpassClosed`.
- **The helper** exits with status 1 when the app's end of the socket
  closes before a reply, so that git fails. Check this on all three
  platforms. A named pipe whose server has gone must not leave the read
  waiting.
- **Windows only,** if the test shows that git's children outlive it: end
  them with git when the job is abandoned. This must use a safe API, with
  no `unsafe` (QA-4). If no safe way exists, stop and ask.

## Tests (QA-3)
Real git on `repo-gen` fixtures, with the local HTTP host that asks for a
password from `d10_askpass_answers_git_credential_prompt`. Every wait is
for a condition, using the deadline the askpass tests already use; no test
measures time (no timing in CI).
- `d10_quit_ends_an_open_askpass_prompt`, in
  `crates/gitcroc-app/tests/integration/app.rs`:
  - a fetch waits on its prompt (`AskpassPrompt` has arrived);
  - the app is shut down as `main` does it;
  - the helper exits with status 1 and the git process ends.
- `d10_askpass_helper_exits_when_the_app_goes`, in
  `crates/gitcroc-app/tests/integration/askpass.rs`: a helper waiting on a
  reply exits with status 1 when the listener and its stream are dropped.
- `d10_quit_closes_the_askpass_dialog`, in
  `crates/gitcroc-ui/tests/integration/askpass.rs`: the dialog is gone
  after the shutdown and its field is empty (SEC-1).
- Revert b5608cf's change to `d10_askpass_prompts_queue` locally: the test
  must no longer hang on Windows. If the fix makes it pass with the second
  fetch still waiting, keep b5608cf anyway, since a test should not lean
  on the shutdown.

## Out of scope
- Asking before quitting while jobs run. rc.1 does not ask either.
- Any other job type that may outlive the app.
- Timeouts on prompts. A prompt still waits as long as the user does.

## Acceptance
- The tests above pass on Linux, macOS and Windows in CI.
- On Windows, a real-window check: start a fetch from an HTTPS remote that
  needs a password (needs M6-18's Fetch button), close the window while
  the dialog is open, and the app, git and the helper are gone at once.
  Until M6-18, the PR says that this check is still to do.
- `just check` passes.
