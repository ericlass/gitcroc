# M6-27 The updater without Tauri

## Goal
`gitcroc-app` has its own update source (PLAT-4). It reads the same `latest.json` and checks each package with the same public key as 1.0.0-rc.1, then installs it on Windows, macOS and the AppImage without Tauri's updater plugin.

The Tauri app was removed in M6-05, so this task proves the source with tests on every platform. Updating installed packages end to end needs the GPUI packages, so M6-28 runs it on the runners in both directions: the GPUI build A to B through this source, and rc.1's Tauri plugin to the GPUI build.

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
  - **Restart:** after a successful install the service reports it, and the UI restarts the app (M6-15).
- **`App` uses it:** `App::start` gets `OwnSource` in place of the source that never finds an update (M6-05, PR 4).
- **Endpoint and key in the CI update test only:**
  - An `update-test` feature takes the loopback endpoint and the test key from build-time variables, which M6-28's update test sets.
  - A build without the feature has no way to change them.
- **Dependencies:** `minisign-verify`, `semver`, and `flate2` with `tar` (macOS only). `minisign` is a dev-dependency, to sign test packages. `cargo deny` passes.

## Tests (QA-3)
- `plat_4_manifest_platform_key_matches_release_manifest`: the key for each OS and architecture is the one `tools/release-manifest` writes.
- `plat_4_only_a_newer_version_is_an_update`: 1.0.0-rc.1 to 1.0.0-rc.2 and to 1.0.0 are updates; the same and older versions aren't.
- `plat_4_manifest_without_this_platform_finds_no_update`.
- `sec_7_refuses_a_package_with_a_bad_signature`.
- `sec_7_refuses_a_signature_for_another_version`.
- `sec_7_refuses_a_download_url_that_is_not_https`.
- `sec_7_public_key_is_the_rc1_key`: it is the same key as `tauri.conf.json` on `v1.0.0-rc.1`. The value is copied into the test, with a comment naming the tag.
- `sec_7_release_build_endpoint_and_key_are_fixed`: without `update-test`, the endpoint and key are the constants.
- `plat_4_nsis_update_arguments_match_tauri`: the arguments are the same as `tauri-plugin-updater` 2.14.0 passes in passive mode with restart.
- `plat_4_appimage_is_replaced_in_place_with_its_mode`.
- `plat_4_failed_appimage_install_leaves_the_old_file`.
- `plat_4_mac_app_swap_restores_the_backup_on_failure`.
- `plat_4_nothing_is_written_before_verification`.
- **A loopback test:** a local HTTP server serves a manifest and a signed package to `OwnSource`, which finds, downloads and verifies it. A tampered package is refused.
- `d12_debug_builds_never_check`, or the existing test of `CHECKS`, still passes.

## Out of scope
- **Packaging the GPUI app**, and the update tests on the runners: GPUI A to B, and rc.1 to the GPUI build (M6-28).
- **The update notice and its buttons in the GPUI app** (M6-15 and M6-26). This task delivers the service they call.
- **MSI**, and self-installing `.deb` or `.rpm` packages. Both stay notify-only.

## Acceptance
- The tests above pass on all three platforms in `just check`.
- `OwnSource` reads the `latest.json` that `tools/release-manifest` writes, unchanged, and accepts the `.sig` format `tauri signer` writes.
