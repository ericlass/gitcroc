# M6-31 Release candidate 1.0.0-rc.2 and the M6 sign-off

## Goal
The release workflow builds gitcroc 1.0.0-rc.2, the first release of the
GPUI app, and Eric publishes it as a pre-release. It is then checked
against M6's done condition (section 13): every UI requirement has a GPUI
test, the section 9 budgets are met with the new scroll budget, and
1.0.0-rc.1 updates to 1.0.0-rc.2 on all three platforms.

## Depends on
M6-30 (#161, merged 2026-11-06). Every M6 row before it is merged too.

## Spec
Respects: QA-1, QA-3, QA-5, PLAT-4, SEC-7, PERF-8, D6, D12, ADR 0003,
ADR 0004, section 9 budgets and stretch targets, section 13 M6 row.

This task changes no spec text and no ADR.

## In scope

### The release pull request (agent)
- **Version:** `1.0.0-rc.2` in the workspace `Cargo.toml`, the only place
  it is written (RELEASING.md step 1). Every crate takes it with
  `version.workspace = true`.
- **Version order.** Before the version is final, record for each package
  how the version is written and how it orders against both
  `1.0.0-rc.1` (the update from rc.1) and `1.0.0` (the update to v1). The
  results go in a table in the sign-off, like M5's "Version order".
  - **Updater:** semver. rc.1 < rc.2 < 1.0.0.
  - **NSIS installer:** M6-28a's installer refuses a downgrade. Check that
    it takes rc.2 over rc.1.
  - **macOS bundle:** check that it accepts the string. The bundle's
    version isn't used for updates.
  - **`.deb` and `.rpm`:** epoch 1 and `~` for `-`, as decided below.
    Record what `dpkg --compare-versions` and `rpmdev-vercmp` (in a Fedora
    container if the machine has no rpm) say for rc.1 to rc.2 and rc.2 to
    1.0.0.
  - **When an order is wrong:** if a format refuses the version, or orders
    rc.2 before rc.1 or after 1.0.0, stop and ask (DEV-2). Do not invent
    another version scheme.
- **Linux package versions: epoch 1** (Eric, 2026-11-06). From rc.2 on,
  the `.deb` and the `.rpm` carry epoch 1 and write a pre-release with `~`.
  So rc.2 is `1:1.0.0~rc.2` and v1 is `1:1.0.0`.
  - **Why:** rc.1's packages have no epoch. Its `.deb` is
    `Version: 1.0.0-rc.1`, and its `.rpm` is version `1.0.0-rc.1`, release
    `1` (both read from the published files, 2026-11-06). Both sort after
    `1.0.0`. Without an epoch, no version sorts after rc.1 and before
    1.0.0 in both formats. Today's `1.0.0~rc.2` `.rpm` sorts before rc.1,
    so `dnf` would call rc.2 a downgrade. With the epoch, rc.1 to rc.2 to
    1.0.0 are upgrades in `dpkg` and in rpm's comparison (checked
    2026-11-06).
  - **`.rpm`:** `epoch = 1` in `[package.metadata.generate-rpm]` of
    `crates/gitcroc-ui/Cargo.toml` (cargo-generate-rpm 0.21.1 reads it).
    The `~` step in `packages.yml` stays.
  - **`.deb`:** cargo-packager 0.11.8 writes `Version:` as given and has
    no epoch setting. After `cargo packager --formats deb`, the step
    unpacks the package with `dpkg-deb -R`, sets
    `Version: 1:<version with ~ for ->` in `DEBIAN/control`, and repacks it
    under the same file name with `dpkg-deb -b --root-owner-group`.
  - **Unchanged:** file names, the AppImage, Windows, macOS and the
    updater's `latest.json`. The epoch lives only in the Linux package
    metadata.
  - **For good:** an epoch is never lowered or removed. Every later `.deb`
    and `.rpm` carries epoch 1, and the tests below enforce it.
  - **Install tests:** `install (linux-x86_64)` and `install (rpm, fedora)`
    each get an "upgrade over rc.1" step. It installs rc.1's package from
    the `v1.0.0-rc.1` release, then the new one with `apt install` or
    `dnf install`. It fails if the package manager refuses the new package
    or calls it a downgrade, and it checks that the installed version is
    the new one. This is the Linux counterpart of
    `install over rc.1 (windows-x64)`.
