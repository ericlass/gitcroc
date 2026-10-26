# GPUI CI and packaging spike (M6-03, throwaway)

Does GPUI run on GitHub's hosted runners, and can every package of section 11
be built without Tauri's bundler? This crate is outside the workspace and
nothing depends on it. The workflow is `.github/workflows/gpui-spike.yml`; it
runs on `workflow_dispatch` and on pushes to `spike/**`, never on pull
requests.

```
cargo test                                    # the two headless GPUI tests and the signature tests
cargo run --release -- --frames 300           # scrolls 300 frames, prints one JSON line, exits 0
cargo packager --release --formats appimage,deb
cargo generate-rpm -o dist/
cargo run --example sign_update -- <out-dir> <version> <file>...
```

- `src/lib.rs`: one window, a list of 10,000 rows and a button.
- `src/main.rs`: `--frames <n>` scrolls 100 px a frame for `n` frames, then
  prints the frame count, frame-time percentiles, the GPU adapter and the
  backend as one JSON line and exits 0. It exits 1 when the window does not
  open or no report comes within `--timeout` seconds (default 120).
- `tests/ui.rs`: `#[gpui::test]` tests on GPUI's test platform. One clicks the
  button through `VisualTestContext::simulate_click` and checks the entity; one
  types into a gpui-component `Input` with `simulate_input` and a backspace.
- `src/update.rs`, `examples/sign_update.rs`, `tests/update_signature.rs`:
  update signatures in the format of Tauri's bundler, checked with
  `minisign-verify` exactly as tauri-plugin-updater 2.14 checks them.
- `ci/`: small scripts that record times, sizes and the app's report in the
  job summary and in a `metrics-*` artifact.
- `build.rs`: copied from `spikes/gpui-history`; links `libxkbcommon-x11.so.0`
  directly on machines without `libxkbcommon-x11-dev`. The workflow installs
  the `-dev` packages, so CI does not need it.

## Results

### On GitHub's hosted runners

**Not run yet: the spike branch has not been pushed.** Each row gets its run
link when it has run.

| Job | Runner | Result | Run |
| --- | --- | --- | --- |
| Headless tests | ubuntu-latest | pending | |
| Headless tests | windows-latest | pending | |
| Headless tests | macos-latest | pending | |
| Window, Xvfb + lavapipe (Vulkan) | ubuntu-22.04 | pending | |
| Window, Xvfb, Vulkan hidden (GL, llvmpipe) | ubuntu-22.04 | pending | |
| Window, headless Weston (Wayland) | ubuntu-22.04 | pending | |
| Window, Direct3D 11 | windows-latest | pending | |
| Window, Metal | macos-latest | pending | |
| NSIS, installed for the current user, started | windows-latest | pending | |
| Portable zip, started | windows-latest | pending | |
| `.app`, `.dmg` (ad hoc), universal binary, installed, started | macos-latest | pending | |
| AppImage, started | ubuntu-22.04 | pending | |
| `.deb`, installed with apt, started | ubuntu-22.04 | pending | |
| `.rpm`, installed with dnf, started | fedora:latest container | pending | |
| Update signatures (NSIS, `.app.tar.gz`, AppImage) | ubuntu-latest | pending | |
| `cargo deny check` | ubuntu-latest | pending | |

### Locally, 2026-10-26

Ubuntu 24.04, Ryzen (24 threads), RTX 4070; Xvfb and Mesa 25.2.8.

| Check | Result |
| --- | --- |
| `cargo test`, `DISPLAY` and `WAYLAND_DISPLAY` unset, no Vulkan driver | pass, 4 tests (2 GPUI, 2 signature) |
| Window under `xvfb-run`, lavapipe | pass: 300 frames, p50 48.0 ms, p99 48.1 ms; `llvmpipe (LLVM 20.1.2, 256 bits)`, software, backend Vulkan |
| Window under `xvfb-run`, `VK_ICD_FILENAMES=/nonexistent.json` | pass: 300 frames, p50 48.0 ms; llvmpipe, backend Gl (wgpu falls back to GL by itself) |
| AppImage, `--frames 60` under Xvfb | pass |
| `.deb`, unpacked with `dpkg-deb -x` (no sudo here), `--frames 60` | pass |
| `.rpm` | built; not installed (no rpm tooling here) |
| Signing the AppImage, verifying as tauri-plugin-updater does | pass |
| Weston | not installed here |

Under Xvfb, frames come every 48 ms whichever renderer draws them: Xvfb's
frame pacing, not the renderer's speed. The numbers prove frames are
presented; they say nothing about speed.

