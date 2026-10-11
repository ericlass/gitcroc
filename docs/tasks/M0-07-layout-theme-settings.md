# M0-07 Layout, theme and settings

## Goal
The window shows the empty three-pane layout of the spec with a tab bar, toolbar and status bar, switches between light and dark themes, and remembers the choice across restarts.

## Depends on
M0-05

## Spec
Implements: UI-23, DATA-1, ARCH-9
Respects: UI-19, UI-20, UI-21, UI-26, UI-27, section 8 layout diagram, section 10 "Files" table, PLAT-1, PLAT-2

## In scope
- Layout (structure only, every pane empty with a placeholder label):
  - Tab bar with one static tab labelled "No workspace" and a disabled "+" button.
  - Toolbar with the buttons of UI-20 (Fetch, Pull, Push, Branch, Merge, Stash, command palette, Settings), all disabled except Settings.
  - Three panes: Tree, History, Changes, separated by two draggable splitters. Minimum pane width 200 px. Double-click on a splitter resets it.
  - Status bar with the text "No jobs running".
- Pane sizes are held in a store keyed by workspace id (ARCH-9), using a fixed placeholder id until workspaces exist in M3. No module-level "current workspace" variable.
- Theme (UI-23):
  - Setting with three values: `system` (default), `light`, `dark`.
  - Implemented only through CSS variables defined in `ui/src/theme/tokens.css` for `:root[data-theme="light"]` and `:root[data-theme="dark"]`. Tokens cover at least: background (3 levels), text (primary, secondary, disabled), border, accent, danger, warning, success, focus ring.
  - `system` follows `prefers-color-scheme` live, without a restart.
  - A Settings dialog with only the theme choice for now.
  - Lint or test that fails on any color literal (`#hex`, `rgb(`, `hsl(`, named colors) in a component stylesheet or inline style outside `tokens.css`.
- Settings file (DATA-1), in `gitcroc-core`:
  - `settings.toml` in the platform config dir under `gitcroc/` (use the `directories` crate; one trait so tests can point it at a temp dir).
  - Content for now: `schema = 1` and `theme = "system"`.
  - Atomic write: write to a temp file in the same directory, flush, then rename over the target.
  - Loading: missing file gives defaults; `schema` lower than current is migrated (no migrations exist yet, but the hook does); `schema` higher than current loads read-only and returns a warning the UI shows as a banner; a file that fails to parse is left untouched, defaults are used, and a warning is shown.
  - Commands `settings_get` and `settings_set_theme`, with generated types.
- Tests:
  - `data_1_write_is_atomic`: after a save, no temp file remains and the target parses; a simulated failure before rename leaves the old file intact.
  - `data_1_newer_schema_is_read_only`: a file with `schema = 99` loads, a save is refused, the warning is returned.
  - `data_1_missing_file_gives_defaults` and `data_1_corrupt_file_is_not_overwritten`.
  - Vitest `UI-23 theme switch`: selecting dark sets `data-theme="dark"` on the root; `system` follows a mocked media query change.
  - Vitest `UI-23 no hardcoded colors`: the lint above, run as a test.
  - Vitest `ARCH-9 pane sizes are stored per workspace`: two workspace ids keep independent sizes.
  - Playwright: drag a splitter and assert the pane width changed and the minimum is enforced; toggle theme and assert the background token changed.

## Out of scope
- Code themes for diffs (UI-24, UI-25) and fonts (UI-26): M1.
- Real tabs and workspaces (M3). Docking the changes pane at the bottom (UI-19): M1.
- Any tree, history or diff content. `cache.db` and the log file.

## Acceptance
- `just dev` shows the layout; splitters drag; the theme switches instantly and survives a restart.
- Changing the OS theme while the setting is `system` changes the app without a restart.
- All tests listed above exist and pass on all three platforms.
