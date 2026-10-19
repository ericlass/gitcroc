# M5-11 Git path in Settings

## Goal
The user can point gitcroc at a git executable that is not on `PATH`, from Settings and from the git setup screen, and go back to automatic detection. M0-06 built detection with an explicit path parameter and left the setting to M5.

## Depends on
M5-06

## Spec
Respects: PLAT-3, D13, ARCH-3, ARCH-7, SEC-2, SEC-3, DATA-1, PLAT-1, PLAT-2, D15, section 10 "Files"

## In scope
- Settings: an optional top-level `git_path` in `settings.toml`, schema unchanged, as earlier optional keys were added (README "Settings keys in M5"). Absent means automatic detection. Stored as an OS path (PLAT-2).
- Detection passes `git_path` as its explicit path and keeps M0-06's order unchanged: a chosen git that exists is used, and reported when it is too old or unreadable; a chosen path with no executable falls through to `PATH` and the other places, as today. The result gains a flag for that case, so Settings can say "The chosen git was not found; using <path>" instead of the user silently getting another git.
- IPC: `settings_set_git_path(path | null)` saves the setting and starts detection again; the result arrives as the `git_status` event (ARCH-7). `git_pick_path` opens the platform's file dialog from Rust, as the folder picker does (SEC-2; the front end gets no dialog permission), and on a choice does the same as `settings_set_git_path`; a cancelled dialog does nothing.
- Settings dialog: a Git section (the version and path shown today move into it) with **Choose git…** and **Use automatic detection** (shown when a path is set).
- Git setup screen (M0-06): gains **Choose git…**, and **Use automatic detection** when a path is set, beside **Check again**, so a user whose git is not on `PATH` can get past it.
- Paths are shown as text (SEC-3). Text comes from the strings module (D15).
- Tests:
  - `plat_3_chosen_git_is_used_first`, `plat_3_chosen_git_too_old_is_reported`, `plat_3_missing_chosen_git_falls_through_and_says_so`, `plat_3_clearing_the_path_detects_again`: with fake git scripts built in a temp dir (QA-2), as M0-06's tests.
  - Settings: `git_path` round trip, absent by default, and a file without it loads unchanged (DATA-1).
  - Vitest: Settings' Git section and the setup screen with a path set and without.
  - Playwright: choosing a path through the fake core's dialog reaches the app after a `git_status` event.

## Out of scope
- Bundling git (D13). Detecting several installed gits and offering a list.

## Acceptance
- The tests above exist and pass on all three platforms, and `just check` passes.
