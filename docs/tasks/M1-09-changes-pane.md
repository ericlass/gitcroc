# M1-09 Changes pane

## Goal
Selecting a commit or the uncommitted changes row shows the commit header, the changed files and the diff of the selected file, inline or side by side.

## Depends on
M1-03, M1-04, M1-08

## Spec
Implements: UI-14, UI-16, UI-17, UI-18, UI-19
Respects: UI-15, ARCH-6, ARCH-8, ARCH-9, PERF-1, SEC-2, SEC-3, UI-23, UI-24, section 9 "Select a commit" budget

## In scope
- Add CodeMirror 6 and `@codemirror/merge` to `ui/`. Record the versions in the pull request.
- For a commit (UI-14): header with full hash, author, committer, dates, full message and parents (each parent clickable to select it in the history). Then the changed files, as a flat list or a folder tree, with a toggle remembered globally in `settings.toml`. Then the diff of the selected file. The first file is selected automatically.
- For a merge commit (UI-16): the diff is against the first parent, with a selector to switch parent; switching reloads the file list.
- For "Uncommitted changes": the Staged and Unstaged lists (untracked files in Unstaged, unmerged files first, marked) and the diff of the selected file. This is the read-only part of UI-15; the stage, unstage and discard controls and the commit box arrive in M2.
- Diff (UI-17):
  - Inline and side-by-side modes, toggled in the pane header and remembered globally in `settings.toml`.
  - Options: ignore whitespace, wrap lines, context lines (3, 10, whole file). Ignore whitespace and context are applied by the core (M1-03); wrap is a view option. All three are remembered globally.
  - Syntax highlighting by file extension, using CodeMirror's language packages for the common languages; unknown types show plain text.
  - The diff view is read-only.
- Special diffs (UI-18), from the M1-03 `FileDiff` kinds: binary shows "binary file changed" with both sizes; images show before and after side by side; a diff over 20,000 changed lines is collapsed with a "Load diff" button; a submodule shows the old and new commit.
- Images are passed as bytes and shown through `blob:` URLs. The CSP (SEC-2) gets only the `img-src` addition this needs. CodeMirror's injected styles must work under the existing CSP, through Tauri's style nonce or constructable stylesheets; `'unsafe-inline'` is not added. Update the SEC-2 test for the new CSP.
- Docking (UI-19): a button in the pane header docks the pane right (default) or bottom. The choice is stored per workspace in the workspace file's pane layout (section 10), together with the pane sizes that M0-07 kept in memory only.
- File lists are virtualized (PERF-1). All repo text is rendered as text, never as HTML (SEC-3); CodeMirror renders content as text.
- Colors for added, removed and changed lines use temporary theme tokens until M1-10 brings code themes.
- Tests:
  - `UI-14 commit header and file list` (Vitest): header fields, flat and tree toggle, first file selected.
  - `UI-16 merge commit parent selector` (Vitest): switching parent requests the other parent's file list.
  - `UI-17 diff modes and options are remembered` (Vitest): toggling mode and options calls the settings commands and survives a reload of the store.
  - `UI-18 special diffs` (Vitest): binary, image, collapsed large diff with "Load diff", submodule.
  - `UI-19 dock position is per workspace` (Vitest) and `ui_19_pane_layout_round_trips_in_workspace_file` (Rust).
  - Playwright, fake core: selecting a commit shows its files and the first diff; switching to side-by-side shows two columns.
  - The SEC-2 test passes with the new CSP.

## Out of scope
- Stage, unstage and discard controls, commit message box (M2, rest of UI-15).
- Conflict editor (M2). Group uncommitted changes (UI-34, M3).
- Code themes and fonts (M1-10).

## Acceptance
- `just dev`: selecting a commit shows its files and diffs in both modes; a merge commit can switch parent; uncommitted changes show both lists; an image change shows both images.
- All tests listed above exist and pass on all three platforms.
