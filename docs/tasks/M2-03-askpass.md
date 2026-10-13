# M2-03 Askpass helper

## Goal
When git needs a credential or an answer it cannot get silently, the user is asked in a dialog, and the answer goes to git and nowhere else.

## Depends on
M2-01

## Spec
Implements: D10, PLAT-1
Respects: ARCH-3, ARCH-5, ARCH-7, SEC-1, SEC-2, SEC-3, D12, section 4 "Authentication prompts"

## In scope
- The helper is the gitcroc executable itself, started by git with `GITCROC_ASKPASS` set. In that mode it does not open a window: it sends git's prompt to the running app over a local socket (a Unix domain socket, a named pipe on Windows) together with a random token the app put in git's environment for that job, prints the answer on stdout and exits. A missing app, a wrong token or a cancelled dialog makes it exit with status 1, which git treats as no answer.
- The runner sets `GIT_ASKPASS` and `SSH_ASKPASS` to the helper, and `SSH_ASKPASS_REQUIRE=prefer`, on every network write (fetch, pull, push, clone, remote branch delete, tag push). Reads never get them. Credential helpers and SSH agents keep answering first (D10).
- The app shows the prompt in a modal dialog of its own window, naming the repo and the job: a text field, masked when the prompt asks for a password or passphrase, and a yes/no choice for an SSH host key question. Prompt text is shown as text (SEC-3).
- The answer is never stored, logged, cached or kept after it is sent (D10, SEC-1).
- Behind one trait in `gitcroc-app`, `AskpassUi`, so the tests can answer without a window (PLAT-1). The socket server and the helper mode are platform code behind the same trait. This task also adds the PLAT-1 test below, which every later platform trait (M2-14: file manager, terminal) joins.
- Tests:
  - `d10_askpass_answers_git_credential_prompt`: `git -c credential.helper= credential fill` for an HTTPS host, run through the runner, gets the username and password from a fake `AskpassUi`; nothing is written to the log, the config dir or the cache dir.
  - `d10_cancelled_prompt_fails_the_job`: the fake dialog cancels; the job ends *failed* with git's message.
  - `d10_askpass_refuses_a_wrong_token`: the helper started with a token the app did not issue gets no answer.
  - `d10_reads_get_no_askpass`: read commands run without `GIT_ASKPASS` and `SSH_ASKPASS`.
  - `plat_1_platform_code_sits_behind_traits`: platform `cfg` blocks in `gitcroc-app` appear only in the implementations of its platform traits.
  - Vitest: the prompt dialog masks a password prompt, shows a host-key question as yes/no, and sends nothing on Escape but the cancel.

## Out of scope
- Fetch, pull, push and clone themselves (M2-07, M2-14); this task tests with `git credential fill`.
- GPG pinentry: git and gpg-agent handle it; a pinentry that needs a terminal makes the commit fail with git's message.

## Acceptance
- `just dev` with an HTTPS remote that needs a password: a fetch from a debug build asks in the dialog and succeeds; Cancel fails it with git's message.
- All tests listed above exist and pass on all three platforms, and `just check` passes.
