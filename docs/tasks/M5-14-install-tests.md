# M5-14 Install tests on the CI runners

## Goal
Every package of section 11 is installed on a GitHub-hosted runner of its platform and the installed app starts, on every push to `main`. On Windows and macOS this is the proof that gitcroc installs, since there is no machine to install it on by hand (Eric, 2026-10-19; README "Windows and macOS proof").

## Depends on
M5-13

## Spec
Respects: D6, D13, PLAT-3, PLAT-4, SEC-7, DATA-3, section 9 "Timing in CI" (README), section 11 platform table, section 12 "End to end smoke" row

## In scope
- A start line in `gitcroc.log`: `gitcroc <version> started (<os> <arch>, <package>)`, written once at start, where `<package>` is what M5-12 detects (installer, app, AppImage, deb, rpm, portable). It carries no path or file content (section 10). A test checks the line.
- `build.yml` gains an install job per platform, after the build job, on the artifacts it just built. Each waits for an outcome, never for a time: a deadline only ends a hung job (README "Timing in CI").
  - Windows (`windows-latest`): the NSIS installer with `/S` installs for the current user; the installed `gitcroc.exe` starts; the start line appears with `installer`; the uninstaller with `/S` removes the program. The portable zip unpacks and its `gitcroc.exe` starts, with `portable`.
  - macOS (`macos-latest`, Apple silicon): the `.dmg` mounts; the app is copied to a temporary Applications folder; `codesign` shows an ad-hoc signature and `lipo` shows `x86_64` and `arm64` (D6); the app starts and the start line appears with `app`.
  - Linux (`ubuntu-22.04`, glibc 2.35, D6): the AppImage starts under `xvfb-run` with `AppImage`; the `.deb` installs with `apt` (its dependencies resolve) and starts, with `deb`; the `.rpm` installs with `dnf` in a Fedora container, checked by `rpm -q`, without starting.
  - Before the first start, each job writes a `settings.toml` with `update_check = false` into the platform config dir, so no install test contacts GitHub (D12; M5-15 tests updates against a local server).
  - The app is then ended. Each job uploads `gitcroc.log` when it fails.
- Each job also checks that the started app found git (PLAT-3): the log shows the `git --version` run with exit code 0.
- The job runs on pushes to `main`, by hand, and on pull requests that change `build.yml`, the bundle config or the install scripts. It is not part of QA-1's required checks.
- Scripts the jobs run live in `.github/scripts/` (bash, and PowerShell on Windows where bash is awkward), each with a comment saying what it checks.

## Out of scope
- Updates (M5-15). Intel Macs (best effort, section 9; no Intel runner is used). Windows 10 itself: `windows-latest` is Windows Server; the pull request notes which version ran.
- Starting the `.rpm` build (no display in the container).

## Acceptance
- The install jobs are green for all six packages on the pull request's run, linked in the pull request, and `just check` passes.
- A broken package fails its job: shown in the pull request with a throwaway commit that is not merged (for example a missing binary in the portable zip).
