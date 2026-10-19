# M5-03 Tests for the decision log

## Goal
Every row of the decision log that no test names yet has a test that proves it, so `just coverage spec` no longer lists D1, D2, D3, D5, D7, D8 or D13.

## Depends on
M5-02

## Spec
Implements: D1, D2, D3, D5, D7, D8, D13
Respects: QA-2, QA-3, QA-4, ADR 0001, ADR 0003, section 4 "Stack" and "Crates" tables, section 9 "Reference data"

## In scope
- Each test proves its decision; it does not only mention the ID. Where an existing test already proves it in full, the ID is added to that test's name or doc comment instead, and the pull request names the test and says why it proves the decision. Workspace-wide tests go in `crates/gitcroc-cli/tests/integration/workspace_policy.rs`, beside ADR 0001's QA-4 test.
- D1 Rust, stable toolchain: `d1_toolchain_is_a_pinned_stable_release`. `rust-toolchain.toml` names a version number (`1.99.0`), not `stable`, `beta` or `nightly`; the workspace `Cargo.toml` sets edition 2024 and its `rust-version` is not newer than the pinned toolchain (section 4 "Stack").
- D2 Tauri 2 shell, TypeScript and SolidJS front end: `d2_shell_is_tauri_2_and_ui_is_solid_and_strict_typescript`. The workspace pins `tauri` at major version 2; `ui/package.json` depends on `solid-js` and `typescript` and on no other UI framework (React, Vue, Svelte, Angular, Preact); `ui/tsconfig.json` has `strict: true`.
- D3 hybrid git: the read half by the `arch_2_*` tests and the write half by `arch_1_writes_go_through_the_runner`, if they prove it in full; add a test only for a gap, for example that `gix` is a dependency of `gitcroc-git` only (section 4 "Crates").
- D5 scale target: `d5_reference_data_matches_the_scale_target`, a unit test in `tools/repo-gen` on the set specs, without generating data:
  - `target-project`: 20 repos in 4 groups, 200,000 tracked files and 2 million lines in total, one repo with 60,000 files, 5,000 commits and 30 branches per repo (section 9);
  - `many-repos`: 200 repos in 10 groups, 2,000 commits and 20 branches each;
  - `big-tree`: 500,000 tracked files.
  - The README's "Sizes the spec leaves open" stays as it is.
- D7 group membership: `d7_a_repo_can_be_in_no_one_or_several_groups_and_groups_do_not_nest`, on the core's workspace model: a repo in none, one and three groups; a group cannot hold a group (the model has no way to express it, so the test reads a workspace file whose `repos` names a group id and expects it refused or ignored, as the M3-04 loader does today; if it does neither, stop and ask).
- D8 group operation model: `grp_6_one_failure_does_not_stop_others` and the result report tests prove "independent, parallel, never atomic, no rollback, always a report" together; add the ID where they prove it. If no test shows that a repo that succeeded stays changed after another failed (no rollback), add `d8_no_rollback_after_a_failure`.
- D13 git 2.40 or newer, not bundled: `d13_requires_git_2_40_or_newer`: `MIN_GIT` is `(2, 40, 0)`, 2.39.9 is too old and 2.40.0 is usable; and no bundle config in `crates/gitcroc-app/` lists a git binary as a resource or sidecar.

## Out of scope
- D12, D15, SEC-7 and PLAT-4: their own M5 tasks add their tests.
- Changing any decision.

## Acceptance
- `just coverage spec --report-only` no longer lists D1, D2, D3, D5, D7, D8 or D13 as missing.
- The tests pass on all three platforms, and `just check` passes.
