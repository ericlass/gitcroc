# M0-04 Tauri shell and SolidJS app

## Goal
`just dev` opens a native window that renders a SolidJS page from bundled assets, and the front-end checks are part of `just check`.

## Depends on
M0-03

## Spec
Implements: SEC-2, QA-1 (front-end half)
No test needed: QA-1 (the workflow itself is the proof)
Respects: D2, ARCH-5, ARCH-6, PLAT-1, UI-23, section 4 "Stack" table

## In scope
- `crates/gitcroc-app` becomes the Tauri 2 application crate (binary plus `tauri.conf.json`, `capabilities/`, icons). Use the current Tauri 2.x release; record the exact version in the pull request.
- `ui/`: Vite + SolidJS + TypeScript with `strict: true`. pnpm, Node version pinned in `.nvmrc`.
- The page shows only the app name and version text. No layout yet (M0-07).
- Security (SEC-2):
  - Content security policy in `tauri.conf.json`: `default-src 'self'`; no remote scripts, styles, frames or fonts; no `unsafe-eval`.
  - Capabilities grant the main window only the app's own commands plus the minimum core permissions needed to show a window. No shell, filesystem or HTTP plugin.
  - Test `sec_2_csp_forbids_remote_sources`: parses `tauri.conf.json` and asserts the policy.
  - Test `sec_2_capabilities_expose_no_shell_or_fs`: parses the capability files and fails on any `shell:`, `fs:` or `http:` permission.
- Front-end tooling: ESLint (typescript-eslint, solid plugin), Vitest with Solid Testing Library, Playwright running against `vite build` + `vite preview` with Chromium only.
- One Vitest test and one Playwright test that assert the page renders the app name, to prove the harness works.
- `just check` gains: `pnpm install --frozen-lockfile`, `tsc --noEmit`, ESLint, Vitest, Playwright. `just dev` runs `tauri dev`.
- CI: install Node and pnpm; on Linux install the WebKitGTK 4.1 build packages; install Playwright's Chromium with caching.
- Window title "gitcroc", default size 1280x800, minimum 960x600.
- App identifier `dev.gitcroc.app`.

## Out of scope
- Any IPC command (M0-05), layout or theme (M0-07), git (M0-06).
- Installers, signing, updater (M0-08, M5).
- WebDriver end-to-end tests (`tauri-driver`); they start in M1.

## Acceptance
- `just dev` opens the window on the developer's platform and shows the app name.
- `just check` passes locally and in CI on all three platforms, now including the front-end checks.
- `cargo metadata` still shows no Tauri dependency in `gitcroc-git`, `gitcroc-core` or `gitcroc-ai` (the `arch_5_*` tests from M0-01 pass).
- Both `sec_2_*` tests exist and pass.
