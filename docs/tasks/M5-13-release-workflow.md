# M5-13 Release workflow

## Goal
Pushing a version tag builds every package of section 11 on CI, signs the update packages with the project's updater key, and prepares a draft GitHub release with the files, checksums, release notes and the `latest.json` the updater reads. Eric reviews and publishes it; nothing is published automatically.

## Depends on
M5-12

## Spec
Implements: SEC-7
Respects: PLAT-4, D6, D12, D14, QA-1, section 11 platform table

## In scope
- `crates/gitcroc-app/tauri.release.conf.json`, merged with `--config` only by the release workflow: `bundle.createUpdaterArtifacts: true`. Nothing else differs from the normal build.
- `.github/workflows/release.yml`:
  - Runs on a pushed tag `v*`. It also runs as a dry run, which builds and uploads workflow artifacts but creates no release, by hand (`workflow_dispatch`) and on pull requests that change the workflow, the release config or `tools/release-manifest`, as `build.yml` does for its files.
  - Fails at once unless the tag is `v` plus the workspace version (`one_version_number`, M0-08).
  - Builds on the same runners and with the same steps as `build.yml` (reuse them; do not copy them by hand into a second file when a reusable workflow or composite action can share them): Windows NSIS installer and portable zip, macOS universal `.dmg` and the updater's `.app.tar.gz`, Linux AppImage, `.deb` and `.rpm`. The private key and its password come from the secrets of M5-12 and appear only as environment variables of the build step.
  - Unsigned for Windows and macOS, macOS ad-hoc signed, as today (SEC-7). No signing certificate or notarization setting anywhere.
  - Collects the files under the names Tauri gives them (the portable zip as `build.yml` names it), the `.sig` files of the update packages, and `SHA256SUMS`.
  - Builds `latest.json` with `tools/release-manifest` (below) and attaches it.
  - Creates a draft release named `gitcroc <version>` with all files. The notes are the version's section of `CHANGELOG.md`, followed by a fixed paragraph that links `docs/install-unsigned.md` (SEC-7: the download page explains how to get past the warning) and names the file for each platform. A version with a pre-release part (`1.0.0-rc.1`) is marked as a pre-release.
  - Only the job that creates the release has `contents: write`.
- `tools/release-manifest`, a tools crate like `req-coverage`: reads the version, the notes, the publication date and the `.sig` files, and writes Tauri's `latest.json`: `windows-x86_64` (the NSIS installer), `darwin-x86_64` and `darwin-aarch64` (both the universal `.app.tar.gz`), `linux-x86_64` (the AppImage), with download URLs `https://github.com/ericlass/gitcroc/releases/download/v<version>/<file>`. The `.deb`, `.rpm` and the portable zip are not in it (PLAT-4).
- `CHANGELOG.md` at the repo root, "Keep a Changelog" style, with an `Unreleased` section; the workflow fails when the version has no section.
- `build.yml` keeps building on pushes to `main` without the updater key, and its header comment no longer says the updater is M5.
- `RELEASING.md` for Eric: bump the version, write the changelog section, push the tag, watch the workflow, check the draft (files, `latest.json`, notes), publish it. A pre-release is published as a pre-release; GitHub's `releases/latest` ignores those, so updaters only see final versions. Pushing tags and publishing releases is Eric's; an agent never does it.
- Tests:
  - `sec_7_builds_are_unsigned_and_macos_is_ad_hoc_signed`: the macOS signing identity is `-`, and no config file sets a Windows certificate, a macOS signing identity other than `-`, or notarization credentials.
  - `sec_7_only_the_release_config_creates_update_packages`.
  - In `tools/release-manifest`: `plat_4_manifest_lists_installer_app_and_appimage_only`, the URLs and platform keys, a missing `.sig` is an error, the notes come from the changelog section.
  - The changelog section is found for a version and missing for another.
- The pull request's own dry run is green and lists the files a release would get. It creates no release.

## Out of scope
- Pushing a tag, creating or publishing a real release (M5-20, M5-30, by Eric).
- Installing and updating the built packages (M5-14, M5-15). Code signing and notarization (SEC-7, LATER).

## Acceptance
- The tests above exist and pass, `just check` passes, and the dry run is green.
- `just coverage spec --report-only` no longer lists SEC-7.
