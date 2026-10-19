# M5-18 User documentation

## Goal
A user can install gitcroc, set it up and use every feature from the documentation in `docs/user/`, without reading the spec.

## Depends on
M5-09, M5-11, M5-12, M5-13

## Spec
Respects: D4, D10, D11, D12, D13, D15, SEC-1, SEC-6, SEC-7, AI-5, AI-6, AI-16, PLAT-4, section 10 "Files", section 11 platform table

## In scope
- Markdown pages in `docs/user/`, written for users: plain words, no requirement IDs, the UI's names for things exactly as the strings module has them (D15).
  - `README.md`: what gitcroc is, what it needs (git 2.40 or newer, D13; the supported systems of D6), and the list of pages.
  - `install.md`: which file to download for each platform, installing it, the unsigned-build warnings and how to get past them (moved here from `docs/install-unsigned.md`, SEC-7), WebView2 on Windows, and how updates arrive for each package (PLAT-4).
  - `getting-started.md`: the window (tabs, toolbar, tree, history, changes, status bar), adding repos, a first commit.
  - `workspaces-and-groups.md`: workspaces and tabs, groups, the target set and multi-selection, the derived branch and "mixed", export and import for sharing a workspace.
  - `operations.md`: every operation of sections 5 and 6 by area; group operations, preflight and its classes, the confirmation, the result report and Retry failed; what the app never does for the user (stash, commit or discard to make an operation succeed).
  - `conflicts-and-ai.md`: the conflict banner, the conflict editor, Mark resolved and Continue; AI providers (API keys and CLIs), the consent dialog, exactly what is sent (AI-5), the workspace switch and the AI features switch, keys kept in the OS keychain (SEC-1), and that AI only proposes.
  - `keyboard.md`: the command palette and every shortcut, generated from or checked against `ui/src/keys.ts` (M5-09).
  - `settings-and-files.md`: every setting, and where `settings.toml`, the workspace files, `cache.db` and `gitcroc.log` live on each platform (section 10); that nothing is stored in a repo.
  - `privacy.md`: the network calls the app makes (git operations, AI calls the user configured, the update check) and how to turn the update check off (D12); no telemetry.
  - `troubleshooting.md`: git not found or too old and the git path setting (M5-11), `safe.directory` and other git safety errors (SEC-6), credentials and the askpass prompt (D10), an unreachable keychain, where the log is and what to attach to a bug report.
- `docs/install-unsigned.md` is removed; every link to it (the release notes paragraph of M5-13, `docs/tasks/M0-signoff.md`, `README.md`) points to `docs/user/install.md`.
- A link test, `user_docs_links_resolve` in `crates/gitcroc-cli/tests/integration/`: every relative link in `docs/user/`, `README.md` and `CONTRIBUTING.md` points to a file that exists, and every `#anchor` to a heading in it.
- A keyboard test: every shortcut in `keys.ts` appears in `keyboard.md` with its keys (Vitest reading the Markdown).
- No screenshots in this task (README "Website and screenshots").

## Out of scope
- The project README (M5-19). A website. Translations.

## Acceptance
- The two tests pass, `just check` passes, and every page has been checked against `just dev`: each name and step it gives exists in the app. The pull request lists anything in the app found to be wrong or confusing while writing, as follow-ups; this task does not change the app.
