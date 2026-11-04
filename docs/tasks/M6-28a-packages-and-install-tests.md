# M6-28a The GPUI packages and their install tests

## Goal
CI builds every package of section 11 from the GPUI app without Tauri's bundler. It installs each package on a runner of its platform and checks that the installed app starts. Installing the GPUI Windows installer over an installed 1.0.0-rc.1 upgrades it in place. This replaces M5-14's proof, which left with Tauri in M6-05.

This is the first of three pull requests:
- **M6-28a, this task:** the packages and their install tests.
- **M6-28b:** the release workflow and its signing step.
- **M6-28c:** the update tests on the runners.

They land in that order (Eric, 2026-11-04).

## Depends on
M6-27. M6-03's findings, recorded in `docs/tasks/README.md` and in `spikes/gpui-ci/README.md` on `spike/gpui-ci` (`d40bb5a`).

## Spec
Respects: D6, D12, D13, PLAT-1, PLAT-3, PLAT-4, SEC-7, QA-4, section 9 "Timing in CI", section 11 platform table, section 12 "End to end smoke" row

## Decisions this task follows
- **The name in the packages is `gitcroc`** (Eric, 2026-11-04).
  - **The app:** the product name is `gitcroc` and the identifier is `dev.gitcroc.desktop`. The installed binary is `gitcroc` (`gitcroc.exe` on Windows), so rc.1's updater installs over the Tauri app.
  - **The build:** the Cargo binary stays `gitcroc-next` until M6-30. Only the packaging step installs it as `gitcroc`.
- **Linux fonts:** the `.deb` depends on `fonts-dejavu-core`, and the `.rpm` requires `dejavu-sans-fonts`. No font is embedded (Eric, 2026-10-27 and 2026-11-04). The AppImage uses the desktop's fonts.
- **File names:** the same as 1.0.0-rc.1's, as `RELEASING.md` lists them, so `tools/release-manifest` and the download page don't change:
  - `gitcroc_<v>_x64-setup.exe`
  - `gitcroc-<v>-windows-x64-portable.zip`
  - `gitcroc_<v>_universal.dmg`
  - `gitcroc.app.tar.gz`
  - `gitcroc_<v>_amd64.AppImage`
  - `gitcroc_<v>_amd64.deb`
  - `gitcroc-<v>-1.x86_64.rpm`

## In scope
- **The package marks the binary** (`platform/package.rs`, PLAT-4):
  - **How it is marked:** `bundle_kind()` reads `option_env!("GITCROC_BUNDLE")` (`deb`, `rpm`, `appimage`, `nsis`, `app`), which the packaging step sets at build time. Today it always returns `None`.
  - **One build per kind:** each kind is its own release build. On Linux that means three builds; only `gitcroc-app` and the binary are rebuilt for each.
  - **Shared binaries:** the portable zip packs the NSIS build's binary. The `.dmg` holds the `.app`'s.
  - **Unmarked builds:** a plain `cargo build` stays unmarked, so it only notifies.
