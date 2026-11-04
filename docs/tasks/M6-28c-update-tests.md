# M6-28c Update tests on the CI runners

## Goal
On each platform's runner, an installed GPUI build finds a newer version on a local update server. It verifies the package with the updater key, installs it and starts as the new version. A package with a bad signature is refused, and the `.deb` only notifies. Then 1.0.0-rc.1's own Tauri updater installs the GPUI build the same way.

This is the proof that gitcroc updates on Windows and macOS (README "Windows and macOS proof"), and that rc.1 users get the GPUI app (PLAT-4). It is the last of M6-28's three pull requests.

## Depends on
M6-28b: its `sign` command and the packages workflow.

## Spec
Respects: PLAT-4, SEC-7, D12, section 9 "Timing in CI", section 11 platform table

## Decisions this task follows
- **How rc.1 reaches a local manifest** (Eric, 2026-11-04):
  - **The build:** rc.1 is built from the `v1.0.0-rc.1` tag with its own M5-15 `update-test` feature and overlay, which point it at the local server and a throwaway key.
  - **Why not the released rc.1:** it has the real endpoint and key built in, so it can't be pointed at a local server.
  - **What proves the in-place upgrade:** the released installer, in M6-28a.

## In scope
- **The `update-test` build starts the update itself** (`gitcroc-app`, with M6-27's feature):
  - **The check:** at start it checks at once, without waiting for the startup to end or for the setting (D12 still holds: only this build does it).
  - **On a package that installs:** the update is installed without asking, and the app restarts into the new version.
  - **The log** (`gitcroc.log`): `update: installed <version>`, `update: refused (<reason>)` or `update: notify only (<version>)`. These are rc.1's lines, so the scripts read both apps' logs the same way.
  - **What stays the same:** `OwnSource`, the verification and `UpdateService`. The feature only adds the start and the log lines.
  - **Release builds:** `update-test` isn't a default feature, `packages.yml` passes it only when the update test asks, and `release.yml` never does.
- **A throwaway key,** generated inside the job with `minisign`, through a `release-manifest keygen` command or the test's own helper. It never leaves the job and isn't the project's key.
- **A new workflow, `.github/workflows/update-test.yml`,** with one job per platform (`windows-latest`, `macos-latest`, `ubuntu-22.04`).
  - **When it runs:** on pushes to `main`, by hand, and on pull requests that change the updater (`crates/gitcroc-app/src/update/**`, `platform/**`), the workflow, `.github/scripts/` or `tools/release-manifest`.
  - **Not required:** it isn't one of QA-1's required checks.
- **The steps of the GPUI update, A to B:**
  1. **Build A and B.** A is the workspace version; B is A with the patch number raised by one. Both are built through `packages.yml` with `update-test`, `GITCROC_UPDATE_TEST_ENDPOINT=http://127.0.0.1:<port>/latest.json` and `GITCROC_UPDATE_TEST_KEY`. B's update package is signed with the throwaway key by `release-manifest sign`.
  2. **Serve B.** `release-manifest local` writes B's `latest.json`, and a local HTTP server on `127.0.0.1` serves it and B's packages.
  3. **Install A** as M6-28a does: the NSIS installer (Windows), the app (macOS), the AppImage (Linux).
  4. **Update.** Start A, wait for `update: installed B`, then wait for M5-14's start line with version B.
  5. **Refuse.** Serve a C whose signature doesn't match, and start B. Wait for `update: refused`, and check B is still installed.
  6. **Notify (Linux only).** Install A's `.deb` and start it with B on offer. Wait for `update: notify only (B)`, and check A is still installed.
- **The steps of rc.1 to the GPUI build:**
  1. **Build rc.1.** Check out `v1.0.0-rc.1` into its own directory and build its NSIS installer, `.app` and AppImage with its `update_build.py` steps for version A only. Its overlay points it at the local server with the throwaway key. This needs rc.1's toolchain in that job: Node, pnpm and, on Linux, WebKitGTK.
  2. **Install rc.1**, then serve the GPUI B from step 1 of the GPUI update.
  3. **Update.** Start rc.1, wait for rc.1's `update: installed B`, then wait for the GPUI start line with version B and the same package (`installer`, `app` or `AppImage`).
  4. **Windows only:** there is still one install directory and one uninstall key, as in M6-28a's upgrade check.
- **Scripts** in `.github/scripts/` reuse M6-28a's install helpers. Rc.1's `update_test.py` and `update_build.py`, on the tag, are the starting point.
  - **Waiting:** each step waits for a log line, never for a time. A deadline only ends a hung job.
  - **On failure:** the job uploads the logs.
  - **No internet after the build (D12):** the server is local, and both builds' only endpoint is the loopback one.
- **Tests in the workspace:**
  - `plat_4_update_test_build_installs_at_start`: with a fake source and the feature's start, an update on a package that installs is installed without a prompt, and the log line is written. The test runs only with the feature on, in `just check-rust`'s feature run if there is one; otherwise in the workflow's own `cargo test --features update-test`.
  - `plat_4_update_test_logs_refused_and_notify_only`.
  - `d12_update_test_is_not_a_default_feature_and_never_released`: `update-test` isn't in `default`, and `release.yml` doesn't pass it.

## Out of scope
- **The real update channel on GitHub:** checked by hand after rc.2 is published (M6-31).
- **The portable zip and the `.rpm`:** they notify by the same code path as the `.deb`, which the tests cover.

## Acceptance
- `update-test.yml` is green on all three platforms on the pull request's run, for both GPUI A to B and rc.1 to GPUI. The run is linked in the pull request.
- The tests above pass, and `just check` passes.
