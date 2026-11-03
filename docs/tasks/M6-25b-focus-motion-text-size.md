# M6-25b Focus, motion and text size

## Goal
The focus and the selection can always be seen, in every theme, without
relying on a background. Dialogs keep the focus as M5-10 required, motion
stops when the system asks, and no label is cut at the largest UI font size
(README decisions table, "Accessibility bar in M6", musts 3, 6 and 7, and
the label check).

## Depends on
M6-24b: the Tab order and focus handling this task builds on.

## Spec
Respects: UI-3, UI-22, UI-23, UI-26, ARCH-6, ARCH-7, D15, QA-2, QA-3, QA-4, section 12 "Front end flows" row

## In scope
- **The focus ring:**
  - **Drawing it:** one shared helper draws it on every tab stop of
    gitcroc's own elements, from the `focus_ring` token. gpui-component's
    controls keep their own ring.
  - **Selected and focused rows:** in the history, the tree, the file
    lists, the palette, the job list and menus, the selected row and the
    focused row have a border, not only a background, in every theme. This
    replaces M5-10's forced-colours check. Windows high-contrast detection
    is LATER.
  - **Testing it:** the existing contrast test already covers the ring's
    contrast (`ui_23_app_theme_colors_meet_wcag_contrast`). It also gets
    the border of the selected row. Whether the ring shows on each stop is
    checked by hand in M6-29, not by a test.
- **Focus:**
  - **On open:** the window focuses something, since with nothing focused,
    Tab does nothing (M6-04 finding 5).
  - **Dialogs:** every dialog takes the focus, keeps Tab inside, closes on
    Escape where M2 to M4 allow it, and gives the focus back to what opened
    it. Tests that already exist for this are kept and listed, not
    duplicated.
- **Reduced motion:**
  - **Following the system:** the app follows `App::reduce_motion`, which
    gpui-base sets from macOS, Windows and the XDG portal.
  - **Linux fallback:** where the portal has no `reduced-motion` key (seen
    on Cinnamon), the app reads `org.gnome.desktop.interface
    enable-animations`. It reads it once at start, off the UI thread
    (ARCH-7), with an argument array and no shell (ARCH-3).
  - **What changes:** under reduced motion the spinners (UI-3) stand still
    and keep their name, progress bars still fill, and nothing else
    animates.
- **Labels at the largest UI font size** (32 px, UI-26):
  - **Sizing:** buttons and tabs size to their label, with no fixed widths.
  - **Testing it:** a `#[gpui::test]` at 32 px compares each control's
    bounds with its label's laid-out width. The test platform's text system
    gives every character one em, which overestimates real text. The test
    covers the toolbar, the tab bar and every dialog.
  - **If a control falls outside the window** at the default window size,
    stop and ask (DEV-2).

## Tests (QA-3)
- `ui_22_the_window_focuses_something_on_open`.
- `ui_22_every_dialog_takes_keeps_and_gives_back_the_focus`: one row per
  dialog.
- `ui_23_selected_rows_have_a_border_in_every_theme`: the history, tree,
  file list, palette and job list.
- `ui_23_reduced_motion_stills_the_spinner_and_fills_the_bar`.
- `ui_26_no_label_is_cut_at_the_largest_ui_font_size`.
- A unit test for reading the Linux fallback, against canned output.

## Out of scope
- **The accessibility tree and the live status:** M6-25a.
- **Windows high contrast:** LATER (README decisions table).
- **Following the system text size:** GPUI has no signal for it (M6-04
  finding 12). UI-26's setting covers it.

## Acceptance
- The tests above exist and pass.
- **On the reference machine:** with animations turned off in Cinnamon, the
  preflight spinner stands still.
- `just check` passes.