- **The packaging config** (`crates/gitcroc-ui/Packager.toml`, or `[package.metadata.packager]`). `cargo-packager` is pinned to the version M6-03 used, 0.11.8.
  - **Product, version and icons:** `product-name = "gitcroc"`, `identifier = "dev.gitcroc.desktop"`, the publisher `gitcroc` and the workspace version. The icons are the ones rc.1 shipped, from the `v1.0.0-rc.1` tag.
  - **Windows (NSIS, current user):** a template of our own, `crates/gitcroc-ui/packaging/nsis/installer.nsi`, based on cargo-packager's.
    - Tauri's `/UPDATE` handling: never uninstall the previous version, skip shortcuts, and refuse a downgrade.
    - Tauri's `/ARGS` handling, for the restarted app.
    - The `MainBinaryName` value under the uninstall key.
    - The install directory `%LOCALAPPDATA%\gitcroc`.
    - The uninstall key `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\gitcroc`.
    - The comment at the top names cargo-packager's template version and each change.
  - **Windows portable zip:** `gitcroc.exe` from the NSIS build, as M0-08 did.
  - **macOS:**
    - **The universal binary:** built for both architectures and joined with `lipo`.
    - **The `.app` and `.dmg`:** built by cargo-packager, ad hoc signed with `codesign -s -` (SEC-7). No certificate or notarization setting appears anywhere.
    - **The update archive:** `gitcroc.app.tar.gz`, made with `tar -czf` as Tauri made it, since cargo-packager makes it only when it signs.
  - **Linux:**
    - **The AppImage and `.deb`:** built by cargo-packager on ubuntu-22.04 (glibc 2.35, D6).
    - **The `.rpm`:** built by `cargo-generate-rpm`.
    - **Runtime dependencies** (from M6-03's `ldd` findings, plus the font):
      - `.deb`: `libc6 (>= 2.35), libxcb1, libxkbcommon0, libxkbcommon-x11-0, libvulkan1, libegl1, libwayland-client0, libwayland-egl1, fonts-dejavu-core`
      - `.rpm`: `libxcb, libxkbcommon, libxkbcommon-x11, vulkan-loader, libglvnd-egl, libwayland-client, libwayland-egl, dejavu-sans-fonts`
- **A reusable workflow, `.github/workflows/packages.yml`** (`workflow_call`), as rc.1's `bundles.yml` was.
  - **Its jobs:** it builds each platform's packages, uploads them as artifacts, and runs that platform's install job on them.
  - **Its inputs:** the version, and whether to make the update archives. M6-28b's release workflow calls it, so the build steps live only here.
- **`.github/workflows/build.yml`** calls it. It runs on pushes to `main`, by hand, and on pull requests that change the packaging config, the template, the workflows or `.github/scripts/`. It is not one of QA-1's required checks.
- **Install jobs.** Scripts live in `.github/scripts/`: Python, and PowerShell where Python is awkward. Rc.1's `install_common.py` and `test-install.py` are the starting point; they're on the tag.
  - **Before the first start:** each job writes `settings.toml` with `update_check = false` into the platform config dir, so nothing contacts GitHub (D12).
  - **What each start must show:** M5-14's start line, `gitcroc <v> started (<os> <arch>, <package>)`, with the right package. The log must also show `git --version` exiting 0 (PLAT-3).
  - **Waiting:** each step waits for a log line, never for a time. A deadline only ends a hung job.
  - **Windows** (`windows-latest`, WARP):
    - **The installer:** installed with `/S`; `gitcroc.exe` starts as `installer`; the uninstaller with `/S` removes the program and its key.
    - **The portable zip:** unpacked; it starts as `portable`.
  - **macOS** (`macos-latest`):
    - **The `.dmg`:** mounted, and the app copied to a temporary Applications folder.
    - **Checks:** `codesign` shows an ad hoc signature, and `lipo` shows `x86_64` and `arm64` (D6).
    - **The start:** the app starts as `app`.
  - **Linux** (`ubuntu-22.04`, under `xvfb-run` with `mesa-vulkan-drivers` and `libfuse2`):
    - **The AppImage:** starts as `AppImage`.
    - **The `.deb`:** installed with `apt`, so its dependencies resolve; it starts as `deb`.
    - **The `.rpm`:** installed with `dnf` in a `fedora:latest` container with no font installed beforehand, so the font dependency is what brings one. It is checked with `rpm -q` and started under `xvfb-run` there as `rpm`.
  - **Windows, over rc.1:**
    1. Install the released `gitcroc_1.0.0-rc.1_x64-setup.exe` from the `v1.0.0-rc.1` release with `/S`. It's the only download from GitHub after the build, and is fetched by `gh release download` with its SHA-256 checked against the release's `SHA256SUMS`.
    2. Run the new installer with `/P /UPDATE`, as rc.1's updater does.
    3. Check there is one install directory, `%LOCALAPPDATA%\gitcroc`, with one uninstall key, whose `DisplayVersion` is the new version and which has `MainBinaryName`.
    4. Check that the app starts as `installer` with the new version.
  - **On failure:** each job uploads `gitcroc.log` and the install's file listing.
- **Tests in the workspace:**
  - `plat_4_bundle_kind_comes_from_the_build`: each `GITCROC_BUNDLE` value maps to its kind, and an unset or unknown value is `None`. The mapping is a function of the string, tested on every platform.
  - `sec_7_packages_are_unsigned_and_macos_is_ad_hoc`: the packaging config and the workflows set no Windows certificate, no macOS identity other than `-`, and no notarization credentials. This is the old `sec_7_builds_are_unsigned_and_macos_is_ad_hoc_signed`, moved to the new files.
  - `plat_4_package_names_match_release_manifest`: the file names the config produces are the ones `tools/release-manifest`'s `Packages::find` accepts.
  - `d6_linux_packages_depend_on_a_font`: the `.deb`'s `Depends` names `fonts-dejavu-core`, and the `.rpm`'s `Requires` names `dejavu-sans-fonts`.
  - `plat_4_nsis_template_keeps_tauris_install_location`: the template's install directory, uninstall key and `MainBinaryName` are Tauri's.
- **Docs:** `RELEASING.md`'s M6 note says the packages build again. The rest of that file is M6-28b's.

## Out of scope
- **The release workflow, signing and `latest.json`:** M6-28b. This task builds the update archives but signs nothing.
- **Updates on the runners:** M6-28c.
- **Renaming the Cargo binary:** M6-30.
- **Intel Macs:** no Intel runner is used.
- **Windows 10 itself:** `windows-latest` is Windows Server, and the pull request notes which version ran.
- **Wayland on CI:** under headless Weston GPUI panics (M6-03).

## Acceptance
- `build.yml` is green on the pull request's run for all seven packages and the rc.1 upgrade, and the run is linked in the pull request.
- A broken package fails its job. The pull request shows it with a throwaway commit that isn't merged, for example a portable zip without `gitcroc.exe`.
- The tests above pass, and `just check` passes.