GPUI logs two harmless lines under Xvfb: `Found no xinput mouse pointers`
and `failed to load font .SystemUIFont` (it falls back to another font).

## Build times and sizes

Locally, release profile with no debug info:

| | Time or size |
| --- | --- |
| Cold build (empty `target/`, crates downloaded) | 2 min 28 s, line-table debug info; 1 min 19 s without debug info |
| Test build, cold | 50 s |
| Binary, not stripped | 50.2 MiB (with line-table debug info: 285 MiB) |
| Binary, stripped | 36.6 MiB |
| AppImage | 13.5 MiB |
| `.deb` | 13.8 MiB |
| `.rpm` | 10.1 MiB |

On the runners: pending. The `linux`, `windows` and `macos` jobs record the
cold build in their first run and the cached build (Swatinem/rust-cache) in
the next; the job summary says whether the cache was hit.

## Linux runtime libraries

`ldd` of the release binary (linked): `libxcb.so.1`, `libxkbcommon.so.0`,
`libxkbcommon-x11.so.0`, `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`.

Loaded at run time with `dlopen` (seen with `LD_DEBUG=files` and in the
binary's strings): `libvulkan.so.1`, `libEGL.so.1` (the GL fallback),
`libwayland-client.so.0` and `libwayland-egl.so.1` (on Wayland). No
fontconfig or freetype: text goes through cosmic-text and fontdb, in Rust.

So the `.deb` depends on `libc6 (>= 2.35), libxcb1, libxkbcommon0,
libxkbcommon-x11-0, libvulkan1, libegl1, libwayland-client0, libwayland-egl1`,
and the `.rpm` requires `libxcb, libxkbcommon, libxkbcommon-x11,
vulkan-loader, libglvnd-egl, libwayland-client, libwayland-egl`. A Vulkan or
GL driver (Mesa) is a recommendation, not a dependency: every desktop has
one. Whether a font package is needed is what the `rpm` job's "no fonts"
step shows.

**glibc:** built on Ubuntu 24.04 the binary needs `GLIBC_2.39`. The release
build must stay on ubuntu-22.04 (glibc 2.35, the floor of D6), as
`bundles.yml` already does.

The build needs `libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev` instead of
WebKitGTK's packages.

## `cargo deny check` with the workspace's `deny.toml`

Locally, cargo-deny 0.20.2: bans and sources pass; licences and advisories
fail. No vulnerabilities.

| Kind | Crate | Through | Needs |
| --- | --- | --- | --- |
| Licence `0BSD` | enum-iterator 2.3.0, enum-iterator-derive 1.5.0 | gpui-component | `0BSD` allowed, or an exception for both crates. 0BSD is public-domain-like, compatible with MIT. |
| Licence `CC0-1.0` | tiny-keccak 2.0.2 | const-random-macro, const-random, ahash, zed-xim (X11 input methods) | an exception, as `notify` has |
| Unmaintained | instant 0.1.13 (RUSTSEC-2024-0384) | gpui-base | an ignore with a reason |
| Unmaintained | rustls-pemfile 2.2.0 (RUSTSEC-2025-0134) | gpui-pre-reqwest, through gpui-kit-assets | an ignore, or no `assets` feature |
| Unmaintained | rustybuzz 0.20.1 (RUSTSEC-2026-0206) | usvg, resvg (gpui, gpui-component) | an ignore |
| Unmaintained | ttf-parser 0.25.1 (RUSTSEC-2026-0192) | fontdb, cosmic-text (gpui-pre-wgpu) | an ignore |

Also:
- 71 duplicate-version warnings (windows-sys, rand, hashbrown and so on);
  `multiple-versions` is `warn`, so they pass.
- The workspace's five advisory ignores and the `webpki-root-certs` exception
  are reported as not encountered, because the spike's tree has no Tauri,
  keyring or reqwest-for-AI. M6 removes the Tauri ones when Tauri goes.
- gpui-kit's default `assets` feature brings in an HTTP client
  (gpui-pre-reqwest) for remote images. gitcroc's network operations go
  through the git CLI (ARCH-1), so M6 should check whether it can turn the
  feature off and supply the icons itself.

## Update packages (PLAT-4)

`latest.json` and its signatures can be made without Tauri's bundler:

- A `.sig` file is the base64 of a minisign signature box. The updater's
  `pubkey` is the base64 of the public key box. `src/update.rs` writes both
  with the `minisign` crate 0.10.
- The trusted comment is `timestamp:<s>\tfile:<name>\tversion:<v>`, as the
  Tauri CLI writes it. The updater compares `version:` with the version
  `latest.json` announces when it is there. It requires it only in
  `restore_update`; gitcroc 1.0.0-rc.1 calls `check()`, which does not.
- **cargo-packager's own signer** (`cargo packager signer sign`, or
  `CARGO_PACKAGER_SIGN_PRIVATE_KEY` while packaging) writes the same box
  without `version:`. rc.1 would accept it, but M6-28 should sign with its
  own step, like `sign_update`, to keep the version check.