- **`CHANGELOG.md`:** a `## [1.0.0-rc.2] - <date>` section, written for
  users (RELEASING.md step 2). `Unreleased` is left empty.
  - **The native app:** the whole UI is native. There is no webview, so
    WebView2 and WebKitGTK are no longer needed. The window draws with
    Vulkan or OpenGL on Linux and needs a working graphics driver. The
    Linux packages depend on a system font.
  - **What `Unreleased` holds now:** the M5-21 to M5-27 changes (title
    bar, history columns, settings tabs and the four fixes). Each one is
    kept only after checking that it holds in the GPUI app, and its words
    are corrected wherever the GPUI app behaves differently.
  - **Known issues:** none carried over from rc.1. Its `.deb` and `.rpm`
    issue is fixed by the epoch: rc.2 installs over rc.1 as an upgrade, and
    1.0.0 will install over rc.2 as one.
- **`docs/tasks/M6-signoff.md`:** an empty sign-off in M5's form, with the
  sections listed under "The sign-off" below, for the person verifying to
  fill in.
- **`crates/gitcroc-ui/tests/a11y_tree/manual.md`:** the opening says that
  the Orca script runs at M6-31. The NVDA and VoiceOver scripts stay as
  they are, for when a Windows PC and a Mac are available.
- **`docs/tasks/README.md`:** the M6-31 row links this file.
- **Coverage:** `just check-rust` runs `just coverage M6` and
  `just coverage spec`, both enforcing (M6-30). Both stay at 0 missing.

### The release (Eric)
Pushing the tag and publishing are Eric's. An agent never does either
(RELEASING.md, M6-28b).
- The release pull request is merged, and CI, Build and Update test are
  green on the merge commit. All three run on a push to `main`.
- Eric tags that commit `v1.0.0-rc.2` and pushes the tag. The Release
  workflow's draft is checked against RELEASING.md step 6, including the
  minisign check of one signature with the project's public key.
- The draft is published as a pre-release.

