# M5-16 Smoke test on Windows and the OS keychains on CI

## Goal
The end-to-end smoke test runs on Windows as well as Linux, as section 12 asks, and the real OS keychain is exercised on the Windows and macOS runners, so the deferred Windows and macOS checks of M0 to M4 are covered by CI (README "Windows and macOS proof").

## Depends on
M4

## Spec
Respects: D11, SEC-1, PLAT-1, PLAT-2, AI-16, section 12 "End to end smoke" row, M2-15, M3-15 and M4-12 smoke flows

## In scope
- `just smoke` on Windows:
  - `ui/webdriver/smoke.ts` runs on Windows: paths built with `node:path`, git run with an argument array, the app and `gitcroc-cli` found with `.exe`, no shell built (ARCH-3 applies to the driver too).
  - `tauri-driver` with Microsoft Edge WebDriver matching the runner's WebView2 version; how the matching driver is found or installed is recorded in the workflow.
  - The same flows as on Linux: add a repo, stage, commit, resolve a merge conflict by hand (M2-15), the group flow (M3-15), the AI flow with the mock provider (M4-12).
  - CI's `smoke` job becomes a matrix of `ubuntu-22.04` and `windows-latest`, and stays a required check (QA-1's list is unchanged: the smoke job was added beside it in M2-15).
  - macOS has no WebDriver for WKWebView; the smoke test does not run there (section 12 names Linux and Windows).
- The OS keychain:
  - `d11_os_keychain_round_trip`, an ignored test in `gitcroc-app`: stores a made-up key with the real `keyring` backend under a test service (`gitcroc-test-<random>`, never `gitcroc`), reads it back, deletes it, and checks it is gone. It never touches a real gitcroc entry.
  - CI runs it on the Windows and macOS runners with `--ignored` for this test only. On Linux the hosted runner has no Secret Service; it stays a local check (M4's sign-off).
- The pull request notes the Windows version and WebView2 version that ran.

## Out of scope
- Benchmarks on Windows and macOS: hosted runners are not reference machines (README "Timing in CI"); they stay deferred.

## Acceptance
- The smoke job is green on both platforms and the keychain test is green on Windows and macOS, on the pull request's run, linked in the pull request; `just check` passes.
