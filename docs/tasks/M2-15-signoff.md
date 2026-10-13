# M2-15 Write tests matrix, end-to-end merge conflict and M2 sign-off

## Goal
Milestone M2 is verified: every operation of section 6 has integration tests for success, failure and, where git can stop in one, conflict; a merge conflict is resolved and continued by hand in the real app; and the M1 budgets still hold with jobs running.

## Depends on
M2-01 to M2-14

## Spec
Respects: QA-1, QA-2, QA-3, QA-5, ARCH-1, OPS-1 to OPS-9, section 12 "Git integration" and "End to end smoke" rows, section 13 M2 row, section 9 budgets

## In scope
- `docs/tasks/M2-signoff.md`, filled in by the person verifying:
  - A table with one row per operation of section 6 and the tests that cover its success, failure and conflict paths, by test name. Conflict applies to pull, merge, rebase, cherry-pick, revert, stash apply and stash pop; the others have "n/a".
  - The end-to-end run below, with the date and commit.
  - The `just bench` and `just bench-ui` results against the M1 sign-off. `input-latency` is measured again with a real job running (a fetch of all `target-project` repos from local bare remotes, then a pull of the largest repo), since section 9's row says "while any job runs".
  - Windows and macOS marked deferred, as decided for M0.
- `ops_matrix_every_operation_has_success_failure_and_conflict_tests`: a test that reads a list of section 6's operations kept beside it and checks that a test named for each operation and path exists under `crates/` (by the naming scheme the M2 tasks used). It fails when an operation is added to `gitcroc-git::ops` without being in the list.
- End-to-end smoke (section 12) through `tauri-driver` on Linux, run locally like `just bench-ui` and in CI's Linux job: start the app on a fixture workspace, add a repo, stage a file, commit, merge a branch that conflicts, resolve the file in the conflict editor with Take theirs and an edit, Mark resolved, Continue; the history then shows the merge commit and the repo is clean. No step waits on a fixed time; each waits for its result in the DOM.
- Add `just coverage M2` to `just check`, so every M2 requirement must have a test (QA-3).
- Update `CONTRIBUTING.md`: the job engine, where writes go (`gitcroc-git::ops`), the askpass helper, and how to run the smoke test.

## Out of scope
- Multi-repo flows and the M3 budget rows.
- Windows and macOS runs (deferred).

## Acceptance
- `docs/tasks/M2-signoff.md` is filled in for Linux, and every operation row has its tests.
- The end-to-end smoke test passes locally and in CI.
- `just check` enforces M2 coverage and passes.
