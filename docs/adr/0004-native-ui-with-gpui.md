# ADR 0004: A native UI with GPUI replaces Tauri

- Status: accepted
- Date: 2026-10-26
- Concerns: D2, section 4, ARCH-5 to ARCH-8, UI-23, PERF-1, PERF-8, SEC-2,
  PLAT-4, QA-1, sections 9 and 11 to 15, milestone M6, task M6-01

## Context

The Tauri UI paces its frames at 60 Hz on the 144 Hz reference display:
`bench-ui` `scroll-frame` has a median of 16 ms. Scrolling stutters, worst in
the diff view. Tuning the webview to get around this was ruled out.

A throwaway spike on branch `spike/gpui-history` rebuilt the history pane and
a whole-commit diff in GPUI (through gpui-kit 0.8.1) on top of `gitcroc-git`.
Its numbers are in `spikes/gpui-history/README.md` on that branch, with the
JSON and screenshots in its `results/`. They were measured on an RTX 4070
under X11, 1920×1080 at 144 Hz:

- History of `target-project`, 5,000 commits: 144 fps, 0 dropped frames,
  CPU p95 4.3 ms a frame, no blank row.
- A 216,000-line diff (the root commit of `target-project`): 144 fps while it
  was still loading.
- The Linux kernel's history, 21,000 commits: 143.8 fps.

The core was kept free of Tauri from M0 (ARCH-5), so replacing the UI leaves
`gitcroc-git`, `gitcroc-core`, `gitcroc-ai` and `gitcroc-cli` unchanged.

## Alternatives considered

- **Slint:** the fallback. Fast and native-looking, but its UI is written in
  its own markup language, and an editable code view would have to be built
  from scratch.
- **egui:** immediate mode, so it redraws everything every frame, and its look
  is far from native.
- **iced:** fewer ready-made widgets for lists, editors and menus.
- **Keeping the webview** and tuning it: ruled out, as above.

## Decision

The UI is written in Rust with GPUI, through gpui-kit and gpui-component, at
pinned exact versions. Eric decided this on 2026-10-26 and accepted the five
decisions of `docs/plans/native-ui-gpui.md`:

1. v1.0.0 is held, and the GPUI build ships as 1.0.0-rc.2.
2. The scroll budget rises to the display's refresh rate, up to 144 Hz.
3. Linux needs Vulkan or GL instead of WebKitGTK.
4. The GPUI UI is developed side by side on `main` as `gitcroc-next` until
   the switch (M6-30), not on a long-lived branch.
5. The app keeps today's look, by porting its tokens, plus the OS accent
   color.

The spec now describes the end state of milestone M6: D2, sections 4, 8 to
15 and the ARCH, UI, PERF, SEC, PLAT and QA rules they hold. Until M6-30 the
Tauri app is still what ships, and its tests stay as they are; each reworded
requirement is implemented by the M6 task that cites it.

## Consequences

- Crates: `gitcroc-app` keeps no UI toolkit; it holds the services and their
  event channel. The new `gitcroc-ui` holds the views and the `gitcroc`
  binary, and is the only crate that depends on GPUI (ARCH-5).
- The IPC layer and its generated types go, with Tauri, tauri-specta,
  specta, the Tauri plugins, `gtk`, Node and the web front end in `ui/`.
- Tests: the front-end tests become GPUI tests in Rust, run headless by
  `cargo test` against the real core on fixture repos. `just check` and QA-1
  become Rust only.
- Platform: Linux needs a Vulkan or GL driver instead of WebKitGTK 4.1;
  Windows no longer needs WebView2.
- Release: v1.0.0 (M5-30) waits for M6. Installed 1.0.0-rc.1 apps update to
  the GPUI build through the same manifest and key (PLAT-4).
- Risk: GPUI is pre-1.0, and its API changes often. Its versions are pinned,
  and each upgrade is a task of its own.

## Note, 2026-10-27

Eric decided that the Tauri app is not kept during the migration: 1.0.0-rc.1
is its last release. M6-05 deletes the Tauri shell, its workflows and the
IPC layer instead of keeping them until M6-30, so decision 4's "side by
side" and the sentence "Until M6-30 the Tauri app is still what ships" no
longer hold. The GPUI UI is still developed on `main` as `gitcroc-next`, and
`ui/` stays in the tree, out of CI, as the reference the tasks port from,
until M6-30 deletes it.
