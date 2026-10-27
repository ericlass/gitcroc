# M6-01 Spec and ADR: a native GPUI UI replaces Tauri

## Goal
The spec describes gitcroc with a native Rust UI built on GPUI instead of the Tauri shell and the SolidJS front end. ADR 0004 records why. Every later M6 task can then cite the new rules. Eric decided this on 2026-10-26, after the spike on `spike/gpui-history`, and accepted the five decisions of `docs/plans/native-ui-gpui.md`.

## Depends on
None. The plan's decisions are answered.

## Spec
Respects: DEV-1, DEV-3, QA-3, every requirement ID it rewords

This task may edit `docs/spec/` and `docs/adr/` (DEV-3).

## In scope
- **The spec describes where M6 ends.** Until M6-30 the Tauri app is still what ships, and its tests stay as they are. Reworded requirements are implemented by the M6 tasks that cite them.
- **No new requirement IDs.** Only existing ones are reworded, so `just coverage spec` stays green with today's tests. A rule that needs a new ID waits for the task that implements it.
- `docs/adr/0004-native-ui-with-gpui.md`, in the format of ADR 0001:
  - **Context.**
    - The Tauri UI paces frames at 60 Hz on the 144 Hz reference display (`bench-ui` `scroll-frame` median 16 ms). It stutters, worst in the diff view.
    - Tuning the webview was ruled out.
    - The spike's numbers are in `spikes/gpui-history/README.md` on `spike/gpui-history`, with its `results/`:
      - history: 144 fps, 0 dropped frames, CPU p95 4.3 ms;
      - a 216,000-line diff: 144 fps while still loading;
      - the Linux kernel's history: 143.8 fps.
  - **Alternatives considered, one line each with the reason it lost:**
    - **Slint:** the fallback. Fast and native-looking, but its UI is written in its own markup language, and an editable code view would have to be built from scratch.
    - **egui:** immediate mode, so it redraws everything every frame; its look is far from native.
    - **iced:** fewer ready-made widgets for lists, editors and menus.
    - **Keeping the webview** and tuning it: ruled out.
  - **Decision.** The UI is written in Rust with GPUI, through gpui-kit and gpui-component, at pinned exact versions. Name the five decisions of the plan.
  - **Consequences.**
    - Crates: `gitcroc-app` keeps no UI toolkit, and the new `gitcroc-ui` holds the views.
    - The IPC layer and its generated types go.
    - Tests: front-end tests become GPUI tests in Rust.
    - Platform: Linux needs Vulkan or GL instead of WebKitGTK.
    - Release: v1.0.0 waits for M6.
    - Risk: GPUI is pre-1.0, and its upgrades are tasks of their own.
- Section 1, decision log:
  - **D2** becomes: "Native Rust UI with GPUI (through gpui-kit and gpui-component); no webview". Source: "Eric, 2026-10-26, after the GPUI spike (ADR 0004); replaces Tauri 2 with SolidJS".
- Section 4:
  - The opening sentence and the diagram: `gitcroc-ui` (GPUI views) on top of `gitcroc-app` (services, platform, askpass, updater). The "typed IPC" arrow becomes in-process calls and an event channel.
  - The Stack table:
    - Shell: GPUI. Its notes say Direct3D 11 on Windows, Metal on macOS, and Vulkan or GL through wgpu on Linux.
    - Front end: Rust GPUI views, view models as GPUI entities.
    - Lists: GPUI `uniform_list` and `list`.
    - Diff and code view: GPUI views of our own, highlighted by `syntect`.
    - The IPC types row is removed.
  - The Crates table:
    - New row `gitcroc-ui`: GPUI views and the `gitcroc` binary; may depend on all of the above.
    - `gitcroc-app` loses "Tauri commands and events" and gains "services and their event channel; no UI toolkit".
    - The `ui/` row is removed.
  - **ARCH-5:** add "Only `gitcroc-ui` depends on GPUI."
  - **ARCH-6:** "front end" becomes "UI".
  - **ARCH-7:** "No work over 2 ms runs on the UI thread. Anything slower than 50 ms is a job that reports progress by events."
  - **ARCH-8:** "Lists reach the UI in pages", with the same page sizes; the reason is memory.
- Section 8:
  - **UI-23:** "Implemented as theme tokens only; no component hardcodes a color. The accent color follows the system's where the platform reports one."
- Section 9:
  - **PERF-1:** "The UI never lays out more rows than fit on screen plus a small overscan."
  - The scroll row becomes "Scroll history or a diff at any speed | the display's refresh rate, up to 144 Hz, with under 1% of frames over 1.5 refresh intervals; no row blank for more than 100 ms".
  - The memory row: "including the webview" becomes "the whole process".
  - **PERF-8:** "frame rate, first paint" stays, measured by `just bench-ui` in the app's `perf-trace` build.
- Section 10:
  - **SEC-2** becomes: "The app loads no code, page or style from anywhere but its own binary; the only files it downloads are update packages, verified before use (SEC-7). Repo text is drawn as text (SEC-3)."
- Section 11:
  - The Webview column becomes "Graphics":
    - Windows: Direct3D 11.
    - macOS: Metal.
    - Linux: Vulkan, or GL where Vulkan is missing.
  - The WebView2 note goes.
  - **PLAT-4:** "Auto-update uses the app's own updater, which reads the same manifest and verifies with the same key as 1.0.0-rc.1, so that release updates to the GPUI build."
- Section 12:
  - The front-end rows become one row: "UI | `cargo test` with GPUI's test context, headless | Views and flows against the real core on fixture repos".
  - The smoke row: "the `e2e` build's scenario runner, Linux and Windows".
  - **QA-1** drops `tsc --noEmit`, ESLint, Vitest and Playwright.
- Section 13:
  - "Six milestones" becomes "Seven".
  - A new M6 row: "M6 Native UI | The whole UI rewritten in GPUI, Tauri and the web front end removed, 1.0.0-rc.2 | Every UI requirement has a GPUI test, the section 9 budgets are met with the new scroll budget, and rc.1 updates to rc.2 on all three platforms".
  - The M5 row's v1.0.0 waits for M6.
- Section 14:
  - The `CLAUDE.md` starter's stack line and invariants: drop "IPC types are generated; never hand-write them"; the ARCH-5 line names `gitcroc-ui` and GPUI.
- Section 15:
  - The "Front-end framework (D2)" answer becomes "GPUI, chosen for performance after a spike (ADR 0004)".
- Outside the spec, in the code commit:
  - `CLAUDE.md`'s first line says "Rust core; Tauri 2 shell and SolidJS UI, being replaced by GPUI in milestone M6 (docs/plans/native-ui-gpui.md)". The invariants stay until M6-30, which writes the final file.
  - `docs/plans/native-ui-gpui.md`: status "accepted", with a link to the ADR.
- The spec and ADR edits are one commit without code (DEV-1).

## Out of scope
- Any code. The decision test `d2_shell_is_tauri_2_and_ui_is_solid_and_strict_typescript` keeps checking the shipped Tauri app until M6-30 rewrites it.
- The user docs (M6-29).

## Acceptance
- ADR 0004 exists and the spec edits above are made, in their own commit.
- No requirement ID was added or removed: `just coverage spec` reports the same missing list as before, which is empty.
- `just check` passes.
- A review against `docs/plans/native-ui-gpui.md` finds no rule of the plan missing from the spec.
