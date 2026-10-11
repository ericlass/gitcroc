# M0-08 Build artifacts and M0 sign-off

## Goal
CI produces installable builds for Windows, macOS and Linux, and milestone M0 is verified against its acceptance criteria.

## Depends on
M0-06, M0-07

## Spec
Implements: QA-1
No test needed: QA-1 (the green workflows and the sign-off checklist are the proof)
Respects: D6, SEC-7, section 11 platform table, PLAT-4

## In scope
- `.github/workflows/build.yml`, triggered manually and on pushes to `main`:
  - Windows x64: NSIS installer and a portable zip. Unsigned (SEC-7).
  - macOS: universal binary (`universal-apple-darwin`) as a `.dmg`, ad-hoc signed (signing identity `-`), not notarized (SEC-7).
  - Linux x86_64 on `ubuntu-22.04`: AppImage, `.deb`, `.rpm`.
  - Each uploaded as a workflow artifact named `gitcroc-<version>-<platform>`.
- Version comes from the workspace `Cargo.toml` only; `tauri.conf.json` and `ui/package.json` read it or are checked against it by a test (`one version number`).
- Updater is not configured in this task (PLAT-4 is M5). The build must not contain an update endpoint.
- Switch `just coverage M0` inside `just check` from report-only to enforcing (see M0-02).
- `docs/tasks/M0-signoff.md`: a checklist filled in by the person verifying, with one line per platform:
  - Artifact downloads and installs (note any OS warning shown for the unsigned build).
  - Window opens with the three-pane layout.
  - Theme switch works and survives restart.
  - With git present the app loads; the reported git version matches `git --version`.
- `docs/install-unsigned.md`: how to get past the unidentified-developer warning on Windows (SmartScreen "More info", "Run anyway") and macOS (right-click, Open, or remove the quarantine attribute), which SEC-7 says the download page must explain.
- Update `CLAUDE.md`: remove the remaining "do not exist yet" note for `just dev`; keep it for `just bench`.

## Out of scope
- Code signing, notarization, auto-update, release publishing (M5).
- Windows on ARM and ARM Linux builds (section 2, LATER).
- WebDriver smoke tests (M1).

## Acceptance
- The build workflow finishes green and offers six files: Windows installer, Windows zip, macOS dmg, AppImage, deb, rpm.
- `just check` passes with coverage enforcing: every ID on an "Implements:" line of an M0 task has a test or a "No test needed" reason.
- `docs/tasks/M0-signoff.md` is completed for all three platforms by a human. On macOS, Apple silicon is required; Intel is best effort (section 9).
