# M5-23 The tab bar is the title bar

## Goal
Asked for in M5-20's verification of `1.0.0-rc.1` (Eric, 2026-10-23): like most current desktop apps, gitcroc draws its own title bar instead of the system's. Eric's decisions: the workspace tab bar becomes the title bar, so the window gains a row of height; and a setting, off by default, brings back the system's title bar for desktops that work better with it.

## Depends on
M5-20

## Spec
Implements: UI-35 (new, in section 8 "Window", added by this task)
Respects: UI-27, UI-28, UI-22, SEC-2, ARCH-6, ARCH-7, DATA-1, D15, PLAT-1, section 10 "Files"

## In scope
- Spec: UI-35 in a new "Window" section of `docs/spec/08-ui.md`, and the window diagram's tab row shows the window buttons.
- Settings: `system_title_bar`, a top-level key of `settings.toml` beside the fonts, default `false`; the schema stays 2, a file without it loads unchanged, and older builds ignore it (DATA-1). Saved with the other appearance settings.
- Window: on Windows and Linux the window has no system decorations unless `system_title_bar` is on. On macOS the system title bar becomes transparent and its title hidden (Tauri's overlay style), so the system's traffic lights stay, over the left end of the tab bar. The choice is applied before the window first shows, and again at once when the setting changes.
- IPC: `window_control(action)` with minimize, toggle maximize, close and start dragging, and `window_maximized()`, as the app's own commands: the window gets no Tauri window permission (SEC-2).
- Tab bar: the empty space after the "+" button moves the window when dragged and maximizes or restores it on a double-click. Tabs keep their own dragging (UI-28). On Windows and Linux, minimize, maximize or restore, and close buttons at its right end; on macOS, room at its left end for the traffic lights. The maximize button shows restore while the window is maximized.
- The screens without a tab bar, looking for git and the git setup screen, get a title bar of their own with the app's name, the same dragging and the same buttons.
- The window buttons are labelled for screen readers and left out of the tab order, as the system's are; the system's keyboard shortcuts for the window keep working.
- Settings dialog: a Window section with **Use the system title bar**.
- User docs: `docs/user/settings-and-files.md` names the setting.
- Text comes from the strings module (D15).
- Tests:
  - Rust: `ui_35_system_title_bar_defaults_off_and_round_trips` (settings), `ui_35_title_bar_style_per_platform` (what the app asks of the window per OS and setting).
  - Vitest: the tab bar shows the window buttons on Windows and Linux, none on macOS but the room for the traffic lights, and none with the system title bar; the maximize button follows the window's state.
  - Playwright: `UI-35` the buttons send their actions to the core, a drag on the empty space moves the window and a double-click maximizes it, a drag on a tab does not; the setting hides the buttons; the git setup screen has the title bar too.

## Out of scope
- A menu bar or an application menu. Custom resize borders: the window keeps the system's resizing.
- Anything that needs a Windows PC or a Mac to check by hand (deferred, as decided for M0 on 2026-10-12); CI builds and smoke-tests both.

## Acceptance
- The tests above pass, and `just check` passes.
- A package change: it ships in the next release candidate (M5-20).
