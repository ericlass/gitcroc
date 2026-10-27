# M6-27 The updater without Tauri

## Goal
`gitcroc-app` has its own update source (PLAT-4). It reads the same `latest.json` and checks each package with the same public key as 1.0.0-rc.1, then installs it on Windows, macOS and the AppImage without Tauri's updater plugin.

The Tauri app switches to it now. The existing update test then proves the new source on all three runners, months before the GPUI build ships, and the GPUI app reuses it unchanged. The other half of PLAT-4, rc.1's Tauri plugin installing the GPUI packages, needs those packages, so M6-28 tests it.

## Depends on
M6-05, PR 4, which moves `Updates` behind `App`.

## Spec
Implements: PLAT-4
Respects: SEC-2, SEC-7, D12, PLAT-1, QA-4, DATA-1, section 11 packages

## In scope
- **`OwnSource`** (`crates/gitcroc-app/src/update/`) implements the core's `UpdateSource`.
- **The check:**
  - **Request:** one `GET` of the fixed endpoint, `https://github.com/ericlass/gitcroc/releases/latest/download/latest.json`. It uses the workspace's `reqwest` with rustls and the system proxy, and has a timeout.
  - **Manifest:** `version`, `notes`, `pub_date` and `platforms`. Reading it fails cleanly on missing fields.
  - **Platform key:** the one `tools/release-manifest` writes: `windows-x86_64`, `darwin-x86_64`, `darwin-aarch64` or `linux-x86_64`.
  - **Version:** an update is found only when the manifest's version is a newer semver than the running one, so rc.1 < rc.2 < 1.0.0. Equal and older versions are not updates.
  - **Download URL:** it must be `https`. Any other scheme is refused.
- **The download and verification (SEC-7):**
  - **Progress:** the download reports progress through the core's `UpdateProgress`.
  - **Signature:** the `.sig` (base64 of a minisign signature) is checked with `minisign-verify` against the public key, which is compiled in as a constant. The constant equals today's `pubkey` in `tauri.conf.json`.
  - **Version:** as Tauri's updater 2.14 does, the trusted comment's `version:` must equal the manifest's version, so an old signed package can't be served as a new version.
  - **Before verification:** nothing is written next to the installed app until the package is verified.
- **Installing, per platform.** Each install step is a function of paths and bytes, so every platform's logic is tested on every platform, as `platform/package.rs` is.
  - **Windows (NSIS installer):**
    - The installer is written to a temporary file.
    - It is started detached with the arguments Tauri's updater passes in passive mode: `/P /UPDATE /R /ARGS`, followed by the app's own arguments.
    - The app then exits.
    - Use `std::process::Command` with creation flags, not `ShellExecuteW`, so there is no `unsafe` (QA-4).
  - **macOS (`.app.tar.gz`):**
    - It is unpacked into a staging directory on the same volume as the running `.app`.
    - The running `.app` is moved to a backup and the new one into its place, with renames.
    - The backup goes back if any step fails, and is removed after success.
    - If the folder isn't writable, the install fails with a message pointing to the download page, instead of asking for an admin password.
  - **AppImage:**
    - The new AppImage, checked to be ELF, is written to a temporary file next to `$APPIMAGE`.
    - It gets the old file's mode and replaces it with a rename. A failure leaves the old file as it was.
  - **`.deb`, `.rpm`, the portable zip and unknown packages:** these only notify, as now (`Package::installs`).
  - **Restart:** after a successful install the service reports it, and the UI restarts the app: Tauri's `restart()` now, GPUI's later.
- **The Tauri app switches to `OwnSource`:**
  - `gitcroc-tauri` drops `tauri-plugin-updater` and its `plugins.updater` config.
  - `build.rs`'s test that read `pubkey` from `tauri.conf.json` now reads the constant.
- **Endpoint and key in the CI update test only:**
  - The `update-test` feature builds take the loopback endpoint and test key from build-time variables, which `update_build.py` sets.
  - A build without the feature has no way to change them.
  - `update_build.py` writes these variables instead of the Tauri overlay's `plugins.updater`. It still signs with `tauri signer` while the Tauri bundler builds the packages.
- **Dependencies:** `minisign-verify`, `semver`, and `flate2` with `tar` (macOS only). `minisign` is a dev-dependency, to sign test packages. `cargo deny` passes.

## Tests (QA-3)
- `plat_4_manifest_platform_key_matches_release_manifest`: the key for each OS and architecture is the one `tools/release-manifest` writes.
- `plat_4_only_a_newer_version_is_an_update`: 1.0.0-rc.1 to 1.0.0-rc.2 and to 1.0.0 are updates; the same and older versions aren't.
- `plat_4_manifest_without_this_platform_finds_no_update`.
- `sec_7_refuses_a_package_with_a_bad_signature`.
- `sec_7_refuses_a_signature_for_another_version`.
- `sec_7_refuses_a_download_url_that_is_not_https`.
- `sec_7_public_key_is_the_rc1_key`: it is the same key as `tauri.conf.json` on `v1.0.0-rc.1`. The value is copied into the test with a comment, so M6-30 can delete the file.
- `sec_7_release_build_endpoint_and_key_are_fixed`: without `update-test`, the endpoint and key are the constants.
- `plat_4_nsis_update_arguments_match_tauri`: the arguments are the same as `tauri-plugin-updater` 2.14.0 passes in passive mode with restart.
- `plat_4_appimage_is_replaced_in_place_with_its_mode`.
- `plat_4_failed_appimage_install_leaves_the_old_file`.
- `plat_4_mac_app_swap_restores_the_backup_on_failure`.
- `plat_4_nothing_is_written_before_verification`.
- **The update test workflow** (`update-test.yml`): A updates to B on Windows, macOS and Linux through `OwnSource`, and the refusal checks still pass.
- `d12_debug_builds_never_check`, or the existing test of `CHECKS`, still passes.

## Out of scope
- **Packaging the GPUI app**, and testing rc.1's Tauri plugin installing GPUI packages (M6-28).
- **The update notice and its buttons in the GPUI app** (M6-15 and M6-26). This task delivers the service they call.
- **MSI**, and self-installing `.deb` or `.rpm` packages. Both stay notify-only.

## Acceptance
- The tests above pass on all three platforms in `just check`.
- `update-test.yml` passes on the PR on all three runners. Its `paths:` filter now includes `crates/gitcroc-app/src/update/**`.
- `cargo tree -p gitcroc-tauri` shows no `tauri-plugin-updater`.
- The released `latest.json` format and the signing step in `release.yml` haven't changed.
