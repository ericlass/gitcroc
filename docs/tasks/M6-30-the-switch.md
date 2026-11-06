# M6-30 The switch

## Goal
The GPUI app is gitcroc. The Cargo binary is named `gitcroc`, and the
old web front end is gone along with everything that only it needed:
`ui/`, specta, Node, pnpm and Vite. `just coverage` counts Rust tests
only, and `just check` also covers M6.

## Depends on
`just coverage M6` empty: true since M6-29d, and still true with M6-29e
(#160) merged. All of M6-29a to M6-29f are merged.

## Spec
Respects: D2, ARCH-5, QA-1, QA-3, QA-5, PLAT-4, DEV-3, DEV-4, section 11
packages, section 12 testing.

This task changes no spec text and no ADR. The spec already describes the
end state since M6-01: D2 has no webview, and QA-1 lists no `tsc`, ESLint,
Vitest or Playwright. The M0 row in section 13 is history and stays as it
is.

## Evidence
Coverage was run on a copy of the tree without `ui/` (2026-11-06, on
`main` at a8aa60e): M0 to M5, M6 and `spec` all report 0 missing. So no
requirement depends only on a test under `ui/`.

## In scope

### The binary is `gitcroc`
- **`crates/gitcroc-ui/Cargo.toml`:** `[[bin]] name = "gitcroc"` and
  `default-run = "gitcroc"`. The "keeps this name until M6-30" comments go.
- **Tests:** `CARGO_BIN_EXE_gitcroc-next` becomes `CARGO_BIN_EXE_gitcroc`
  (`tests/integration/support.rs` and every other use).
- **`justfile`:** `dev-next` becomes `dev`, and `dev`, `bench-ui` and
  `smoke` build and start `gitcroc`.
- **`.github/workflows/ci.yml`:** the first-frame step builds and starts
  `gitcroc`.
- **`.github/workflows/packages.yml`:**
  - Every build step builds `--bin gitcroc`.
  - The `cp … gitcroc-next … gitcroc` copy steps go.
  - `lipo` reads `release/gitcroc` from both targets.
  - The cargo-packager and `cargo-generate-rpm` metadata already name
    `gitcroc` and stay as they are.
  - The packages keep their names, identifier, install directory and
    uninstall key (M6-28a), so the update paths M6-28c tested don't change.
- **Doc comments and user-facing text** that name `gitcroc-next`:
  - `crates/gitcroc-ui/src/main.rs` (usage), `lib.rs` and `perf.rs`;
  - `crates/gitcroc-cli/src/bench_ui.rs` (usage string);
  - `crates/gitcroc-app/tests/integration/app.rs`;
  - `crates/gitcroc-ui/tests/a11y_tree/main.rs` and `manual.md`, whose
    commands become `--bin gitcroc` and `target/release/gitcroc`;
  - `docs/user/images/README.md`;
  - `CONTRIBUTING.md`.
- **Config, cache and workspace files** keep their paths and formats.
  `gitcroc-next` already used gitcroc's, so there is nothing to migrate.

### `ui/` and Node leave the repo
- Delete `ui/` (209 tracked files) and the root `.nvmrc`.
- **`justfile`:** the `ui-reference` recipe goes.
- **`.gitignore`:** the Node and `ui/` entries go.
- **Rust code and tests that read `ui/`:**
  - `ai_1_no_model_name_is_hardcoded` (`gitcroc-app`) scans the `.rs`
    files under `crates` only. Its `files.len() > 50` floor still holds.
  - The D14 check of `ui/package.json`'s license in `workspace_policy.rs`
    goes. The crate license checks stay.
  - `quality_gates.rs`'s coverage fixture stops writing `ui/src/app.ts`.
    The `crates/demo/src/lib.rs` case already proves that non-test code
    doesn't count.
- **Doc comments that cite a port source** under `ui/src` or `ui/e2e`
  (50 mentions in 43 Rust files) now point at the source at the
  `v1.0.0-rc.1` tag, for example "a port of rc.1's
  `ui/src/layout/StatusBar.tsx`". A path to a file that no longer exists in
  the tree is never left bare.
- **What stays:** `update-test.yml` keeps pnpm and Node, because it builds
  1.0.0-rc.1 from its tag (`rc1/ui`, `rc1/.nvmrc`) for the rc.1 to GPUI
  update test, and `update_rc1_build.py` stays unchanged. That uses the
  tag's checkout, not this repo's files. Its comments say so.

### specta leaves
- **`Cargo.toml`:** the workspace `specta` dependency and its comment go.
- **Crate manifests:** the `specta` dependency goes from `gitcroc-git`,
  `gitcroc-core`, `gitcroc-ai` and `gitcroc-app`.
- **Code:** every `specta::Type` derive goes (about 180, in 49 files), as
  does `use specta::Type` in `gitcroc-app/src/ai.rs`. `Serialize` and
  `Deserialize` stay wherever a type is still written to disk, logged or
  printed by the CLI. Where a serde derive existed only for IPC and nothing
  serializes the type any more, it stays as well: removing it is not this
  task's job.
- **`Cargo.lock`:** updated. `cargo deny check` passes, and any
  `deny.toml` entry that existed only for specta's tree goes.

### Coverage counts Rust only
- **`tools/req-coverage`:**
  - `all_test_code` scans `crates/` only. The `OLD_UI` and `NEW_UI`
    constants, `PORT_MILESTONE`, `unported` and the M6 branch in `run` go,
    along with their unit tests.
  - `just coverage M6` becomes the plain milestone report, the same as
    every other milestone.
  - The doc comments say what it scans now.
- **`justfile`:** `check-rust` runs `just coverage M6` after M5, so M6's
  coverage is now required (QA-1, QA-3). M6-31's task file, written later,
  must keep it green.

### Docs outside the spec
- **`CLAUDE.md`:**
  - The first line says the native UI is gitcroc's UI. It drops "being
    written in milestone M6" and the `ui/` sentence.
  - The `just ui-reference` command line goes.
- **`CONTRIBUTING.md`:**
  - **The opening and the `ui/` paragraph:** these say the app is
    `crates/gitcroc-ui`, binary `gitcroc`.
  - **Prerequisites:** the Node and pnpm item goes.
  - **The coverage section:** this says only `crates/` is searched.
  - **The theme section** (line 103) describes the native rules. Colors
    live only in `crates/gitcroc-ui/src/theme`, and the UI-23 test in
    `tests/integration/colors.rs` enforces this.
  - **The strings section** (line 109) describes `src/strings.rs`, and the
    D15 test in `tests/integration/strings.rs` enforces it.
  - The wording of both sections follows what those tests check today, not
    the old front end's rules.
  - **The Builds section:** this says `packages.yml` builds the packages
    from `gitcroc`. It drops the "no packages until M6-28" text.
- **`docs/tasks/README.md`:**
  - The M6-30 row links this file.
  - The decision rows that say "until M6-30" (how M6 is developed, `ui/`
    during M6) and the "Summary IPC types" row get a line saying they
    ended with M6-30.
  - The rows themselves stay, as the record.
- **Left as they are:** `docs/plans/native-ui-gpui.md`, ADR 0004 and the
  finished task files. They are history.

## Tests (QA-3)
- **`d2_the_app_binary_is_gitcroc`** (`workspace_policy.rs`):
  - `gitcroc-ui` has exactly one non-test binary target, named `gitcroc`,
    and it is the package's `default-run`;
  - no workspace manifest, workflow or `justfile` names `gitcroc-next`.
- **`d2_no_node_toolchain_in_the_repo`** (`workspace_policy.rs`): no
  tracked `package.json`, `pnpm-lock.yaml`, `.nvmrc` or `ui/` directory
  exists outside `spikes/`. Workflows reach Node only by checking out
  `v1.0.0-rc.1` (the `rc1/` paths in `update-test.yml`).
- **`d2_no_crate_depends_on_specta`** (`workspace_policy.rs`): no
  workspace package depends on `specta`, directly or through the lock
  file.
- **`qa_3_coverage_scans_crates_only`** (`req-coverage`'s unit tests): in
  a fixture, a requirement ID mentioned only in a test file under `ui/`
  counts as missing for any milestone, M6 included.
- **`qa_3_only_rust_files_hold_test_code`** (`req-coverage`'s unit tests):
  this replaces the front end case of the test-file rules.
- **The existing tests still pass:**
  - `qa_3_coverage_script_lists_ids_without_test` and the other
    `req-coverage` tests that remain;
  - `ai_1_no_model_name_is_hardcoded`;
  - D14 and the D2 webview tests;
  - the smoke test, the a11y tree test and the first-frame CI step, all
    under the new binary name.

## Out of scope
- **Release work:** a version bump, the changelog for rc.2, publishing
  and the manual screen reader scripts (M6-31).
- **Spec and ADR edits.** If the switch turns up spec text that still
  describes the old front end, stop and ask (DEV-2, DEV-3).
- **The plan and finished task files:** these are not rewritten.
- **Removing serde derives** that only IPC needed.
- **Building rc.1 from its tag** in `update-test.yml`: this stays until
  rc.1 is no longer an update source.

## Size (DEV-4)
- **Deletions:** about 53,000 lines, from `ui/`, plus about 180 derive
  lines.
- **Other changes:** about 300 changed lines outside tests (renames,
  workflows, `req-coverage`, docs). That is within DEV-4's 400.
- **One pull request:** the deletions are mechanical. Splitting off specta
  would leave the binary rename and the `ui/` deletion, which touch the
  same files (`justfile`, `CONTRIBUTING.md`), on two branches. So this
  stays one pull request. Its description lists the deletions separately
  from the reviewed changes.

## Acceptance
- `just check` passes. It now includes `just coverage M6`, and every
  coverage report lists 0 missing.
- `cargo tree -i specta` finds nothing.
- `git ls-files ui .nvmrc` is empty.
- CI is green on all three platforms, including the first-frame step,
  the smoke test and the a11y tree test under the name `gitcroc`.
- The packages workflow builds every package from `--bin gitcroc` with no
  copy step, and its install tests pass.