### The sign-off
`docs/tasks/M6-signoff.md` is filled in by the person verifying, in the
form of `M5-signoff.md`:
- **Requirement coverage:** the output of `just coverage spec` and
  `just coverage M6` on the tagged commit: covered, no test needed (ADR
  0003's four), and 0 missing. "Every UI requirement has a GPUI test."
- **Version order:** the table from the release pull request.
- **The release:** the merge commit, the tag, the Release run and the
  checked draft, as M5's checklist has them, with the links.
- **CI proof on the tagged commit,** with links. This is the Windows and
  macOS proof (README "Windows and macOS proof"):
  - **Build:** `install (windows-x64)`, `install over rc.1 (windows-x64)`,
    `install (macos-universal)`, `install (linux-x86_64)` and
    `install (rpm, fedora)` (M6-28a), the last two with their "upgrade
    over rc.1" step.
  - **Update test:** `update (…)` on all three platforms, for GPUI A to B
    and for rc.1 to GPUI (M6-28c). This is "rc.1 updates to rc.2 on all
    three platforms", proven on the runners with the commit rc.2 is built
    from.
  - **CI:** `check-rust (…)` and `check (…)` on all three platforms,
    including the smoke test on Linux and Windows (M6-29a), the a11y tree
    test (M6-25a), and the keychain round trip on Windows and macOS.
- **By hand on Linux, from the published pre-release:**
  - the AppImage downloads and starts;
  - the `.deb` or the `.rpm` installs and starts, saying which one;
  - **an installed rc.1 to rc.2:** rc.1 is installed from its release, and
    rc.2's package is installed over it with `apt` or `dnf`, which treats
    it as an upgrade. The tabs, workspaces and settings are still there;
  - the git version in Settings matches `git --version`;
  - **the real update channel (M6-28c's "out of scope"):** Settings,
    Updates runs a check against
    `releases/latest/download/latest.json`. While no final release exists,
    `releases/latest` serves nothing, because pre-releases are ignored.
    Record what Settings shows. Then check that the `latest.json` on the
    rc.2 release's own page lists version `1.0.0-rc.2` with the four
    platform keys.
- **Budgets:** on the reference machine, on the tagged commit:
  - `taskset -c 0-7 just bench-ui --runs 3` and
    `taskset -c 0-7 just bench --runs 3`, then `--stretch` for the
    stretch sets.
  - Tables in the form of `M6-29-measurements.md`, with M6-29's median in
    a column beside the new one. Every gating row passes, memory and the
    scroll budget included. The stretch results are reported (PERF-8:
    UI-side budgets are measured before each release).
- **Accessibility, Linux:** the Orca script in
  `crates/gitcroc-ui/tests/a11y_tree/manual.md`, on X11 and on Wayland, with
  its table copied into the sign-off and filled in. Also, by hand, the
  focus ring is seen on each Tab stop in the light and dark themes (README
  "Accessibility bar in M6").
- **Keyboard only, by hand:** the mixed-branch scenario of section 5 from
  start to end without the mouse, as in M5's sign-off.
- **Documentation:** each page of `docs/user/` is read against rc.2, and
  its screenshots match the app.
- **Windows and macOS:** install and update are covered by CI, as above.
  Budgets, the NVDA and VoiceOver scripts and the by-hand checks are
  deferred: there is no machine (Eric, 2026-10-12, 2026-10-19, and again
  for M6-31 on 2026-11-06).

### Follow-ups
- **Numbering:** anything the verification finds wrong becomes a task
  numbered M6-31a, M6-31b and so on, which is fixed before the sign-off
  ends and listed under "Follow-up tasks" in the sign-off.
- **A new release candidate:** a fix that changes a package after rc.2
  is published means `1.0.0-rc.3`, with the release, the version-order
  table and the by-hand checks repeated for it.

## Tests (QA-3)
This task adds no requirement. The epoch's tests are named after the
package rule they guard (D6, section 11 packages):
- **`d6_linux_packages_carry_epoch_1`** (`workspace_policy.rs`):
  `[package.metadata.generate-rpm]` has `epoch = 1`, and the `.deb` step
  of `packages.yml` sets a `Version:` starting with `1:`.
- **The build job checks the built packages:** `dpkg-deb -f … Version` is
  `1:<version with ~>`, and the `.rpm`'s epoch, version and release are
  `1`, `<version with ~>` and `1`, read with `rpm -qp` in the Fedora
  install job.
- **The "upgrade over rc.1" steps** above.

The existing tests and checks that guard the release still pass with the
new version:
- the release workflow's changelog check. The dry run finds the `1.0.0-rc.2` section and
  builds every package under the new version. The pull request starts one
  by itself, because it changes `packages.yml`. The run is linked in the
  pull request;
- `d12_update_test_is_not_a_default_feature_and_never_released`;
- both coverage checks in `just check-rust`.

## Out of scope
- **v1.0.0** and the update from the release candidate to it (M5-30).
- **NVDA and VoiceOver on real machines,** and the budgets on Windows and
  macOS: deferred, as above.
- **Spec and ADR edits.** If the sign-off turns up spec text that no
  longer matches the app, stop and ask (DEV-2, DEV-3).

## Size (DEV-4)
The release pull request changes:
- the version line;
- the epoch: one metadata line, about ten lines in the `.deb` step, and
  the two upgrade steps;
- the changelog section, the empty sign-off, a paragraph of `manual.md`
  and a README row.

That's well under 400 lines. It changes `packages.yml`, so it starts the
release dry run by itself. The filled-in sign-off is committed in its own pull request once
the checks are done.

## Acceptance
- `just check` passes with both coverage checks enforcing, on the release
  pull request and on the tagged commit.
- 1.0.0-rc.2 is published as a pre-release.
- `docs/tasks/M6-signoff.md` is filled in for it. Every check above passes
  or is turned into a follow-up task that is then fixed.
