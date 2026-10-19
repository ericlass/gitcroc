# M5-01 App identifier

## Goal
The app ships with the identifier `dev.gitcroc.desktop` instead of `dev.gitcroc.app`, before the first release makes a change cost user data. This closes the open question in section 15 (Eric, 2026-10-19).

## Depends on
M4

## Spec
Respects: D6, SEC-2, DATA-1, section 15 "App identifier", section 10 "Files"

## In scope
- `crates/gitcroc-app/tauri.conf.json`: `identifier` becomes `dev.gitcroc.desktop`.
- The "App identifier" row of the task README's choices table names the new identifier and this task.
- Look for every other place that names the old identifier and change it: config overlays such as `tauri.perf.conf.json`, workflows, the smoke and bench drivers, docs and tests. Generated files under `crates/gitcroc-app/gen/` are regenerated, not edited by hand.
- What does not change, and the pull request says so:
  - gitcroc's own files: the config, cache and log dirs are named `gitcroc` (M0-07) and do not depend on the identifier;
  - keychain entries: service `gitcroc`, account `ai-provider-<id>` (M4-05).
- What moves: the webview's own storage, which holds no source of truth (ARCH-6). On the developer's machine it starts empty once.
- This task may edit `docs/spec/` and `docs/adr/` (DEV-3):
  - `docs/adr/0002-app-identifier.md`: context (Tauri's advice against an identifier ending in `.app`, what the operating systems key to it), the decision, and its consequences, in the format of ADR 0001.
  - Section 15: the "App identifier" question is checked and states the answer in one sentence, citing the ADR. The count in the section's first sentence becomes "All ten questions are answered".
  - The spec change is its own commit, without code (DEV-1).
- Tests in `crates/gitcroc-app/tests/integration/build.rs`:
  - `the_identifier_is_dev_gitcroc_desktop`: `tauri.conf.json` has the new identifier, and no overlay config in `crates/gitcroc-app/` sets another.

## Out of scope
- Moving data from the old identifier: no release has used it.
- Renaming the config, cache or log dirs.

## Acceptance
- The test passes, `just check` passes, and `just bundle` on Linux builds bundles whose metadata carries the new identifier (the `.deb`'s desktop file and the AppImage's), checked by hand and noted in the pull request.
- ADR 0002 and the section 15 edit are in their own commit.
