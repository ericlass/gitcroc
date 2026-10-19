# M5-20 Release candidate and M5 sign-off

## Goal
gitcroc 1.0.0-rc.1 is built by the release workflow, published by Eric as a pre-release, and verified against M5's done condition, except the final update to 1.0.0 (M5-30). Every requirement ID of the spec has a test, and `just check` enforces it.

## Depends on
M5-01 to M5-19

## Spec
Respects: QA-1, QA-3, QA-5, PLAT-4, SEC-7, PERF-8, D6, D12, ADR 0003, section 9 budgets and stretch targets, section 13 M5 row

## In scope
- Version `1.0.0-rc.1` in the workspace `Cargo.toml` (and `ui/package.json` if it repeats it; `one_version_number`, M0-08), and its `CHANGELOG.md` section: what v1 does, by area.
- Before the version change is final, check that every package orders the pre-release before `1.0.0`, so the later update is not seen as a downgrade: the updater's semver comparison, `dpkg --compare-versions` for the `.deb`'s version as Tauri writes it, `rpmdev-vercmp` (or `rpm`'s own comparison) for the `.rpm`'s, and that the NSIS installer and the macOS bundle accept the version string. If any format refuses it or orders it after `1.0.0`, stop and ask (DEV-2); do not invent another version scheme.
- `just check-rust` runs `just coverage M5`, and `just coverage spec` without `--report-only`: both must report 0 missing (QA-3).
- `docs/tasks/M5-signoff.md`, filled in by the person verifying, in the form of the earlier sign-offs:
  - Requirement coverage: the output of `just coverage spec` (covered, no test needed with ADR 0003's four, 0 missing) and `just coverage M5`.
  - The release: Eric pushes the tag `v1.0.0-rc.1` (never an agent), the release workflow's run, the draft checked (all files, `SHA256SUMS`, `latest.json` with its four platform keys, the notes and the unsigned-build paragraph), and published as a pre-release.
  - CI proof on the tagged commit, with links: the install jobs (M5-14), the update tests (M5-15), the smoke test on Linux and Windows and the keychain test on Windows and macOS (M5-16). These are the Windows and macOS proof (README "Windows and macOS proof").
  - By hand on Linux, from the published pre-release: the AppImage downloads and starts; the `.deb` or the `.rpm` installs and starts; the git version in Settings matches `git --version`; Settings' Updates section shows a check (it finds nothing newer, since `releases/latest` ignores pre-releases).
  - Budgets: `just bench` and `taskset -c 0-7 just bench-ui` on the reference machine, every section 9 row within budget, memory included (M5-17), in a table beside M4's; the stretch results reported (PERF-8: UI-side budgets are measured before each release).
  - Accessibility, by hand on Linux with Orca: open a workspace, move through the tree and the history, stage and commit, open the palette and run Fetch on a group, read the result report, resolve a conflict with take ours and Mark resolved. Each step's announcement noted.
  - Keyboard only, by hand: the mixed-branch scenario of section 5 from start to end without the mouse.
  - Documentation: each page of `docs/user/` read against the release candidate.
  - Windows and macOS: install and update covered by CI as above; budgets, screen readers and the by-hand checks deferred, as decided for M0 (no machine).
- Anything the verification finds wrong becomes a task numbered from M5-21 (up to M5-29, so it comes before M5-30 in file order); each is fixed before M5-30 and listed in the sign-off. A fix that changes a package means a new release candidate (`rc.2`), with the release and by-hand checks of this file repeated for it.

## Out of scope
- v1.0.0 and the update from the release candidate to it (M5-30).

## Acceptance
- `just check` passes with both coverage checks enforcing.
- The release candidate is published as a pre-release, and `docs/tasks/M5-signoff.md` is filled in for it, with every check above passing or turned into a follow-up task.
