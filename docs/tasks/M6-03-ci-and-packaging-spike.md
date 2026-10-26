# M6-03 Spike: GPUI on the CI runners, and packaging without Tauri

## Goal
We know whether GPUI tests and a real GPUI window run on GitHub's hosted runners for all three platforms. We also know whether `cargo-packager` and `cargo-generate-rpm` can build every package of section 11. M6-06, M6-28 and M6-29 then build on proven ground.

This is a spike in the sense of section 14. It ends with findings and a working proof, not with production code.

## Depends on
None. It can run beside M6-01, M6-02 and M6-04.

## Spec
Respects: QA-1, QA-4, D6, D13, SEC-7, PLAT-4, section 11 packages, section 12 smoke test

## In scope
- **`spikes/gpui-ci/`.** A standalone crate like `spikes/gpui-history/`, outside the workspace, with:
  - **A small app:** one window with a list of 10,000 rows and a button.
    - With `--frames <n>` it renders `n` frames while scrolling, prints the frame count, the frame-time percentiles, the GPU adapter and the backend, then exits 0.
    - It exits non-zero when no frame was presented.
  - **Two `#[gpui::test]` tests** using the `test-support` feature:
    - one clicks the button through `VisualTestContext` and asserts on an entity;
    - one types into a gpui-component input.
  - Both run with no window server and no GPU.
- **A workflow, `.github/workflows/gpui-spike.yml`.** It runs on `workflow_dispatch` and on pushes to `spike/**` only, never on pull requests to `main`. Its jobs:
  1. **Headless tests:** `cargo test` of the spike on `ubuntu-latest`, `windows-latest` and `macos-latest`.
  2. **Linux real window:** the app with `--frames 300`, run three ways:
     - under `xvfb-run` with Mesa's lavapipe (software Vulkan);
     - the same with Vulkan hidden, to show whether wgpu falls back to GL on llvmpipe;
     - under a headless Wayland compositor (`weston --backend=headless`), if it is quick to set up.
  3. **Windows real window:** the app with `--frames 300`. Record whether Direct3D 11 gets a hardware adapter or WARP.
  4. **macOS real window:** the app with `--frames 300`, to show Metal works on the runner.
  5. **Packaging.** Build packages of the app with `cargo-packager` and install each on its runner, then start the installed app with `--frames 60`:
     - NSIS (current user) and a portable zip on Windows;
     - `.app`, then `.dmg` with `codesign -s -`, and a universal binary through `lipo` on macOS;
     - AppImage and `.deb` on Linux;
     - `.rpm` through `cargo-generate-rpm`, installed in a Fedora container if the Ubuntu runner can't install it.
  6. **Update package.** Sign the NSIS installer, the `.app.tar.gz` and the AppImage with a throwaway minisign key, and verify each signature with `minisign-verify` in a test. This shows the format of the existing `latest.json` can be produced without Tauri's bundler (PLAT-4).
- **Measurements for each runner:**
  - cold and cached build times of the release binary;
  - binary size, stripped and not;
  - package sizes;
  - the Linux runtime libraries the binary needs (`ldd`), for section 11 and the `.deb` and `.rpm` dependencies.
- **NSIS upgrade over Tauri.** Read how Tauri's NSIS installer names the install directory and the uninstall registry key for `dev.gitcroc.desktop`. Say whether `cargo-packager`'s NSIS template can match them, so that installing over a Tauri install upgrades it (M6-27). This is a finding only; changing a template is out of scope.
- **`cargo deny check`** on the spike's dependency tree with the workspace's `deny.toml`. List every licence or advisory that would need an allowance.
- **Findings in `spikes/gpui-ci/README.md`:**
  - a table of which jobs passed, with links to the runs;
  - the numbers above;
  - what M6-06 should copy into `ci.yml`;
  - what fails, and the fallback for it, for example the smoke test on Linux only.

## Out of scope
- Changing `ci.yml`, `build.yml` or `release.yml`; M6-06 and M6-28 do that.
- Real gitcroc code in the spike app.

## Acceptance
- Every job above has run at least once, and the README records each result with a link to its run.
- The README states plainly whether each of these works on hosted runners:
  - headless GPUI tests;
  - a real window on each platform;
  - each package built and started.
- `just check` still passes, because nothing in the workspace changed.
