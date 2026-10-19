# M5-15 Update tests on the CI runners

## Goal
On each platform's runner, an installed gitcroc finds a newer version on a local update server, verifies it with the updater key and installs it, and starts as the new version; a package with a bad signature is refused; the `.deb` only notifies. This is the proof that gitcroc updates on Windows and macOS (README "Windows and macOS proof").

## Depends on
M5-14

## Spec
Respects: PLAT-4, SEC-7, D12, section 9 "Timing in CI" (README), section 11 platform table

## In scope
- An `update-test` Cargo feature of `gitcroc-app`, never in release builds (a test checks it is not a default feature and the release workflow does not pass it):
  - At start it checks at once, and when an update is available on a package that installs, it installs it without asking and restarts. The result is written to `gitcroc.log`: `update: installed <version>`, `update: refused (<reason>)` or `update: notify only (<version>)`.
  - It changes nothing else of M5-12: the same `UpdateSource`, the same verification.
- A test overlay config, written by the job at run time and never committed: the endpoint `http://127.0.0.1:<port>/latest.json`, the plugin's setting that allows plain http for it, and a throwaway public key.
- The throwaway key pair is generated inside the job with `tauri signer generate` and never leaves it; it is not the project's key (README "Updater key").
- A job per platform in a new workflow `update-test.yml` (pushes to `main`, by hand, and pull requests that change the updater, the workflow or its scripts):
  1. Build version A (the workspace version) and version B (A with the patch number raised by one, set in the overlay) with the `update-test` feature and the overlay, B with update packages signed by the throwaway key.
  2. Write `latest.json` for B with `tools/release-manifest` (M5-13) and serve it and B's packages from a local HTTP server on `127.0.0.1`.
  3. Install A as M5-14 does: the NSIS installer (Windows), the app (macOS), the AppImage (Linux).
  4. Start A; wait for `update: installed B` and then for M5-14's start line with version B.
  5. Serve a C whose signature does not match; start B; wait for `update: refused` and check B is still installed.
  6. Linux only: install A's `.deb` and start it with B available; wait for `update: notify only (B)` and check A is still installed.
- Each step waits for a log line, never for a time; a deadline only ends a hung job. The job uploads the logs when it fails.
- No step reaches the internet after the build (D12): the server is local, and the overlay has no other endpoint.

## Out of scope
- The real update channel on GitHub (checked by hand on Linux in M5-30). The portable zip and the `.rpm`: they notify by the same code path as the `.deb`, which M5-12's tests cover.

## Acceptance
- `update-test.yml` is green on all three platforms on the pull request's run, linked in the pull request, and `just check` passes.
