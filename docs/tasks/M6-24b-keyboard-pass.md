# M6-24b The keyboard pass

## Goal
Every operation of sections 5 and 6 is reachable without a mouse from its
own place in the window, not only through the palette, and one test proves
it operation by operation (UI-22). `KEYMAP` holds every app-wide shortcut
of `docs/user/keyboard.md`.

## Depends on
- M6-24a: the palette and its shortcut column.
- M6-09 to M6-23c: the places whose keys this task audits. Most already
  have their own `ui_22_*` test; those keep passing.

## Spec
Implements: UI-22 (every operation reachable without a mouse)
Respects: OPS-3, OPS-6, OPS-9, AI-11, UI-6, UI-7, UI-13, UI-15, UI-20, UI-21, UI-28, GRP-9, ARCH-6, D15, QA-2, QA-3, QA-4, section 12 "UI" row

## In scope
- **The keymap**, a port of the rest of `ui/src/keys.ts`:
  - Ctrl/Cmd+, opens Settings. It is the Settings row's shortcut in the
    palette;
  - `keyboard.md`'s app-wide table and `KEYMAP` are the same list: the docs
    test checks both ways, and the "until M6-30" note in `keys.rs` goes;
  - no app-wide shortcut shadows a key of gpui-component's input or code
    editor. The pull request lists the keymaps that were checked.
- **The audit**, place by place, against `keyboard.md` and rc.1's
  `ui/e2e/keyboard.spec.ts`. Close every gap it finds; at least check:
  - the tab bar: Left, Right, Home, End and Delete, and the tab and "+"
    menus (UI-28);
  - the tree: Enter on a branch and on a group's Branches name, Shift and
    Ctrl/Cmd with the arrows, Ctrl/Cmd+Space, Shift+F10 and the context
    menu key;
  - the history: the commit menu from the keyboard on the selected commit,
    and a multi-selection for cherry-pick with Shift and Ctrl/Cmd+Space
    (UI-13);
  - the changes pane: Space or Enter stages or unstages the selected file,
    or moves into an unmerged file's editor; Delete (Backspace on macOS)
    asks to discard (GRP-9); a hunk's controls by Tab and F7 (UI-15);
  - the conflict editor: Take ours, theirs and both, OPS-6's choice of
    version, and Accept, Reject and Edit of a proposal (AI-11) keep their
    keys;
  - the status bar opens the job list, and the job list and a result report
    are navigable (UI-21);
  - menus: arrows, Home and End move, Escape gives the focus back.
- **The user documentation** (`docs/user/keyboard.md`): corrected wherever
  the GPUI app differs from it, as the audit finds.

## Tests (QA-3)
GPUI tests against the real core on `repo-gen` fixtures (QA-2), driven only
by keystrokes: no click, no mouse event.
- `ui_22_every_operation_reachable_without_a_mouse`: one row per operation
  of sections 5 and 6, the same list as `palette_catalog()` less the window
  actions. Each row starts from a fixed state, reaches the operation through
  its own place (toolbar, menu, pane control), not the palette, and asserts
  that its dialog opened or its job started.
- `ui_22_settings_opens_with_its_shortcut`.
- `ui_22_menus_move_with_the_arrow_keys_and_give_the_focus_back_on_escape`.
- `ui_22_the_job_list_a_jobs_output_and_its_report_are_reachable_by_keyboard`.
- In `keys.rs`: `ui_22_no_two_shortcuts_share_keys`,
  `ui_22_keyboard_md_and_the_keymap_list_the_same_shortcuts` (replacing
  the one-way docs test), and
  `ui_22_no_shortcut_shadows_an_editor_key`.
- The existing `ui_22_*` tests keep passing.

## Out of scope
- **Screen readers, focus announcements and contrast:** M6-25.
- **Custom key bindings:** not in v1.

## Acceptance
- **On the reference machine,** `just dev-next` with no mouse: a pass
  through `keyboard.md`, including a merge conflict resolved and continued.
- The pull request lists each gap the audit found and how it was closed.
- `just check` passes.