- cargo-packager makes `<app>.app.tar.gz` itself only when it signs. The
  workflow makes it with `tar -czf`, as Tauri does.
- `tests/update_signature.rs` checks valid, tampered, wrong-version and
  wrong-key cases, then, in the `update` job, the three real packages signed
  with one throwaway key.

## NSIS: installing over a Tauri install (M6-27)

How Tauri's installer (tauri-cli 2.12.2's template) and cargo-packager
0.11.8's template name things, for the current-user install:

| | Tauri, `dev.gitcroc.desktop` | cargo-packager |
| --- | --- | --- |
| Install directory | `%LOCALAPPDATA%\${PRODUCTNAME}` = `%LOCALAPPDATA%\gitcroc` | `%LOCALAPPDATA%\${PRODUCTNAME}` |
| Uninstall key | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}` | the same |
| Previous location | `HKCU\Software\${MANUFACTURER}\${PRODUCTNAME}`, default value | the same |
| `MANUFACTURER` | `publisher`, default the second part of the identifier: `gitcroc` | `publisher`, the same default |
| `MainBinaryName` value under the uninstall key | written | not written |
| `/P` (passive), `/S`, `/R` (restart), `/NS` (no shortcut) | yes | yes |
| `/UPDATE` | never uninstalls the previous version; skips shortcuts and WebView2; refuses downgrades | ignored |
| `/ARGS` (arguments for the restarted app) | yes | ignored |

Finding: with `product-name = "gitcroc"` and `identifier =
"dev.gitcroc.desktop"` (publisher `gitcroc`), cargo-packager's installer finds
the Tauri install's directory and keys, so it installs over it. rc.1's
updater starts the new installer with `/P /UPDATE` and, to restart, `/R /ARGS
...`. cargo-packager's template ignores `/UPDATE` and `/ARGS`, but `/P` alone
already skips its reinstall page, so it does not uninstall the previous
version either: files are overwritten in place, as with Tauri. The
differences: shortcuts are created again, a downgrade is not refused in
passive mode, the restarted app gets no arguments, and the uninstall key has
no `MainBinaryName`. The Tauri uninstaller is replaced by cargo-packager's.

M6-27 should give cargo-packager a custom template (its `nsis.template`
option) with Tauri's `/UPDATE` and `/ARGS` handling and the `MainBinaryName`
value, keep the binary name `gitcroc.exe`, and prove the upgrade on a runner
by installing rc.1's real installer first. The spike installs as
`gitcroc-gpui-spike` so it never touches a real gitcroc install; the
`windows` job records the directory and keys it gets.

## What M6-06 should copy into `ci.yml`

To be confirmed by the runs:

- Linux build packages `libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev`
  instead of WebKitGTK's.
- Headless GPUI tests are plain `cargo test`, on all three runners, with no
  Xvfb and no GPU.
- The smoke test's real window on Linux: `xvfb-run` with
  `mesa-vulkan-drivers` and `VK_ICD_FILENAMES` pointing at lavapipe's ICD
  file (`/usr/share/vulkan/icd.d/lvp_icd*.json`). Without Vulkan, wgpu falls
  back to GL on llvmpipe with no setting.
- Swatinem/rust-cache keyed per runner, as now.
- The release build stays on ubuntu-22.04 for glibc 2.35.

## Failures and fallbacks

Filled in from the runs. Planned fallbacks if a job fails:

| If this fails | Fallback |
| --- | --- |
| Window on Windows or macOS | The smoke test runs on Linux only; Windows and macOS keep the headless tests and the package build. |
| Headless Weston | Xvfb only on CI; Wayland checked by hand. |
| AppImage needs FUSE | `APPIMAGE_EXTRACT_AND_RUN=1`, or install `libfuse2`. |
| `.rpm` starts only with a font installed | `Requires:` a font package, or ship a font in the binary. |
| cargo-packager's NSIS template | A custom template, as above. |
