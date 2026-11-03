# M6-24a The command palette

## Goal
Ctrl/Cmd+K, or the toolbar's **Command palette**, opens the palette. The
user types, moves through the list with the keys, and runs an operation or
selects a repo, group or branch in the tree (UI-22). Every operation runs
what its own place in the window runs; the palette adds no dialog, no way
to run an operation and no rule of its own.

## Depends on
- M6-18 to M6-23c: every place the catalog's operations live in, which
  this task collects. Their task files left the palette's entries to M6-24.
- M6-10: the tree's selection, which a repo, group or branch row sets.
- M6-06: the toolbar's disabled palette button and `keys.rs`.

## Spec
Implements: UI-22 (the palette; M6-24b completes keyboard reach)
Respects: AI-3, AI-16, UI-6, UI-8, UI-20, GRP-5, GRP-9, OPS-9, PERF-1, SEC-3, ARCH-6, ARCH-7, ARCH-8, ARCH-9, D15, QA-2, QA-3, QA-4, section 9 "UI input latency"

The core is done (M5-07): `palette.rs` holds the catalog, matches and ranks,
answers at most 100 items with the total, and leaves the AI entries out
while AI features are off. `palette_query(workspace_id, text)` (M6-05) gives
`None` when a newer query of the workspace superseded it. The UI filters
nothing (ARCH-6).

## In scope
- **Opening and closing**, a port of `ui/src/palette/Palette.tsx` and M5-26:
  - Ctrl+K (Cmd+K on macOS) anywhere in the main window, also inside the
    diff and the conflict editor, and the toolbar's **Command palette**,
    enabled now (UI-20). The shortcut is a `KEYMAP` row;
  - not while another dialog is open;
  - the key again, Escape, or a click outside it closes it. A click inside
    leaves it open, and the outside click does nothing else. The focus goes
    back to what had it.
- **The window**: a text box above a `uniform_list` (PERF-1). Each row shows
  its kind, its text, its detail (a repo's branch, a group's size, a
  branch's repo), the reason it cannot apply, and an operation's shortcut
  from `KEYMAP`, if it has one. A line under the list says when more
  matched than are shown, or that nothing matched; an error says the
  search failed.
- **Typing** sends `palette_query` for the active workspace on each change,
  off the UI thread (ARCH-7). An answer the core superseded, or one older
  than the one shown, is dropped. The list keeps the last answer until a
  newer one arrives, so typing never waits (section 9).
- **Keys:** Up, Down, Page Up, Page Down, Home and End move the highlight
  and keep it in view; Enter runs the highlighted row.
- **The commands**, a port of `ui/src/palette/commands.ts`:
  - every `PaletteOp` has one place in the window: the toolbar and the
    window, the tab bar, the tree, the history, the changes pane's file
    lists, the commit box, the conflict banner, the conflict editor, or a
    file's choice of version (OPS-6). One exhaustive `match`, so a new
    operation in the core fails to compile until it has a place;
  - each place provides its operations with the function its own button,
    menu entry or control calls, and the rule that enables that control:
    the same dialog, the same preflight for a target set of several repos,
    the same confirmation for a destructive one (GRP-9);
  - a place provides only while it is shown, for the workspace it shows
    (ARCH-9);
  - an operation that cannot apply now is disabled with the reason its
    control gives. One whose subject the selection does not give (Rename
    branch while a repo is selected, Revert with no commit selected) says
    what to select. Enter does nothing on either.
- **Repo, group and branch rows** select it in the tree as a click does: the
  tree scrolls to it and the history follows. A branch expands its repo and
  folders to show its row (M6-10's reveal). The palette closes and the
  focus goes to the tree.
- **AI** (AI-3, AI-16): no AI entry while AI features are off, from the
  core. Resolve with AI and Resolve all with AI go through M6-23b's and
  M6-23c's paths, so a missing provider opens Settings at the AI tab and
  consent is asked as from the editor or the banner.
- **Settings** opens the Settings dialog, at the AI tab until M6-26.
  **Check for updates** opens the update dialog when a version is on offer
  (M6-15b); otherwise it is disabled with a reason until M6-26's Updates
  tab can show a check's result.
- **Strings (D15):** `strings.palette` moves from `ui/src/strings/en.ts` to
  `strings.rs`. Operation labels come from the core.

## Tests (QA-3)
GPUI tests against the real core on `repo-gen` fixtures (QA-2). No test
waits on timing.
- `ui_22_palette_opens_by_key_and_by_button_and_closes_giving_the_focus_back`:
  by Escape and by the key.
- `ui_22_a_click_outside_the_palette_closes_it_and_one_inside_does_not`.
- `ui_22_the_palette_opens_from_inside_the_conflict_editor`.
- `ui_22_the_palette_keeps_the_last_answer_and_drops_a_superseded_one`.
- `ui_22_the_keys_move_the_highlight_and_enter_runs_it`.
- `ui_22_fetch_on_a_group_goes_through_its_preflight_and_a_hard_reset_asks_first`.
- `ui_22_an_operation_that_cannot_apply_shows_why_and_enter_does_nothing`:
  one disabled by its control's rule and one missing its subject.
- `ui_22_the_palette_selects_a_repo_a_group_and_a_remote_branch`: the tree
  selection and the history follow; the remote branch's row is revealed.
- `ui_22_a_line_says_when_more_matched_than_are_shown`.
- `ui_22_an_operation_with_a_shortcut_shows_its_keys`.
- `ui_22_every_palette_operation_runs_its_action`: for every op of
  `palette_catalog()`, with a state that gives its subject, choosing it
  does what its own control does: the same dialog opens or the same job
  starts.
- `ai_3_resolve_with_ai_from_the_palette_opens_settings_without_a_provider`.
- `ai_16_the_palette_has_no_ai_entries_while_ai_is_off`.
- `perf_1_the_palette_list_lays_out_only_the_visible_rows`.
- `sec_3_palette_names_render_as_text`: a branch and a repo named with
  markup.

## Out of scope
- **New shortcuts and the keyboard audit:** M6-24b.
- **Custom key bindings:** not in v1.
- **The Updates tab and the rest of Settings:** M6-26.

## Acceptance
- **On the reference machine,** `just dev-next` on `target-project`: typing
  in the palette while the focus refresh runs stays responsive (noted in
  the pull request; no test asserts a time). Run Fetch on a group, select a
  remote branch, and open Settings, all from the palette.
- `just check` passes.
