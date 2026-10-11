# M0-06 Git detection

## Goal
On startup the app finds the system `git`, checks that it is version 2.40 or newer, and shows a setup screen when it is missing or too old.

## Depends on
M0-05

## Spec
Implements: PLAT-3, ARCH-3
Respects: D13, ARCH-1, ARCH-5, ARCH-7, PLAT-1, PLAT-2, SEC-3

## In scope
- `gitcroc-git`: the CLI runner, minimal form.
  - `GitRunner::run(args: &[OsString], cwd: Option<&Path>) -> Result<Output, GitError>` using `tokio::process::Command`.
  - Never a shell; arguments passed as an array (ARCH-3). Sets `LC_ALL=C` and `GIT_TERMINAL_PROMPT=0`. Captures stdout and stderr as bytes.
  - `GitError` carries the exit code and stderr.
  - No per-repo locking yet (ARCH-4 is M2).
- `gitcroc-git`: detection.
  - Search order: an explicit path from settings if set (the setting itself arrives in M0-07; accept an `Option<PathBuf>` parameter here), then `PATH`, then on Windows `%ProgramFiles%\Git\cmd\git.exe`, `%ProgramFiles(x86)%\Git\cmd\git.exe` and `%LocalAppData%\Programs\Git\cmd\git.exe`.
  - Runs `git --version` and parses it. Must handle at least: `git version 2.43.0`, `git version 2.47.1.windows.2`, `git version 2.39.5 (Apple Git-154)`, and release candidates such as `2.48.0.rc1`.
  - Result: `GitInstall { path: PathBuf, version: (u32, u32, u32) }` or one of `NotFound`, `TooOld { found, required }`, `Unparseable { output }`.
  - Minimum version is one constant, `MIN_GIT = (2, 40, 0)`.
- `gitcroc-core`: holds the detection result in app state; exposes `git_status()`.
- `gitcroc-cli`: `gitcroc-cli git-info` prints the result as JSON and exits 0 when usable, 1 otherwise.
- `gitcroc-app`: detection runs as an async task at startup, never blocking window creation (ARCH-7). Command `git_status` returns the result; command `git_recheck` runs detection again.
- UI: when the result is not usable, a full-window setup screen replaces the app. It states what is wrong (not found, or found version X but 2.40 is required), shows the path that was checked, gives a one-line install hint per platform, and has a "Check again" button. The git path and version are rendered as text, never as HTML (SEC-3).
- Tests:
  - `arch_3_runner_passes_arguments_without_a_shell`: runs `git --version` plus a run with an argument containing spaces, quotes and `;`, asserting it arrives as one argument (use `git -c alias.x=... ` or `git config --file <tmp> --get` on a temp file; no network, no repo needed).
  - `arch_3_runner_sets_c_locale_and_disables_prompts`: asserts the environment passed to the process.
  - `plat_3_version_parsing`: table test over the version strings above plus garbage input.
  - `plat_3_detects_git_on_path`: with real git on the runner.
  - `plat_3_reports_not_found`: with an empty `PATH` and no explicit path.
  - `plat_3_reports_too_old`: using a fake `git` script that prints `git version 2.30.0` (shell script on Unix, `.cmd` on Windows; built in code in a temp dir, QA-2).
  - Vitest `PLAT-3 setup screen`: the fake core returning `TooOld` shows the setup screen with both versions; returning usable shows the app.

## Out of scope
- Any other git command, porcelain parsers, `gix` (M1).
- Bundling git (D13 says system git only).
- The settings UI for an explicit git path (M5 settings; the parameter exists from this task).
- The askpass helper (M2).

## Acceptance
- With git 2.40 or newer installed, `just dev` shows the normal page and `gitcroc-cli git-info` prints the path and version.
- With `PATH` cleared, the setup screen appears; restoring it and pressing "Check again" shows the app without a restart.
- All tests listed above exist and pass on all three platforms.
