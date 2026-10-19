# M5-12 Update check and auto-update

## Goal
The app checks for a new version, tells the user, and updates itself on the Windows installer, the macOS app and the AppImage; the `.deb`, `.rpm` and the Windows portable zip only say that a new version exists (PLAT-4). Updates are verified with the project's updater key (SEC-7), and the user can turn the check off; it is the only network call the app makes on its own (D12).

## Depends on
M5-01, M5-10, M5-11 (Settings is edited by M5-11 first)

## Before starting (Eric, once)
- Generate the updater key pair on his own machine, for example `pnpm --dir ui exec tauri signer generate -w ~/.tauri/gitcroc-updater.key`, with a password, and keep a backup of the private key outside the repo.
- Store the private key and its password as the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (`gh secret set`).
- Give the agent the public key (the `.pub` file's content). The private key and the password are never shown to, typed by or stored by an agent, and never enter the repo (README "Updater key").

## Spec
Implements: PLAT-4, D12
Respects: SEC-7, SEC-2, SEC-3, ARCH-5, ARCH-6, ARCH-7, ARCH-9, OPS-9, PLAT-1, DATA-1, UI-21, D15, section 9 "Cold start", section 11 platform table

## In scope
- Configuration:
  - `tauri-plugin-updater` in `gitcroc-app` only (ARCH-5), with the public key and one endpoint: `https://github.com/ericlass/gitcroc/releases/latest/download/latest.json`. Windows install mode `passive`.
  - `bundle.createUpdaterArtifacts` is not set in `tauri.conf.json`; the release config of M5-13 sets it, so a build without the private key still works.
  - The UI gets no updater permission (SEC-2): every call goes through the app's own commands.
  - The M0-08 test `no_updater_until_m5` is replaced by `sec_7_updates_are_verified_with_the_project_key`: the public key is set, the only endpoint is the one above, and it is https.
- Which builds check: release bundles only. Debug builds, `just dev`, and builds with the `e2e` or `perf-trace` feature never check, so no test, smoke run or bench touches the network (D12).
- Core (`gitcroc-core::update`), without Tauri (ARCH-5): an `UpdateSource` trait (check, download and install with progress) that `gitcroc-app` implements with the plugin, and a fake for tests.
  - Setting `update_check`, an optional top-level key in `settings.toml`, default on (README "Settings keys in M5").
  - When it checks: once after start, when the active workspace's startup refresh has ended, so it never delays the cold start (section 9); then every 24 hours while the app runs. Nothing about checks is saved. Tests drive the schedule with a simulated clock, never real time.
  - State for the UI: `update_status` returns idle, checking, up to date, available (version, date, notes), downloading (progress), ready to restart, or failed (reason); `update_status_changed` is the event.
  - A failed check (offline, GitHub down) is not shown as an error: Settings shows it with the time, and the log records it. The next check runs on schedule.
  - `update_check_now` runs a check at once, also when automatic checks are off.
- How the app updates depends on its package (README "Updates by package"):
  - Windows installer, macOS app, AppImage: **Install and restart**.
  - `.deb`, `.rpm`, Windows portable zip: no install; **Open download page** opens `https://github.com/ericlass/gitcroc/releases/latest` in the browser from Rust, as the file manager is opened (M2-14). The URL is fixed in the app, never taken from the manifest.
  - How the running app knows its package: from Tauri where the pinned version tells it, otherwise from the environment (`APPIMAGE` on Linux) and the install location. The method is recorded in the code; M5-14 checks it on the built packages.
- Installing:
  - `update_install` runs a job, "Update to <version>", with download progress in the status bar and the job list (UI-21, ARCH-7). A package whose signature does not match the public key is refused by the plugin; the job ends *failed* with the reason, and nothing is installed (SEC-7).
  - It is refused while another job runs. When a tab holds unsaved conflict editor state (OPS-9), the UI asks first, as closing a tab does (M3-02).
  - When the download is verified, the app restarts into the new version (on Windows the passive installer does it).
- UI:
  - When an update is available, the status bar shows "gitcroc <version> is available", announced through M5-10's live region. Clicking it, or the palette entry, opens a dialog: the current and the new version, the release notes as plain text (SEC-3), and **Install and restart** or **Open download page**, and **Later**. **Later** hides the notice until the next start or a newer version.
  - Settings gains an Updates section: **Check for updates automatically** (on by default), **Check now**, and the last result with its time.
  - The palette gains "Check for updates" (M5-07's catalog, app kind).
- Tests (with the fake `UpdateSource`; none reaches the network, none waits for a time):
  - `d12_no_check_while_the_setting_is_off`, `d12_first_check_after_the_startup_refresh_then_every_24_hours`, `d12_debug_and_test_builds_never_check`.
  - `d12_only_the_ai_and_update_code_open_connections`: a dependency test over `cargo metadata`: of the project's crates only `gitcroc-ai` depends on an HTTP client directly, and `gitcroc-app` only through the updater plugin. Git's network operations go through the git CLI (ARCH-1).
  - `sec_7_update_check_can_be_turned_off`.
  - `plat_4_installer_app_and_appimage_install`, `plat_4_deb_rpm_and_portable_zip_only_notify`.
  - `plat_4_a_bad_signature_installs_nothing` (the fake reports the plugin's verification error).
  - The install job is refused while another job runs, and the UI asks first with unsaved conflict editor state.
  - Vitest: the notice, the dialog for both package kinds, the Settings section. Playwright: from the notice to **Install and restart** with the fake core.
  - The update dialog and Settings' Updates section join M5-10's axe views, in both themes.

## Out of scope
- The release workflow and `latest.json` (M5-13). Installing and updating real packages (M5-14, M5-15).
- Update channels, skipping a version, or rollback.

## Acceptance
- The tests above exist and pass on all three platforms, and `just check` passes.
- `just coverage spec --report-only` no longer lists PLAT-4 or D12.
