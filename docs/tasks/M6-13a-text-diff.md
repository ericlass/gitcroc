# M6-13a Text diff: both modes and options

## Goal
The GPUI changes pane shows the selected file's text diff, as rc.1's does:
inline, the core's hunks with both line numbers, or side by side, both
files with unchanged runs folded to the chosen context. The pane header
switches the mode and sets the options: ignore whitespace, wrap lines and
context lines. F7 and Shift+F7 move between hunks.

This is the first of three parts of M6-13. Syntax highlighting and the
diffs that are not text are M6-13b; docking is M6-13c.

## Depends on
- M6-12: the changes pane and its selected file.
- M6-07: the code themes the diff draws with.

## Spec
Implements: UI-17
Respects: UI-14, UI-15, UI-16, UI-22, UI-23, UI-25, UI-29, ARCH-6, ARCH-7, ARCH-8, PERF-1, SEC-3, D15, QA-3, QA-4

## In scope
- **Reading the diff** (ARCH-7, ARCH-8), a port of `loadDiff` in
  `ui/src/stores/changes.ts`:
  - the selected file's diff is read with `diff_get`, by the view's handle
    and the file's `FileRef`, one file at a time;
  - the request is `ignore_whitespace` and `context` from the settings,
    and `full_text` in side-by-side mode;
  - a newer selection, view or option drops an older answer; a failed read
    says why in the diff area;
  - an unmerged file has no diff (its conflict editor is M6-19);
  - a new view, a parent switch and a refresh (OPS-7) read the diff again.
- **Inline mode** (UI-17), a port of `inlineDoc` in
  `ui/src/changes/diffDoc.ts`: each hunk's `@@ -a,b +c,d @@` header, then
  its lines, each with its old and new line number. Added and removed lines
  have the code theme's colours (UI-25).
- **Side-by-side mode** (UI-17), as `changeRanges` and the MergeView in
  `ui/src/changes/DiffView.tsx` show it:
  - the old file on the left and the new on the right, one row per line,
    with a run of removed lines beside the run of added lines that replaced
    it, and an empty cell where one side has fewer;
  - unchanged runs are folded to the context (3 or 10 lines on each side of
    a change, nothing for the whole file), as rc.1 folds them: a run folds
    only when at least 4 lines would be hidden. A fold says how many lines
    it hides, and a click unfolds it;
  - each hunk's header is a row above its first change;
  - while the whole files have not arrived, the diff at hand shows inline.
- **The rows** are built off the UI thread from the core's diff (ARCH-7),
  as `SharedString`s, and drawn in one virtualized list (PERF-1). Without
  wrapping, long lines scroll sideways; with wrapping, they wrap and the
  list measures each row.
- **The pane header** (UI-17), a port of the toolbar in
  `ui/src/changes/ChangesPane.tsx`: Inline and Side by side as a toggle
  pair, Ignore whitespace and Wrap lines as checkboxes, and Context as a
  choice of 3 lines, 10 lines and the whole file. Each is kept with
  `settings_set_changes` and applies to every workspace, through the
  `ChangesPrefs` global. A change of mode, whitespace or context reads the
  diff again; wrap does not.
- **Notes** in the diff area:
  - "No lines changed." for a diff without hunks;
  - the missing final newline of either side;
  - for uncommitted changes with Ignore whitespace on, that hunks can be
    staged with it off.
- **F7 and Shift+F7** (UI-22): while a diff is shown, they scroll to the
  next or previous hunk, wrapping around, and give the focus to its
  header. The keys are added to `keys.rs`'s table and the keymap docs.
- **Per tab** (UI-29): each tab's pane keeps its diff, its scroll position,
  its unfolded runs and the hunk F7 last went to.
- **SEC-3:** file content is drawn as text.
- **Strings (D15):** the diff's text for this task moves from
  `ui/src/strings/en.ts` to `strings.rs`.

## Tests (QA-3)
- `ui_17_inline_diff_shows_the_hunks_with_both_line_numbers`: the header,
  and the context, removed and added lines with their numbers; a missing
  final newline is named.
- `ui_17_side_by_side_pairs_the_changed_lines_and_folds_the_rest`: both
  sides, the folded run with its count, and a click unfolds it.
- `ui_17_diff_options_are_kept_and_read_the_diff_again`: whitespace and
  context read again with the new request (mode in the side-by-side
  test); wrap does not; all four are in the settings.
- `ui_17_wrap_lines_wraps_long_lines`: without wrapping, a long line
  scrolls sideways and only sideways; with it, the line wraps.
- `ui_22_f7_moves_between_hunks`.
- `ui_29_each_tab_keeps_its_diff`: diff and scroll position come back
  after a tab switch, and the other tab's diff takes the mode chosen in
  the first.
- `perf_1_the_diff_lays_out_only_the_rows_on_screen`.
- `sec_3_diff_content_renders_as_text`.
- Unit tests of the rows: inline numbering, side-by-side pairing of uneven
  runs, folds at the start, middle and end of a file, the 4-line minimum,
  whole-file context, and files without a final newline.
- `ui_23_no_hardcoded_colors`, `d15_ui_text_comes_from_the_strings_module`
  and the keymap docs test keep passing over the new files.

## Out of scope
- **Syntax highlighting** (UI-24) and the **binary, image, too large,
  folder and submodule diffs** with **Load diff** (UI-18): M6-13b. Until
  then they show their kind's name.
- **Docking** (UI-19): M6-13c.
- **Stage, unstage and discard per hunk** (UI-15): M6-16. F7 gives the
  focus to a hunk's first control once there are any.
- **Background tabs releasing their diff** (UI-31): M6-14.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`: a
  commit's file shows inline and side by side; each option changes the
  diff; F7 moves between hunks.
- `just coverage M6` no longer lists UI-17.
- `just check` passes.
